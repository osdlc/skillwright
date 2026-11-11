//! The vault layout: where the root is, what each folder may hold, and what a
//! path is by its position alone.
//!
//! ```text
//! <root>/
//! ├── skillwright.md
//! ├── dist/
//! └── marketplaces/<m>/
//!     ├── <m>.md
//!     └── plugins/<p>/
//!         ├── <p>.md
//!         ├── skills/<s>/<s>.md   (also agents/, commands/)
//!         └── hooks/hooks.json
//! ```
//!
//! [`read_layout`] walks `marketplaces/` once and checks every folder against
//! its row of the table in spec section 3. A vault that breaks the layout is
//! refused before any note is read, with every problem in one message
//! (spec section 11).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::settings::ROOT_FILE;
use crate::vfs::Vfs;

/// The folder at the root that holds the marketplaces.
pub const MARKETPLACES_DIR: &str = "marketplaces";

/// The folder at the root a build writes to, one folder per marketplace.
pub const DIST_DIR: &str = "dist";

/// The folder of a marketplace that holds its plugins.
pub const PLUGINS_DIR: &str = "plugins";

/// The folder of a plugin that holds its hooks file.
pub const HOOKS_DIR: &str = "hooks";

/// The plugin's hooks file, inside [`HOOKS_DIR`].
pub const HOOKS_FILE: &str = "hooks.json";

/// The names no marketplace, plugin or component may take: each is a
/// structure folder.
pub const RESERVED_NAMES: [&str; 6] = [
    MARKETPLACES_DIR,
    PLUGINS_DIR,
    "skills",
    "agents",
    "commands",
    HOOKS_DIR,
];

/// The folder of a plugin that holds each component kind.
pub const KIND_DIRS: [(&str, EntityKind); 3] = [
    ("skills", EntityKind::Skill),
    ("agents", EntityKind::Agent),
    ("commands", EntityKind::Command),
];

/// The folders a plugin and a skill share out: each ships under its own name.
pub const SHARED_DIRS: [&str; 3] = ["references", "scripts", "assets"];

/// The never-ship folders, with whether a skill or plugin (true) or any
/// component folder (false) may hold each.
const NEVER_SHIP_DIRS: [(&str, bool); 4] = [
    ("docs", false),
    ("tests", false),
    ("fixtures", false),
    ("src", true),
];

/// The file a plugin folder keeps its changelog in.
pub const CHANGELOG_FILE: &str = "CHANGELOG.md";

/// The layout every error prints.
pub const EXPECTED_LAYOUT: &str = "\
<root>/
├── skillwright.md                   build settings and shared defaults
└── marketplaces/<m>/
    ├── <m>.md
    └── plugins/<p>/
        ├── <p>.md
        ├── skills/<s>/<s>.md   (also agents/, commands/)
        └── hooks/hooks.json";

/// The reason token of a layout refusal.
pub const LAYOUT_REASON: &str = "layout";

/// The kind a component folder (`skills`, `agents`, `commands`) holds.
pub fn kind_dir(name: &str) -> Option<EntityKind> {
    KIND_DIRS
        .iter()
        .find(|(dir, _)| *dir == name)
        .map(|(_, kind)| *kind)
}

/// Whether `dir` is a vault root: it holds the file `skillwright.md` and the
/// folder `marketplaces/`. A path the host cannot stat counts as absent.
pub fn is_vault_root(vfs: &dyn Vfs, dir: &Path) -> bool {
    root_parts(vfs, dir).is_ok_and(|(file, folder)| file && folder)
}

/// Whether `dir` holds the root file, and whether it holds `marketplaces/`.
/// A missing path is `false`; any other host error is returned.
fn root_parts(vfs: &dyn Vfs, dir: &Path) -> Result<(bool, bool), CoreError> {
    Ok((
        is_kind(vfs, &dir.join(ROOT_FILE), false)?,
        is_kind(vfs, &dir.join(MARKETPLACES_DIR), true)?,
    ))
}

