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

use markdown_relink::{FileEdits, RenamePlan, RenameResult, VaultFacts};
use tradecraft_core::authoring::{AuthoringOp, FieldEdit, apply_field_edits};

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
    /// (`drifted`, `not_previewed`, `unreadable`, `write_failed`). Its links
    /// still name the old path.
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

/// Execute `op` against the vault at `vault_dir`.
pub fn apply(vault_dir: &Path, op: &AuthoringOp) -> Result<Outcome, ApplyError> {
    run(vault_dir, op, Mode::Apply)
}

/// The same decisions and the same refusals as [`apply`], with nothing
/// written.
pub fn preview(vault_dir: &Path, op: &AuthoringOp) -> Result<Outcome, ApplyError> {
    run(vault_dir, op, Mode::Preview)
}

fn run(vault_dir: &Path, op: &AuthoringOp, mode: Mode) -> Result<Outcome, ApplyError> {
    match op {
        AuthoringOp::Create {
            path,
            bytes,
            scaffold,
        } => create(vault_dir, path, bytes, scaffold, mode),
        AuthoringOp::Rename {
            from,
            to,
            bundle,
            frontmatter,
        }
        | AuthoringOp::Move {
            from,
            to,
            bundle,
            frontmatter,
        } => relocate(vault_dir, from, to, *bundle, frontmatter, &[], mode),
        AuthoringOp::ConvertKind {
            from,
            to,
            bundle,
            frontmatter,
            scaffold,
        } => relocate(vault_dir, from, to, *bundle, frontmatter, scaffold, mode),
        AuthoringOp::Delete { paths, trash_dir } => delete(vault_dir, paths, trash_dir, mode),
        AuthoringOp::RenamePlugin {
            from,
            to,
            dir_from,
            dir_to,
            frontmatter,
            members,
        } => rename_plugin(
            vault_dir,
            from,
            to,
            dir_from,
            dir_to,
            frontmatter,
            members,
            mode,
        ),
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
    vault_dir: &Path,
    from: &str,
    to: &str,
    bundle: bool,
    frontmatter: &[FieldEdit],
    scaffold: &[String],
    mode: Mode,
) -> Result<Outcome, ApplyError> {
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
    move_note(vault_dir, &from, &to, bundle, mode, &mut outcome)?;
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
    vault_dir: &Path,
    from: &str,
    to: &str,
    bundle: bool,
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    let facts = VaultFacts::scan(vault_dir);
    let plan = if bundle {
        markdown_relink::plan_bundle(vault_dir, &facts, from, to)?
    } else {
        markdown_relink::plan(vault_dir, &facts, from, to)?
    };
    if let Some(conflict) = plan.conflict {
        return Err(ApplyError::Refused(conflict));
    }

    if !mode.writes() {
        record_planned(&plan, outcome);
        return Ok(());
    }

    // `execute` re-plans and treats `expect` as the assertion "these are the
    // bytes the user was shown"; a file missing from it is skipped as
    // `not_previewed`. The plan just computed IS what this run previewed, so
    // its own `expect` set is the honest one to pass, and `subject_etag` is
    // `None` because a CLI has no etag from an earlier session to assert.
    let spliced = spliced_etags(vault_dir, &plan);
    let executed = if bundle {
        markdown_relink::execute_bundle(vault_dir, &facts, from, to, None, &plan.expect)
    } else {
        markdown_relink::execute(vault_dir, &facts, from, to, None, &plan.expect)
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
    if let Some(conflict) = result.conflict.clone() {
        if !result.ok {
            return Err(ApplyError::Refused(conflict));
        }
        // `ok` with a conflict: the bundle directory moved and its links were
        // rewritten, and only the entry file's own rename failed.
        record_executed(&plan, &result, from, to, outcome);
        return Err(ApplyError::partial(
            outcome.clone(),
            ApplyError::Refused(conflict),
        ));
    }
    record_executed(&plan, &result, from, to, outcome);
    Ok(())
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
/// where the second is already trashed moves none of them.
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

// ── RenamePlugin ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn rename_plugin(
    vault_dir: &Path,
    from: &str,
    to: &str,
    dir_from: &str,
    dir_to: &str,
    frontmatter: &[FieldEdit],
    members: &[(String, Vec<FieldEdit>)],
    mode: Mode,
) -> Result<Outcome, ApplyError> {
    let from = rel(from)?;
    let to = rel(to)?;
    let dir_from = rel(dir_from)?;
    let dir_to = rel(dir_to)?;
    // Every member is checked up front, so a bad one refuses before the
    // folder moves — nothing here writes yet.
    let rebased_members = members
        .iter()
        .map(|(member, edits)| {
            let member = rel(member)?;
            if member == from || member == dir_to || member.starts_with(&format!("{dir_to}/")) {
                return Err(ApplyError::Refused(format!(
                    "member {member} is the plugin note or lies under {dir_to}"
                )));
            }
            if !resolve(vault_dir, &member)?.is_file() {
                return Err(ApplyError::Refused(format!(
                    "member {member} is not a file in the vault"
                )));
            }
            let rebased = match rebase(&member, &dir_from, &dir_to) {
                Some(rebased) => rebased?,
                None => member,
            };
            Ok::<_, ApplyError>((rebased, edits.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Every splice is tried on the notes where they sit, so YAML the splice
    // refuses stops the run before the folder moves.
    check_frontmatter(vault_dir, &from, frontmatter)?;
    for (member, edits) in members {
        check_frontmatter(vault_dir, member, edits)?;
    }

    let mut outcome = Outcome::new(mode);
    // A plugin's entry note is a bundle entry (`<dir>/<dir>.md`), so the
    // folder and every member inside it ride along on the bundle move; a
    // member outside it is edited in place.
    move_note(vault_dir, &from, &to, true, mode, &mut outcome)?;
    // From here the move has landed — a later failure is reported with what
    // already wrote, not as a bare refusal.
    if let Err(err) = rename_plugin_after_move(
        vault_dir,
        &to,
        frontmatter,
        &rebased_members,
        mode,
        &mut outcome,
    ) {
        return Err(ApplyError::after_write(mode, outcome, err));
    }
    Ok(outcome)
}

fn rename_plugin_after_move(
    vault_dir: &Path,
    to: &str,
    frontmatter: &[FieldEdit],
    rebased_members: &[(String, Vec<FieldEdit>)],
    mode: Mode,
    outcome: &mut Outcome,
) -> Result<(), ApplyError> {
    edit_frontmatter(vault_dir, to, frontmatter, mode, outcome)?;
    for (rebased, edits) in rebased_members {
        edit_frontmatter(vault_dir, rebased, edits, mode, outcome)?;
    }
    Ok(())
}

/// A member's path in the new tree, or `None` for a member outside
/// `dir_from`, which does not move. Members are keyed on the old tree, which
/// no longer exists once the folder has moved.
fn rebase(path: &str, dir_from: &str, dir_to: &str) -> Option<Result<String, ApplyError>> {
    let tail = path.strip_prefix(&format!("{dir_from}/"))?;
    Some(rel(&format!("{dir_to}/{tail}")))
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
    fn move_relocates_the_note() {
        let v = vault();
        write(v.path(), "inbox/a.md", "---\nname: a\n---\n\nbody\n");
        write(v.path(), "notes/b.md", "link to [[a]]\n");

        let op = AuthoringOp::Move {
            from: "inbox/a.md".to_owned(),
            to: "archive/a.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
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

    #[test]
    fn rename_plugin_moves_the_folder_and_rekeys_its_members() {
        let v = vault();
        write(
            v.path(),
            "plugins/tools/tools.md",
            "---\nname: tools\n---\n\nplugin\n",
        );
        write(
            v.path(),
            "plugins/tools/skills/keyed.md",
            "---\nname: keyed\nsw-plugin: tools\n---\n\nmember\n",
        );

        let op = AuthoringOp::RenamePlugin {
            from: "plugins/tools/tools.md".to_owned(),
            to: "plugins/gear/gear.md".to_owned(),
            dir_from: "plugins/tools".to_owned(),
            dir_to: "plugins/gear".to_owned(),
            frontmatter: vec![FieldEdit::set("name", Value::Str("gear".to_owned()))],
            members: vec![(
                "plugins/tools/skills/keyed.md".to_owned(),
                vec![FieldEdit::set("sw-plugin", Value::Str("gear".to_owned()))],
            )],
        };
        apply(v.path(), &op).unwrap();

        assert!(!v.path().join("plugins/tools").exists());
        assert!(read(v.path(), "plugins/gear/gear.md").contains("name: gear"));
        assert!(read(v.path(), "plugins/gear/skills/keyed.md").contains("sw-plugin: gear"));
    }

    /// A vault holding only the `tools` plugin note.
    fn tools_plugin() -> tempfile::TempDir {
        let v = vault();
        write(
            v.path(),
            "plugins/tools/tools.md",
            "---\nname: tools\n---\n\nplugin\n",
        );
        v
    }

    /// Rename `tools` to `gear`, rekeying the one `member`.
    fn rename_tools_to_gear(member: &str) -> AuthoringOp {
        AuthoringOp::RenamePlugin {
            from: "plugins/tools/tools.md".to_owned(),
            to: "plugins/gear/gear.md".to_owned(),
            dir_from: "plugins/tools".to_owned(),
            dir_to: "plugins/gear".to_owned(),
            frontmatter: vec![],
            members: vec![(
                member.to_owned(),
                vec![FieldEdit::set("sw-plugin", Value::Str("gear".to_owned()))],
            )],
        }
    }

    #[test]
    fn rename_plugin_edits_a_member_outside_dir_from_where_it_sits() {
        let v = tools_plugin();
        write(
            v.path(),
            "skills/stray.md",
            "---\nname: stray\nsw-plugin: tools\n---\n\nmember\n",
        );

        apply(v.path(), &rename_tools_to_gear("skills/stray.md")).unwrap();

        assert!(v.path().join("plugins/gear/gear.md").exists());
        assert!(read(v.path(), "skills/stray.md").contains("sw-plugin: gear"));
    }

    #[test]
    fn rename_plugin_refuses_a_bad_member_path_without_moving_anything() {
        let v = tools_plugin();

        let err = apply(v.path(), &rename_tools_to_gear("../outside.md")).unwrap_err();

        assert!(matches!(err, ApplyError::Path { .. }), "{err:?}");
        assert!(v.path().join("plugins/tools/tools.md").exists());
        assert!(!v.path().join("plugins/gear").exists());
    }

    #[test]
    fn rename_plugin_refuses_a_missing_or_misplaced_member_without_moving_anything() {
        for member in [
            "skills/missing.md",
            "plugins/tools/skills/missing.md",
            "skills",
            "plugins/tools/tools.md",
            "plugins/gear/gear.md",
        ] {
            let v = tools_plugin();
            write(v.path(), "skills/other.md", "---\nname: other\n---\n");

            let err = apply(v.path(), &rename_tools_to_gear(member)).unwrap_err();

            assert!(matches!(err, ApplyError::Refused(_)), "{member}: {err:?}");
            assert!(v.path().join("plugins/tools/tools.md").exists(), "{member}");
            assert!(!v.path().join("plugins/gear").exists(), "{member}");
        }
    }

    /// If a member's frontmatter edit fails once the bundle has already
    /// moved, the failure carries the [`Outcome`] that already landed rather
    /// than discarding it behind a bare refusal. The YAML splice is checked
    /// before the move, so the failure here is a write: the member sits in a
    /// read-only directory.
    #[cfg(unix)]
    #[test]
    fn rename_plugin_reports_the_move_when_a_members_edit_fails_after_it() {
        let v = tools_plugin();
        write(
            v.path(),
            "plugins/tools/skills/member.md",
            "---\nname: member\nsw-plugin: tools\n---\n\nmember\n",
        );
        let locked = v.path().join("plugins/tools/skills");
        if !lock(&locked) {
            return;
        }

        let err = apply(
            v.path(),
            &rename_tools_to_gear("plugins/tools/skills/member.md"),
        )
        .unwrap_err();
        unlock(&v.path().join("plugins/gear/skills"));

        let ApplyError::Partial { outcome, .. } = err else {
            panic!("expected a partial outcome, got {err:?}");
        };
        assert!(!v.path().join("plugins/tools").exists());
        assert!(v.path().join("plugins/gear/gear.md").exists());
        assert_eq!(
            outcomes(&outcome),
            vec![
                ("plugins/gear/gear.md", "renamed"),
                ("plugins/gear/skills/member.md", "moved"),
            ]
        );
    }

    /// A preview of a plugin rename lists the move and every edit, and
    /// writes nothing.
    #[test]
    fn rename_plugin_preview_lists_the_edits_and_writes_nothing() {
        let v = vault();
        write(
            v.path(),
            "plugins/tools/tools.md",
            "---\nname: tools\n---\n\nplugin\n",
        );
        write(
            v.path(),
            "plugins/tools/skills/keyed.md",
            "---\nname: keyed\nsw-plugin: tools\n---\n\nmember\n",
        );
        let before = tree(v.path());

        let mut op = rename_tools_to_gear("plugins/tools/skills/keyed.md");
        if let AuthoringOp::RenamePlugin { frontmatter, .. } = &mut op {
            frontmatter.push(FieldEdit::set("name", Value::Str("gear".to_owned())));
        }
        let outcome = preview(v.path(), &op).unwrap();

        assert_eq!(tree(v.path()), before);
        let rows = outcomes(&outcome);
        assert!(
            rows.contains(&("plugins/gear/gear.md", "renamed")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("plugins/gear/gear.md", "field-set")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("plugins/gear/skills/keyed.md", "field-set")),
            "{rows:?}"
        );
    }

    /// A member whose YAML the splice refuses is refused before the folder
    /// moves, by the preview and the real run alike.
    #[test]
    fn rename_plugin_refuses_a_member_frontmatter_edit_before_moving() {
        let v = vault();
        write(
            v.path(),
            "plugins/tools/tools.md",
            "---\nname: tools\n---\n\nplugin\n",
        );
        write(
            v.path(),
            "plugins/tools/skills/broken.md",
            "---\nsw-plugin: tools\nsw-plugin: again\n---\n\nmember\n",
        );
        let before = tree(v.path());

        let op = rename_tools_to_gear("plugins/tools/skills/broken.md");
        let err = preview(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Frontmatter { .. }), "{err:?}");
        let err = apply(v.path(), &op).unwrap_err();
        assert!(matches!(err, ApplyError::Frontmatter { .. }), "{err:?}");

        assert_eq!(tree(v.path()), before);
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
        if !lock(&v.path().join("locked")) {
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
        unlock(&v.path().join("locked"));

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
    fn lock(dir: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe = dir.join(".probe");
        let writable = std::fs::create_dir(&probe).is_ok();
        let _ = std::fs::remove_dir(&probe);
        !writable
    }

    #[cfg(unix)]
    fn unlock(dir: &Path) {
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
        if !lock(&v.path().join("locked")) {
            return;
        }
        let op = AuthoringOp::Create {
            path: "plugins/x/x.md".to_owned(),
            bytes: "---\nname: x\n---\n".to_owned(),
            scaffold: vec!["locked/sub".to_owned()],
        };
        let err = apply(v.path(), &op).unwrap_err();
        unlock(&v.path().join("locked"));

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
        assert!(matches!(*source, ApplyError::Refused(_)), "{source:?}");
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

    /// The same for a plain note: `execute` rewrites the linking note, then
    /// the note itself cannot leave its read-only folder.
    #[cfg(unix)]
    #[test]
    fn a_plain_move_that_fails_after_rewriting_links_reports_the_rewrites() {
        let v = vault();
        write(v.path(), "locked/alpha.md", "---\nname: alpha\n---\n");
        write(v.path(), "notes/n.md", "see [[alpha]]\n");
        std::fs::create_dir_all(v.path().join("moved")).unwrap();
        if !lock(&v.path().join("locked")) {
            return;
        }

        let op = AuthoringOp::Move {
            from: "locked/alpha.md".to_owned(),
            to: "moved/gamma.md".to_owned(),
            bundle: false,
            frontmatter: vec![],
        };
        let err = apply(v.path(), &op).unwrap_err();
        unlock(&v.path().join("locked"));

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
        let facts = VaultFacts::scan(v.path());
        let plan = markdown_relink::plan(v.path(), &facts, "alpha.md", "gamma.md").unwrap();
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
