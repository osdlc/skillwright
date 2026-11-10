//! The host side of authoring: turning an [`AuthoringOp`] into writes.
//!
//! `tradecraft_core::authoring` decides and returns a plan; nothing in it
//! touches a filesystem, because it also compiles for the browser. This module
//! is the other half for this host — the one that has a real directory, a
//! trash folder, and `markdown_relink`'s link-rewriting rename.
//!
//! [`apply`] writes. [`preview`] runs the same code paths and stops before the
//! first byte, so a `--dry-run` and a real run report the same rows. Both hand
//! back an [`Outcome`] of raw facts — paths, link-edit counts, which
//! frontmatter keys moved — and never a rendered line: the wording belongs to
//! the command layer.

use std::path::{Path, PathBuf};

use markdown_corpus::{Corpus, LoadOptions, SplitMode};
use markdown_relink::{FileEdits, RenamePlan, RenameResult, VaultFacts};
use tradecraft_core::authoring::{AuthoringOp, FieldEdit, apply_field_edits};
use tradecraft_core::vfs::{PreviewVfs, Vfs};

/// Write or rehearse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Apply,
    Preview,
}

impl Mode {
    fn writes(self) -> bool {
        self == Mode::Apply
    }
}

/// Everything an op did, or would do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Outcome {
    /// True when nothing was written.
    pub preview: bool,
    /// Each fact, in the order it happened.
    pub entries: Vec<AppliedEntry>,
}

impl Outcome {
    fn new(mode: Mode) -> Self {
        Self {
            preview: !mode.writes(),
            entries: Vec::new(),
        }
    }

    fn push(&mut self, path: impl Into<String>, applied: Applied) {
        self.entries.push(AppliedEntry {
            path: path.into(),
            applied,
        });
    }
}

/// One reportable fact, against the vault-relative path it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AppliedEntry {
    /// The vault-relative path.
    pub path: String,
    /// What happened to the path.
    pub applied: Applied,
}

/// What happened to one path. The variants carry the facts a report line
/// needs; [`Applied::outcome`] and [`Applied::reason`] are the printable
/// reduction of them, not a second source of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Applied {
    /// A note written where none existed.
    Created,
    /// A directory made — a bundle folder, or a container kind's member
    /// folder.
    Scaffolded,
    /// The subject moved. `from` is its old vault-relative path.
    Renamed {
        /// The old vault-relative path.
        from: String,
    },
    /// A peer file carried along by a bundle move.
    PeerMoved {
        /// The peer's old vault-relative path.
        from: String,
    },
    /// A third note whose inbound links were repointed.
    LinksRewritten {
        /// How many links were repointed.
        edits: u64,
    },
    /// A frontmatter key set to a value.
    FieldSet {
        /// The frontmatter key.
        key: String,
    },
    /// A frontmatter key removed.
    FieldRemoved {
        /// The frontmatter key.
        key: String,
    },
    /// A path moved under the trash folder.
    Trashed {
        /// The vault-relative path under the trash folder.
        to: String,
    },
    /// A note `markdown_relink` declined to write, with its stable reason
    /// (`drifted`, `not_previewed`, `unreadable`, `write_failed`), or a
    /// folder the rename could not list (`unreadable`). Its links still name
    /// the old path.
    Skipped {
        /// The stable reason token.
        reason: String,
    },
}

impl Applied {
    /// The stable token a report row prints in its outcome column.
    pub fn outcome(&self) -> &'static str {
        match self {
            Applied::Created => "created",
            Applied::Scaffolded => "scaffolded",
            Applied::Renamed { .. } => "renamed",
            Applied::PeerMoved { .. } => "moved",
            Applied::LinksRewritten { .. } => "relinked",
            Applied::FieldSet { .. } => "field-set",
            Applied::FieldRemoved { .. } => "field-removed",
            Applied::Trashed { .. } => "trashed",
            Applied::Skipped { .. } => "skipped",
        }
    }

    /// The specifics, when there are any.
    pub fn reason(&self) -> Option<String> {
        match self {
            Applied::Created | Applied::Scaffolded => None,
            Applied::Renamed { from } | Applied::PeerMoved { from } => Some(format!("from {from}")),
            Applied::LinksRewritten { edits } => Some(format!("{edits} link(s)")),
            Applied::FieldSet { key } | Applied::FieldRemoved { key } => Some(key.clone()),
            Applied::Trashed { to } => Some(format!("to {to}")),
            Applied::Skipped { reason } => Some(reason.clone()),
        }
    }
}

/// Why an op could not be carried out. Every variant but two is raised
/// before the write it guards: [`ApplyError::Io`] names the operation that
/// failed, and [`ApplyError::Partial`] is raised when a later step in the
/// same op fails after an earlier step already wrote — it carries what
/// landed alongside the failure that stopped the rest.
#[derive(Debug)]
#[non_exhaustive]
pub enum ApplyError {
    /// A path was not vault-relative, or escaped the vault.
    Path {
        /// The offending path.
        path: String,
        /// Why it was refused.
        reason: String,
    },
    /// A guard said no — an occupied destination, a rename conflict
    /// `markdown_relink` reported.
    Refused(String),
    /// A filesystem operation failed.
    Io {
        /// The operation that failed.
        what: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The frontmatter splice failed.
    Frontmatter {
        /// The note whose frontmatter failed.
        path: String,
        /// Why the splice failed.
        reason: String,
    },
    /// `markdown_relink` failed for a reason that is not a conflict.
    Transform(markdown_relink::TransformError),
    /// A step after an already-landed write failed. `outcome` is what
    /// landed; `source` is why the rest of the op did not.
    Partial {
        /// What landed before the failure.
        outcome: Box<Outcome>,
        /// Why the rest of the op did not run.
        source: Box<ApplyError>,
    },
}

impl ApplyError {
    /// Pair `source` with the [`Outcome`] an earlier step in the same op
    /// already wrote, so a caller reporting the failure can still show what
    /// landed.
    fn partial(outcome: Outcome, source: ApplyError) -> Self {
        ApplyError::Partial {
            outcome: Box::new(outcome),
            source: Box::new(source),
        }
    }