/// The vault root at or above `start`: the first folder that holds both
/// `skillwright.md` and `marketplaces/` (spec section 1 rule 2). Any other
/// `skillwright.md` on the way, such as a skill's note, is passed over.
///
/// # Errors
///
/// * `refused: layout` when no folder up to the top of the file system is a
///   root. The error names each `skillwright.md` passed over.
/// * [`CoreError::Io`] — a path the host cannot stat for a reason other than
///   it being missing.
pub fn find_vault_root(vfs: &dyn Vfs, start: &Path) -> Result<PathBuf, CoreError> {
    let mut passed = Vec::new();
    for dir in start.ancestors() {
        match root_parts(vfs, dir)? {
            (true, true) => return Ok(dir.to_path_buf()),
            (true, false) => passed.push(dir.join(ROOT_FILE)),
            _ => {}
        }
    }
    let mut message = format!(
        "no folder at or above {} holds both `{ROOT_FILE}` and `{MARKETPLACES_DIR}/`",
        start.display()
    );
    if !passed.is_empty() {
        let named: Vec<String> = passed.iter().map(|f| f.display().to_string()).collect();
        message.push_str(&format!(
            ". Passed over {}, which has no `{MARKETPLACES_DIR}/` beside it",
            named.join(", ")
        ));
    }
    Err(layout_error(message))
}

/// Refuses a `dir` given as the root (`--vault`) that is not one.
///
/// # Errors
///
/// * `refused: layout`, naming what `dir` lacks.
/// * [`CoreError::Io`] — a path the host cannot stat for a reason other than
///   it being missing.
pub fn check_vault_root(vfs: &dyn Vfs, dir: &Path) -> Result<(), CoreError> {
    let (file, folder) = root_parts(vfs, dir)?;
    let mut missing = Vec::new();
    if !file {
        missing.push(format!("`{ROOT_FILE}`"));
    }
    if !folder {
        missing.push(format!("`{MARKETPLACES_DIR}/`"));
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(layout_error(format!(
        "{} is not a vault root: it has no {}",
        dir.display(),
        missing.join(" and no ")
    )))
}

/// What a path under the root is, by its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Position {
    /// `marketplaces/<m>/<m>.md`.
    Marketplace {
        /// The marketplace folder.
        marketplace: String,
    },
    /// `marketplaces/<m>/plugins/<p>/<p>.md`.
    Plugin {
        /// The marketplace folder.
        marketplace: String,
        /// The plugin folder.
        plugin: String,
    },
    /// `marketplaces/<m>/plugins/<p>/<kind>/<c>/<c>.md`.
    Component {
        /// The marketplace folder.
        marketplace: String,
        /// The plugin folder.
        plugin: String,
        /// The component's kind, from its kind folder.
        kind: EntityKind,
        /// The component folder.
        name: String,
    },
}

impl Position {
    /// The kind of note at this position.
    pub fn kind(&self) -> EntityKind {
        match self {
            Self::Marketplace { .. } => EntityKind::Marketplace,
            Self::Plugin { .. } => EntityKind::Plugin,
            Self::Component { kind, .. } => *kind,
        }
    }

    /// The folder the note is named for.
    pub fn name(&self) -> &str {
        match self {
            Self::Marketplace { marketplace } => marketplace,
            Self::Plugin { plugin, .. } => plugin,
            Self::Component { name, .. } => name,
        }
    }

    /// The marketplace folder it sits in.
    pub fn marketplace(&self) -> &str {
        match self {
            Self::Marketplace { marketplace }
            | Self::Plugin { marketplace, .. }
            | Self::Component { marketplace, .. } => marketplace,
        }
    }

    /// The plugin folder it sits in, `None` for a marketplace.
    pub fn plugin(&self) -> Option<&str> {
        match self {
            Self::Marketplace { .. } => None,
            Self::Plugin { plugin, .. } | Self::Component { plugin, .. } => Some(plugin),
        }
    }
}

