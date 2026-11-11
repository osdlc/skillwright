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

/// Why `name` cannot name a `what` (a marketplace, plugin or component
/// folder), or `None`: a [`RESERVED_NAMES`] entry, or a name the Agent Skills
/// name rule refuses.
pub fn name_problem(name: &str, what: &str) -> Option<String> {
    if RESERVED_NAMES.contains(&name) {
        return Some(format!(
            "`{name}` is a structure folder's name and cannot be a {what}"
        ));
    }
    crate::discover::name_refusal(name, what)
}

/// The folder of a plugin that holds each component kind.
pub const KIND_DIRS: [(&str, EntityKind); 3] = [
    ("skills", EntityKind::Skill),
    ("agents", EntityKind::Agent),
    ("commands", EntityKind::Command),
];

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

/// One of the folders a plugin and a skill share out, each shipping under
/// its own name (spec section 3 rules 12 and 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SharedDir {
    /// `references/`: docs the agent reads. Its Markdown is compiled.
    References,
    /// `scripts/`: code the agent runs.
    Scripts,
    /// `assets/`: files used in output.
    Assets,
}

impl SharedDir {
    /// The shared folder named `name`.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "references" => Some(Self::References),
            "scripts" => Some(Self::Scripts),
            "assets" => Some(Self::Assets),
            _ => None,
        }
    }
}

/// What a file under `marketplaces/` is, by its place in the layout. The
/// walk decides it once; every copy set is built from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// `marketplaces/<m>/<m>.md`.
    MarketplaceNote,
    /// `plugins/<p>/<p>.md`.
    PluginNote,
    /// `plugins/<p>/CHANGELOG.md`.
    Changelog,
    /// `plugins/<p>/hooks/hooks.json`.
    HooksFile,
    /// `plugins/<p>/<kind>/<c>/<c>.md`.
    ComponentNote {
        /// The component's kind, from its kind folder.
        kind: EntityKind,
        /// The component folder.
        name: String,
    },
    /// A file in a plugin's or a skill's `references/`, `scripts/` or
    /// `assets/`, at any depth.
    Shared {
        /// The skill folder holding it, `None` for the plugin's own.
        skill: Option<String>,
        /// Which shared folder.
        dir: SharedDir,
        /// The path from the plugin or skill folder, such as
        /// `scripts/src/main.js`: where it lands in the pack or the skill.
        path: String,
    },
    /// A file in a never-ship folder: `src/`, `docs/`, `tests/` or
    /// `fixtures/` directly in a plugin or component folder.
    NeverShip {
        /// That folder, root-relative.
        folder: String,
    },
}

/// One file under `marketplaces/` and its place.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LayoutFile {
    /// The file, absolute. A symlink keeps its own path; reading it reads
    /// the target, so it ships as a copy.
    pub path: PathBuf,
    /// The path from the root, `/`-separated.
    pub rel: String,
    /// The marketplace folder it sits in.
    pub marketplace: String,
    /// The plugin folder it sits in, `None` for a marketplace's note.
    pub plugin: Option<String>,
    /// What it is.
    pub place: Place,
}

impl LayoutFile {
    /// The note's position, or `None` for a file that is not a note.
    pub fn position(&self) -> Option<Position> {
        let marketplace = self.marketplace.clone();
        let plugin = self.plugin.clone();
        match &self.place {
            Place::MarketplaceNote => Some(Position::Marketplace { marketplace }),
            Place::PluginNote => Some(Position::Plugin {
                marketplace,
                plugin: plugin?,
            }),
            Place::ComponentNote { kind, name } => Some(Position::Component {
                marketplace,
                plugin: plugin?,
                kind: *kind,
                name: name.clone(),
            }),
            _ => None,
        }
    }
}