    /// `source` raised after an earlier step in the same op: [`Partial`]
    /// when that step wrote, `source` itself in a preview, where nothing
    /// did.
    ///
    /// [`Partial`]: ApplyError::Partial
    fn after_write(mode: Mode, outcome: Outcome, source: ApplyError) -> Self {
        if mode.writes() {
            ApplyError::partial(outcome, source)
        } else {
            source
        }
    }
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Path { path, reason } => write!(f, "bad path `{path}`: {reason}"),
            ApplyError::Refused(reason) => write!(f, "{reason}"),
            ApplyError::Io { what, source } => write!(f, "{what}: {source}"),
            ApplyError::Frontmatter { path, reason } => {
                write!(f, "editing the frontmatter of {path}: {reason}")
            }
            ApplyError::Transform(e) => write!(f, "{e}"),
            ApplyError::Partial { source, .. } => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplyError::Io { source, .. } => Some(source),
            ApplyError::Transform(e) => Some(e),
            ApplyError::Partial { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<markdown_relink::TransformError> for ApplyError {
    fn from(e: markdown_relink::TransformError) -> Self {
        ApplyError::Transform(e)
    }
}

/// A vault an op runs on.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct Vault<'a> {
    /// Its folder, which every write lands under.
    pub dir: &'a Path,
    /// What relinking reads. It leaves out generated output, so a rename
    /// neither edits a built copy nor resolves a link against one.
    pub source: &'a dyn Vfs,
}

impl<'a> Vault<'a> {
    /// The vault in `dir`, relinked by reading `source`.
    pub fn new(dir: &'a Path, source: &'a dyn Vfs) -> Self {
        Self { dir, source }
    }
}

/// Execute `op` against `vault`.
pub fn apply(vault: Vault<'_>, op: &AuthoringOp) -> Result<Outcome, ApplyError> {
    run(vault, op, Mode::Apply)
}

/// The same decisions and the same refusals as [`apply`], with nothing
/// written.
pub fn preview(vault: Vault<'_>, op: &AuthoringOp) -> Result<Outcome, ApplyError> {
    run(vault, op, Mode::Preview)
}

fn run(vault: Vault<'_>, op: &AuthoringOp, mode: Mode) -> Result<Outcome, ApplyError> {
    match op {
        AuthoringOp::Create {
            path,
            bytes,
            scaffold,
        } => create(vault.dir, path, bytes, scaffold, mode),
        AuthoringOp::Rename {
            from,
            to,
            bundle,
            frontmatter,
        } => relocate(vault, from, to, *bundle, frontmatter, &[], mode),
        AuthoringOp::Move { from, to, bundle } => {
            relocate(vault, from, to, *bundle, &[], &[], mode)
        }
        AuthoringOp::ConvertKind {
            from,
            to,
            bundle,
            frontmatter,
            scaffold,
        } => relocate(vault, from, to, *bundle, frontmatter, scaffold, mode),
        AuthoringOp::Delete { paths, trash_dir } => delete(vault.dir, paths, trash_dir, mode),
        other => Err(ApplyError::Refused(format!(
            "this build of skillwright cannot apply {other:?}"
        ))),
    }
}

// ── Create ──────────────────────────────────────────────────────────────────

fn create(
    vault_dir: &Path,
    path: &str,
    bytes: &str,
    scaffold: &[String],
    mode: Mode,
) -> Result<Outcome, ApplyError> {
    let path = rel(path)?;
    let abs = resolve(vault_dir, &path)?;
    if abs.exists() {
        return Err(ApplyError::Refused(format!("{path} already exists")));
    }
    let dirs = scaffold
        .iter()
        .map(|d| rel(d))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some((parent, _)) = path.rsplit_once('/') {
        check_dir_creatable(vault_dir, parent)?;
    }
    for dir in &dirs {
        check_dir_creatable(vault_dir, dir)?;
    }

    let mut outcome = Outcome::new(mode);
    if mode.writes() {
        if let Some(parent) = abs.parent() {
            make_dir(parent)?;
        }
        write_note(&abs, bytes)?;
    }
    outcome.push(path, Applied::Created);
    // From here the note has landed — a scaffold that fails is reported with
    // it, not as a bare error.
    if let Err(err) = scaffold_dirs(vault_dir, &dirs, mode, &mut outcome) {
        return Err(ApplyError::after_write(mode, outcome, err));
    }
    Ok(outcome)
}

/// Make each scaffold directory, outside preview, and record it.
fn scaffold_dirs(
    vault_dir: &Path,
    dirs: &[String],
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    for dir in dirs {
        if mode.writes() {
            make_dir(&resolve(vault_dir, dir)?)?;
        }
        outcome.push(dir.clone(), Applied::Scaffolded);
    }
    Ok(())
}

// ── Rename / Move / ConvertKind ─────────────────────────────────────────────

fn relocate(
    vault: Vault<'_>,
    from: &str,
    to: &str,
    bundle: bool,
    frontmatter: &[FieldEdit],
    scaffold: &[String],
    mode: Mode,
) -> Result<Outcome, ApplyError> {
    let vault_dir = vault.dir;
    let from = rel(from)?;
    let to = rel(to)?;
    let dirs = scaffold
        .iter()
        .map(|d| rel(d))
        .collect::<Result<Vec<_>, _>>()?;
    for dir in &dirs {
        check_dir_creatable(vault_dir, dir)?;
    }
    check_frontmatter(vault_dir, &from, frontmatter)?;

    let mut outcome = Outcome::new(mode);
    move_note(vault, &from, &to, bundle, mode, &mut outcome)?;
    // From here the move has landed — a later failure is reported with what
    // already wrote, not as a bare refusal.
    if let Err(err) = relocate_after_move(vault_dir, &to, frontmatter, &dirs, mode, &mut outcome) {
        return Err(ApplyError::after_write(mode, outcome, err));
    }
    Ok(outcome)
}

fn relocate_after_move(
    vault_dir: &Path,
    to: &str,
    frontmatter: &[FieldEdit],
    dirs: &[String],
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    edit_frontmatter(vault_dir, to, frontmatter, mode, outcome)?;
    scaffold_dirs(vault_dir, dirs, mode, outcome)
}

/// Plan the move, refuse on a conflict, and — outside preview — execute it,
/// recording the subject, every relinked note, every peer and every skip.
///
/// `execute` rewrites the linking notes before its final rename and rolls
/// nothing back, so a failure it reports can follow writes that landed. Such
/// a failure comes back as [`ApplyError::Partial`] carrying them; a bare
/// [`ApplyError::Refused`] or [`ApplyError::Transform`] means nothing was
/// written.
fn move_note(
    vault: Vault<'_>,
    from: &str,
    to: &str,
    bundle: bool,
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    let root_file = |path: &str| path.rsplit('/').next() == Some(tradecraft_core::ROOT_FILE);
    // The vault's own root file is no folder's entry: moving it would move
    // the vault, and moving a note onto it would replace the vault's config.
    // Compared without case: on a case-insensitive file system, as macOS's
    // is by default, `SKILLWRIGHT.md` is that file.
    for (path, what) in [(from, "move"), (to, "be replaced")] {
        if path.eq_ignore_ascii_case(tradecraft_core::ROOT_FILE) {
            return Err(ApplyError::Refused(format!(
                "{path} is the vault's root file, so it cannot {what}"
            )));
        }
    }
    // A marketplace's entry is exactly `marketplaces/<m>/skillwright.md`. Any
    // other `<dir>/skillwright.md`, such as a bundle named `skillwright`, is an
    // ordinary `<dir>/<dir>.md` bundle entry the relinker moves.
    let marketplace_entry = |path: &str| tradecraft_core::is_marketplace_entry(Path::new(path));
    if bundle && (marketplace_entry(from) || marketplace_entry(to)) {
        if root_file(from) && root_file(to) {
            return move_marketplace(vault, from, to, mode, outcome);
        }
        return move_folder(vault.dir, from, to, mode, outcome);
    }
    relink_move(vault, from, to, bundle, mode, outcome)
}

/// [`move_note`] once the entry is known to be one the relinker can name.
fn relink_move(
    vault: Vault<'_>,
    from: &str,
    to: &str,
    bundle: bool,
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    let vault_dir = vault.dir;
    let (corpus, unreadable) = load_vault(vault, from, to, bundle)?;
    // Links resolve against every file a build sees, so a rename picks the
    // target a build does. Only notes outside dot-named folders are edited:
    // `.github`, `.claude` or another checkout under `.worktrees` is not the
    // author's notes, and a rename must not write there.
    let mut facts = VaultFacts::from_corpus(&corpus);
    facts.links.retain(|path, _| !in_dot_folder(path));
    facts.frontmatter.retain(|path, _| !in_dot_folder(path));
    let plan = if bundle {
        markdown_relink::plan_bundle(vault.source, vault_dir, &facts, from, to)?
    } else {
        markdown_relink::plan(vault.source, vault_dir, &facts, from, to)?
    };
    if let Some(conflict) = plan.conflict {
        return Err(ApplyError::Refused(conflict));
    }

    if mode.writes() {
        match execute_move(vault_dir, &facts, &plan, from, to, bundle, outcome) {
            Err(ApplyError::Partial {
                mut outcome,
                source,
            }) => {
                record_unreadable(unreadable, &mut outcome);
                return Err(ApplyError::Partial { outcome, source });
            }
            other => other?,
        }
    } else {
        record_planned(&plan, outcome);
    }
    record_unreadable(unreadable, outcome);
    Ok(())
}

/// Moves a marketplace folder, rewriting links into it from outside.
///
/// The relinker names a bundle by `<dir>/<dir>.md`, and a marketplace's entry
/// is `skillwright.md`. So the entry takes the folder's name for the move and
/// gets its own back after, and the relinker plans and executes the move as a
/// bundle. A preview does the same and puts the name back. A path in the root
/// file, and a link that names `skillwright.md` itself, stay as written.
fn move_marketplace(
    vault: Vault<'_>,
    from: &str,
    to: &str,
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    let leaf = |path: &str| -> Option<String> {
        let (dir, _) = path.rsplit_once('/')?;
        Some(format!("{dir}/{}.md", dir.rsplit('/').next()?))
    };
    let (Some(temp_from), Some(temp_to)) = (leaf(from), leaf(to)) else {
        return move_folder(vault.dir, from, to, mode, outcome);
    };
    let (root_from, root_to) = (resolve(vault.dir, from)?, resolve(vault.dir, to)?);
    let (abs_from, abs_to) = (
        resolve(vault.dir, &temp_from)?,
        resolve(vault.dir, &temp_to)?,
    );
    // Every refusal that needs no rename comes first, in both modes, so a
    // refused move touches nothing. The target folder must be new: whatever
    // `<m>.md` it holds is then the move's own.
    let new_dir = root_to.parent().map(Path::to_path_buf).unwrap_or_default();
    if new_dir.exists() {
        let to_dir = to.rsplit_once('/').map_or(to, |(dir, _)| dir);
        return Err(ApplyError::Refused(format!("{to_dir} already exists")));
    }
    if abs_from != root_from && abs_from.exists() {
        return Err(ApplyError::Refused(format!(
            "{temp_from} already exists, and the move needs that name free"
        )));
    }
    if mode.writes()
        && let Some(parent) = new_dir.parent()
        && parent.metadata().is_ok_and(|m| m.permissions().readonly())
    {
        return Err(ApplyError::Refused(format!(
            "{} is read-only, so the folder cannot move",
            parent.display()
        )));
    }
    let before = outcome.entries.len();
    let (moved, restored) = if mode.writes() {
        if abs_from != root_from {
            std::fs::rename(&root_from, &abs_from).map_err(|source| ApplyError::Io {
                what: format!("preparing {from}"),
                source,
            })?;
        }
        let moved = relink_move(vault, &temp_from, &temp_to, true, mode, outcome);
        let restored = restore_entry(
            vault.dir, &root_from, &abs_from, &new_dir, &root_to, &abs_to,
        );
        (moved, restored)
    } else {
        // A preview renames in memory, over the same view, and writes nothing.
        let overlay = PreviewVfs::over(vault.source);
        if abs_from != root_from {
            overlay
                .rename(&root_from, &abs_from)
                .map_err(|e| ApplyError::Refused(e.to_string()))?;
        }
        let view = Vault {
            dir: vault.dir,
            source: &overlay,
        };
        (
            relink_move(view, &temp_from, &temp_to, true, mode, outcome),
            Ok(()),
        )
    };
    let names = OwnNames {
        from,
        temp_from: &temp_from,
        temp_to: &temp_to,
        landed: landed(&temp_from, &temp_to),
        now: [(&root_to, to), (&root_from, from)]
            .into_iter()
            .find(|(path, _)| path.is_file())
            .map(|(_, now)| now),
    };
    // Once, on whichever outcome the caller reads: a partial failure's own,
    // or this one.
    match settle(moved, restored, outcome) {
        Err(ApplyError::Partial {
            mut outcome,
            mut source,
        }) => {
            if let Some(entries) = outcome.entries.get_mut(before..) {
                names.rows(entries);
            }
            names.error(&mut source);
            Err(ApplyError::Partial { outcome, source })
        }
        other => {
            names.rows(&mut outcome.entries[before..]);
            other
        }
    }
}

/// The temporary names a marketplace move gives its entry, and what each
/// stands for once the move is over.
struct OwnNames<'a> {
    from: &'a str,
    /// `<m>.md` in the source folder.
    temp_from: &'a str,
    /// `<m>.md` in the target folder.
    temp_to: &'a str,
    /// The source's `<m>.md` in the target folder, where the entry is when
    /// the folder moved and its own rename did not.
    landed: String,
    /// The entry's own name where it is now, `None` when `restore_entry`
    /// could not put it back: then the temporary paths are true.
    now: Option<&'a str>,
}

impl OwnNames<'_> {
    /// Names each row's path by the entry's own name. A rename's source is
    /// always the entry's own name, which is what the user moved.
    fn rows(&self, entries: &mut [AppliedEntry]) {
        for entry in entries {
            if let Some(now) = self.now {
                if entry.path == self.temp_from {
                    entry.path = self.from.to_owned();
                } else if entry.path == self.temp_to || entry.path == self.landed {
                    entry.path = now.to_owned();
                }
            }
            if let Applied::Renamed { from: old } = &mut entry.applied
                && *old == self.temp_from
            {
                *old = self.from.to_owned();
            }
        }
    }

    /// Names the entry where it is now in `err`'s message, where that names
    /// the landed temporary path.
    fn error(&self, err: &mut ApplyError) {
        let Some(now) = self.now else {
            return;
        };
        match err {
            ApplyError::Refused(message) | ApplyError::Io { what: message, .. } => {
                *message = replace_path(message, &self.landed, now);
            }
            _ => {}
        }
    }
}

/// Where an entry moved from `from` to `to` sits when the folder moved and
/// its own rename did not: inside `to`'s folder, under `from`'s file name.
fn landed(from: &str, to: &str) -> String {
    let leaf = from.rsplit('/').next().unwrap_or(from);
    match to.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{leaf}"),
        None => leaf.to_owned(),
    }
}