/// What the root-relative, `/`-separated `rel` is, or `None` for a file that
/// is not a note: a resource, a hooks file, a changelog.
pub fn classify(rel: &str) -> Option<Position> {
    let parts: Vec<&str> = rel.split('/').collect();
    let note = |name: &str, file: &str| file.strip_suffix(".md") == Some(name);
    match parts.as_slice() {
        [MARKETPLACES_DIR, m, file] if note(m, file) => Some(Position::Marketplace {
            marketplace: (*m).to_owned(),
        }),
        [MARKETPLACES_DIR, m, PLUGINS_DIR, p, file] if note(p, file) => Some(Position::Plugin {
            marketplace: (*m).to_owned(),
            plugin: (*p).to_owned(),
        }),
        [MARKETPLACES_DIR, m, PLUGINS_DIR, p, kind, c, file] if note(c, file) => {
            Some(Position::Component {
                marketplace: (*m).to_owned(),
                plugin: (*p).to_owned(),
                kind: kind_dir(kind)?,
                name: (*c).to_owned(),
            })
        }
        _ => None,
    }
}

/// Whether the root-relative `rel` sits in a never-ship folder: `docs/`,
/// `tests/`, `fixtures/` or `src/` directly in a plugin or component folder.
pub fn never_ships(rel: &str) -> bool {
    never_ship_depth(rel).is_some()
}

/// How many segments of `rel` reach the never-ship folder holding it.
fn never_ship_depth(rel: &str) -> Option<usize> {
    let parts: Vec<&str> = rel.split('/').collect();
    let never = |at: usize| {
        parts.len() > at + 1 && NEVER_SHIP_DIRS.iter().any(|(dir, _)| *dir == parts[at])
    };
    if parts.len() < 5 || parts[0] != MARKETPLACES_DIR || parts[2] != PLUGINS_DIR {
        return None;
    }
    if never(4) {
        return Some(5);
    }
    if kind_dir(parts[4]).is_some() && never(6) {
        return Some(7);
    }
    None
}

/// Whether a link from the root-relative file `from` to `to` goes into a
/// never-ship folder. A file inside that folder links within it freely.
pub fn links_into_never_ship(from: &str, to: &str) -> bool {
    let Some(depth) = never_ship_depth(to) else {
        return false;
    };
    let folder: Vec<&str> = to.split('/').take(depth).collect();
    let from: Vec<&str> = from.split('/').collect();
    !(from.len() > depth && from[..depth] == folder[..])
}

/// The vault's layout: every file under `marketplaces/` a build reads, in
/// walk order, and the marketplaces by folder.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Layout {
    /// The vault root.
    pub root: PathBuf,
    /// Every file under `marketplaces/`, absolute, in walk order. Dot-named
    /// entries and `node_modules/` are left out.
    pub files: Vec<PathBuf>,
    /// The marketplace folders, sorted.
    pub marketplaces: Vec<String>,
}

/// One kind of layout problem, in the order a report lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Case {
    /// Section 11 case 3, and section 3 rule 3 for a plugin.
    MissingNote,
    /// Case 4.
    KindFolderOutOfPlace,
    /// Case 5.
    NestedMarketplaces,
    /// Case 6.
    NotNamedEntry,
    /// Case 7.
    RefusedBuildKey,
    /// Case 8.
    UnlistedEntry,
    /// Section 3 rule 5.
    BadName,
    /// Decision D7.
    McpJson,
}

impl Case {
    fn heading(self) -> &'static str {
        match self {
            Self::MissingNote => "a marketplace or plugin folder has no note named for it",
            Self::KindFolderOutOfPlace => {
                "a `skills/`, `agents/`, `commands/`, `hooks/` or `plugins/` folder is out of place"
            }
            Self::NestedMarketplaces => "a `marketplaces/` folder is inside `marketplaces/`",
            Self::NotNamedEntry => {
                "a component's note is not `<name>/<name>.md` (no flat notes, no `SKILL.md`)"
            }
            Self::RefusedBuildKey => "the root file has a removed `build:` key; delete it",
            Self::UnlistedEntry => "a folder holds an entry its place does not allow",
            Self::BadName => {
                "a name breaks the name rule: 1 to 64 characters of `a-z`, `0-9` and `-`, \
                 with no hyphen at either end or twice in a row, and none of the six reserved \
                 names"
            }
            Self::McpJson => "a plugin holds `.mcp.json`, which v2 has no place for",
        }
    }
}