/// The vault's layout: every file under `marketplaces/` a build reads, in
/// walk order, each with its place, and the marketplaces by folder.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Layout {
    /// The vault root.
    pub root: PathBuf,
    /// Every file under `marketplaces/`, in walk order. Dot-named entries
    /// and `node_modules/` are left out.
    pub files: Vec<LayoutFile>,
    /// The marketplace folders, sorted.
    pub marketplaces: Vec<String>,
}

impl Layout {
    /// The files of marketplace `marketplace`, in walk order.
    pub fn files_of<'a>(&'a self, marketplace: &'a str) -> impl Iterator<Item = &'a LayoutFile> {
        self.files
            .iter()
            .filter(move |file| file.marketplace == marketplace)
    }

    /// The file at the absolute `path`, if the walk read it.
    pub fn file(&self, path: &Path) -> Option<&LayoutFile> {
        self.files.iter().find(|file| file.path == path)
    }

    /// Whether a link from the file `from` to the file `to` goes into a
    /// never-ship folder. A file inside that folder links within it freely.
    pub fn links_into_never_ship(&self, from: &Path, to: &Path) -> bool {
        let folder = |path: &Path| match self.file(path).map(|file| &file.place) {
            Some(Place::NeverShip { folder }) => Some(folder.as_str()),
            _ => None,
        };
        folder(to).is_some_and(|target| folder(from) != Some(target))
    }
}

/// Why a link may not reach a file: the reason token, what is wrong and the
/// rule it breaks. A link and a symlink are refused with the same one
/// (spec section 4, containment rule 2).
pub(crate) struct Unreachable {
    pub(crate) token: &'static str,
    pub(crate) what: String,
    pub(crate) rule: &'static str,
}

impl Unreachable {
    /// A path that resolves outside marketplace `market` (containment rule 1).
    pub(crate) fn outside(market: &str) -> Self {
        Unreachable {
            token: "link-out-of-scope",
            what: format!("resolves outside `{MARKETPLACES_DIR}/{market}/`"),
            rule: "section 4 containment rule 1",
        }
    }

    /// A file no link may reach, such as a plugin's note.
    pub(crate) fn no_link_may_reach(rel: &str) -> Self {
        Unreachable {
            token: "link-out-of-scope",
            what: format!("names {rel}, which no link may reach"),
            rule: "section 4, scope of a link",
        }
    }

    /// The refusal naming `file`, the linking file or the symlink, and
    /// `written`, the link as written or the symlink's target (section 4
    /// rule 14).
    pub(crate) fn refuse(self, file: &str, written: &str) -> CoreError {
        link_refusal(self.token, file, written, self.what, self.rule)
    }
}

/// A link error naming `file`, the link as `written`, what is wrong and the
/// rule it broke (spec section 4 rule 14).
pub(crate) fn link_refusal(
    token: &str,
    file: &str,
    written: &str,
    what: impl std::fmt::Display,
    rule: &str,
) -> CoreError {
    CoreError::refused(token, format!("{file}: `{written}` {what} ({rule})"))
}

/// Why a link from plugin `plugin` of marketplace `market` may not reach
/// `to`, or `None` when it may (spec section 4, scope of a link).
pub(crate) fn reach(market: &str, plugin: Option<&str>, to: &LayoutFile) -> Option<Unreachable> {
    if to.marketplace != market {
        return Some(Unreachable::outside(market));
    }
    if to.plugin.as_deref() != plugin {
        return Some(Unreachable {
            token: "link-out-of-scope",
            what: format!("names a file in another plugin, {}", to.rel),
            rule: "section 4, scope of a link",
        });
    }
    match &to.place {
        Place::ComponentNote { .. } | Place::Shared { .. } => None,
        Place::NeverShip { .. } => Some(Unreachable {
            token: "excluded-link",
            what: format!("links to {}, which never ships", to.rel),
            rule: "section 4 rule 10",
        }),
        _ => Some(Unreachable::no_link_may_reach(&to.rel)),
    }
}