/// A marketplace move's result, once `restore_entry` has tried to give the
/// entry its name back. The move landed, or partly landed, whenever the
/// restore fails, because the entry was renamed first: so a failed restore
/// is [`ApplyError::Partial`] with what landed, and when the move failed too
/// its error is kept in front of the restore's.
fn settle(
    moved: Result<(), ApplyError>,
    restored: Result<(), ApplyError>,
    outcome: &Outcome,
) -> Result<(), ApplyError> {
    let Err(restore) = restored else {
        return moved;
    };
    let both = |first: ApplyError, restore: ApplyError| match restore {
        ApplyError::Io { what, source } => ApplyError::Io {
            what: format!("{first}; then {what}"),
            source,
        },
        other => ApplyError::Refused(format!("{first}; then {other}")),
    };
    Err(match moved {
        Ok(()) => ApplyError::partial(outcome.clone(), restore),
        Err(ApplyError::Partial { outcome, source }) => ApplyError::Partial {
            outcome,
            source: Box::new(both(*source, restore)),
        },
        Err(first) => ApplyError::partial(outcome.clone(), both(first, restore)),
    })
}

/// `message` with each whole occurrence of the path `old` replaced by `new`.
/// An occurrence is whole when no path character runs on from either end,
/// so `a/m.md` is not found inside `a/m.mdx` or `xa/m.md`.
fn replace_path(message: &str, old: &str, new: &str) -> String {
    let is_path_char = |c: char| c.is_alphanumeric() || matches!(c, '/' | '-' | '_' | '.');
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(at) = rest.find(old) {
        let (head, tail) = rest.split_at(at);
        let after = &tail[old.len()..];
        let starts = head.chars().next_back().is_none_or(|c| !is_path_char(c));
        // A `.` after the path ends a sentence unless a path character
        // follows it.
        let mut next = after.chars();
        let ends = match next.next() {
            None => true,
            Some('.') => next.next().is_none_or(|c| !is_path_char(c)),
            Some(c) => !is_path_char(c),
        };
        out.push_str(head);
        out.push_str(if starts && ends { new } else { old });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Give the marketplace entry its `skillwright.md` name back, wherever the
/// folder ended up. The two ends are independent: the source folder gets its
/// entry back when the move did not happen, and the new folder, which the move
/// created, when it did. A partly failed move leaves the entry under the
/// source's `<m>.md` name inside the new folder, so that name is tried too.
fn restore_entry(
    vault_dir: &Path,
    root_from: &Path,
    abs_from: &Path,
    new_dir: &Path,
    root_to: &Path,
    abs_to: &Path,
) -> Result<(), ApplyError> {
    // Vault-relative, as every other path in a report.
    let shown = |path: &Path| {
        path.strip_prefix(vault_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let rename = |from: &Path, to: &Path| {
        std::fs::rename(from, to).map_err(|source| ApplyError::Io {
            what: format!(
                "restoring {}, so the entry is still at {}",
                shown(to),
                shown(from)
            ),
            source,
        })
    };
    let mut result = Ok(());
    if abs_from != root_from && abs_from.exists() && !root_from.exists() {
        result = rename(abs_from, root_from);
    }
    if abs_to != root_to && new_dir.exists() && !root_to.exists() {
        let kept = abs_from.file_name().map(|name| new_dir.join(name));
        for candidate in std::iter::once(abs_to.to_path_buf()).chain(kept) {
            // Only a file is the entry: a folder of the user's may hold the
            // name.
            if candidate.is_file() {
                let renamed = rename(&candidate, root_to);
                result = result.and(renamed);
                break;
            }
        }
    }
    result
}

/// Moves a folder whose entry note the relinker cannot name: a marketplace's
/// `skillwright.md` is not `<dir>/<dir>.md`. Links inside it move with it. A
/// link into it from outside and a path in the root file stay as written.
fn move_folder(
    vault_dir: &Path,
    from: &str,
    to: &str,
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    let (old, new) = (resolve(vault_dir, from)?, resolve(vault_dir, to)?);
    let (Some(old_dir), Some(new_dir)) = (old.parent(), new.parent()) else {
        return Err(ApplyError::Refused(format!("{from} has no folder to move")));
    };
    if new_dir.exists() {
        return Err(ApplyError::Refused(format!(
            "{} already exists",
            new_dir.display()
        )));
    }
    if mode.writes() {
        let io = |source| ApplyError::Io {
            what: format!("moving {from} to {to}"),
            source,
        };
        if let Some(parent) = new_dir.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::rename(old_dir, new_dir).map_err(io)?;
        // The folder has moved: a failure from here is reported with it.
        if let Some(leaf) = old.file_name()
            && let Err(source) = std::fs::rename(new_dir.join(leaf), &new)
        {
            outcome.push(
                landed(from, to),
                Applied::Renamed {
                    from: from.to_owned(),
                },
            );
            return Err(ApplyError::partial(outcome.clone(), io(source)));
        }
    }
    outcome.push(
        to,
        Applied::Renamed {
            from: from.to_owned(),
        },
    );
    Ok(())
}

/// Execute `plan`, recording what it wrote.
fn execute_move(
    vault_dir: &Path,
    facts: &VaultFacts,
    plan: &RenamePlan,
    from: &str,
    to: &str,
    bundle: bool,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    // `execute` re-plans and treats `expect` as the assertion "these are the
    // bytes the user was shown"; a file missing from it is skipped as
    // `not_previewed`. The plan just computed IS what this run previewed, so
    // its own `expect` set is the honest one to pass, and `subject_etag` is
    // `None` because a CLI has no etag from an earlier session to assert.
    let spliced = spliced_etags(vault_dir, plan);
    let executed = if bundle {
        markdown_relink::execute_bundle(vault_dir, facts, from, to, None, &plan.expect)
    } else {
        markdown_relink::execute(vault_dir, facts, from, to, None, &plan.expect)
    };
    let result = match executed {
        Ok(result) => result,
        Err(err) => {
            let before = outcome.entries.len();
            record_rewritten_on_disk(vault_dir, &spliced, outcome);
            let err = ApplyError::Transform(err);
            return Err(if outcome.entries.len() == before {
                err
            } else {
                ApplyError::partial(outcome.clone(), err)
            });
        }
    };
    if !result.ok
        && let Some(conflict) = result.conflict
    {
        return Err(ApplyError::Refused(conflict));
    }
    record_executed(plan, &result, from, to, outcome);
    match result.conflict {
        // `ok` with a conflict: the bundle directory moved and its links were
        // rewritten, and only the entry file's own rename failed. The entry
        // kept its old leaf name inside the new directory; no row names that
        // path, so the refusal does.
        Some(conflict) => {
            let landed = landed(from, to);
            Err(ApplyError::partial(
                outcome.clone(),
                ApplyError::Refused(format!("{conflict}; the entry note is now at {landed}")),
            ))
        }
        None => Ok(()),
    }
}

/// A planned note `execute` would rewrite, and the etag its bytes will have
/// once it does.
struct SplicedNote {
    path: String,
    etag: String,
    edits: u64,
}

/// Read each planned note that still matches the plan and work out the etag
/// `execute`'s splice will give it. A note that already drifted is left
/// out: `execute` will skip it.
fn spliced_etags(vault_dir: &Path, plan: &RenamePlan) -> Vec<SplicedNote> {
    edited_files(plan)
        .filter_map(|file| {
            let bytes = std::fs::read(resolve(vault_dir, &file.path).ok()?).ok()?;
            if etag_write::etag(&bytes) != file.etag {
                return None;
            }
            Some(SplicedNote {
                path: file.path.clone(),
                etag: etag_write::etag(&markdown_relink::splice(&bytes, &file.edits)),
                edits: file.edits.len() as u64,
            })
        })
        .collect()
}

/// After `execute` failed, record every note whose bytes now match the
/// spliced etag: `execute` rewrote it before the step that failed. A note
/// someone else changed meanwhile matches neither etag and is not claimed.
fn record_rewritten_on_disk(vault_dir: &Path, spliced: &[SplicedNote], outcome: &mut Outcome) {
    for note in spliced {
        let Ok(abs) = resolve(vault_dir, &note.path) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&abs) else {
            continue;
        };
        if etag_write::etag(&bytes) == note.etag {
            outcome.push(
                note.path.clone(),
                Applied::LinksRewritten { edits: note.edits },
            );
        }
    }
}

/// A `skipped` row for each path the rename could not read, after the rows
/// for what it did.
fn record_unreadable(paths: Vec<String>, outcome: &mut Outcome) {
    for path in paths {
        outcome.push(
            path,
            Applied::Skipped {
                reason: "unreadable".to_owned(),
            },
        );
    }
}

/// The vault as a build reads it: compile's walk, so the planner resolves a
/// link to the file a build does, and the lenient split, so a block only that
/// split takes is frontmatter here too, not body links to rewrite. Unlike a
/// build's skills source, symlinks are listed, not refused: one anywhere in
/// the vault must not block a rename.
///
/// A folder the walk cannot list is skipped, since a corner of the vault the
/// rename cannot read must not block every rename, and returned so it gets a
/// `skipped` row. One in or under a dot-named folder is not returned: a
/// rename edits nothing there anyway. One that holds `from` or `to`, or lies
/// inside a moved bundle, stops the rename instead, with an error naming it.
fn load_vault(
    vault: Vault<'_>,
    from: &str,
    to: &str,
    bundle: bool,
) -> Result<(Corpus, Vec<String>), ApplyError> {
    let vault_dir = vault.dir;
    let (from, to) = (Path::new(from), Path::new(to));
    // What the move carries: the note, or a bundle's whole folder.
    let moved = if bundle {
        from.parent().unwrap_or(from)
    } else {
        from
    };
    let mut unreadable = Vec::new();
    // A walk that lists and stats only, so finding what cannot be read costs
    // no note reads.
    for err in tradecraft_core::compile::vault_walk_errors(vault.source, vault_dir) {
        let relative = err.path().strip_prefix(vault_dir).unwrap_or(err.path());
        if from.starts_with(relative) || to.starts_with(relative) || relative.starts_with(moved) {
            return Err(ApplyError::Io {
                what: format!("walking {}", err.path().display()),
                source: err.into_io_error(),
            });
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        // Every file in a folder named here sits under its own name too.
        let edits_nothing = if err.path().is_dir() {
            has_dot_name(&relative)
        } else {
            in_dot_folder(&relative)
        };
        if !edits_nothing {
            unreadable.push(relative);
        }
    }
    let mut walk = tradecraft_core::compile::vault_walk_options();
    walk.propagate_errors = false;
    let options = LoadOptions::default()
        .with_link_walk(walk.clone())
        .with_frontmatter_walk(walk)
        .with_split(SplitMode::Lenient);
    Ok((Corpus::load(vault.source, vault_dir, options), unreadable))
}

/// Whether the vault-relative `path` sits under a dot-named folder.
fn in_dot_folder(path: &str) -> bool {
    path.rsplit_once('/')
        .is_some_and(|(folders, _)| has_dot_name(folders))
}

/// Whether any name in the `/`-separated `path` starts with a dot.
fn has_dot_name(path: &str) -> bool {
    path.split('/').any(|name| name.starts_with('.'))
}

fn record_planned(plan: &RenamePlan, outcome: &mut Outcome) {
    outcome.push(
        plan.to.clone(),
        Applied::Renamed {
            from: plan.from.clone(),
        },
    );
    for file in edited_files(plan) {
        outcome.push(
            file.path.clone(),
            Applied::LinksRewritten {
                edits: file.edits.len() as u64,
            },
        );
    }
    for peer in &plan.peers {
        outcome.push(
            peer.to.clone(),
            Applied::PeerMoved {
                from: peer.from.clone(),
            },
        );
    }
}

fn record_executed(
    plan: &RenamePlan,
    result: &RenameResult,
    from: &str,
    to: &str,
    outcome: &mut Outcome,
) {
    if result.renamed {
        outcome.push(
            to.to_owned(),
            Applied::Renamed {
                from: from.to_owned(),
            },
        );
    }
    for path in &result.edited {
        let edits = edited_files(plan)
            .find(|f| &f.path == path)
            .map_or(0, |f| f.edits.len() as u64);
        outcome.push(path.clone(), Applied::LinksRewritten { edits });
    }
    for peer in &result.peers_moved {
        outcome.push(
            peer.to.clone(),
            Applied::PeerMoved {
                from: peer.from.clone(),
            },
        );
    }
    for skip in &result.skipped {
        outcome.push(
            skip.path.clone(),
            Applied::Skipped {
                reason: skip.reason.clone(),
            },
        );
    }
}

fn edited_files(plan: &RenamePlan) -> impl Iterator<Item = &FileEdits> {
    plan.files.iter().filter(|f| !f.edits.is_empty())
}

// ── Delete ──────────────────────────────────────────────────────────────────

/// Every destination is checked before anything moves: a delete of three paths
/// where the second is already trashed moves none of them. A later move can
/// still fail, for example on a path nested under an earlier one; the error is
/// then [`ApplyError::Partial`] with what already moved.
fn delete(
    vault_dir: &Path,
    paths: &[String],
    trash_dir: &str,
    mode: Mode,
) -> Result<Outcome, ApplyError> {
    let trash = rel(trash_dir)?;
    let mut moves = Vec::new();
    for path in paths {
        let from = rel(path)?;
        let to = rel(&format!("{trash}/{from}"))?;
        let from_abs = resolve(vault_dir, &from)?;
        let to_abs = resolve(vault_dir, &to)?;
        if !from_abs.exists() {
            return Err(ApplyError::Refused(format!("{from} does not exist")));
        }
        if to_abs.exists() {
            return Err(ApplyError::Refused(format!(
                "{to} already exists — empty the trash or rename it first"
            )));
        }
        moves.push((from, to, from_abs, to_abs));
    }

    let mut outcome = Outcome::new(mode);
    for (from, to, from_abs, to_abs) in moves {
        if mode.writes()
            && let Err(err) = trash_one(&from, &to, &from_abs, &to_abs)
        {
            // A move after an earlier one landed reports what already moved.
            return Err(if outcome.entries.is_empty() {
                err
            } else {
                ApplyError::partial(outcome, err)
            });
        }
        outcome.push(from, Applied::Trashed { to });
    }
    Ok(outcome)
}

fn trash_one(from: &str, to: &str, from_abs: &Path, to_abs: &Path) -> Result<(), ApplyError> {
    if let Some(parent) = to_abs.parent() {
        make_dir(parent)?;
    }
    // A same-filesystem rename, which is what a trash folder inside the vault
    // always is. A cross-device move is an `Io` error, not a silent copy.
    std::fs::rename(from_abs, to_abs).map_err(|source| ApplyError::Io {
        what: format!("moving {from} to {to}"),
        source,
    })
}

// ── Shared ──────────────────────────────────────────────────────────────────

/// Read the note at `path` and splice `edits` into it, without writing.
/// Run on a note before it moves, in both modes, so YAML the splice refuses
/// stops the run before anything is written.
fn check_frontmatter(vault_dir: &Path, path: &str, edits: &[FieldEdit]) -> Result<(), ApplyError> {
    if edits.is_empty() {
        return Ok(());
    }
    spliced_frontmatter(vault_dir, path, edits).map(|_| ())
}

/// The note at `path` with `edits` spliced in, and where it lives.
fn spliced_frontmatter(
    vault_dir: &Path,
    path: &str,
    edits: &[FieldEdit],
) -> Result<(PathBuf, String), ApplyError> {
    let abs = resolve(vault_dir, path)?;
    let source = std::fs::read_to_string(&abs).map_err(|source| ApplyError::Io {
        what: format!("reading {path}"),
        source,
    })?;
    let edited = apply_field_edits(&source, edits).map_err(|e| ApplyError::Frontmatter {
        path: path.to_owned(),
        reason: e.to_string(),
    })?;
    Ok((abs, edited))
}

fn edit_frontmatter(
    vault_dir: &Path,
    path: &str,
    edits: &[FieldEdit],
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    if edits.is_empty() {
        return Ok(());
    }
    if mode.writes() {
        let (abs, edited) = spliced_frontmatter(vault_dir, path, edits)?;
        write_note(&abs, &edited)?;
    }
    for edit in edits {
        let applied = match edit.value {
            Some(_) => Applied::FieldSet {
                key: edit.key.clone(),
            },
            None => Applied::FieldRemoved {
                key: edit.key.clone(),
            },
        };
        outcome.push(path.to_owned(), applied);
    }
    Ok(())
}

fn rel(raw: &str) -> Result<String, ApplyError> {
    path_guard::lexical::validate_relative(raw).map_err(|e| ApplyError::Path {
        path: raw.to_owned(),
        reason: e.to_string(),
    })
}

fn resolve(vault_dir: &Path, raw: &str) -> Result<PathBuf, ApplyError> {
    path_guard::Guard::std()
        .resolve(vault_dir, raw)
        .map_err(|e| ApplyError::Path {
            path: raw.to_owned(),
            reason: e.to_string(),
        })
}

/// Refuse a vault-relative directory that cannot be made because a path
/// component of it that exists is not a directory. Run before the first
/// write, in both modes, so a preview raises the refusal the real run would.
fn check_dir_creatable(vault_dir: &Path, dir: &str) -> Result<(), ApplyError> {
    let mut at = vault_dir.to_path_buf();
    let mut prefix = String::new();
    for part in dir.split('/') {
        at.push(part);
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        match std::fs::metadata(&at) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(ApplyError::Refused(format!(
                    "cannot create {dir}: {prefix} is not a directory"
                )));
            }
            // Nothing there: this component and the rest are made fresh.
            Err(_) => return Ok(()),
        }
    }
    Ok(())
}

fn make_dir(abs: &Path) -> Result<(), ApplyError> {
    std::fs::create_dir_all(abs).map_err(|source| ApplyError::Io {
        what: format!("creating {}", abs.display()),
        source,
    })
}

fn write_note(abs: &Path, contents: &str) -> Result<(), ApplyError> {
    etag_write::write_atomic(abs, contents.as_bytes()).map_err(|e| ApplyError::Io {
        what: format!("writing {}", abs.display()),
        source: std::io::Error::other(e.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tradecraft_core::vfs::NativeVfs;

    fn apply(dir: &Path, op: &AuthoringOp) -> Result<Outcome, ApplyError> {
        super::apply(Vault::new(dir, &NativeVfs), op)
    }

    fn preview(dir: &Path, op: &AuthoringOp) -> Result<Outcome, ApplyError> {
        super::preview(Vault::new(dir, &NativeVfs), op)
    }
    use std::collections::BTreeMap;
    use tradecraft_core::authoring::TRASH_DIR;
    use yaml_splice::Value;

    fn vault() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let abs = dir.join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(abs, body).unwrap();
    }

    fn read(dir: &Path, rel: &str) -> String {
        std::fs::read_to_string(dir.join(rel)).unwrap()
    }

    /// Every file under `dir`, relative path to bytes — the shape a dry run
    /// must leave untouched.
    fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(next) = stack.pop() {
            for entry in std::fs::read_dir(&next).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let key = path
                        .strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    out.insert(key, std::fs::read(&path).unwrap());
                }
            }
        }
        out
    }

    /// Make each folder under `dir` and take every permission off it.
    /// Whether the first can still be listed: root lists any folder, so a
    /// test of an unlistable one has nothing to test.
    #[cfg(unix)]
    fn lock(dir: &Path, folders: &[&str]) -> bool {
        use std::os::unix::fs::PermissionsExt;
        for rel in folders {
            let abs = dir.join(rel);
            std::fs::create_dir_all(&abs).unwrap();
            std::fs::set_permissions(&abs, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        std::fs::read_dir(dir.join(folders[0])).is_ok()
    }

    /// Give back what [`lock`] took, from each of `folders` that exists.
    #[cfg(unix)]
    fn unlock(dir: &Path, folders: &[&str]) {
        use std::os::unix::fs::PermissionsExt;
        for rel in folders {
            let _ = std::fs::set_permissions(dir.join(rel), std::fs::Permissions::from_mode(0o755));
        }
    }

    fn outcomes(outcome: &Outcome) -> Vec<(&str, &str)> {
        outcome
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.applied.outcome()))
            .collect()
    }

    #[test]
    fn create_writes_the_note_and_its_scaffold() {
        let v = vault();
        let op = AuthoringOp::Create {
            path: "skills/alpha/alpha.md".to_owned(),
            bytes: "---\nname: alpha\n---\n\nBody\n".to_owned(),
            scaffold: vec!["skills/alpha/references".to_owned()],
        };
        let outcome = apply(v.path(), &op).unwrap();

        assert_eq!(
            outcomes(&outcome),
            vec![
                ("skills/alpha/alpha.md", "created"),
                ("skills/alpha/references", "scaffolded"),
            ]
        );
        assert!(read(v.path(), "skills/alpha/alpha.md").contains("name: alpha"));
        assert!(v.path().join("skills/alpha/references").is_dir());
    }

    #[test]
    fn create_refuses_an_occupied_path() {
        let v = vault();
        write(v.path(), "notes/a.md", "original");
        let op = AuthoringOp::Create {
            path: "notes/a.md".to_owned(),
            bytes: "replacement".to_owned(),
            scaffold: vec![],
        };

        let err = apply(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Refused(_)), "{err:?}");
        assert_eq!(read(v.path(), "notes/a.md"), "original");
    }

    #[test]
    fn rename_rewrites_an_inbound_link() {
        let v = vault();
        write(
            v.path(),
            "notes/Alpha.md",
            "---\nname: Alpha\n---\n\nsubject\n",
        );
        write(v.path(), "notes/Beta.md", "see [[Alpha]] for more\n");

        let op = AuthoringOp::Rename {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![FieldEdit::set("name", Value::Str("Gamma".to_owned()))],
        };
        let outcome = apply(v.path(), &op).unwrap();

        assert!(!v.path().join("notes/Alpha.md").exists());
        assert_eq!(read(v.path(), "notes/Beta.md"), "see [[Gamma]] for more\n");
        assert!(read(v.path(), "notes/Gamma.md").contains("name: Gamma"));
        assert_eq!(
            outcomes(&outcome),
            vec![
                ("notes/Gamma.md", "renamed"),
                ("notes/Beta.md", "relinked"),
                ("notes/Gamma.md", "field-set"),
            ]
        );
    }

    #[test]
    fn rename_reads_a_loosely_fenced_note_as_a_build_does() {
        // Spaces after the fences: only the lenient split takes this block as
        // frontmatter, and a build reads it that way. The rename does too, so
        // the link in it is a frontmatter value, left alone as one in a
        // strictly fenced block is, and only the body link moves.
        let v = vault();
        write(v.path(), "notes/Alpha.md", "subject\n");
        for (rel, fence, close) in [
            ("notes/Loose.md", "--- ", "---  "),
            ("notes/Strict.md", "---", "---"),
        ] {
            write(
                v.path(),
                rel,
                &format!("{fence}\nup: \"[[Alpha]]\"\n{close}\nsee [[Alpha]]\n"),
            );
        }

        let op = AuthoringOp::Rename {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        apply(v.path(), &op).unwrap();

        assert_eq!(
            read(v.path(), "notes/Loose.md"),
            "--- \nup: \"[[Alpha]]\"\n---  \nsee [[Gamma]]\n"
        );
        assert_eq!(
            read(v.path(), "notes/Strict.md"),
            "---\nup: \"[[Alpha]]\"\n---\nsee [[Gamma]]\n"
        );
    }

    /// A folder the rename's walk cannot list is skipped with an
    /// `unreadable` row naming it, after the rows for what the rename did,
    /// and the rename goes on. One under a dot-named folder gets no row: a
    /// rename edits nothing there anyway. Renaming into that folder stops,
    /// and the error names it.
    #[cfg(unix)]
    #[test]
    fn a_rename_skips_a_folder_it_cannot_list_unless_it_moves_into_it() {
        let v = vault();
        write(v.path(), "notes/Alpha.md", "subject\n");
        write(v.path(), "notes/a.md", "see [[Alpha]]\n");
        let folders = ["notes/locked", ".cache", ".worktrees/b/locked"];
        let listable = lock(v.path(), &folders);
        let rename = |to: &str| {
            let op = AuthoringOp::Rename {
                from: "notes/Alpha.md".to_owned(),
                to: to.to_owned(),
                bundle: false,
                frontmatter: vec![],
            };
            apply(v.path(), &op)
        };
        let into = rename("notes/locked/Gamma.md");
        let beside = rename("notes/Gamma.md");
        unlock(v.path(), &folders);
        if listable {
            return;
        }

        let err = into.unwrap_err().to_string();
        assert!(err.contains("notes/locked"), "{err}");

        let outcome = beside.unwrap();
        assert_eq!(
            outcomes(&outcome),
            [
                ("notes/Gamma.md", "renamed"),
                ("notes/a.md", "relinked"),
                ("notes/locked", "skipped"),
            ],
            "{outcome:?}"
        );
        assert_eq!(
            outcome.entries[2].applied,
            Applied::Skipped {
                reason: "unreadable".to_owned()
            }
        );
        assert_eq!(read(v.path(), "notes/a.md"), "see [[Gamma]]\n");
    }

    /// A folder the walk cannot list inside a bundle stops the bundle's
    /// move, naming the folder: the move would carry files whose inbound
    /// links the rename cannot see.
    #[cfg(unix)]
    #[test]
    fn a_bundle_rename_stops_on_a_folder_inside_it_it_cannot_list() {
        let v = vault();
        write(v.path(), "skills/alpha/alpha.md", "subject\n");
        write(v.path(), "notes/a.md", "see [[alpha]]\n");
        let listable = lock(v.path(), &["skills/alpha/assets"]);
        let op = AuthoringOp::Rename {
            from: "skills/alpha/alpha.md".to_owned(),
            to: "skills/gamma/gamma.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let result = apply(v.path(), &op);
        // Wherever the folder ended up.
        unlock(v.path(), &["skills/alpha/assets", "skills/gamma/assets"]);
        if listable {
            return;
        }

        let err = result.unwrap_err().to_string();
        assert!(err.contains("skills/alpha/assets"), "{err}");
        assert_eq!(read(v.path(), "skills/alpha/alpha.md"), "subject\n");
    }

    /// A rename resolves links against dot-named folders too, but edits
    /// none of their files: they are tooling and other checkouts, not notes.
    #[test]
    fn a_rename_edits_no_file_in_a_dot_named_folder() {
        let v = vault();
        write(v.path(), "notes/Alpha.md", "subject\n");
        let linking = [
            "notes/a.md",
            ".github/x.md",
            ".worktrees/b/y.md",
            ".claude/z.md",
        ];
        for rel in linking {
            write(v.path(), rel, "see [[Alpha]]\n");
        }

        let op = AuthoringOp::Rename {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        apply(v.path(), &op).unwrap();

        assert_eq!(read(v.path(), "notes/a.md"), "see [[Gamma]]\n");
        for rel in &linking[1..] {
            assert_eq!(read(v.path(), rel), "see [[Alpha]]\n", "{rel}");
        }
    }

    #[test]
    fn a_rename_reads_an_ambiguous_link_as_a_build_does() {
        // `[[Guide]]` names two files, one under a dot-named folder. A build
        // sees both and refuses the link as ambiguous. The rename sees both
        // too, so renaming `docs/long/Guide.md` leaves the link alone. A walk
        // without dot folders saw one candidate and rewrote it to `[[Manual]]`.
        let v = vault();
        write(v.path(), ".config/Guide.md", "one\n");
        write(v.path(), "docs/long/Guide.md", "two\n");
        write(v.path(), "notes/a.md", "see [[Guide]]\n");

        let entry = v.path().join("notes/a.md");
        let built = tradecraft_core::compile::compile_note(
            &NativeVfs,
            &tradecraft_core::compile::CompileSource::new(v.path(), &entry, "a"),
            "see [[Guide]]\n",
        )
        .unwrap_err()
        .to_string();
        assert!(built.contains(".config/Guide.md"), "{built}");
        assert!(built.contains("docs/long/Guide.md"), "{built}");

        let op = AuthoringOp::Rename {
            from: "docs/long/Guide.md".to_owned(),
            to: "docs/long/Manual.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        apply(v.path(), &op).unwrap();
        assert_eq!(read(v.path(), "notes/a.md"), "see [[Guide]]\n");
    }

    #[test]
    fn a_rename_relinks_a_bare_link_a_build_reads_in_the_note_s_own_folder() {
        // `[[contract]]` names a file in each bundle. A build reads it as the
        // one in the linking note's bundle, and so does the rename: renaming
        // that file rewrites the link, so it never falls through to p2's.
        let v = vault();
        write(
            v.path(),
            "plugins/p1/skills/doc/doc.md",
            "see [[contract]]\n",
        );
        write(
            v.path(),
            "plugins/p1/skills/doc/references/contract.md",
            "one\n",
        );
        write(
            v.path(),
            "plugins/p2/skills/doc/references/contract.md",
            "two\n",
        );

        let entry = v.path().join("plugins/p1/skills/doc/doc.md");
        let built = tradecraft_core::compile::compile_note(
            &NativeVfs,
            &tradecraft_core::compile::CompileSource::new(v.path(), &entry, "doc"),
            "see [[contract]]\n",
        )
        .unwrap();
        assert!(
            built.body.contains("references/contract.md"),
            "{}",
            built.body
        );

        let op = AuthoringOp::Rename {
            from: "plugins/p1/skills/doc/references/contract.md".to_owned(),
            to: "plugins/p1/skills/doc/references/terms.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        apply(v.path(), &op).unwrap();
        assert_eq!(
            read(v.path(), "plugins/p1/skills/doc/doc.md"),
            "see [[terms]]\n"
        );
    }

    #[test]
    fn a_marketplace_rename_moves_the_folder_and_relinks_notes_outside_it() {
        let v = vault();
        write(
            v.path(),
            "marketplaces/m1/skillwright.md",
            "---\nname: m1\n---\n",
        );
        write(
            v.path(),
            "marketplaces/m1/plugins/review/skills/triage.md",
            "---\nname: triage\ndescription: d\n---\nbody\n",
        );
        write(
            v.path(),
            "notes.md",
            "see [t](marketplaces/m1/plugins/review/skills/triage.md)\n",
        );
        let op = AuthoringOp::Rename {
            from: "marketplaces/m1/skillwright.md".to_owned(),
            to: "marketplaces/m2/skillwright.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let before = tree(v.path());
        preview(v.path(), &op).unwrap();
        assert_eq!(tree(v.path()), before, "a preview writes nothing");

        let outcome = apply(v.path(), &op).unwrap();
        assert!(v.path().join("marketplaces/m2/skillwright.md").is_file());
        assert!(!v.path().join("marketplaces/m1").exists());
        assert!(!v.path().join("marketplaces/m2/m2.md").exists());
        assert_eq!(
            read(v.path(), "notes.md"),
            "see [t](marketplaces/m2/plugins/review/skills/triage.md)\n"
        );
        assert!(outcome.entries.iter().all(|e| !e.path.ends_with("m2.md")));
        assert!(outcome.entries.iter().any(|e| matches!(
            &e.applied,
            Applied::Renamed { from } if from == "marketplaces/m1/skillwright.md"
        ) && e.path == "marketplaces/m2/skillwright.md"));
    }

    /// A refused marketplace rename, real or preview, leaves every file as it
    /// was, including a user's `m2/m2.md` and the source's `m1/m1.md`.
    #[test]
    fn a_refused_marketplace_rename_changes_nothing() {
        for (setup, reason) in [
            ("marketplaces/m2/m2.md", "already exists"),
            ("marketplaces/m1/m1.md", "needs that name free"),
        ] {
            let v = vault();
            write(
                v.path(),
                "marketplaces/m1/skillwright.md",
                "---\nname: m1\n---\n",
            );
            write(
                v.path(),
                "marketplaces/m1/plugins/p/skills/a.md",
                "---\nname: a\n---\n",
            );
            write(
                v.path(),
                "notes.md",
                "[a](marketplaces/m1/plugins/p/skills/a.md)\n",
            );
            write(v.path(), setup, "USER\n");
            let op = AuthoringOp::Rename {
                from: "marketplaces/m1/skillwright.md".to_owned(),
                to: "marketplaces/m2/skillwright.md".to_owned(),
                bundle: true,
                frontmatter: vec![],
            };
            let before = tree(v.path());
            let err = preview(v.path(), &op).unwrap_err();
            assert!(err.to_string().contains(reason), "{err}");
            assert_eq!(tree(v.path()), before, "preview, {setup}");
            let err = apply(v.path(), &op).unwrap_err();
            assert!(err.to_string().contains(reason), "{err}");
            assert_eq!(tree(v.path()), before, "apply, {setup}");
        }
    }

    #[test]
    fn a_marketplace_rename_preview_counts_the_links_it_would_rewrite() {
        let v = vault();
        write(
            v.path(),
            "marketplaces/m1/skillwright.md",
            "---\nname: m1\n---\n",
        );
        write(
            v.path(),
            "marketplaces/m1/plugins/p/skills/a.md",
            "---\nname: a\n---\n",
        );
        write(
            v.path(),
            "notes.md",
            "[a](marketplaces/m1/plugins/p/skills/a.md)\n",
        );
        let op = AuthoringOp::Rename {
            from: "marketplaces/m1/skillwright.md".to_owned(),
            to: "marketplaces/m2/skillwright.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let before = tree(v.path());
        let outcome = preview(v.path(), &op).unwrap();
        assert_eq!(tree(v.path()), before);
        assert!(outcome.entries.iter().any(|e| e.path == "notes.md"));
    }

    #[cfg(unix)]
    #[test]
    fn a_marketplace_rename_into_a_read_only_folder_rewrites_no_links() {
        let v = vault();
        write(
            v.path(),
            "marketplaces/m1/skillwright.md",
            "---\nname: m1\n---\n",
        );
        write(
            v.path(),
            "marketplaces/m1/plugins/p/skills/a.md",
            "---\nname: a\n---\n",
        );
        write(
            v.path(),
            "notes.md",
            "[a](marketplaces/m1/plugins/p/skills/a.md)\n",
        );
        let op = AuthoringOp::Rename {
            from: "marketplaces/m1/skillwright.md".to_owned(),
            to: "marketplaces/m2/skillwright.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let before = tree(v.path());
        if !lock_writes(&v.path().join("marketplaces")) {
            return;
        }
        let err = apply(v.path(), &op).unwrap_err();
        unlock_writes(&v.path().join("marketplaces"));
        assert!(err.to_string().contains("read-only"), "{err}");
        assert_eq!(tree(v.path()), before);
    }

    /// A marketplace folder named `skillwright` is renamed like any other:
    /// the temporary entry name is its own, and links into it are rewritten.
    #[test]
    fn a_marketplace_named_skillwright_renames_and_relinks() {
        let v = vault();
        write(
            v.path(),
            "marketplaces/skillwright/skillwright.md",
            "---\nname: skillwright\n---\n",
        );
        write(
            v.path(),
            "marketplaces/skillwright/plugins/p/skills/a.md",
            "---\nname: a\n---\n",
        );
        write(
            v.path(),
            "notes.md",
            "[a](marketplaces/skillwright/plugins/p/skills/a.md)\n",
        );
        let op = AuthoringOp::Rename {
            from: "marketplaces/skillwright/skillwright.md".to_owned(),
            to: "marketplaces/shop/skillwright.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let before = tree(v.path());
        preview(v.path(), &op).unwrap();
        assert_eq!(tree(v.path()), before);
        apply(v.path(), &op).unwrap();
        assert!(v.path().join("marketplaces/shop/skillwright.md").is_file());
        assert!(!v.path().join("marketplaces/skillwright").exists());
        assert_eq!(
            read(v.path(), "notes.md"),
            "[a](marketplaces/shop/plugins/p/skills/a.md)\n"
        );
    }

    /// A marketplace's `skillwright.md` is its entry whatever the folder is
    /// called, as the core reads it: a member named `skillwright` moves the
    /// same way as one named `m1`.
    #[test]
    fn a_marketplace_entry_moves_alike_whatever_its_folder_is_called() {
        for name in ["m1", "skillwright"] {
            let v = vault();
            let entry = format!("marketplaces/{name}/skillwright.md");
            write(v.path(), &entry, "---\nname: m\n---\n");
            write(
                v.path(),
                &format!("marketplaces/{name}/plugins/p/skills/a.md"),
                "---\nname: a\n---\n",
            );
            write(
                v.path(),
                "notes.md",
                &format!("[a](marketplaces/{name}/plugins/p/skills/a.md)\n"),
            );
            let op = AuthoringOp::Move {
                from: entry.clone(),
                to: "marketplaces/shop/shop.md".to_owned(),
                bundle: true,
            };
            let outcome = apply(v.path(), &op).unwrap();
            assert_eq!(
                outcomes(&outcome),
                vec![("marketplaces/shop/shop.md", "renamed")],
                "{name}"
            );
            assert!(
                v.path().join("marketplaces/shop/shop.md").is_file(),
                "{name}"
            );
        }
    }

    /// A marketplace rename that half-lands reports the entry by its own
    /// name, not the temporary `<m>.md` name the move gave it.
    #[test]
    fn a_partial_marketplace_rename_names_no_temporary_path() {
        let v = vault();
        write(
            v.path(),
            "marketplaces/m1/skillwright.md",
            "---\nname: m1\n---\n",
        );
        write(
            v.path(),
            "marketplaces/m1/plugins/p/skills/a.md",
            "---\nname: a\n---\n",
        );
        write(
            v.path(),
            "notes.md",
            "[a](marketplaces/m1/plugins/p/skills/a.md)\n",
        );
        // A folder where the relinker renames the entry makes that rename fail
        // after the folder moved.
        write(v.path(), "marketplaces/m1/m2.md/keep.txt", "x");
        let op = AuthoringOp::Rename {
            from: "marketplaces/m1/skillwright.md".to_owned(),
            to: "marketplaces/m2/skillwright.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let err = apply(v.path(), &op).unwrap_err();
        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(v.path().join("marketplaces/m2/skillwright.md").is_file());
        assert!(v.path().join("marketplaces/m2/m2.md/keep.txt").is_file());
        for entry in &outcome.entries {
            assert!(!entry.path.ends_with("/m1.md"), "{outcome:?}");
            assert!(!entry.path.ends_with("/m2.md"), "{outcome:?}");
        }
        let message = source.to_string();
        assert!(!message.contains("m1.md"), "{message}");
        assert!(
            message.contains("marketplaces/m2/skillwright.md"),
            "{message}"
        );
    }

    /// A marketplace move whose entry name cannot be put back reports what
    /// landed, and says where the entry is; when the move failed too, both
    /// failures are said.
    #[test]
    fn a_failed_entry_restore_is_partial_and_keeps_both_errors() {
        let mut landed = Outcome::new(Mode::Apply);
        landed.push("notes.md", Applied::LinksRewritten { edits: 1 });
        let restore = || ApplyError::Io {
            what: "restoring m2/skillwright.md, so the entry is still at m2/m2.md".to_owned(),
            source: std::io::Error::other("denied"),
        };

        let err = settle(Ok(()), Err(restore()), &landed).unwrap_err();
        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert_eq!(outcomes(&outcome), vec![("notes.md", "relinked")]);
        assert!(source.to_string().contains("still at m2/m2.md"), "{source}");

        let moved =
            ApplyError::partial(landed.clone(), ApplyError::Refused("moving failed".into()));
        let err = settle(Err(moved), Err(restore()), &Outcome::new(Mode::Apply)).unwrap_err();
        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert_eq!(outcomes(&outcome), vec![("notes.md", "relinked")]);
        let message = source.to_string();
        assert!(message.contains("moving failed"), "{message}");
        assert!(message.contains("still at m2/m2.md"), "{message}");

        let err = settle(
            Err(ApplyError::Refused("bare".into())),
            Err(restore()),
            &landed,
        )
        .unwrap_err();
        let ApplyError::Partial { source, .. } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(source.to_string().contains("bare"), "{source}");
        assert!(matches!(settle(Ok(()), Ok(()), &landed), Ok(())));
    }

    /// The rows and the error say where the entry is: under its own name
    /// once it is put back, and under the temporary one while it is not.
    #[test]
    fn own_names_follow_where_the_entry_is() {
        let rows = || {
            let mut outcome = Outcome::new(Mode::Apply);
            outcome.push(
                "m2/m2.md",
                Applied::Renamed {
                    from: "m1/m1.md".to_owned(),
                },
            );
            outcome.push(
                "m1/m1.md",
                Applied::Skipped {
                    reason: "entry_rename_failed".to_owned(),
                },
            );
            outcome.entries
        };
        let names = |now| OwnNames {
            from: "m1/skillwright.md",
            temp_from: "m1/m1.md",
            temp_to: "m2/m2.md",
            landed: "m2/m1.md".to_owned(),
            now,
        };
        let paths = |entries: &[AppliedEntry]| -> Vec<String> {
            entries.iter().map(|e| e.path.clone()).collect()
        };

        let mut entries = rows();
        names(Some("m2/skillwright.md")).rows(&mut entries);
        assert_eq!(
            paths(&entries),
            vec!["m2/skillwright.md", "m1/skillwright.md"]
        );

        let mut entries = rows();
        names(None).rows(&mut entries);
        assert_eq!(paths(&entries), vec!["m2/m2.md", "m1/m1.md"]);
        assert!(matches!(
            &entries[0].applied,
            Applied::Renamed { from } if from == "m1/skillwright.md"
        ));

        let mut err = ApplyError::Io {
            what: "the entry note is now at m2/m1.md; then restoring".to_owned(),
            source: std::io::Error::other("denied"),
        };
        names(Some("m2/skillwright.md")).error(&mut err);
        assert!(
            err.to_string().contains("now at m2/skillwright.md;"),
            "{err}"
        );
    }

    /// A failed restore names its paths from the vault root.
    #[cfg(unix)]
    #[test]
    fn a_failed_restore_names_vault_relative_paths() {
        let v = vault();
        write(v.path(), "marketplaces/m1/m1.md", "---\nname: m1\n---\n");
        let m1 = v.path().join("marketplaces/m1");
        if !lock_writes(&m1) {
            return;
        }
        let err = restore_entry(
            v.path(),
            &m1.join("skillwright.md"),
            &m1.join("m1.md"),
            &v.path().join("marketplaces/m2"),
            &v.path().join("marketplaces/m2/skillwright.md"),
            &v.path().join("marketplaces/m2/m2.md"),
        );
        unlock_writes(&m1);
        let message = err.unwrap_err().to_string();
        assert!(
            message.contains("restoring marketplaces/m1/skillwright.md"),
            "{message}"
        );
        assert!(!message.contains(&*v.path().to_string_lossy()), "{message}");
    }

    #[test]
    fn a_temporary_path_is_replaced_only_where_it_is_whole() {
        let old = "marketplaces/m2/m1.md";
        let new = "marketplaces/m2/skillwright.md";
        assert_eq!(
            replace_path(&format!("now at {old}."), old, new),
            format!("now at {new}.")
        );
        assert_eq!(
            replace_path(&format!("now at {old}; see {old}x"), old, new),
            format!("now at {new}; see {old}x")
        );
        assert_eq!(
            replace_path(&format!("at x{old} and {old}.bak"), old, new),
            format!("at x{old} and {old}.bak")
        );
    }

    /// When the folder moves and the entry's own rename then fails, the
    /// error says what already moved rather than claiming nothing did.
    #[test]
    fn a_folder_move_whose_entry_rename_fails_reports_the_folder_move() {
        let v = vault();
        write(
            v.path(),
            "marketplaces/m1/skillwright.md",
            "---\nname: m1\n---\n",
        );
        // A folder where the entry's new name goes makes its rename fail.
        write(v.path(), "marketplaces/m1/shop.md/keep.txt", "x");
        let op = AuthoringOp::Move {
            from: "marketplaces/m1/skillwright.md".to_owned(),
            to: "marketplaces/shop/shop.md".to_owned(),
            bundle: true,
        };
        let err = apply(v.path(), &op).unwrap_err();
        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(matches!(*source, ApplyError::Io { .. }), "{source:?}");
        assert!(v.path().join("marketplaces/shop/skillwright.md").is_file());
        assert_eq!(
            outcomes(&outcome),
            vec![("marketplaces/shop/skillwright.md", "renamed")]
        );
    }

    /// A bundle named `skillwright` anywhere but directly under
    /// `marketplaces/` is an ordinary `<dir>/<dir>.md` bundle: it moves with
    /// its links rewritten.
    #[test]
    fn a_bundle_named_skillwright_outside_marketplaces_is_relinked() {
        for (from, to) in [
            (
                "skills/group/skillwright/skillwright.md",
                "skills/group/sw/sw.md",
            ),
            ("tools/skillwright/skillwright.md", "tools/sw/sw.md"),
        ] {
            let v = vault();
            write(v.path(), from, "---\nname: skillwright\n---\n");
            write(v.path(), "notes.md", &format!("[s]({from})\n"));
            let op = AuthoringOp::Move {
                from: from.to_owned(),
                to: to.to_owned(),
                bundle: true,
            };
            let outcome = apply(v.path(), &op).unwrap();
            assert!(v.path().join(to).is_file(), "{from}");
            assert_eq!(read(v.path(), "notes.md"), format!("[s]({to})\n"), "{from}");
            assert!(
                outcomes(&outcome).contains(&("notes.md", "relinked")),
                "{from}: {outcome:?}"
            );
        }
    }

    /// Nothing moves onto the vault's root file either.
    #[test]
    fn moving_onto_the_vaults_root_file_is_refused() {
        for (from, bundle) in [("notes/a.md", false), ("skills/a/a.md", true)] {
            let v = vault();
            write(v.path(), from, "---\nname: a\n---\n");
            let op = AuthoringOp::Move {
                from: from.to_owned(),
                to: "skillwright.md".to_owned(),
                bundle,
            };
            let before = tree(v.path());
            let err = preview(v.path(), &op).unwrap_err();
            assert!(err.to_string().contains("root file"), "{from}: {err}");
            let err = apply(v.path(), &op).unwrap_err();
            assert!(err.to_string().contains("root file"), "{from}: {err}");
            assert_eq!(tree(v.path()), before, "{from}");
        }
    }

    /// The root-file guard does not depend on the file system's case: on a
    /// case-insensitive one, `SKILLWRIGHT.md` is the vault's root file.
    #[test]
    fn the_root_file_guard_ignores_case() {
        for (from, to) in [
            ("SKILLWRIGHT.md", "x/x.md"),
            ("notes/a.md", "Skillwright.md"),
        ] {
            let v = vault();
            write(v.path(), "skillwright.md", "---\nname: v\n---\n");
            write(v.path(), "notes/a.md", "---\nname: a\n---\n");
            let op = AuthoringOp::Move {
                from: from.to_owned(),
                to: to.to_owned(),
                bundle: false,
            };
            let err = apply(v.path(), &op).unwrap_err();
            assert!(
                err.to_string().contains("root file"),
                "{from} -> {to}: {err}"
            );
        }
    }

    /// The vault's own root file is not a folder's entry: moving it is
    /// refused, and nothing changes.
    #[test]
    fn moving_the_vaults_root_file_is_refused() {
        for (to, bundle) in [
            ("x/skillwright.md", true),
            ("x/x.md", true),
            ("x.md", false),
        ] {
            let v = vault();
            write(v.path(), "skillwright.md", "---\nname: v\n---\n");
            write(v.path(), "skills/a.md", "---\nname: a\n---\n");
            let op = AuthoringOp::Move {
                from: "skillwright.md".to_owned(),
                to: to.to_owned(),
                bundle,
            };
            let before = tree(v.path());
            let err = preview(v.path(), &op).unwrap_err();
            assert!(matches!(err, ApplyError::Refused(_)), "{to}: {err:?}");
            let err = apply(v.path(), &op).unwrap_err();
            assert!(matches!(err, ApplyError::Refused(_)), "{to}: {err:?}");
            assert_eq!(tree(v.path()), before, "{to}");
        }
    }

    /// A bundled component named `skillwright` is an ordinary `<dir>/<dir>.md`
    /// bundle: converting it previews and applies, with links rewritten.
    #[test]
    fn a_converted_bundle_named_skillwright_does_not_recurse() {
        let v = vault();
        write(
            v.path(),
            "skills/skillwright/skillwright.md",
            "---\nname: skillwright\n---\n",
        );
        write(
            v.path(),
            "notes.md",
            "[s](skills/skillwright/skillwright.md)\n",
        );
        let op = AuthoringOp::ConvertKind {
            from: "skills/skillwright/skillwright.md".to_owned(),
            to: "commands/skillwright/skillwright.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
            scaffold: vec![],
        };
        let before = tree(v.path());
        preview(v.path(), &op).unwrap();
        assert_eq!(tree(v.path()), before);
        apply(v.path(), &op).unwrap();
        assert!(
            v.path()
                .join("commands/skillwright/skillwright.md")
                .is_file()
        );
        assert_eq!(
            read(v.path(), "notes.md"),
            "[s](commands/skillwright/skillwright.md)\n"
        );
    }

    #[test]
    fn move_relocates_the_note() {
        let v = vault();
        write(v.path(), "inbox/a.md", "---\nname: a\n---\n\nbody\n");
        write(v.path(), "notes/b.md", "link to [[a]]\n");

        let op = AuthoringOp::Move {
            from: "inbox/a.md".to_owned(),
            to: "archive/a.md".to_owned(),
            bundle: false,
        };
        apply(v.path(), &op).unwrap();

        assert!(v.path().join("archive/a.md").exists());
        assert!(!v.path().join("inbox/a.md").exists());
        // The basename did not change, so the bare-name link is left alone.
        assert_eq!(read(v.path(), "notes/b.md"), "link to [[a]]\n");
    }

    #[test]
    fn delete_moves_into_the_trash() {
        let v = vault();
        write(v.path(), "notes/a.md", "gone");
        let op = AuthoringOp::Delete {
            paths: vec!["notes/a.md".to_owned()],
            trash_dir: TRASH_DIR.to_owned(),
        };
        let outcome = apply(v.path(), &op).unwrap();

        assert!(!v.path().join("notes/a.md").exists());
        assert_eq!(read(v.path(), ".trash/notes/a.md"), "gone");
        assert_eq!(outcomes(&outcome), vec![("notes/a.md", "trashed")]);
    }

    #[test]
    fn delete_refuses_an_occupied_trash_slot() {
        let v = vault();
        write(v.path(), "notes/a.md", "current");
        write(v.path(), ".trash/notes/a.md", "earlier");
        let op = AuthoringOp::Delete {
            paths: vec!["notes/a.md".to_owned()],
            trash_dir: TRASH_DIR.to_owned(),
        };

        let err = apply(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Refused(_)), "{err:?}");
        assert_eq!(read(v.path(), "notes/a.md"), "current");
        assert_eq!(read(v.path(), ".trash/notes/a.md"), "earlier");
    }

    #[test]
    fn a_preview_writes_nothing() {
        let v = vault();
        write(
            v.path(),
            "notes/Alpha.md",
            "---\nname: Alpha\n---\n\nsubject\n",
        );
        write(v.path(), "notes/Beta.md", "see [[Alpha]]\n");
        let before = tree(v.path());

        let op = AuthoringOp::Rename {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![FieldEdit::set("name", Value::Str("Gamma".to_owned()))],
        };
        let outcome = preview(v.path(), &op).unwrap();

        assert!(outcome.preview);
        assert_eq!(tree(v.path()), before);
        assert_eq!(
            outcomes(&outcome),
            vec![
                ("notes/Gamma.md", "renamed"),
                ("notes/Beta.md", "relinked"),
                ("notes/Gamma.md", "field-set"),
            ]
        );
    }

    /// A frontmatter splice the note's YAML refuses is refused before the
    /// note moves, by the preview and the real run alike, so a dry run does
    /// not promise what the real run cannot do.
    #[test]
    fn a_frontmatter_edit_the_yaml_refuses_is_refused_before_moving() {
        let v = vault();
        write(
            v.path(),
            "notes/Alpha.md",
            "---\nname: Alpha\nname: Again\n---\n\nsubject\n",
        );
        let before = tree(v.path());

        let op = AuthoringOp::Rename {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![FieldEdit::set("name", Value::Str("Gamma".to_owned()))],
        };
        let err = preview(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Frontmatter { .. }), "{err:?}");
        let err = apply(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Frontmatter { .. }), "{err:?}");

        assert_eq!(tree(v.path()), before);
    }

    /// A step after the move lands (here, a scaffold under a read-only
    /// directory) reports what already happened rather than a bare error.
    #[cfg(unix)]
    #[test]
    fn convert_kind_reports_the_move_when_a_later_scaffold_fails_after_it() {
        let v = vault();
        write(v.path(), "notes/Alpha.md", "---\nname: Alpha\n---\n");
        std::fs::create_dir_all(v.path().join("locked")).unwrap();
        if !lock_writes(&v.path().join("locked")) {
            return;
        }

        let op = AuthoringOp::ConvertKind {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
            scaffold: vec!["locked/sub".to_owned()],
        };
        let err = apply(v.path(), &op).unwrap_err();
        unlock_writes(&v.path().join("locked"));

        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(matches!(*source, ApplyError::Io { .. }), "{source:?}");
        assert!(v.path().join("notes/Gamma.md").exists());
        assert!(!v.path().join("notes/Alpha.md").exists());
        assert_eq!(outcomes(&outcome), vec![("notes/Gamma.md", "renamed")]);
    }

    /// A scaffold directory that collides with a file is refused before the
    /// note moves, by the preview and the real run alike.
    #[test]
    fn convert_kind_refuses_a_scaffold_that_collides_with_a_file_before_moving() {
        let v = vault();
        write(
            v.path(),
            "notes/Alpha.md",
            "---\nname: Alpha\n---\n\nsubject\n",
        );
        write(v.path(), "notes/collide", "occupied");

        let op = AuthoringOp::ConvertKind {
            from: "notes/Alpha.md".to_owned(),
            to: "notes/Gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
            scaffold: vec!["notes/collide/references".to_owned()],
        };
        let err = preview(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Refused(_)), "{err:?}");
        let err = apply(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Refused(_)), "{err:?}");

        assert!(v.path().join("notes/Alpha.md").exists());
        assert!(!v.path().join("notes/Gamma.md").exists());
    }

    /// A note whose folder would sit where a file is, or whose scaffold
    /// would, is refused before anything is written, by the preview and
    /// the real run alike.
    #[test]
    fn create_refuses_a_folder_that_collides_with_a_file() {
        let v = vault();
        write(v.path(), "notes/collide", "occupied");
        let cases = [
            ("notes/collide/x.md", vec![]),
            ("plugins/x/x.md", vec!["notes/collide/skills".to_owned()]),
        ];
        for (path, scaffold) in cases {
            let op = AuthoringOp::Create {
                path: path.to_owned(),
                bytes: "---\nname: x\n---\n".to_owned(),
                scaffold,
            };
            let err = preview(v.path(), &op).unwrap_err();
            assert!(matches!(err, ApplyError::Refused(_)), "{path}: {err:?}");
            let err = apply(v.path(), &op).unwrap_err();
            assert!(matches!(err, ApplyError::Refused(_)), "{path}: {err:?}");
            assert!(!v.path().join(path).exists(), "{path}");
        }
    }

    /// Make `dir` read-only and report whether that took: a test run as root
    /// can still write there, and then the failure under test cannot happen.
    #[cfg(unix)]
    fn lock_writes(dir: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe = dir.join(".probe");
        let writable = std::fs::create_dir(&probe).is_ok();
        let _ = std::fs::remove_dir(&probe);
        !writable
    }

    #[cfg(unix)]
    fn unlock_writes(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A scaffold directory that fails after the note was written reports
    /// the note it left behind.
    #[cfg(unix)]
    #[test]
    fn create_reports_the_note_when_a_scaffold_fails_after_it() {
        let v = vault();
        std::fs::create_dir_all(v.path().join("locked")).unwrap();
        if !lock_writes(&v.path().join("locked")) {
            return;
        }
        let op = AuthoringOp::Create {
            path: "plugins/x/x.md".to_owned(),
            bytes: "---\nname: x\n---\n".to_owned(),
            scaffold: vec!["locked/sub".to_owned()],
        };
        let err = apply(v.path(), &op).unwrap_err();
        unlock_writes(&v.path().join("locked"));

        let ApplyError::Partial { outcome, .. } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(v.path().join("plugins/x/x.md").exists());
        assert_eq!(outcomes(&outcome), vec![("plugins/x/x.md", "created")]);
    }

    /// A later path that fails to move after an earlier one was trashed
    /// reports the one that moved.
    #[test]
    fn delete_reports_what_was_trashed_when_a_later_move_fails() {
        let v = vault();
        write(v.path(), "a/b.md", "inner");
        write(v.path(), "a/c.md", "sibling");
        let op = AuthoringOp::Delete {
            paths: vec!["a/b.md".to_owned(), "a".to_owned()],
            trash_dir: TRASH_DIR.to_owned(),
        };
        let err = apply(v.path(), &op).unwrap_err();

        let ApplyError::Partial { outcome, .. } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert_eq!(read(v.path(), ".trash/a/b.md"), "inner");
        assert_eq!(outcomes(&outcome), vec![("a/b.md", "trashed")]);
    }

    /// A bundle whose directory moved but whose entry file could not take
    /// its new name reports the rewritten links and the failed rename,
    /// rather than a bare refusal.
    #[test]
    fn a_bundle_move_whose_entry_rename_fails_reports_what_landed() {
        let v = vault();
        write(v.path(), "review/review.md", "---\nname: review\n---\n");
        write(v.path(), "review/code-review.md/keep.txt", "occupied");
        write(v.path(), "other.md", "see [[review]]\n");

        let op = AuthoringOp::Rename {
            from: "review/review.md".to_owned(),
            to: "code-review/code-review.md".to_owned(),
            bundle: true,
            frontmatter: vec![FieldEdit::set("name", Value::Str("code-review".to_owned()))],
        };
        assert!(preview(v.path(), &op).is_ok());
        let err = apply(v.path(), &op).unwrap_err();

        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        let ApplyError::Refused(message) = *source else {
            panic!("expected a refusal, got {source:?}");
        };
        assert!(
            message.ends_with("the entry note is now at code-review/review.md"),
            "{message}"
        );
        assert!(!v.path().join("review").exists());
        assert!(v.path().join("code-review/review.md").exists());
        let rows = outcomes(&outcome);
        assert!(rows.contains(&("other.md", "relinked")), "{rows:?}");
        assert!(rows.contains(&("review/review.md", "skipped")), "{rows:?}");
    }

    /// A move whose final rename fails after the linking notes were already
    /// rewritten reports those notes rather than a bare transform error.
    #[test]
    fn a_move_that_fails_after_rewriting_links_reports_the_rewrites() {
        let v = vault();
        write(v.path(), "skills/alpha/alpha.md", "---\nname: alpha\n---\n");
        write(v.path(), "notes/n.md", "see [[alpha]]\n");

        let op = AuthoringOp::Rename {
            from: "skills/alpha/alpha.md".to_owned(),
            to: "skills/alpha/sub/sub.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let err = apply(v.path(), &op).unwrap_err();

        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(matches!(*source, ApplyError::Transform(_)), "{source:?}");
        assert_eq!(read(v.path(), "notes/n.md"), "see [[sub]]\n");
        assert_eq!(outcomes(&outcome), vec![("notes/n.md", "relinked")]);
    }

    /// A rename that half-lands still reports the folders the walk could not
    /// list, after the rows for what landed.
    #[cfg(unix)]
    #[test]
    fn a_partial_move_keeps_the_unreadable_rows() {
        let v = vault();
        write(v.path(), "skills/alpha/alpha.md", "---\nname: alpha\n---\n");
        write(v.path(), "notes/n.md", "see [[alpha]]\n");
        let folders = ["notes/locked"];
        let listable = lock(v.path(), &folders);
        let op = AuthoringOp::Rename {
            from: "skills/alpha/alpha.md".to_owned(),
            to: "skills/alpha/sub/sub.md".to_owned(),
            bundle: true,
            frontmatter: vec![],
        };
        let err = apply(v.path(), &op);
        unlock(v.path(), &folders);
        if listable {
            return;
        }

        let ApplyError::Partial { outcome, .. } = err.unwrap_err() else {
            panic!("expected a partial outcome");
        };
        assert_eq!(
            outcomes(&outcome),
            vec![("notes/n.md", "relinked"), ("notes/locked", "skipped")]
        );
    }

    /// The same for a plain note: `execute` rewrites the linking note, then
    /// the note itself cannot leave its read-only folder.
    #[cfg(unix)]
    #[test]
    fn a_plain_move_that_fails_after_rewriting_links_reports_the_rewrites() {
        let v = vault();
        write(v.path(), "locked/alpha.md", "---\nname: alpha\n---\n");
        write(v.path(), "notes/n.md", "see [[alpha]]\n");
        std::fs::create_dir_all(v.path().join("moved")).unwrap();
        if !lock_writes(&v.path().join("locked")) {
            return;
        }

        let op = AuthoringOp::Move {
            from: "locked/alpha.md".to_owned(),
            to: "moved/gamma.md".to_owned(),
            bundle: false,
        };
        let err = apply(v.path(), &op).unwrap_err();
        unlock_writes(&v.path().join("locked"));

        let ApplyError::Partial { outcome, source } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(matches!(*source, ApplyError::Transform(_)), "{source:?}");
        assert!(v.path().join("locked/alpha.md").exists());
        assert_eq!(read(v.path(), "notes/n.md"), "see [[gamma]]\n");
        assert_eq!(outcomes(&outcome), vec![("notes/n.md", "relinked")]);
    }

    /// After a failed `execute`, a note that carries the planned rewrite is
    /// reported, and one someone else changed is not.
    #[test]
    fn only_notes_carrying_the_planned_rewrite_are_reported_as_rewritten() {
        let v = vault();
        write(v.path(), "alpha.md", "---\nname: alpha\n---\n");
        write(v.path(), "rewritten.md", "see [[alpha]]\n");
        write(v.path(), "drifted.md", "also [[alpha]]\n");
        let source = Vault::new(v.path(), &NativeVfs);
        let (corpus, _) = load_vault(source, "alpha.md", "gamma.md", false).unwrap();
        let facts = VaultFacts::from_corpus(&corpus);
        let plan =
            markdown_relink::plan(&NativeVfs, v.path(), &facts, "alpha.md", "gamma.md").unwrap();
        let spliced = spliced_etags(v.path(), &plan);

        write(v.path(), "rewritten.md", "see [[gamma]]\n");
        write(v.path(), "drifted.md", "also [[alpha]], edited elsewhere\n");
        let mut outcome = Outcome::new(Mode::Apply);
        record_rewritten_on_disk(v.path(), &spliced, &mut outcome);

        assert_eq!(outcomes(&outcome), vec![("rewritten.md", "relinked")]);
    }

    /// `markdown_link::normalize_target` lowercases and percent-decodes but does
    /// not Unicode-normalize, so a file stored decomposed (`cafe` + U+0301)
    /// and a composed link (`caf\u{e9}`) key differently: the link does not
    /// resolve to the note, is not in the rewrite set, and is left exactly as
    /// written. The rename itself still lands.
    #[test]
    fn an_nfc_link_does_not_match_an_nfd_filename() {
        let v = vault();
        let nfd = "notes/cafe\u{301}.md";
        let nfc_link = "see [[caf\u{e9}]]\n";
        write(v.path(), nfd, "---\nname: cafe\n---\n\nsubject\n");
        write(v.path(), "notes/Beta.md", nfc_link);

        let op = AuthoringOp::Rename {
            from: nfd.to_owned(),
            to: "notes/tea.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        let outcome = apply(v.path(), &op).unwrap();

        assert!(v.path().join("notes/tea.md").exists());
        assert!(!v.path().join(nfd).exists());
        assert_eq!(read(v.path(), "notes/Beta.md"), nfc_link);
        assert_eq!(outcomes(&outcome), vec![("notes/tea.md", "renamed")]);
    }

    /// The same rename with the link spelled the way the filename is stored:
    /// keys match and the inbound link is rewritten.
    #[test]
    fn an_nfd_link_matches_an_nfd_filename() {
        let v = vault();
        let nfd = "notes/cafe\u{301}.md";
        write(v.path(), nfd, "---\nname: cafe\n---\n\nsubject\n");
        write(v.path(), "notes/Beta.md", "see [[cafe\u{301}]]\n");

        let op = AuthoringOp::Rename {
            from: nfd.to_owned(),
            to: "notes/tea.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        apply(v.path(), &op).unwrap();

        assert_eq!(read(v.path(), "notes/Beta.md"), "see [[tea]]\n");
    }
}