/// The problems found, by case.
#[derive(Default)]
struct Problems(BTreeMap<Case, Vec<String>>);

impl Problems {
    fn add(&mut self, case: Case, what: String) {
        self.0.entry(case).or_default().push(what);
    }

    fn into_result(self) -> Result<(), CoreError> {
        if self.0.is_empty() {
            return Ok(());
        }
        let mut message = String::from("the vault is not in the skillwright layout:");
        for (case, items) in self.0 {
            message.push_str(&format!("\n- {}:", case.heading()));
            for item in items {
                message.push_str(&format!("\n  - {item}"));
            }
        }
        Err(CoreError::refused(
            LAYOUT_REASON,
            format!("{message}\nThe expected layout:\n{EXPECTED_LAYOUT}"),
        ))
    }
}

/// A layout refusal that is not a list of problems, with the expected layout.
fn layout_error(message: String) -> CoreError {
    CoreError::refused(
        LAYOUT_REASON,
        format!("{message}. The expected layout:\n{EXPECTED_LAYOUT}"),
    )
}

/// Reads the layout of the vault at `root` and checks it: the root's direct
/// children, every folder under `marketplaces/` against its row of the table
/// in spec section 3, and the root file's removed `build:` keys.
///
/// # Errors
///
/// * `refused: layout` — the root is not a vault root, or the vault breaks the
///   layout. Every problem is in the one message, each case once with every
///   path it applies to.
/// * `refused: unsafe-path` — a symlink under `marketplaces/`.
/// * [`CoreError::Io`] — a folder that cannot be listed.
pub fn read_layout(vfs: &dyn Vfs, root: &Path) -> Result<Layout, CoreError> {
    check_vault_root(vfs, root)?;
    let mut walk = Walk {
        vfs,
        root,
        files: Vec::new(),
        problems: Problems::default(),
    };
    for entry in walk.list(root)? {
        if entry.is_dir && (entry.name == PLUGINS_DIR || kind_or_hooks(&entry.name)) {
            walk.problems
                .add(Case::KindFolderOutOfPlace, format!("{}/", entry.name));
        }
    }
    if let Ok(bytes) = vfs.read_file(&root.join(ROOT_FILE)) {
        let text = String::from_utf8_lossy(&bytes);
        for key in crate::settings::refused_build_keys(&text) {
            walk.problems
                .add(Case::RefusedBuildKey, format!("`{key}` in {ROOT_FILE}"));
        }
    }
    let mut marketplaces = Vec::new();
    for entry in walk.list(&root.join(MARKETPLACES_DIR))? {
        let rel = format!("{MARKETPLACES_DIR}/{}", entry.name);
        if !entry.is_dir {
            walk.problems.add(
                Case::UnlistedEntry,
                format!("{rel} (`{MARKETPLACES_DIR}/` holds marketplace folders only)"),
            );
            continue;
        }
        if walk.named(&rel, &entry.name) {
            walk.marketplace(&rel, &entry.name)?;
            marketplaces.push(entry.name);
        }
    }
    walk.problems.into_result()?;
    marketplaces.sort();
    Ok(Layout {
        root: root.to_path_buf(),
        files: walk.files,
        marketplaces,
    })
}

/// Whether `name` is a component kind folder or `hooks`.
fn kind_or_hooks(name: &str) -> bool {
    name == HOOKS_DIR || kind_dir(name).is_some()
}

/// One folder entry as the walk sees it.
struct Entry {
    name: String,
    is_dir: bool,
}