/// Refuses a file whose real path, with symlinks resolved, is one a link from
/// the file's own place may not reach (spec section 4, containment rule 2).
/// The refusal names the symlink on the way and its target as written. A
/// file in a never-ship folder is not checked: nothing ships it.
fn check_real_paths(vfs: &dyn Vfs, layout: &Layout) -> Result<(), CoreError> {
    let mut real_markets = BTreeMap::new();
    for file in &layout.files {
        if matches!(file.place, Place::NeverShip { .. }) {
            continue;
        }
        let market_dir = layout.root.join(MARKETPLACES_DIR).join(&file.marketplace);
        let real_market = match real_markets.get(&file.marketplace) {
            Some(real) => real,
            None => real_markets
                .entry(file.marketplace.clone())
                .or_insert(crate::guard::real_path(vfs, &market_dir)?),
        };
        let real = crate::guard::real_path(vfs, &file.path)?;
        let Ok(inside) = file.path.strip_prefix(&market_dir) else {
            continue;
        };
        if real == real_market.join(inside) {
            continue;
        }
        let real_rel = real.strip_prefix(real_market).ok().map(|inside| {
            format!(
                "{MARKETPLACES_DIR}/{}/{}",
                file.marketplace,
                crate::skill::forward_slashed(inside)
            )
        });
        let target = real_rel
            .as_deref()
            .and_then(|rel| layout.files.iter().find(|found| found.rel == rel));
        let why = match target {
            Some(target) => reach(&file.marketplace, file.plugin.as_deref(), target),
            None => Some(Unreachable::no_link_may_reach(
                &real_rel.unwrap_or_else(|| real.display().to_string()),
            )),
        };
        if let Some(why) = why {
            let link = file
                .path
                .ancestors()
                .take_while(|path| path.starts_with(&market_dir))
                .find(|path| vfs.is_symlink(path))
                .unwrap_or(&file.path);
            let written = vfs.read_link(link).map_or_else(
                |_| real.display().to_string(),
                |to| to.display().to_string(),
            );
            let rel = link.strip_prefix(&layout.root).unwrap_or(link);
            return Err(why.refuse(&crate::skill::forward_slashed(rel), &written));
        }
    }
    Ok(())
}