/// The walk's state: the files read and the problems found.
struct Walk<'a> {
    vfs: &'a dyn Vfs,
    root: &'a Path,
    files: Vec<PathBuf>,
    problems: Problems,
}

impl Walk<'_> {
    /// The entries of `dir`, sorted, less dot-named ones and `node_modules`.
    /// A symlink is refused.
    fn list(&self, dir: &Path) -> Result<Vec<Entry>, CoreError> {
        let mut entries = self.vfs.read_dir(dir)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut out = Vec::new();
        for entry in entries {
            if entry.name.starts_with('.') || entry.name == "node_modules" {
                continue;
            }
            let path = dir.join(&entry.name);
            if self.vfs.is_symlink(&path) {
                return Err(CoreError::refused(
                    "unsafe-path",
                    format!(
                        "{}: a symlink in a vault. It reads as inside the tree and resolves \
                         outside it; replace it with the real file or folder",
                        path.display()
                    ),
                ));
            }
            out.push(Entry {
                name: entry.name,
                is_dir: entry.is_dir,
            });
        }
        Ok(out)
    }

    /// Whether the folder `rel` may take the name `name`; a problem is added
    /// when it may not. A reserved name breaks the name rule.
    fn named(&mut self, rel: &str, name: &str) -> bool {
        if RESERVED_NAMES.contains(&name) || crate::discover::name_refusal(name, "name").is_some() {
            self.problems.add(Case::BadName, format!("{rel}/"));
            return false;
        }
        true
    }

    /// Reports a structure folder met where none belongs, or descends into an
    /// ordinary one. Used inside shared and never-ship folders, where any
    /// layout of plain folders is allowed.
    fn free(&mut self, rel: &str) -> Result<(), CoreError> {
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir {
                self.files.push(self.root.join(&child));
            } else if entry.name == MARKETPLACES_DIR {
                self.problems
                    .add(Case::NestedMarketplaces, format!("{child}/"));
            } else if entry.name == PLUGINS_DIR || kind_or_hooks(&entry.name) {
                self.problems
                    .add(Case::KindFolderOutOfPlace, format!("{child}/"));
            } else {
                self.free(&child)?;
            }
        }
        Ok(())
    }

    /// An entry `rel` its folder does not list, as a folder or a file.
    fn unlisted(&mut self, rel: &str, entry: &Entry, allowed: &str) {
        let shown = if entry.is_dir {
            format!("{rel}/")
        } else {
            rel.to_owned()
        };
        if entry.is_dir && entry.name == MARKETPLACES_DIR {
            self.problems.add(Case::NestedMarketplaces, shown);
        } else if entry.is_dir && (entry.name == PLUGINS_DIR || kind_or_hooks(&entry.name)) {
            self.problems.add(Case::KindFolderOutOfPlace, shown);
        } else {
            self.problems.add(
                Case::UnlistedEntry,
                format!("{shown} (it may hold {allowed})"),
            );
        }
    }

    /// `marketplaces/<m>/`.
    fn marketplace(&mut self, rel: &str, name: &str) -> Result<(), CoreError> {
        let note = format!("{name}.md");
        let allowed = format!("`{note}` and `{PLUGINS_DIR}/`");
        let mut has_note = false;
        let mut old_note = false;
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir && entry.name == note {
                has_note = true;
                self.files.push(self.root.join(&child));
            } else if entry.is_dir && entry.name == PLUGINS_DIR {
                self.plugins(&child)?;
            } else if !entry.is_dir && entry.name == ROOT_FILE {
                old_note = true;
            } else {
                self.unlisted(&child, &entry, &allowed);
            }
        }
        if !has_note {
            let hint = if old_note {
                format!(" (its `{ROOT_FILE}` is not read: the marketplace note is now `{note}`)")
            } else {
                String::new()
            };
            self.problems
                .add(Case::MissingNote, format!("{rel}/{note}{hint}"));
        } else if old_note {
            self.problems.add(
                Case::UnlistedEntry,
                format!("{rel}/{ROOT_FILE} (the marketplace note is `{note}`)"),
            );
        }
        Ok(())
    }

    /// `marketplaces/<m>/plugins/`.
    fn plugins(&mut self, rel: &str) -> Result<(), CoreError> {
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir {
                self.problems.add(
                    Case::UnlistedEntry,
                    format!("{child} (`{PLUGINS_DIR}/` holds plugin folders only)"),
                );
            } else if self.named(&child, &entry.name) {
                self.plugin(&child, &entry.name)?;
            }
        }
        Ok(())
    }

    /// `plugins/<p>/`.
    fn plugin(&mut self, rel: &str, name: &str) -> Result<(), CoreError> {
        let note = format!("{name}.md");
        let allowed = format!(
            "`{note}`, `{CHANGELOG_FILE}`, `references/`, `scripts/`, `assets/`, `skills/`, \
             `agents/`, `commands/`, `hooks/`, `src/`, `docs/`, `tests/` and `fixtures/`"
        );
        let dir = self.root.join(rel);
        if self.vfs.exists(&dir.join(".mcp.json")) {
            self.problems.add(Case::McpJson, format!("{rel}/.mcp.json"));
        }
        let mut has_note = false;
        for entry in self.list(&dir)? {
            let child = format!("{rel}/{}", entry.name);
            let name_is = |n: &str| entry.name == n;
            if !entry.is_dir && (name_is(&note) || name_is(CHANGELOG_FILE)) {
                has_note |= name_is(&note);
                self.files.push(self.root.join(&child));
            } else if entry.is_dir
                && (SHARED_DIRS.contains(&entry.name.as_str())
                    || NEVER_SHIP_DIRS.iter().any(|(dir, _)| name_is(dir)))
            {
                self.free(&child)?;
            } else if entry.is_dir && kind_dir(&entry.name).is_some() {
                self.kind_folder(&child)?;
            } else if entry.is_dir && name_is(HOOKS_DIR) {
                self.hooks(&child)?;
            } else {
                self.unlisted(&child, &entry, &allowed);
            }
        }
        if !has_note {
            self.problems
                .add(Case::MissingNote, format!("{rel}/{note}"));
        }
        Ok(())
    }

    /// `plugins/<p>/hooks/`: `hooks.json` only.
    fn hooks(&mut self, rel: &str) -> Result<(), CoreError> {
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir && entry.name == HOOKS_FILE {
                self.files.push(self.root.join(&child));
            } else {
                self.unlisted(&child, &entry, &format!("`{HOOKS_FILE}` only"));
            }
        }
        Ok(())
    }

    /// `skills/`, `agents/` or `commands/`.
    fn kind_folder(&mut self, rel: &str) -> Result<(), CoreError> {
        let kind = rel.rsplit('/').next().and_then(kind_dir);
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir {
                if entry.name.ends_with(".md") {
                    self.problems.add(
                        Case::NotNamedEntry,
                        format!(
                            "{child} (a flat note: write `{}/{}`)",
                            entry.name.trim_end_matches(".md"),
                            entry.name
                        ),
                    );
                } else {
                    self.problems.add(
                        Case::UnlistedEntry,
                        format!("{child} (a kind folder holds component folders only)"),
                    );
                }
            } else if self.named(&child, &entry.name) {
                self.component(&child, &entry.name, kind == Some(EntityKind::Skill))?;
            }
        }
        Ok(())
    }

    /// `skills/<s>/`, `agents/<a>/` or `commands/<c>/`.
    fn component(&mut self, rel: &str, name: &str, skill: bool) -> Result<(), CoreError> {
        let note = format!("{name}.md");
        let allowed = if skill {
            format!(
                "`{note}`, `references/`, `scripts/`, `assets/`, `src/`, `docs/`, `tests/` and \
                 `fixtures/`"
            )
        } else {
            format!("`{note}`, `docs/`, `tests/` and `fixtures/`")
        };
        let mut has_note = false;
        let mut skill_md = false;
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            let shared = skill && SHARED_DIRS.contains(&entry.name.as_str());
            let never = NEVER_SHIP_DIRS
                .iter()
                .any(|(dir, skill_only)| entry.name == *dir && (skill || !skill_only));
            if !entry.is_dir && entry.name == note {
                has_note = true;
                self.files.push(self.root.join(&child));
            } else if !entry.is_dir && entry.name.eq_ignore_ascii_case("SKILL.md") {
                skill_md = true;
                self.problems.add(
                    Case::NotNamedEntry,
                    format!("{child} (write `{name}/{note}`)"),
                );
            } else if entry.is_dir && (shared || never) {
                self.free(&child)?;
            } else {
                self.unlisted(&child, &entry, &allowed);
            }
        }
        if !has_note && !skill_md {
            self.problems
                .add(Case::NotNamedEntry, format!("{rel}/ (it has no `{note}`)"));
        }
        Ok(())
    }
}

/// Whether `path` is there and is a folder (`dir`) or a file (`!dir`).
fn is_kind(vfs: &dyn Vfs, path: &Path, dir: bool) -> Result<bool, CoreError> {
    match vfs.metadata(path) {
        Ok(meta) => Ok(meta.is_dir == dir),
        Err(err) if err.is_not_found() || err.kind() == std::io::ErrorKind::NotADirectory => {
            Ok(false)
        }
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_reads_the_position_alone() {
        assert_eq!(
            classify("marketplaces/acme/acme.md"),
            Some(Position::Marketplace {
                marketplace: "acme".to_owned()
            })
        );
        assert_eq!(
            classify("marketplaces/acme/plugins/tools/tools.md"),
            Some(Position::Plugin {
                marketplace: "acme".to_owned(),
                plugin: "tools".to_owned()
            })
        );
        let skill = classify("marketplaces/acme/plugins/tools/agents/triage/triage.md").unwrap();
        assert_eq!(skill.kind(), EntityKind::Agent);
        assert_eq!(skill.name(), "triage");
        assert_eq!(skill.plugin(), Some("tools"));
        assert_eq!(skill.marketplace(), "acme");
        for rel in [
            "marketplaces/acme/plugins/tools/skills/review/references/x.md",
            "marketplaces/acme/plugins/tools/skills/review/other.md",
            "marketplaces/acme/plugins/tools/skills/review.md",
            "marketplaces/acme/plugins/tools/hooks/hooks/hooks.md",
            "marketplaces/acme/other.md",
            "notes/marketplaces/acme/acme.md",
        ] {
            assert_eq!(classify(rel), None, "{rel}");
        }
    }

    #[test]
    fn never_ship_folders_sit_directly_in_a_plugin_or_component() {
        for rel in [
            "marketplaces/m/plugins/p/docs/x.md",
            "marketplaces/m/plugins/p/src/lib.rs",
            "marketplaces/m/plugins/p/skills/s/tests/t.md",
            "marketplaces/m/plugins/p/commands/c/fixtures/f.json",
        ] {
            assert!(never_ships(rel), "{rel}");
        }
        for rel in [
            "marketplaces/m/plugins/p/skills/s/references/docs/x.md",
            "marketplaces/m/plugins/p/assets/tests/t.json",
            "marketplaces/m/docs/x.md",
            "marketplaces/m/plugins/p/docs",
        ] {
            assert!(!never_ships(rel), "{rel}");
        }
    }

    #[test]
    fn a_link_into_a_never_ship_folder_is_caught_unless_it_starts_there() {
        let to = "marketplaces/m/plugins/p/skills/s/docs/x.md";
        assert!(links_into_never_ship(
            "marketplaces/m/plugins/p/skills/s/s.md",
            to
        ));
        assert!(!links_into_never_ship(
            "marketplaces/m/plugins/p/skills/s/docs/y.md",
            to
        ));
        assert!(!links_into_never_ship(
            "marketplaces/m/plugins/p/skills/s/s.md",
            "marketplaces/m/plugins/p/skills/s/references/x.md"
        ));
    }
}