/// One kind of layout problem, in the order a report lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Case {
    /// Case 2.
    RefusedRootKey,
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
            Self::RefusedRootKey => {
                "the root file has `workspace`, `name`, or a key that is not a shared default"
            }
            Self::MissingNote => "a marketplace or plugin folder has no note named for it",
            Self::KindFolderOutOfPlace => {
                "a `skills/`, `agents/`, `commands/`, `hooks/` or `plugins/` folder is out of place"
            }
            Self::NestedMarketplaces => "a `marketplaces/` folder is inside `marketplaces/`",
            Self::NotNamedEntry => {
                "a component's note is not `<name>/<name>.md` (no flat notes, no `SKILL.md`)"
            }
            Self::RefusedBuildKey => {
                "the root file has a removed key, a `build:` key or `packs` in a harness block; \
                 delete it"
            }
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
/// in spec section 3, and the keys the root file refuses. Each file is given
/// its [`Place`].
///
/// # Errors
///
/// * `refused: layout` — the root is not a vault root, or the vault breaks the
///   layout. Every problem is in the one message, each case once with every
///   path it applies to.
/// * `refused: link-out-of-scope` or `excluded-link` — a file whose real
///   path, with symlinks resolved, a link from its place may not reach:
///   outside its marketplace folder, in another plugin, in a never-ship
///   folder (spec section 4, containment rule 2). The refusal is the one a
///   link spelling that target gets, naming the symlink and its target.
/// * `refused: unsafe-path` — a symlink to a folder the walk is inside, one
///   whose target cannot be read, or one outside `marketplaces/<m>/`; or a
///   file whose real path the host cannot tell.
/// * [`CoreError::Io`] — a folder that cannot be listed.
pub fn read_layout(vfs: &dyn Vfs, root: &Path) -> Result<Layout, CoreError> {
    let root = &crate::vfs::normalize(root);
    check_vault_root(vfs, root)?;
    let mut walk = Walk {
        vfs,
        root,
        files: Vec::new(),
        problems: Problems::default(),
        marketplace: None,
        plugin: None,
    };
    // Root entries other than the three the vault uses are left alone and not
    // read (spec section 3), so a symlink among them is not checked: only its
    // name and kind matter.
    for entry in vfs.read_dir(root)? {
        let path = root.join(&entry.name);
        let is_dir = if !vfs.is_symlink(&path) {
            entry.is_dir
        } else if [ROOT_FILE, MARKETPLACES_DIR, DIST_DIR].contains(&entry.name.as_str()) {
            walk.linked(&path)?
        } else {
            vfs.canonicalize(&path)
                .is_ok_and(|target| vfs.metadata(&target).is_ok_and(|m| m.is_dir))
        };
        if is_dir && (entry.name == PLUGINS_DIR || kind_or_hooks(&entry.name)) {
            walk.problems
                .add(Case::KindFolderOutOfPlace, format!("{}/", entry.name));
        }
    }
    if let Ok(bytes) = vfs.read_file(&root.join(ROOT_FILE)) {
        let text = String::from_utf8_lossy(&bytes);
        let (keys, build) = crate::settings::refused_root_file_keys(&text);
        for (key, why) in keys {
            walk.problems.add(
                Case::RefusedRootKey,
                format!("`{key}` in {ROOT_FILE}: {why}"),
            );
        }
        for key in build {
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
    let layout = Layout {
        root: root.to_path_buf(),
        files: walk.files,
        marketplaces,
    };
    check_real_paths(vfs, &layout)?;
    Ok(layout)
}

/// Whether `name` is a component kind folder or `hooks`.
fn kind_or_hooks(name: &str) -> bool {
    name == HOOKS_DIR || kind_dir(name).is_some()
}

/// One folder entry as the walk sees it. A symlink to a folder in scope is
/// a folder.
struct Entry {
    name: String,
    is_dir: bool,
}

/// What a folder of free layout holds: shared files, or never-ship ones.
enum Free {
    /// A `references/`, `scripts/` or `assets/` folder of the plugin or of
    /// `skill`, which sits at `base`.
    Shared {
        skill: Option<String>,
        dir: SharedDir,
        base: String,
    },
    /// The never-ship folder itself, root-relative.
    NeverShip(String),
}

/// The walk's state: the files read, the problems found, and the
/// marketplace and plugin being walked.
struct Walk<'a> {
    vfs: &'a dyn Vfs,
    root: &'a Path,
    files: Vec<LayoutFile>,
    problems: Problems,
    marketplace: Option<String>,
    plugin: Option<String>,
}

impl Walk<'_> {
    /// The entries of `dir`, sorted, less dot-named ones and `node_modules`.
    /// A symlink is checked by [`Self::linked`].
    fn list(&self, dir: &Path) -> Result<Vec<Entry>, CoreError> {
        let mut entries = self.vfs.read_dir(dir)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut out = Vec::new();
        for entry in entries {
            if entry.name.starts_with('.') || entry.name == "node_modules" {
                continue;
            }
            let path = dir.join(&entry.name);
            let is_dir = if self.vfs.is_symlink(&path) {
                self.linked(&path)?
            } else {
                entry.is_dir
            };
            out.push(Entry {
                name: entry.name,
                is_dir,
            });
        }
        Ok(out)
    }

    /// Accepts the symlink at `path` when its real target lies inside the
    /// marketplace folder being walked (spec section 4, containment rule 2),
    /// and says whether that target is a folder. Where in the marketplace it
    /// may lead is [`check_real_paths`]'s to check. A pack holds no symlinks:
    /// reading the link reads its target, so it ships as a copy.
    fn linked(&self, path: &Path) -> Result<bool, CoreError> {
        let refuse = |why: String| {
            CoreError::refused(
                "unsafe-path",
                format!(
                    "{}: a symlink {why}. Point it inside its marketplace folder, or replace it \
                     with the real file or folder",
                    path.display()
                ),
            )
        };
        let Some(market) = &self.marketplace else {
            return Err(refuse(format!(
                "outside a marketplace folder (`{MARKETPLACES_DIR}/<m>/`)"
            )));
        };
        let market_dir = self.root.join(MARKETPLACES_DIR).join(market);
        let scope = self.vfs.canonicalize(&market_dir)?;
        let Ok(target) = self.vfs.canonicalize(path) else {
            return Err(refuse("whose real target cannot be read".to_owned()));
        };
        if !target.starts_with(&scope) {
            // Refused as a link spelling the target is (containment rule 2).
            let written = self.vfs.read_link(path).unwrap_or_else(|_| target.clone());
            let rel = path.strip_prefix(self.root).unwrap_or(path);
            return Err(Unreachable::outside(market).refuse(
                &crate::skill::forward_slashed(rel),
                &written.display().to_string(),
            ));
        }
        let is_dir = self.vfs.metadata(&target)?.is_dir;
        if !is_dir {
            return Ok(false);
        }
        // Every folder the walk is inside, from the link's holder up to the
        // marketplace folder. A target that holds any of them leads back into
        // the descent, and the walk would never end.
        for folder in path
            .ancestors()
            .skip(1)
            .take_while(|folder| folder.starts_with(&market_dir))
        {
            if self.vfs.canonicalize(folder)?.starts_with(&target) {
                return Err(refuse(
                    "to a folder the walk is inside, so it would never end".to_owned(),
                ));
            }
        }
        Ok(true)
    }

    /// Records the file `rel` at `place`.
    fn push(&mut self, rel: String, place: Place) {
        self.files.push(LayoutFile {
            path: self.root.join(&rel),
            rel,
            marketplace: self.marketplace.clone().unwrap_or_default(),
            plugin: self.plugin.clone(),
            place,
        });
    }

    /// Whether the folder `rel` may take the name `name`; a problem is added
    /// when it may not.
    fn named(&mut self, rel: &str, name: &str) -> bool {
        if name_problem(name, "name").is_some() {
            self.problems.add(Case::BadName, format!("{rel}/"));
            return false;
        }
        true
    }

    /// Reports a structure folder met where none belongs, or descends into an
    /// ordinary one. Used inside shared and never-ship folders, where any
    /// layout of plain folders is allowed.
    fn free(&mut self, rel: &str, what: &Free) -> Result<(), CoreError> {
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir {
                let place = match what {
                    Free::Shared { skill, dir, base } => Place::Shared {
                        skill: skill.clone(),
                        dir: *dir,
                        path: child[base.len() + 1..].to_owned(),
                    },
                    Free::NeverShip(folder) => Place::NeverShip {
                        folder: folder.clone(),
                    },
                };
                self.push(child, place);
            } else if entry.name == MARKETPLACES_DIR {
                self.problems
                    .add(Case::NestedMarketplaces, format!("{child}/"));
            } else if entry.name == PLUGINS_DIR || kind_or_hooks(&entry.name) {
                self.problems
                    .add(Case::KindFolderOutOfPlace, format!("{child}/"));
            } else {
                self.free(&child, what)?;
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
        self.marketplace = Some(name.to_owned());
        self.plugin = None;
        let note = format!("{name}.md");
        let allowed = format!("`{note}` and `{PLUGINS_DIR}/`");
        let mut has_note = false;
        let mut old_note = false;
        for entry in self.list(&self.root.join(rel))? {
            let child = format!("{rel}/{}", entry.name);
            if !entry.is_dir && entry.name == note {
                has_note = true;
                self.push(child, Place::MarketplaceNote);
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
        self.marketplace = None;
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
                self.plugin = Some(entry.name.clone());
                self.plugin(&child, &entry.name)?;
                self.plugin = None;
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
            let shared = SharedDir::named(&entry.name).filter(|_| entry.is_dir);
            if !entry.is_dir && name_is(&note) {
                has_note = true;
                self.push(child, Place::PluginNote);
            } else if !entry.is_dir && name_is(CHANGELOG_FILE) {
                self.push(child, Place::Changelog);
            } else if let Some(dir) = shared {
                let what = Free::Shared {
                    skill: None,
                    dir,
                    base: rel.to_owned(),
                };
                self.free(&child, &what)?;
            } else if entry.is_dir && NEVER_SHIP_DIRS.iter().any(|(dir, _)| name_is(dir)) {
                self.free(&child, &Free::NeverShip(child.clone()))?;
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
                self.push(child, Place::HooksFile);
            } else {
                self.unlisted(&child, &entry, &format!("`{HOOKS_FILE}` only"));
            }
        }
        Ok(())
    }

    /// `skills/`, `agents/` or `commands/`.
    fn kind_folder(&mut self, rel: &str) -> Result<(), CoreError> {
        let Some(kind) = rel.rsplit('/').next().and_then(kind_dir) else {
            return Ok(());
        };
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
                self.component(&child, &entry.name, kind)?;
            }
        }
        Ok(())
    }

    /// `skills/<s>/`, `agents/<a>/` or `commands/<c>/`.
    fn component(&mut self, rel: &str, name: &str, kind: EntityKind) -> Result<(), CoreError> {
        let skill = kind == EntityKind::Skill;
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
            let shared = SharedDir::named(&entry.name).filter(|_| skill && entry.is_dir);
            let never = NEVER_SHIP_DIRS
                .iter()
                .any(|(dir, skill_only)| entry.name == *dir && (skill || !skill_only));
            if !entry.is_dir && entry.name == note {
                has_note = true;
                let place = Place::ComponentNote {
                    kind,
                    name: name.to_owned(),
                };
                self.push(child, place);
            } else if !entry.is_dir && entry.name.eq_ignore_ascii_case("SKILL.md") {
                skill_md = true;
                self.problems.add(
                    Case::NotNamedEntry,
                    format!("{child} (write `{name}/{note}`)"),
                );
            } else if let Some(dir) = shared {
                let what = Free::Shared {
                    skill: Some(name.to_owned()),
                    dir,
                    base: rel.to_owned(),
                };
                self.free(&child, &what)?;
            } else if entry.is_dir && never {
                self.free(&child, &Free::NeverShip(child.clone()))?;
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
    fn name_problem_refuses_a_structure_name_and_a_bad_slug() {
        assert!(name_problem("tools", "plugin name").is_none());
        let reserved = name_problem("skills", "plugin name").unwrap();
        assert!(reserved.contains("structure folder"), "{reserved}");
        assert!(name_problem("Tools", "plugin name").is_some());
        assert!(name_problem("", "plugin name").is_some());
    }

    /// A vault at `/v` holding `files` under plugin `tools` of `acme`.
    fn vault(files: &[&str]) -> crate::vfs::MemVfs {
        let vfs = crate::vfs::MemVfs::new();
        let plugin = "/v/marketplaces/acme/plugins/tools";
        let mut all = vec![
            "/v/skillwright.md".to_owned(),
            "/v/marketplaces/acme/acme.md".to_owned(),
            format!("{plugin}/tools.md"),
        ];
        all.extend(files.iter().map(|f| format!("{plugin}/{f}")));
        for path in all {
            let path = Path::new(&path);
            vfs.create_dir_all(path.parent().unwrap()).unwrap();
            vfs.write_file(path, b"---\n---\n").unwrap();
        }
        vfs
    }

    /// Each file's place, by its path from the plugin folder.
    fn places(files: &[&str]) -> BTreeMap<String, Place> {
        let layout = read_layout(&vault(files), Path::new("/v")).unwrap();
        layout
            .files
            .into_iter()
            .map(|file| {
                let rel = file
                    .rel
                    .strip_prefix("marketplaces/acme/plugins/tools/")
                    .unwrap_or(&file.rel)
                    .to_owned();
                (rel, file.place)
            })
            .collect()
    }

    #[test]
    fn each_file_is_given_its_place_once() {
        let got = places(&[
            "CHANGELOG.md",
            "assets/logo.png",
            "scripts/src/main.js",
            "src/cli.ts",
            "hooks/hooks.json",
            "skills/review/review.md",
            "skills/review/references/docs/x.md",
            "skills/review/docs/x.md",
            "agents/triage/triage.md",
            "agents/triage/tests/t.md",
        ]);
        let shared = |skill: Option<&str>, dir, path: &str| Place::Shared {
            skill: skill.map(str::to_owned),
            dir,
            path: path.to_owned(),
        };
        let never = |folder: &str| Place::NeverShip {
            folder: format!("marketplaces/acme/plugins/tools/{folder}"),
        };
        let want: BTreeMap<String, Place> = [
            ("marketplaces/acme/acme.md", Place::MarketplaceNote),
            ("tools.md", Place::PluginNote),
            ("CHANGELOG.md", Place::Changelog),
            (
                "assets/logo.png",
                shared(None, SharedDir::Assets, "assets/logo.png"),
            ),
            (
                "scripts/src/main.js",
                shared(None, SharedDir::Scripts, "scripts/src/main.js"),
            ),
            ("src/cli.ts", never("src")),
            ("hooks/hooks.json", Place::HooksFile),
            (
                "skills/review/review.md",
                Place::ComponentNote {
                    kind: EntityKind::Skill,
                    name: "review".to_owned(),
                },
            ),
            (
                "skills/review/references/docs/x.md",
                shared(
                    Some("review"),
                    SharedDir::References,
                    "references/docs/x.md",
                ),
            ),
            ("skills/review/docs/x.md", never("skills/review/docs")),
            (
                "agents/triage/triage.md",
                Place::ComponentNote {
                    kind: EntityKind::Agent,
                    name: "triage".to_owned(),
                },
            ),
            ("agents/triage/tests/t.md", never("agents/triage/tests")),
        ]
        .into_iter()
        .map(|(rel, place)| (rel.to_owned(), place))
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn a_note_position_comes_from_its_place() {
        let layout = read_layout(&vault(&["agents/triage/triage.md"]), Path::new("/v")).unwrap();
        let agent = layout
            .files
            .iter()
            .find_map(|file| file.position().filter(|p| p.name() == "triage"))
            .unwrap();
        assert_eq!(agent.kind(), EntityKind::Agent);
        assert_eq!(agent.plugin(), Some("tools"));
        assert_eq!(agent.marketplace(), "acme");
    }

    #[test]
    fn a_link_into_a_never_ship_folder_is_caught_unless_it_starts_there() {
        let layout = read_layout(
            &vault(&[
                "skills/s/s.md",
                "skills/s/docs/x.md",
                "skills/s/docs/y.md",
                "skills/s/references/x.md",
            ]),
            Path::new("/v"),
        )
        .unwrap();
        let at = |rel: &str| Path::new("/v/marketplaces/acme/plugins/tools").join(rel);
        let to = at("skills/s/docs/x.md");
        assert!(layout.links_into_never_ship(&at("skills/s/s.md"), &to));
        assert!(!layout.links_into_never_ship(&at("skills/s/docs/y.md"), &to));
        assert!(
            !layout.links_into_never_ship(&at("skills/s/s.md"), &at("skills/s/references/x.md"))
        );
    }
}
