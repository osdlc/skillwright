//! The decision plane for authoring a vault: where a note goes, what it is
//! called, what it would collide with, and what its frontmatter says.
//!
//! Every function here decides and returns an [`AuthoringOp`]. None of them
//! writes: the host executes the plan, because only the host has the
//! vocabulary a write needs — content-addressed reads, etags, a trash folder,
//! a link-rewriting rename. This module has a `&dyn Vfs` and nothing else, so
//! it compiles for `wasm32-unknown-unknown` and answers the same way on every
//! host.
//!
//! The placement rules, the naming rules and the refusals are the ones the
//! vault's own discovery scan enforces — asked here, before the note exists,
//! by running that scan over a scratch [`MemVfs`] holding exactly the one
//! composed file ([`scratch_scan`]). Restating the rules would be a second
//! answer to the same question; running them cannot drift.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use yaml_splice::edit_frontmatter;
use yaml_splice::parse::parse_mapping;

use crate::discover::{
    DiscoveredComponent, KIND_DIRS, discover_components, folder_kind, folder_placement,
    name_refusal, slugify,
};
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::namespace::Namespace;
use crate::paths::WORKSPACE_DIR;
use crate::source_view::{SourceView, notes_over};
use crate::vault::read_vault_skill;
use crate::vfs::{MemVfs, Vfs};

/// Where an authored note sits, as the author named it. Every level is
/// optional and an empty string is not an answer — see `named`.
///
/// The neutral shape of a host's own placement wire type: the levels a path
/// is derived from, and nothing a transport cares about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Placement {
    /// The marketplace folder, when named.
    pub marketplace: Option<String>,
    /// The plugin folder, when named.
    pub plugin: Option<String>,
    /// The kind, spelled by name. Mutually exclusive with [`Self::folder`].
    pub kind: Option<String>,
    /// A plain folder of the author's own, for a note that has no kind.
    pub folder: Option<String>,
}

/// One frontmatter key set to a value, or removed when `value` is `None`.
///
/// No serde: [`yaml_splice::Value`] is not serde-aware, and teaching it to be
/// would put a wire concern in a pure YAML editor.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEdit {
    /// The frontmatter key.
    pub key: String,
    /// The new value, or `None` to remove the key.
    pub value: Option<yaml_splice::Value>,
}

impl FieldEdit {
    /// An edit that sets `key` to `value`.
    pub fn set(key: impl Into<String>, value: yaml_splice::Value) -> Self {
        Self {
            key: key.into(),
            value: Some(value),
        }
    }

    /// An edit that removes `key`.
    pub fn remove(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: None,
        }
    }
}

/// What a host has to do to carry out one authoring decision.
///
/// Paths are vault-relative and `/`-separated, the same spelling the vault's
/// scan and the notes' own wikilinks use. Nothing here names an etag, a trash
/// entry or a CAS token: those are the host's, and a plan that carried them
/// could not be computed in a browser.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AuthoringOp {
    /// Write `bytes` at `path`, then make each `scaffold` directory.
    Create {
        /// Vault-relative path of the new note.
        path: String,
        /// Full text of the new note.
        bytes: String,
        /// Directories to create after the write.
        scaffold: Vec<String>,
    },
    /// Move `from` to `to` — the whole bundle folder when `bundle` — then
    /// apply `frontmatter` to the note at `to`.
    Rename {
        /// Current vault-relative path.
        from: String,
        /// New vault-relative path.
        to: String,
        /// Whether the note's whole bundle folder moves.
        bundle: bool,
        /// Frontmatter edits to apply to the note at `to`.
        frontmatter: Vec<FieldEdit>,
    },
    /// The same move, to a placement rather than to a name. The folder names
    /// the plugin, so no frontmatter changes.
    Move {
        /// Current vault-relative path.
        from: String,
        /// New vault-relative path.
        to: String,
        /// Whether the note's whole bundle folder moves.
        bundle: bool,
    },
    /// Trash every path, into `trash_dir`.
    Delete {
        /// Vault-relative paths to trash.
        paths: Vec<String>,
        /// The vault-relative folder the paths move into.
        trash_dir: String,
    },
    /// Move into another kind's folder, restate the kind, and make the member
    /// folders a container kind holds.
    ConvertKind {
        /// Current vault-relative path.
        from: String,
        /// New vault-relative path.
        to: String,
        /// Whether the note's whole bundle folder moves.
        bundle: bool,
        /// Frontmatter edits to apply to the note at `to`.
        frontmatter: Vec<FieldEdit>,
        /// Member directories to create.
        scaffold: Vec<String>,
    },
}

/// The vault-relative folder a delete moves its paths into.
pub const TRASH_DIR: &str = ".trash";

/// The vault-relative folder that holds plugins.
pub const SKILLS_ROOT: &str = "plugins";

/// Frontmatter keys no conversion removes: both are the note's identity, and
/// every kind carries them.
const UNDROPPABLE_KEYS: &[&str] = &["name", "description"];

/// Why a name with nothing sluggable in it is refused, stated once. `what` is
/// the level being named — a component name, a plugin name, a note name — so
/// the refusal says which one had nothing to slug.
pub fn nameless(what: &str) -> String {
    format!("a {what} needs at least one letter or digit, so a filename can be slugged from it")
}

/// Refusals a container placement can produce, spelled once so the prose does
/// not pick up the indentation a wrapped string literal would.
const MARKETPLACE_NESTED: &str =
    "a marketplace is not authored inside a marketplace — its own name is the folder it makes";

const PLUGIN_NESTED: &str =
    "a plugin is not authored inside a plugin — its own name is the folder it makes";

/// The scratch vault [`scratch_scan`] builds its one file under. Named so a
/// refusal's prose can have it stripped back off: it is not a path anybody
/// has, and showing it to an author would be showing them a lie.
pub const SCRATCH_ROOT: &str = "/vault";

/// A placement level as the author named it, or `None` where they named
/// nothing: an empty string is not an answer.
fn named(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

/// The vault-relative folder a created note is written into.
///
/// Every placement level names one folder, so each must be a single non-empty
/// segment. The kind maps through [`KIND_DIRS`]; container kinds take
/// `container_dir`, which names the folder the note itself makes.
pub fn new_note_dir(placement: Option<&Placement>) -> Result<String, String> {
    let Some(placement) = placement else {
        return Ok("skills".to_owned());
    };
    let segment = |level: &str, name: Option<&str>| -> Result<Option<String>, String> {
        match name.map(str::trim) {
            None => Ok(None),
            Some("") => Err(format!("the {level} is named, but named nothing")),
            Some(name) if name.contains(['/', '\\']) || name == "." || name == ".." => {
                Err(format!("`{name}` is not one folder name for the {level}"))
            }
            Some(name) => Ok(Some(name.to_owned())),
        }
    };
    // A kind and a plain folder are two different derivations, so naming both
    // is not a placement with a preference — it is two placements, and picking
    // one would write the note somewhere the author did not ask for.
    let kind = match (named(&placement.kind), named(&placement.folder)) {
        (Some(_), Some(_)) => {
            return Err(
                "a placement names a kind or a plain folder, never both — a plain note has no kind"
                    .to_owned(),
            );
        }
        (None, None) => {
            return Err("the placement names neither a kind nor a folder".to_owned());
        }
        (None, Some(folder)) => return plain_note_dir(folder, placement, &segment),
        (Some(kind), None) => kind,
    };
    let kind = kind
        .parse::<EntityKind>()
        .map_err(|e| format!("the placement's kind: {e}"))?;
    // A container is authored as its folder's own note rather than placed
    // inside a kind folder, so its directory is the level it will sit at and
    // the folder is named for it by the bundling rule.
    if kind.holds_components() {
        return container_dir(kind, placement, &segment);
    }

    let Some((kind_dir, _)) = KIND_DIRS.iter().find(|(_, k)| *k == kind) else {
        return Err(format!(
            "no folder holds a {} — the kind directories are {}",
            kind.as_str(),
            KIND_DIRS
                .iter()
                .map(|(dir, _)| *dir)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let mut dir = placement_prefix(placement, &segment)?;
    dir.push_str(kind_dir);
    Ok(dir)
}

/// The marketplace and plugin levels of a placement, as the folder prefix
/// every derived directory starts with. Empty when the placement names
/// neither.
fn placement_prefix(
    placement: &Placement,
    segment: &impl Fn(&str, Option<&str>) -> Result<Option<String>, String>,
) -> Result<String, String> {
    let mut dir = String::new();
    if let Some(market) = segment("marketplace", placement.marketplace.as_deref())? {
        dir.push_str(WORKSPACE_DIR);
        dir.push('/');
        dir.push_str(&market);
        dir.push('/');
    }
    if let Some(plugin) = segment("plugin", placement.plugin.as_deref())? {
        dir.push_str(SKILLS_ROOT);
        dir.push('/');
        dir.push_str(&plugin);
        dir.push('/');
    }
    Ok(dir)
}

/// Where a plain note goes: the same levels a component's placement names,
/// then a folder of the author's own instead of a kind folder.
///
/// The folder is filing and nothing else. Discovery reads a note's kind off
/// the kind folder above it, so a plain note has to sit outside every one of
/// them — and a note with neither `name` nor `description` is skipped by the
/// scan wherever it sits, which is what makes `roles/` a peer resource rather than a component
/// the scan then refuses to classify. Naming a kind folder here is refused
/// rather than allowed-and-ignored: the author asking for `skills/` means
/// something by it, and it is not what would happen.
fn plain_note_dir(
    folder: &str,
    placement: &Placement,
    segment: &impl Fn(&str, Option<&str>) -> Result<Option<String>, String>,
) -> Result<String, String> {
    let folder = segment("folder", Some(folder))?
        .ok_or_else(|| "the folder is named, but named nothing".to_owned())?;
    if let Some((dir, kind)) = KIND_DIRS.iter().find(|(dir, _)| *dir == folder) {
        return Err(format!(
            "`{dir}` is the folder a {} lands in — a plain note names a folder of its own",
            kind.as_str()
        ));
    }
    let mut dir = placement_prefix(placement, segment)?;
    dir.push_str(&folder);
    Ok(dir)
}

/// Where a container note's own folder goes.
///
/// A marketplace sits directly under the marketplaces root; a plugin sits
/// under one marketplace's plugins root, so it needs the marketplace named and
/// says so rather than guessing at one. Neither may name a level below itself:
/// a marketplace inside a plugin is not a shape the reader can resolve, and
/// accepting it would write a note nothing ever finds.
fn container_dir(
    kind: EntityKind,
    placement: &Placement,
    segment: &impl Fn(&str, Option<&str>) -> Result<Option<String>, String>,
) -> Result<String, String> {
    match kind {
        EntityKind::Marketplace => {
            if segment("plugin", placement.plugin.as_deref())?.is_some() {
                return Err("a marketplace is not authored inside a plugin".to_owned());
            }
            if segment("marketplace", placement.marketplace.as_deref())?.is_some() {
                return Err(MARKETPLACE_NESTED.to_owned());
            }
            Ok(WORKSPACE_DIR.to_owned())
        }
        EntityKind::Plugin => {
            if segment("plugin", placement.plugin.as_deref())?.is_some() {
                return Err(PLUGIN_NESTED.to_owned());
            }
            match segment("marketplace", placement.marketplace.as_deref())? {
                Some(market) => Ok(format!("{WORKSPACE_DIR}/{market}/{SKILLS_ROOT}")),
                None => Ok(SKILLS_ROOT.to_owned()),
            }
        }
        other => Err(format!("{} holds no components", other.as_str())),
    }
}

/// Why `slug` cannot be a `what`: nothing sluggable in the name it came from,
/// or a slug the skills-spec grammar refuses. `None` means it is usable.
///
/// A rename composes no note, so the cap is checked here too — otherwise an
/// over-long name refuses the vault's next scan instead of the rename.
pub fn slug_refusal(slug: &str, what: &str) -> Option<String> {
    if slug.is_empty() {
        return Some(nameless(what));
    }
    name_refusal(slug, what)
}

/// The note a create would author: its derived path, and the edits as they
/// would actually be written.
///
/// The path is derived rather than accepted because the scan requires the
/// filename to slugify to the note's `name` (`<slug>.md`, or
/// `<slug>/<slug>.md` for a bundle). Slugging the name and building both the
/// path and the `name` edit from that one value makes the two agree by
/// construction — there is no second spelling to keep in sync, in this module
/// or in any client. The directory above the filename comes from the
/// placement ([`new_note_dir`]).
///
/// `Err` is the refusal, in the words a draft shows while the author types:
/// a name with nothing to slug, or a placement naming what no folder can be.
pub fn derive_create(
    placement: Option<&Placement>,
    name: &str,
    bundled: bool,
    fields: &[FieldEdit],
) -> Result<(String, Vec<FieldEdit>), String> {
    let slug = slugify(name);
    if let Some(refusal) = slug_refusal(&slug, "component name") {
        return Err(refusal);
    }
    let dir = new_note_dir(placement)?;
    // A container note names its folder, so it is bundled whatever the request
    // said: `marketplaces/house.md` is a note the reader would find as a loose
    // component rather than as a marketplace.
    let kind = placement_kind(placement);
    let bundled = bundled || kind.is_some_and(EntityKind::holds_components);
    let path = if bundled {
        let entry = kind.map_or_else(
            || format!("{slug}.md"),
            |kind| crate::discover::entry_file(kind, &slug),
        );
        format!("{dir}/{slug}/{entry}")
    } else {
        format!("{dir}/{slug}.md")
    };
    Ok((path, with_name(fields, &slug)))
}

/// The directory a component bundle's entry note owns, or `None` for a note
/// that travels alone. Under a kind folder the scan's own shape rule decides:
/// a note directly inside one (`skills/skills.md`, `skills/SKILL.md`) is a
/// lone note, and so is a path the rule gives no shape (`plugins/p/SKILL.md`).
/// Outside every kind folder, a note shaped `<dir>/<dir>.md` or
/// `<dir>/SKILL.md` (extension case-insensitive) owns `<dir>`. The shape and
/// nothing else: the plans here decide what moves, never how a host moves it.
/// [`crate::discover::path_role`] answers the same question for
/// `skillwright.md` paths under kind folders.
pub fn bundle_dir(entry: &str) -> Option<&str> {
    let (dir, file) = entry.rsplit_once('/')?;
    let bundled = match crate::discover::is_bundle_entry(entry) {
        Some(bundled) => bundled,
        None if crate::discover::under_a_kind_folder(Path::new(""), Path::new(dir)) => false,
        None => {
            let dir_name = dir.rsplit('/').next().unwrap_or(dir);
            crate::discover::is_skill_entry(file)
                || crate::discover::md_stem(file) == Some(dir_name)
        }
    };
    bundled.then_some(dir)
}

/// The entry file a bundle named `name` gets as a `to_kind`: `SKILL.md` stays
/// `SKILL.md` while it is still a skill, and anything else is `<name>.md` —
/// only a skill may use `SKILL.md`, so a skill converted to another kind must
/// take its folder's name or discovery cannot read it back.
fn bundle_entry_file(entry: &str, name: &str, to_kind: EntityKind) -> String {
    let file = entry.rsplit('/').next().unwrap_or(entry);
    if to_kind == EntityKind::Skill && crate::discover::is_skill_entry(file) {
        file.to_owned()
    } else {
        crate::discover::entry_file(to_kind, name)
    }
}

/// The entry's own row in the vault's current scan, when discovery lists it.
/// `None` for a note discovery skips or cannot read — none of the ops below
/// can safely reason about a component they cannot resolve.
pub fn find_component(
    vfs: &dyn Vfs,
    vault_dir: &Path,
    entry: &str,
    ns: &Namespace,
) -> Option<DiscoveredComponent> {
    discover_components(vfs, vault_dir, None, ns)
        .ok()?
        .components
        .into_iter()
        .find(|c| c.vault_relative_path == entry)
}

/// The destination a rename to `slug` implies: the same directory for a
/// single-file component, or `<parent>/<slug>/<slug>.md` for a bundle — its
/// directory renamed to match, exactly as its own file must. A `SKILL.md`
/// entry keeps its filename; only its folder is renamed.
pub fn rename_destination(entry: &str, slug: &str) -> String {
    match bundle_dir(entry) {
        Some(dir) => {
            // A rename keeps the kind, and only a skill has a `SKILL.md`.
            let market = dir.rsplit_once('/').map(|(parent, _)| parent) == Some(WORKSPACE_DIR);
            let kind = if market {
                EntityKind::Marketplace
            } else {
                EntityKind::Skill
            };
            let file = bundle_entry_file(entry, slug, kind);
            match dir.rsplit_once('/') {
                Some((parent, _)) => format!("{parent}/{slug}/{file}"),
                None => format!("{slug}/{file}"),
            }
        }
        None => match entry.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{slug}.md"),
            None => format!("{slug}.md"),
        },
    }
}

/// The destination a move to `placement` implies, derived the way
/// [`derive_create`] derives a create's path: the kind folder comes from
/// `kind` — the component's OWN resolved kind, since a move cannot change
/// what a note is — never from the placement. A bundle keeps its own folder
/// name; a lone note keeps its own filename.
pub fn move_destination(
    entry: &str,
    kind: EntityKind,
    marketplace: Option<&str>,
    plugin: Option<&str>,
) -> Result<String, String> {
    let dir = new_note_dir(Some(&Placement {
        marketplace: marketplace.map(str::to_owned),
        plugin: plugin.map(str::to_owned),
        kind: Some(kind.as_str().to_owned()),
        folder: None,
    }))?;
    Ok(match bundle_dir(entry) {
        Some(from_dir) => {
            let name = from_dir.rsplit('/').next().unwrap_or(from_dir);
            format!("{dir}/{name}/{}", bundle_entry_file(entry, name, kind))
        }
        None => {
            let leaf = entry.rsplit('/').next().unwrap_or(entry);
            format!("{dir}/{leaf}")
        }
    })
}

/// Every markdown note still inside a container's own folder besides its
/// entry note — what [`plan_delete_component`] refuses to discard silently.
/// Walked directly rather than through [`discover_components`]: this only
/// needs to know "is anything else in here", not each member's resolved key.
/// The vault's build output is hidden ([`crate::notes_view`]).
pub fn container_members(
    vfs: &dyn Vfs,
    vault_dir: &Path,
    dir: &str,
    entry: &str,
    ns: &Namespace,
) -> Vec<String> {
    let notes = notes_over(vfs, vault_dir, None, None, ns);
    let mut members: Vec<String> =
        crate::walk::readable_vault_files(&notes, &vault_dir.join(dir), vault_dir)
            .into_iter()
            .filter(|abs| {
                abs.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            })
            .filter_map(|abs| {
                let rel = abs.strip_prefix(vault_dir).ok()?;
                Some(rel.to_string_lossy().replace('\\', "/"))
            })
            .filter(|rel| rel != entry)
            .collect();
    members.sort();
    members
}

/// The marketplace directory a component publishes onto — the four values
/// that decide it, and the whole of a component's identity at publish time.
#[derive(Debug, Clone, Copy)]
pub struct PublishKey<'a> {
    /// The marketplace folder, when there is one.
    pub marketplace: Option<&'a str>,
    /// The plugin folder, when there is one.
    pub plugin: Option<&'a str>,
    /// The kind of component.
    pub kind: EntityKind,
    /// The component's name.
    pub name: &'a str,
}

/// Another component already publishing onto `(marketplace, plugin, kind,
/// name)`, at a path other than `excluding`. The key is passed by value, so a
/// rename or move can ask without having composed a component.
pub fn key_collision(
    vfs: &dyn Vfs,
    vault_dir: &Path,
    ns: &Namespace,
    key: &PublishKey<'_>,
    excluding: &str,
) -> Option<String> {
    let PublishKey {
        marketplace,
        plugin,
        kind,
        name,
    } = *key;
    discover_components(vfs, vault_dir, None, ns)
        .ok()?
        .components
        .into_iter()
        .find(|s| {
            s.marketplace.as_deref() == marketplace
                && s.plugin.as_deref() == plugin
                && s.kind == kind
                && s.name == name
                && s.vault_relative_path != excluding
        })
        .map(|other| {
            format!(
                "`{name}` is already the name of {} — two components cannot publish onto one \
                 marketplace directory",
                other.vault_relative_path
            )
        })
}

/// Whether renaming `entry` to `to` (final component name `slug`) would
/// collide with another component's `(marketplace, plugin, kind, name)` key.
/// A host's own move has no notion of a publish key, so this is asked again
/// at apply time rather than trusted from a preview that may be stale by the
/// time the button is pressed.
pub fn rename_collision(
    vfs: &dyn Vfs,
    vault_dir: &Path,
    ns: &Namespace,
    entry: &str,
    to: &str,
    slug: &str,
) -> Option<String> {
    let ours = find_component(vfs, vault_dir, entry, ns)?;
    let placement = folder_placement(vault_dir, &vault_dir.join(to));
    // The folder names the plugin, so the note follows its destination.
    let plugin = placement.plugin;
    key_collision(
        vfs,
        vault_dir,
        ns,
        &PublishKey {
            marketplace: placement.marketplace.as_deref(),
            plugin: plugin.as_deref(),
            kind: ours.kind,
            name: slug,
        },
        entry,
    )
}

/// The name a component's own file or folder carries: the bundle folder's
/// leaf for a bundle, the filename's stem for a lone note.
fn component_slug(entry: &str) -> &str {
    match bundle_dir(entry) {
        Some(dir) => dir.rsplit('/').next().unwrap_or(dir),
        None => {
            let leaf = entry.rsplit('/').next().unwrap_or(entry);
            crate::discover::md_stem(leaf).unwrap_or(leaf)
        }
    }
}

/// The (marketplace, plugin) folders a conversion to `to_kind` puts the note
/// under, given the ones it sits in now.
///
/// A leaf keeps both: the note stays where it is and only its kind folder
/// changes. A container sits at the level ABOVE the one its members do, so
/// each drops the levels below it — converting to a plugin makes the note a
/// sibling of the plugin it was in, converting to a marketplace takes it out
/// of the marketplace it was in. Converting AWAY from a container needs no
/// arm of its own: a container note's own placement already omits the level
/// it names, so its members' level is not one it can stay at.
fn convert_placement(
    to_kind: EntityKind,
    marketplace: Option<&str>,
    plugin: Option<&str>,
) -> (Option<String>, Option<String>) {
    match to_kind {
        EntityKind::Marketplace => (None, None),
        EntityKind::Plugin => (marketplace.map(str::to_owned), None),
        _ => (marketplace.map(str::to_owned), plugin.map(str::to_owned)),
    }
}

/// The destination converting `entry` to `to_kind` implies — the
/// kind-directory sibling of [`move_destination`], which it delegates to for
/// every leaf kind.
///
/// The one shape a move never produces: a container is authored in a folder
/// of its own whatever shape the note has now, so a lone file gains a folder:
/// `<name>/<name>.md` for a plugin, `<name>/skillwright.md` for a marketplace.
pub fn convert_kind_destination(
    entry: &str,
    to_kind: EntityKind,
    marketplace: Option<&str>,
    plugin: Option<&str>,
) -> Result<String, String> {
    if !to_kind.holds_components() {
        return move_destination(entry, to_kind, marketplace, plugin);
    }
    let dir = new_note_dir(Some(&Placement {
        marketplace: marketplace.map(str::to_owned),
        plugin: plugin.map(str::to_owned),
        kind: Some(to_kind.as_str().to_owned()),
        folder: None,
    }))?;
    let name = component_slug(entry);
    Ok(format!(
        "{dir}/{name}/{}",
        bundle_entry_file(entry, name, to_kind)
    ))
}

/// The member folders a container of `kind` holds, read off [`KIND_DIRS`] so
/// the scaffold and the scan cannot disagree on what a plugin's folders are
/// called. Empty for every kind that holds nothing.
fn container_scaffold_dirs(kind: EntityKind) -> Vec<&'static str> {
    match kind {
        EntityKind::Marketplace => KIND_DIRS
            .iter()
            .filter(|(_, k)| *k == EntityKind::Plugin)
            .map(|(dir, _)| *dir)
            .collect(),
        EntityKind::Plugin => KIND_DIRS
            .iter()
            .filter(|(_, k)| !k.holds_components())
            .map(|(dir, _)| *dir)
            .collect(),
        _ => Vec::new(),
    }
}

/// How the core's discovery would judge this note, before it exists.
///
/// A real discovery pass over a scratch in-memory vault holding exactly this
/// one file at exactly this one vault-relative path. The path carries the real
/// folder levels, so a listed component comes back with its key already
/// resolved.
///
/// A note the scan neither lists nor refuses — a non-skill note that opts
/// in to nothing, say — is judged by the reader ([`read_vault_skill`]) and
/// comes back [`ScratchVerdict::Skipped`]. A note the scan refuses is
/// [`ScratchVerdict::Refused`] whatever its kind, in the scan's own words.
pub fn scratch_scan(source: &str, rel_path: &str, ns: &Namespace) -> ScratchVerdict {
    match scratch_vault(source, rel_path) {
        Ok((vfs, abs)) => scan_scratch(&vfs, &abs, rel_path, ns),
        Err(refusal) => ScratchVerdict::Refused(refusal),
    }
}

/// [`scratch_scan`] over a scratch vault the caller keeps.
fn scan_scratch(vfs: &MemVfs, abs: &Path, rel_path: &str, ns: &Namespace) -> ScratchVerdict {
    let root = Path::new(SCRATCH_ROOT);
    match discover_components(vfs, root, None, ns) {
        Ok(mut found) if found.components.len() == 1 => {
            ScratchVerdict::Listed(Box::new(found.components.remove(0)))
        }
        Ok(found) => match read_vault_skill(vfs, abs, ns) {
            Ok(_)
                if crate::discover::folder_kind(Path::new(SCRATCH_ROOT), abs)
                    .is_some_and(|k| k != EntityKind::Skill) =>
            {
                match found.skipped.first() {
                    Some(skipped) => ScratchVerdict::Refused(skipped.sentence()),
                    None => ScratchVerdict::Skipped,
                }
            }
            Ok(_) => ScratchVerdict::Refused(format!(
                "the composed note is not one the vault's scan would list at `{rel_path}`"
            )),
            Err(e) => ScratchVerdict::Refused(e.to_string()),
        },
        Err(e) => ScratchVerdict::Refused(e.to_string()),
    }
}

/// A scratch in-memory vault holding `source` at `rel_path`, and that file's
/// absolute path in it.
fn scratch_vault(source: &str, rel_path: &str) -> Result<(MemVfs, std::path::PathBuf), String> {
    let vfs = MemVfs::new();
    let root = Path::new(SCRATCH_ROOT);
    let abs = root.join(rel_path);
    vfs.create_dir_all(abs.parent().unwrap_or(root))
        .and_then(|()| vfs.write_file(&abs, source.as_bytes()))
        .map_err(|e| e.to_string())?;
    Ok((vfs, abs))
}

/// Why the hook renderer would refuse this hook note, if it would.
///
/// Discovery lists a hook with no `event` or no payload; only the render at
/// publish refuses it. A write asks the renderer so the refusal comes at the
/// save. The scan and the draft preview do not: a form validates and previews
/// on every keystroke, and asks for `event` only on a later step.
fn hook_refusal(vfs: &MemVfs, abs: &Path, ns: &Namespace) -> Option<String> {
    let note = match read_vault_skill(vfs, abs, ns) {
        Ok(note) => note,
        Err(e) => return Some(e.to_string()),
    };
    crate::projection::to_adapt_hook(&note.projection, None)
        .err()
        .map(|e| e.to_string())
}

/// The note already holding this publish key, if another one is.
///
/// Uniqueness is per (marketplace, plugin, kind, name) — the directory a
/// component publishes onto — not per name: a `review` skill in one plugin
/// beside a `review` skill in another is two directories, and legal. Worth
/// its own check because discovery refuses the *whole* scan on a collision:
/// a note that introduced one would empty the list for every other note in
/// the vault, and the author would be reading a refusal about two paths
/// rather than about the thing they just did.
///
/// A vault whose scan is already refusing answers `None` — the check cannot
/// run, and the note being authored may well be the fix.
pub fn duplicate_key(
    vfs: &dyn Vfs,
    vault_dir: &Path,
    ours: &DiscoveredComponent,
    ns: &Namespace,
) -> Option<String> {
    key_collision(
        vfs,
        vault_dir,
        ns,
        &PublishKey {
            marketplace: ours.marketplace.as_deref(),
            plugin: ours.plugin.as_deref(),
            kind: ours.kind,
            name: &ours.name,
        },
        &ours.vault_relative_path,
    )
}

/// Why this note would be refused: by the scratch scan, by the hook renderer
/// (`hook_refusal`), or by the publish key it would collide with in the real
/// vault. `None` means writable.
pub fn refusal_for(
    vfs: &dyn Vfs,
    source: &str,
    rel_path: &str,
    vault_dir: &Path,
    ns: &Namespace,
) -> Option<String> {
    refusal(vfs, source, rel_path, vault_dir, ns, true)
}

/// [`refusal_for`] without the hook renderer: what a draft preview shows. A
/// draft is previewed while the author is still typing, before the step that
/// asks for a hook's `event`.
pub fn draft_refusal_for(
    vfs: &dyn Vfs,
    source: &str,
    rel_path: &str,
    vault_dir: &Path,
    ns: &Namespace,
) -> Option<String> {
    refusal(vfs, source, rel_path, vault_dir, ns, false)
}

fn refusal(
    vfs: &dyn Vfs,
    source: &str,
    rel_path: &str,
    vault_dir: &Path,
    ns: &Namespace,
    render_hooks: bool,
) -> Option<String> {
    let (scratch, abs) = match scratch_vault(source, rel_path) {
        Ok(kept) => kept,
        Err(refusal) => return Some(refusal),
    };
    match scan_scratch(&scratch, &abs, rel_path, ns) {
        ScratchVerdict::Refused(refusal) => Some(refusal),
        ScratchVerdict::Skipped => None,
        ScratchVerdict::Listed(ours) => {
            if render_hooks
                && ours.kind == EntityKind::Hook
                && let Some(refusal) = hook_refusal(&scratch, &abs, ns)
            {
                return Some(refusal);
            }
            duplicate_key(vfs, vault_dir, &ours, ns)
        }
    }
}

/// The complete note text a request would produce.
///
/// `existing` is the file as it sits on disk for a rewrite, `None` for a new
/// note. A note with no frontmatter block — or an empty one — is seeded with a
/// throwaway `name: ''` so the document root is the mapping the edit needs;
/// the first edit overwrites that value, so the seed never survives.
///
/// `split` and `edit_frontmatter` agree on split form — the last line
/// unterminated — so the block `edit_frontmatter` returns is exactly what
/// `compose` expects, with no newline bookkeeping of our own.
pub fn compose_note(
    existing: Option<&str>,
    fields: &[FieldEdit],
    body: &str,
) -> Result<String, CoreError> {
    let split = existing.map(frontmatter_lossless::split);
    let mut shape = split.as_ref().map_or_else(
        || frontmatter_lossless::new_shape(frontmatter_lossless::NewShapeOptions::default()),
        |s| s.shape,
    );
    let (yaml, seeded) = match split.as_ref().and_then(|s| s.yaml) {
        Some(yaml) if !yaml.trim().is_empty() => (yaml.to_owned(), false),
        _ => ("name: ''".to_owned(), true),
    };
    // An adjacent-fence empty block (`---\n---`) records `close_sep: false`,
    // because it had no separator byte to restore. Filling that block gives it
    // content, and content needs the separator back — without this the closing
    // fence is written onto the end of the last key.
    if seeded {
        shape.close_sep = true;
    }

    let outcome = edit_frontmatter(&yaml, &splice_edits(fields)).map_err(|e| {
        CoreError::refused(
            "frontmatter-edit",
            format!("frontmatter could not be edited: {e}"),
        )
    })?;
    let block = String::from_utf8(outcome.bytes).map_err(|e| {
        CoreError::refused(
            "frontmatter-encoding",
            format!("edited frontmatter is not UTF-8: {e}"),
        )
    })?;
    Ok(frontmatter_lossless::compose(Some(&block), body, shape))
}

/// How the scratch discovery pass judged one composed note.
#[derive(Debug)]
pub enum ScratchVerdict {
    /// The scan lists it, and the component carries the resolved
    /// (marketplace, plugin, kind, name) the folders and frontmatter decided.
    Listed(Box<DiscoveredComponent>),
    /// Legitimately skipped — a non-skill note the scan neither lists nor
    /// refuses.
    Skipped,
    /// The scan refuses it, in the core's own words.
    Refused(String),
}

/// Where a conversion lands and the member folders it creates.
struct ConvertTarget {
    to: String,
    /// Member folders the conversion creates, vault-relative.
    scaffold: Vec<String>,
}

/// [`ConvertTarget`], or the refusal that stops the conversion. Asked by a
/// preview and again by the apply, the same discipline [`rename_collision`]
/// states for its own check.
fn convert_target(
    vfs: &dyn Vfs,
    vault_dir: &Path,
    ns: &Namespace,
    entry: &str,
    to_kind: EntityKind,
) -> Result<ConvertTarget, String> {
    let Some(component) = find_component(vfs, vault_dir, entry, ns) else {
        return Err(format!(
            "`{entry}` is not a component the vault's scan lists"
        ));
    };
    if component.kind == to_kind {
        return Err(format!("`{entry}` is already a {}", to_kind.as_str()));
    }
    // The hook kind is out of this action's vocabulary: a hook's event and
    // matcher have no counterpart in another kind, and nothing here would
    // know what to do with them.
    if component.kind == EntityKind::Hook || to_kind == EntityKind::Hook {
        return Err("converting to or from `hook` is not offered here".to_owned());
    }
    // Converting away from a container would orphan what it holds: the
    // members are grouped by the folder, and the folder stops being a
    // container the moment its note stops naming one.
    if component.kind.holds_components()
        && let Some(dir) = bundle_dir(entry)
    {
        let members = container_members(vfs, vault_dir, dir, entry, ns);
        if !members.is_empty() {
            return Err(format!(
                "`{entry}` still holds {}: {}",
                if members.len() == 1 {
                    "a component"
                } else {
                    "components"
                },
                members.join(", "),
            ));
        }
    }

    let (to_market, to_plugin) = convert_placement(
        to_kind,
        component.marketplace.as_deref(),
        component.plugin.as_deref(),
    );
    let to = convert_kind_destination(entry, to_kind, to_market.as_deref(), to_plugin.as_deref())?;

    // The key the destination publishes onto. A leaf conversion stays in its
    // plugin. A container sits above the plugin level and publishes in none.
    let key_plugin = if to_kind.holds_components() {
        None
    } else {
        component.plugin.clone()
    };
    if let Some(refusal) = key_collision(
        vfs,
        vault_dir,
        ns,
        &PublishKey {
            marketplace: to_market.as_deref(),
            plugin: key_plugin.as_deref(),
            kind: to_kind,
            name: &component.name,
        },
        entry,
    ) {
        return Err(refusal);
    }

    let scaffold = match bundle_dir(&to) {
        Some(container_dir) => container_scaffold_dirs(to_kind)
            .into_iter()
            .map(|d| format!("{container_dir}/{d}"))
            .collect(),
        None => Vec::new(),
    };
    Ok(ConvertTarget { to, scaffold })
}

/// The kind a placement names, when it names one this crate knows.
fn placement_kind(placement: Option<&Placement>) -> Option<EntityKind> {
    placement
        .and_then(|p| p.kind.as_deref())
        .and_then(|kind| kind.trim().parse::<EntityKind>().ok())
}

/// The member folders a newly created container gets, vault-relative.
fn create_scaffold(path: &str, kind: Option<EntityKind>) -> Vec<String> {
    let (Some(dir), Some(kind)) = (bundle_dir(path), kind) else {
        return Vec::new();
    };
    container_scaffold_dirs(kind)
        .into_iter()
        .map(|d| format!("{dir}/{d}"))
        .collect()
}

/// [`FieldEdit`] in the editor's own vocabulary. Total: `None` is a removal,
/// and nothing here can produce the `Rename` this module never asks for.
pub fn splice_edits(edits: &[FieldEdit]) -> Vec<yaml_splice::FieldEdit> {
    edits
        .iter()
        .map(|edit| match &edit.value {
            Some(value) => yaml_splice::FieldEdit::Set {
                key: edit.key.clone(),
                value: value.clone(),
            },
            None => yaml_splice::FieldEdit::Remove {
                key: edit.key.clone(),
            },
        })
        .collect()
}

/// `edits` with `name` forced to `slug`, appended at the front when the
/// caller named no `name` at all.
fn with_name(edits: &[FieldEdit], slug: &str) -> Vec<FieldEdit> {
    let mut edits = edits.to_vec();
    let named = FieldEdit::set("name", yaml_splice::Value::Str(slug.to_owned()));
    match edits.iter_mut().find(|e| e.key == "name") {
        Some(existing) => *existing = named,
        None => edits.insert(0, named),
    }
    edits
}

/// The frontmatter of `source` as a mapping, or `None` for no block.
fn frontmatter_mapping(source: &str) -> Result<Option<yaml_splice::Mapping>, CoreError> {
    let yaml = match frontmatter_lossless::split(source).yaml {
        Some(yaml) if !yaml.trim().is_empty() => yaml,
        _ => return Ok(None),
    };
    parse_mapping(yaml).map(Some).map_err(|e| {
        CoreError::refused(
            "frontmatter-read",
            format!("frontmatter could not be read: {e}"),
        )
    })
}

/// One frontmatter key's raw value, read out of `source` as a string.
///
/// `None` for no frontmatter block and for a key the frontmatter does not
/// have. Exists so a caller can read one raw, unmodeled key without this
/// crate modeling that key as a typed field. The key is read as spelled: for
/// one of our own fields, which a note may also write inside the nested
/// `sw:` block, use [`read_ns_field`].
pub fn read_field(source: &str, key: &str) -> Result<Option<String>, CoreError> {
    Ok(frontmatter_mapping(source)?
        .and_then(|mapping| mapping.get(key).map(|value| value.to_string())))
}

/// One of our own fields (`version`, say) read out of `source`, in any shape
/// a note may write it: bare, the flat `sw-<field>` key, or `<field>` inside
/// the nested `sw:` block.
pub fn read_ns_field(
    source: &str,
    ns: &Namespace,
    field: &str,
) -> Result<Option<String>, CoreError> {
    let Some(mapping) = frontmatter_mapping(source)? else {
        return Ok(None);
    };
    if let Some(value) = mapping.get(&ns.key(field)) {
        return Ok(Some(value.to_string()));
    }
    Ok(mapping
        .get(ns.block_key())
        .and_then(yaml_splice::Value::as_mapping)
        .and_then(|block| block.get(field))
        .map(|value| value.to_string()))
}

/// The edits to apply to `source` that set (or, for `None`, remove) each of
/// `changes`, in the shape the note already uses.
///
/// A note with a nested `sw:` block gets one edit rewriting that block; any
/// other note gets flat keys, prefixed under a namespace. Writing a flat key beside an
/// existing block would give the note both shapes, which discovery refuses.
///
/// [`FieldEdit`] addresses top-level keys only, so the block is written back
/// whole: its keys keep their order and a new one is appended, but a comment
/// inside the block is lost (a trailing one on its last entry moves to the
/// `sw:` line), quoting on its values is dropped, and a flow-style block
/// (`sw: {…}`) comes back in block style. Every byte outside the block is
/// kept.
pub fn ns_field_edits(
    source: &str,
    ns: &Namespace,
    changes: &[(&str, Option<yaml_splice::Value>)],
) -> Result<Vec<FieldEdit>, CoreError> {
    let Some(mut block) = nested_block(source, ns)? else {
        return Ok(changes
            .iter()
            .map(|(field, value)| FieldEdit {
                key: ns.key(field),
                value: value.clone(),
            })
            .collect());
    };
    let before = block.clone();
    for (field, value) in changes {
        match value {
            Some(value) => {
                block.insert(*field, value.clone());
            }
            None => {
                block.remove(field);
            }
        }
    }
    Ok(if block == before {
        Vec::new()
    } else if block.is_empty() {
        vec![FieldEdit::remove(ns.block_key())]
    } else {
        vec![FieldEdit::set(
            ns.block_key(),
            yaml_splice::Value::Map(block),
        )]
    })
}

/// How `field` is spelled in `source`, for a message: `sw.<field>` for a
/// note with a nested `sw:` block, its flat key otherwise — the shape
/// [`ns_field_edits`] writes in.
pub fn ns_field_label(source: &str, ns: &Namespace, field: &str) -> Result<String, CoreError> {
    Ok(match nested_block(source, ns)? {
        Some(_) => format!("{}.{field}", ns.block_key()),
        None => ns.key(field),
    })
}

/// The note's nested `sw:` block, or `None` when it has none or the key
/// holds something other than a mapping.
fn nested_block(source: &str, ns: &Namespace) -> Result<Option<yaml_splice::Mapping>, CoreError> {
    Ok(
        match frontmatter_mapping(source)?.and_then(|m| m.get(ns.block_key()).cloned()) {
            Some(yaml_splice::Value::Map(block)) => Some(block),
            // `sw:` with nothing under it is an empty block, not a broken one.
            Some(yaml_splice::Value::Null) => Some(yaml_splice::Mapping::new()),
            _ => None,
        },
    )
}

/// [`ns_field_edits`] for the note at vault-relative `rel`, in a plan's own
/// refusal vocabulary.
fn note_ns_edits(
    vfs: &dyn Vfs,
    vault: &Path,
    rel: &str,
    ns: &Namespace,
    changes: &[(&str, Option<yaml_splice::Value>)],
) -> Result<Vec<FieldEdit>, String> {
    let bytes = vfs.read_file(&vault.join(rel)).map_err(|e| e.to_string())?;
    let source = String::from_utf8(bytes).map_err(|e| format!("`{rel}` is not UTF-8: {e}"))?;
    ns_field_edits(&source, ns, changes).map_err(|e| e.to_string())
}

/// The note as it would be written to disk, frontmatter spliced and body kept.
///
/// See [`compose_note`] for the seeding rule; this is that function with the
/// body read back off `source`, which is what a host applying an op's
/// `frontmatter` list to an existing note needs.
pub fn apply_field_edits(source: &str, edits: &[FieldEdit]) -> Result<String, CoreError> {
    compose_note(
        Some(source),
        edits,
        frontmatter_lossless::split(source).body,
    )
}

// ── The plans ───────────────────────────────────────────────────────────────

/// The note a create is asked for: where it goes, what it is called, and
/// what it says.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct NewNote<'a> {
    /// Where the author put it, or `None` for the default placement.
    pub placement: Option<&'a Placement>,
    /// The name as typed; the filename is slugged from it.
    pub name: &'a str,
    /// Author it as `<slug>/<slug>.md` with a folder of its own. A container
    /// kind is bundled whatever this says.
    pub bundled: bool,
    /// The frontmatter, complete rather than a patch. A `name` edit here is
    /// overwritten with the slug.
    pub fields: &'a [FieldEdit],
    /// Everything below the frontmatter block.
    pub body: &'a str,
}

impl<'a> NewNote<'a> {
    /// A note called `name`, at the default placement, not bundled, with no
    /// fields and an empty body. Set the other fields after.
    pub fn new(name: &'a str) -> Self {
        Self {
            placement: None,
            name,
            bundled: false,
            fields: &[],
            body: "",
        }
    }
}

/// The vault as a planner reads it, less its build output, so a direct read
/// in a planner cannot see the output.
fn planner_notes<'v>(vfs: &'v dyn Vfs, vault: &Path, ns: &Namespace) -> SourceView<'v> {
    notes_over(vfs, vault, None, None, ns)
}

/// Write a new note: its derived path, its composed bytes, and the member
/// folders a container kind gets. The path comes from [`derive_create`].
///
/// `Err` is the refusal, in the words a draft shows while the author types: a
/// name with nothing to slug, a placement naming what no folder can be, or a
/// note the vault's own scan would reject once written.
pub fn plan_create_note(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    note: &NewNote<'_>,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    let (path, edits) = derive_create(note.placement, note.name, note.bundled, note.fields)?;
    let bytes = compose_note(None, &edits, note.body).map_err(|e| e.to_string())?;
    if let Some(refusal) = refusal_for(&notes, &bytes, &path, vault, ns) {
        return Err(refusal);
    }
    let scaffold = create_scaffold(&path, placement_kind(note.placement));
    Ok(AuthoringOp::Create {
        path,
        bytes,
        scaffold,
    })
}

/// Rename a component: move the note (and, for a bundle, its folder), then
/// set `name` to the new slug.
pub fn plan_rename_component(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
    new_name: &str,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    let slug = slugify(new_name);
    if let Some(refusal) = slug_refusal(&slug, "component name") {
        return Err(refusal);
    }
    let to = rename_destination(entry, &slug);
    if let Some(refusal) = rename_collision(&notes, vault, ns, entry, &to, &slug) {
        return Err(refusal);
    }
    Ok(AuthoringOp::Rename {
        from: entry.to_owned(),
        to,
        bundle: bundle_dir(entry).is_some(),
        frontmatter: vec![FieldEdit::set("name", yaml_splice::Value::Str(slug))],
    })
}

/// Move a component to a new placement.
///
/// The folder decides the plugin, so no frontmatter changes. `plugin: None`
/// shares the note at the marketplace level.
pub fn plan_move_component(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
    marketplace: Option<&str>,
    plugin: Option<&str>,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    let Some(component) = find_component(&notes, vault, entry, ns) else {
        return Err(format!(
            "`{entry}` is not a component the vault's scan lists"
        ));
    };
    let to = move_destination(entry, component.kind, marketplace, plugin)?;
    if let Some(refusal) = key_collision(
        &notes,
        vault,
        ns,
        &PublishKey {
            marketplace,
            plugin,
            kind: component.kind,
            name: &component.name,
        },
        entry,
    ) {
        return Err(refusal);
    }
    Ok(AuthoringOp::Move {
        from: entry.to_owned(),
        to,
        bundle: bundle_dir(entry).is_some(),
    })
}

/// Delete a component, plus the one check a generic delete cannot make:
/// refusing a container note (plugin or marketplace) whose folder still holds
/// members. Deleting an empty container's note is allowed — nothing else is
/// there to lose. When the scan cannot resolve the note (it is skipped, or
/// the scan refuses), the folders it sits in decide its kind, and a note no
/// folder names is checked as a container would be, since its whole folder
/// is what the delete trashes.
pub fn plan_delete_component(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    let may_hold_members =
        resolved_kind(&notes, vault, ns, entry).is_none_or(EntityKind::holds_components);
    if may_hold_members && let Some(dir) = bundle_dir(entry) {
        let members = container_members(&notes, vault, dir, entry, ns);
        if !members.is_empty() {
            return Err(format!(
                "`{}` still holds {}: {}",
                entry,
                if members.len() == 1 {
                    "a component"
                } else {
                    "components"
                },
                members.join(", "),
            ));
        }
    }
    Ok(deletion(entry))
}

/// Delete a container's whole folder — its own note plus every component
/// beneath it — whether or not it still holds members. Holding members is
/// what a container delete is for, so [`plan_delete_component`]'s refusal is
/// dropped here; only the kind is checked — against the folder the note sits
/// in when the scan cannot resolve it.
pub fn plan_delete_container(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
    expected_kind: EntityKind,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    if resolved_kind(&notes, vault, ns, entry) != Some(expected_kind) {
        return Err(format!(
            "`{}` is not a {}'s own entry note",
            entry,
            expected_kind.as_str()
        ));
    }
    Ok(deletion(entry))
}

/// What a delete removes: a bundle's whole folder, or a lone note's file.
fn deletion(entry: &str) -> AuthoringOp {
    AuthoringOp::Delete {
        paths: vec![bundle_dir(entry).unwrap_or(entry).to_owned()],
        trash_dir: TRASH_DIR.to_owned(),
    }
}

/// The kind the scan lists `entry` as, or, when the scan cannot resolve it,
/// the kind the folders it sits in name.
fn resolved_kind(vfs: &dyn Vfs, vault: &Path, ns: &Namespace, entry: &str) -> Option<EntityKind> {
    match find_component(vfs, vault, entry, ns) {
        Some(component) => Some(component.kind),
        None => folder_kind(vault, &vault.join(entry)),
    }
}

/// Rename a plugin: move its folder and set its own `name`. Its members ride
/// along with the folder, which is what names their plugin.
pub fn plan_rename_plugin(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
    new_name: &str,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    let slug = slugify(new_name);
    if let Some(refusal) = slug_refusal(&slug, "plugin name") {
        return Err(refusal);
    }
    let components = discover_components(&notes, vault, None, ns)
        .map_err(|e| e.to_string())?
        .components;
    let Some(component) = components.iter().find(|c| c.vault_relative_path == entry) else {
        return Err(format!(
            "`{entry}` is not a component the vault's scan lists"
        ));
    };
    if component.kind != EntityKind::Plugin {
        return Err(format!("`{entry}` is not a plugin's own entry note"));
    }
    let to = rename_destination(entry, &slug);
    if let Some(refusal) = rename_collision(&notes, vault, ns, entry, &to, &slug) {
        return Err(refusal);
    }
    if bundle_dir(entry).is_none() {
        return Err(format!(
            "`{entry}` is not a plugin's own `<dir>/<dir>.md` entry note, so its folder has no \
             name to move"
        ));
    }
    Ok(AuthoringOp::Rename {
        from: entry.to_owned(),
        to,
        bundle: true,
        frontmatter: vec![FieldEdit::set("name", yaml_splice::Value::Str(slug))],
    })
}

/// Rename a marketplace: [`plan_rename_component`] with a kind guard. A
/// marketplace's own note has no member key to rewrite, so there is no
/// third step the way a plugin rename has one.
pub fn plan_rename_marketplace(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
    new_name: &str,
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    if let Some(component) = find_component(&notes, vault, entry, ns)
        && component.kind != EntityKind::Marketplace
    {
        return Err(format!("`{entry}` is not a marketplace's own entry note"));
    }
    plan_rename_component(&notes, vault, ns, entry, new_name)
}

/// Convert a note to another kind: move it into the new kind's folder, remove
/// the keys the new kind does not carry, and make the member folders a
/// container kind holds.
///
/// `drop_keys` is the caller's list, not this crate's: the table saying which
/// fields a kind carries belongs to whatever renders the form, and a second
/// copy here would be a second answer to the same question. Only
/// `UNDROPPABLE_KEYS` are refused.
pub fn plan_convert_kind(
    vfs: &dyn Vfs,
    vault: &Path,
    ns: &Namespace,
    entry: &str,
    to_kind: EntityKind,
    drop_keys: &[String],
) -> Result<AuthoringOp, String> {
    let notes = planner_notes(vfs, vault, ns);
    // Trimmed, because the edit loop below trims before it emits a removal:
    // an untrimmed guard lets `" name"` through and then drops the real
    // `name` key, which is the one the vault's discovery scan reads.
    if let Some(key) = drop_keys
        .iter()
        .find(|k| UNDROPPABLE_KEYS.contains(&k.trim()))
    {
        return Err(format!(
            "`{key}` is carried by every kind and is not dropped by a conversion"
        ));
    }
    // The namespace block is dropped field by field, never whole.
    if let Some(key) = drop_keys
        .iter()
        .find(|k| !ns.is_bare() && k.trim() == ns.block_key())
    {
        return Err(format!(
            "`{key}` holds skillwright's fields; drop them by their `{}` keys",
            ns.key("<field>")
        ));
    }
    let target = convert_target(&notes, vault, ns, entry, to_kind)?;
    // The folder carries the new kind. Our own fields go in whichever shape
    // the note uses, flat or nested; any other key is removed as spelled.
    let mut changes: Vec<(&str, Option<yaml_splice::Value>)> = Vec::new();
    let mut others: Vec<&str> = Vec::new();
    let mut dropped: BTreeSet<&str> = BTreeSet::new();
    for key in drop_keys {
        let key = key.trim();
        if key.is_empty() || !dropped.insert(key) {
            continue;
        }
        match key.strip_prefix(ns.prefix()) {
            Some(field) if !field.is_empty() => changes.push((field, None)),
            _ => others.push(key),
        }
    }
    let mut frontmatter = note_ns_edits(&notes, vault, entry, ns, &changes)?;
    frontmatter.extend(others.into_iter().map(FieldEdit::remove));
    Ok(AuthoringOp::ConvertKind {
        from: entry.to_owned(),
        to: target.to,
        bundle: bundle_dir(entry).is_some(),
        frontmatter,
        scaffold: target.scaffold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_splice::Value;

    const VAULT: &str = "/vault";

    fn vault_of(files: &[(&str, &str)]) -> crate::vfs::MemVfs {
        let vfs = crate::vfs::MemVfs::new();
        for (path, text) in files {
            vfs.insert(path, text).unwrap();
        }
        vfs
    }

    const USE_IT: &str = "---\ndescription: Use it.\n---\nBody.\n";

    /// A guard: it passes without the planner's own wrap too, since the
    /// planner reads the vault only through [`discover_components`], which
    /// hides the output itself.
    #[test]
    fn plan_rename_component_leaves_output_unedited() {
        let vfs = vault_of(&[
            (
                "/vault/skillwright.md",
                "---\nname: v\nclaude: {packs: moved}\nbuild:\n  out: dist\n---\n",
            ),
            ("/vault/plugins/rev/skills/a.md", USE_IT),
            ("/vault/dist/moved/rev/.claude-plugin/plugin.json", "{}"),
            ("/vault/dist/moved/rev/skills/b/SKILL.md", USE_IT),
        ]);
        let op = plan_rename_component(
            &vfs,
            Path::new(VAULT),
            &sw(),
            "plugins/rev/skills/a.md",
            "b",
        )
        .unwrap();
        let AuthoringOp::Rename { from, to, .. } = op else {
            panic!("{op:?}");
        };
        assert_eq!(from, "plugins/rev/skills/a.md");
        assert_eq!(to, "plugins/rev/skills/b.md");
    }

    /// The note at `notes/x/x.md` is in no kind folder, so a delete checks
    /// its folder as a container's. An earlier build left Pi packs, moved out
    /// of a dot folder, in that folder.
    #[test]
    fn container_members_skips_output() {
        let vfs = vault_of(&[
            (
                "/vault/skillwright.md",
                "---\nname: v\npi: {packs: pi-packs}\nbuild:\n  out: notes/x/market\n---\n",
            ),
            ("/vault/notes/x/x.md", "Notes.\n"),
            ("/vault/notes/x/market/pi-packs/rev/package.json", "{}"),
            (
                "/vault/notes/x/market/pi-packs/rev/skills/rev-a/SKILL.md",
                USE_IT,
            ),
        ]);
        let vault = Path::new(VAULT);
        let members = container_members(&vfs, vault, "notes/x", "notes/x/x.md", &sw());
        assert!(members.is_empty(), "{members:?}");
        assert!(plan_delete_component(&vfs, vault, &sw(), "notes/x/x.md").is_ok());
    }

    fn ns_rewrite(source: &str, changes: &[(&str, Option<Value>)]) -> (Vec<FieldEdit>, String) {
        let edits = ns_field_edits(source, &sw(), changes).unwrap();
        let out = apply_field_edits(source, &edits).unwrap();
        (edits, out)
    }

    fn str_value(s: &str) -> Option<Value> {
        Some(Value::Str(s.to_owned()))
    }

    #[test]
    fn ns_field_edits_is_empty_when_the_block_already_holds_the_value() {
        let source = "---\nname: a\nsw:\n  version: 1.0.0\n---\nbody\n";
        let (edits, out) = ns_rewrite(source, &[("version", str_value("1.0.0"))]);
        assert!(edits.is_empty());
        assert_eq!(out, source);
    }

    #[test]
    fn ns_field_edits_removes_a_block_its_changes_empty() {
        let source = "---\nname: a\nsw:\n  version: 1.0.0\ntail: x\n---\nbody\n";
        let (edits, out) = ns_rewrite(source, &[("version", None)]);
        assert_eq!(edits, vec![FieldEdit::remove("sw")]);
        assert_eq!(out, "---\nname: a\ntail: x\n---\nbody\n");
    }

    #[test]
    fn ns_field_edits_fills_a_null_block() {
        let source = "---\nname: a\nsw:\ntail: x\n---\nbody\n";
        let (_, out) = ns_rewrite(source, &[("plugin", str_value("p"))]);
        assert_eq!(out, "---\nname: a\nsw:\n  plugin: p\ntail: x\n---\nbody\n");
    }

    #[test]
    fn ns_field_edits_falls_back_to_flat_keys_beside_a_block_that_is_not_a_map() {
        let source = "---\nname: a\nsw: oops\n---\nbody\n";
        let (edits, _) = ns_rewrite(source, &[("plugin", str_value("p"))]);
        assert_eq!(
            edits,
            vec![FieldEdit::set("sw-plugin", Value::Str("p".to_owned()))]
        );
    }

    /// The costs the doc names, pinned so a splicer upgrade that changes
    /// them shows up here: a flow block comes back in block style and its
    /// quoting is dropped, while bytes outside the block are kept.
    #[test]
    fn ns_field_edits_rewrites_a_flow_block_in_block_style() {
        let source =
            "---\nname: a # keep\nsw: {version: 1.0.0, plugin: \"p\"}\ntail: x\n---\nbody\n";
        let (_, out) = ns_rewrite(source, &[("version", str_value("1.0.1"))]);
        assert_eq!(
            out,
            "---\nname: a # keep\nsw:\n  version: 1.0.1\n  plugin: p\ntail: x\n---\nbody\n"
        );
    }

    #[test]
    fn ns_field_edits_drops_a_comment_inside_the_block() {
        let source = "---\nname: a\nsw:\n  # note\n  plugin: \"p\"\n  version: 1.0.0 # last\ntail: x\n---\nbody\n";
        let (_, out) = ns_rewrite(source, &[("version", str_value("1.0.1"))]);
        assert_eq!(
            out,
            "---\nname: a\nsw: # last\n  plugin: p\n  version: 1.0.1\ntail: x\n---\nbody\n"
        );
    }

    #[test]
    fn a_plugin_rename_fails_with_the_scan_error_when_the_scan_cannot_run() {
        let vfs = one_plugin_unscannable();
        let scan_error = discover_components(&vfs, vault(), None, &sw())
            .unwrap_err()
            .to_string();

        let refusal = plan_rename_plugin(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/tools.md",
            "gear",
        )
        .unwrap_err();
        assert_eq!(refusal, scan_error);
    }

    #[test]
    fn ns_field_label_spells_the_key_the_way_the_note_does() {
        let nested = "---\nname: a\nsw:\n  version: 1.0.0\n---\n";
        let flat = "---\nname: a\nsw-version: 1.0.0\n---\n";
        assert_eq!(
            ns_field_label(nested, &sw(), "version").unwrap(),
            "sw.version"
        );
        assert_eq!(
            ns_field_label(flat, &sw(), "version").unwrap(),
            "sw-version"
        );
    }

    fn sw() -> Namespace {
        Namespace::sw()
    }

    fn vault() -> &'static Path {
        Path::new(VAULT)
    }

    fn write(vfs: &MemVfs, rel: &str, content: &str) {
        let path = Path::new(VAULT).join(rel);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(&path, content.as_bytes()).unwrap();
    }

    /// A component note: the two keys every kind carries, plus whatever else.
    fn note(name: &str, extra: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: Use this skill to test {name}.\n{extra}---\n\n# Body\n"
        )
    }

    #[test]
    fn a_hook_the_renderer_would_refuse_is_refused_at_the_save() {
        let vfs = MemVfs::new();
        let at = "marketplaces/m/plugins/p/hooks/lint-on-write.md";
        let hook = |extra: &str, body: &str| {
            format!(
                "---\nname: lint-on-write\ndescription: Lints after a write.\n{extra}---\n\n{body}"
            )
        };

        let no_event = refusal_for(&vfs, &hook("", "npm run lint\n"), at, vault(), &sw());
        assert!(
            no_event
                .as_deref()
                .is_some_and(|r| r.contains("hook-missing-event")),
            "{no_event:?}"
        );

        let no_url = refusal_for(
            &vfs,
            &hook("event: PostToolUse\ntype: http\n", ""),
            at,
            vault(),
            &sw(),
        );
        assert!(
            no_url
                .as_deref()
                .is_some_and(|r| r.contains("hook-missing-payload")),
            "{no_url:?}"
        );

        assert!(
            matches!(
                scratch_scan(&hook("", ""), at, &sw()),
                ScratchVerdict::Listed(_)
            ),
            "the scan a form runs per keystroke still lists an incomplete hook"
        );

        assert_eq!(
            draft_refusal_for(&vfs, &hook("", ""), at, vault(), &sw()),
            None,
            "a draft preview does not ask the renderer"
        );

        let complete = refusal_for(
            &vfs,
            &hook("event: PostToolUse\n", "npm run lint\n"),
            at,
            vault(),
            &sw(),
        );
        assert_eq!(complete, None);
    }

    fn fields(name: &str) -> Vec<FieldEdit> {
        vec![
            FieldEdit::set("name", Value::Str(name.to_owned())),
            FieldEdit::set(
                "description",
                Value::Str(format!("Use this skill to test {name}.")),
            ),
        ]
    }

    fn placement(marketplace: &str, plugin: &str, kind: &str) -> Placement {
        Placement {
            marketplace: Some(marketplace.to_owned()),
            plugin: Some(plugin.to_owned()),
            kind: Some(kind.to_owned()),
            folder: None,
        }
    }

    /// One plugin holding one skill, the shape most of these tests start from.
    fn one_plugin() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/tools.md",
            &note("tools", ""),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/review.md",
            &note("review", ""),
        );
        vfs
    }

    /// [`one_plugin`] plus two unrelated notes on one publish key, so the
    /// whole scan refuses.
    fn one_plugin_unscannable() -> MemVfs {
        let vfs = one_plugin();
        write(&vfs, "marketplaces/house/skills/a.md", &note("dup", ""));
        write(&vfs, "marketplaces/house/skills/b.md", &note("dup", ""));
        assert!(discover_components(&vfs, vault(), None, &sw()).is_err());
        vfs
    }

    // ── Placement and naming ────────────────────────────────────────────────

    #[test]
    fn a_create_with_no_placement_lands_in_the_vaults_skills_folder() {
        assert_eq!(new_note_dir(None).unwrap(), "skills");
    }

    #[test]
    fn a_kind_folder_is_read_off_the_kind_map() {
        assert_eq!(
            new_note_dir(Some(&placement("house", "tools", "command"))).unwrap(),
            "marketplaces/house/plugins/tools/commands"
        );
    }

    #[test]
    fn a_placement_names_a_kind_or_a_folder_never_both() {
        let mut p = placement("house", "tools", "skill");
        p.folder = Some("roles".to_owned());
        assert_eq!(
            new_note_dir(Some(&p)).unwrap_err(),
            "a placement names a kind or a plain folder, never both — a plain note has no kind"
        );
    }

    #[test]
    fn a_placement_names_one_of_them() {
        assert_eq!(
            new_note_dir(Some(&Placement::default())).unwrap_err(),
            "the placement names neither a kind nor a folder"
        );
    }

    #[test]
    fn a_plain_folder_may_not_be_a_kind_folder() {
        let p = Placement {
            marketplace: Some("house".to_owned()),
            plugin: Some("tools".to_owned()),
            kind: None,
            folder: Some("skills".to_owned()),
        };
        assert_eq!(
            new_note_dir(Some(&p)).unwrap_err(),
            "`skills` is the folder a skill lands in — a plain note names a folder of its own"
        );
    }

    #[test]
    fn a_level_that_is_not_one_folder_name_is_refused() {
        let mut p = placement("../elsewhere", "tools", "skill");
        p.marketplace = Some("../elsewhere".to_owned());
        assert_eq!(
            new_note_dir(Some(&p)).unwrap_err(),
            "`../elsewhere` is not one folder name for the marketplace"
        );
    }

    #[test]
    fn a_plugin_is_authored_inside_a_marketplace() {
        assert_eq!(
            new_note_dir(Some(&Placement {
                marketplace: Some("house".to_owned()),
                plugin: None,
                kind: Some("plugin".to_owned()),
                folder: None,
            }))
            .unwrap(),
            "marketplaces/house/plugins"
        );
        assert_eq!(
            new_note_dir(Some(&placement("house", "tools", "plugin"))).unwrap_err(),
            PLUGIN_NESTED
        );
    }

    #[test]
    fn a_marketplace_names_no_level_below_itself() {
        assert_eq!(
            new_note_dir(Some(&Placement {
                marketplace: None,
                plugin: None,
                kind: Some("marketplace".to_owned()),
                folder: None,
            }))
            .unwrap(),
            "marketplaces"
        );
        assert_eq!(
            new_note_dir(Some(&placement("house", "tools", "marketplace"))).unwrap_err(),
            "a marketplace is not authored inside a plugin"
        );
    }

    #[test]
    fn a_name_with_nothing_sluggable_is_refused() {
        assert_eq!(
            slug_refusal("", "component name").unwrap(),
            nameless("component name")
        );
        assert!(slug_refusal("review", "component name").is_none());
    }

    // ── Create ──────────────────────────────────────────────────────────────

    #[test]
    fn a_create_derives_its_path_from_the_slugged_name() {
        let vfs = one_plugin();
        let p = placement("house", "tools", "skill");
        let edits = fields("ignored");
        let op = plan_create_note(&vfs, vault(), &sw(), &{
            let mut value = NewNote::new("Code Review");
            value.fields = &edits;
            value.body = "\n# Body\n";
            value.placement = Some(&p);
            value
        })
        .unwrap();
        let AuthoringOp::Create {
            path,
            bytes,
            scaffold,
        } = op
        else {
            panic!("expected a create");
        };
        assert_eq!(
            path,
            "marketplaces/house/plugins/tools/skills/code-review.md"
        );
        assert!(bytes.contains("name: code-review"), "{bytes}");
        assert!(scaffold.is_empty());
    }

    #[test]
    fn a_bundled_create_gets_a_folder_of_its_own() {
        let vfs = one_plugin();
        let p = placement("house", "tools", "skill");
        let edits = fields("deploy");
        let AuthoringOp::Create { path, .. } = plan_create_note(&vfs, vault(), &sw(), &{
            let mut value = NewNote::new("deploy");
            value.fields = &edits;
            value.body = "\n# Body\n";
            value.placement = Some(&p);
            value.bundled = true;
            value
        })
        .unwrap() else {
            panic!("expected a create");
        };
        assert_eq!(
            path,
            "marketplaces/house/plugins/tools/skills/deploy/deploy.md"
        );
    }

    #[test]
    fn a_created_container_is_bundled_and_scaffolded() {
        let vfs = one_plugin();
        let p = Placement {
            marketplace: Some("house".to_owned()),
            plugin: None,
            kind: Some("plugin".to_owned()),
            folder: None,
        };
        let edits = fields("extras");
        let AuthoringOp::Create { path, scaffold, .. } = plan_create_note(&vfs, vault(), &sw(), &{
            let mut value = NewNote::new("extras");
            value.fields = &edits;
            value.body = "\n# Body\n";
            value.placement = Some(&p);
            value
        })
        .unwrap() else {
            panic!("expected a create");
        };
        assert_eq!(path, "marketplaces/house/plugins/extras/extras.md");
        assert_eq!(
            scaffold,
            vec![
                "marketplaces/house/plugins/extras/skills",
                "marketplaces/house/plugins/extras/commands",
                "marketplaces/house/plugins/extras/agents",
                "marketplaces/house/plugins/extras/hooks",
            ]
        );
    }

    #[test]
    fn a_create_onto_an_occupied_publish_key_is_refused() {
        let vfs = one_plugin();
        let p = placement("house", "tools", "skill");
        let edits = fields("review");
        let refusal = plan_create_note(&vfs, vault(), &sw(), &{
            let mut value = NewNote::new("review");
            value.fields = &edits;
            value.body = "\n# Body\n";
            value.placement = Some(&p);
            value.bundled = true;
            value
        })
        .unwrap_err();
        assert!(
            refusal.contains("two components cannot publish onto one marketplace directory"),
            "{refusal}"
        );
    }

    #[test]
    fn a_create_the_scan_would_refuse_is_refused_here() {
        let vfs = one_plugin();
        let p = placement("house", "tools", "skill");
        let edits = [
            FieldEdit::set("name", Value::Str("audit".to_owned())),
            FieldEdit::set("description", Value::Str("d".repeat(1025))),
        ];
        let refusal = plan_create_note(&vfs, vault(), &sw(), &{
            let mut value = NewNote::new("audit");
            value.fields = &edits;
            value.body = "\n# Body\n";
            value.placement = Some(&p);
            value
        })
        .unwrap_err();
        assert!(
            refusal.contains("not one the vault's scan would list"),
            "{refusal}"
        );
    }

    // ── Rename ──────────────────────────────────────────────────────────────

    #[test]
    fn a_rename_keeps_a_lone_note_in_its_folder() {
        assert_eq!(
            rename_destination("plugins/tools/skills/review.md", "audit"),
            "plugins/tools/skills/audit.md"
        );
    }

    #[test]
    fn a_rename_moves_a_bundle_folder_with_its_entry() {
        assert_eq!(
            rename_destination("plugins/tools/skills/review/review.md", "audit"),
            "plugins/tools/skills/audit/audit.md"
        );
        assert_eq!(
            rename_destination("plugins/tools/skills/review/SKILL.md", "audit"),
            "plugins/tools/skills/audit/SKILL.md"
        );
        assert_eq!(
            bundle_dir("plugins/tools/skills/review/SKILL.md"),
            Some("plugins/tools/skills/review")
        );
    }

    /// A `SKILL.md` skill keeps its entry file on a move, and takes its
    /// folder's name when converted to a kind that cannot use `SKILL.md`.
    #[test]
    fn a_skill_md_entry_survives_a_move_but_not_a_conversion() {
        let entry = "marketplaces/house/plugins/tools/skills/audit/SKILL.md";
        assert_eq!(
            move_destination(entry, EntityKind::Skill, Some("house"), Some("extras")).unwrap(),
            "marketplaces/house/plugins/extras/skills/audit/SKILL.md"
        );
        assert_eq!(
            convert_kind_destination(entry, EntityKind::Command, Some("house"), Some("tools"))
                .unwrap(),
            "marketplaces/house/plugins/tools/commands/audit/audit.md"
        );
    }

    #[test]
    fn a_rename_sets_the_name_key_to_the_new_slug() {
        let vfs = one_plugin();
        let AuthoringOp::Rename {
            from,
            to,
            bundle,
            frontmatter,
        } = plan_rename_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
            "Deep Audit",
        )
        .unwrap()
        else {
            panic!("expected a rename");
        };
        assert_eq!(from, "marketplaces/house/plugins/tools/skills/review.md");
        assert_eq!(to, "marketplaces/house/plugins/tools/skills/deep-audit.md");
        assert!(!bundle);
        assert_eq!(
            frontmatter,
            vec![FieldEdit::set("name", Value::Str("deep-audit".to_owned()))]
        );
    }

    #[test]
    fn a_rename_onto_an_occupied_key_is_refused() {
        let vfs = one_plugin();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/audit.md",
            &note("audit", ""),
        );
        let refusal = plan_rename_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
            "audit",
        )
        .unwrap_err();
        assert!(
            refusal.starts_with("`audit` is already the name of"),
            "{refusal}"
        );
    }

    #[test]
    fn a_rename_to_an_unsluggable_name_is_refused() {
        let vfs = one_plugin();
        assert_eq!(
            plan_rename_component(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/skills/review.md",
                "   ",
            )
            .unwrap_err(),
            nameless("component name")
        );
    }

    // ── Move ────────────────────────────────────────────────────────────────

    #[test]
    fn a_move_keeps_the_component_kind_and_its_own_filename() {
        assert_eq!(
            move_destination(
                "plugins/tools/skills/review.md",
                EntityKind::Skill,
                Some("house"),
                Some("extras"),
            )
            .unwrap(),
            "marketplaces/house/plugins/extras/skills/review.md"
        );
    }

    #[test]
    fn a_move_names_the_new_path_and_nothing_else() {
        let vfs = one_plugin();
        let AuthoringOp::Move { from, to, bundle } = plan_move_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
            Some("house"),
            Some("extras"),
        )
        .unwrap() else {
            panic!("expected a move");
        };
        assert_eq!(from, "marketplaces/house/plugins/tools/skills/review.md");
        assert_eq!(to, "marketplaces/house/plugins/extras/skills/review.md");
        assert!(!bundle);
    }

    #[test]
    fn a_move_of_a_note_the_scan_does_not_list_is_refused() {
        let vfs = one_plugin();
        assert_eq!(
            plan_move_component(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/skills/absent.md",
                Some("house"),
                Some("extras"),
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/skills/absent.md` is not a component the vault's \
             scan lists"
        );
    }

    // ── Delete ──────────────────────────────────────────────────────────────

    #[test]
    fn a_delete_trashes_the_note() {
        let vfs = one_plugin();
        let AuthoringOp::Delete { paths, trash_dir } = plan_delete_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
        )
        .unwrap() else {
            panic!("expected a delete");
        };
        assert_eq!(
            paths,
            vec!["marketplaces/house/plugins/tools/skills/review.md"]
        );
        assert_eq!(trash_dir, ".trash");
    }

    #[test]
    fn a_delete_of_a_bundle_takes_its_whole_folder() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/deploy/deploy.md",
            &note("deploy", ""),
        );
        let AuthoringOp::Delete { paths, .. } = plan_delete_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/deploy/deploy.md",
        )
        .unwrap() else {
            panic!("expected a delete");
        };
        assert_eq!(
            paths,
            vec!["marketplaces/house/plugins/tools/skills/deploy"]
        );
    }

    #[test]
    fn a_leaf_delete_refuses_a_container_that_still_holds_members() {
        let vfs = one_plugin();
        assert_eq!(
            plan_delete_component(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/tools.md",
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/tools.md` still holds a component: \
             marketplaces/house/plugins/tools/skills/review.md"
        );
    }

    #[test]
    fn a_container_delete_allows_the_members() {
        let vfs = one_plugin();
        let AuthoringOp::Delete { paths, .. } = plan_delete_container(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/tools.md",
            EntityKind::Plugin,
        )
        .unwrap() else {
            panic!("expected a delete");
        };
        assert_eq!(paths, vec!["marketplaces/house/plugins/tools"]);
    }

    #[test]
    fn a_container_delete_checks_the_kind() {
        let vfs = one_plugin();
        assert_eq!(
            plan_delete_container(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/tools.md",
                EntityKind::Marketplace,
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/tools.md` is not a marketplace's own entry note"
        );
    }

    // ── Plugin and marketplace rename ───────────────────────────────────────

    #[test]
    fn a_plugin_rename_moves_the_folder_and_edits_only_its_own_note() {
        let vfs = one_plugin();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/keyed.md",
            &note("keyed", ""),
        );
        let AuthoringOp::Rename {
            from,
            to,
            bundle,
            frontmatter,
        } = plan_rename_plugin(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/tools.md",
            "Gear",
        )
        .unwrap()
        else {
            panic!("expected a plugin rename");
        };
        assert_eq!(from, "marketplaces/house/plugins/tools/tools.md");
        assert_eq!(to, "marketplaces/house/plugins/gear/gear.md");
        assert!(bundle);
        assert_eq!(
            frontmatter,
            vec![FieldEdit::set("name", Value::Str("gear".to_owned()))]
        );
    }

    #[test]
    fn a_plugin_rename_checks_the_kind() {
        let vfs = one_plugin();
        assert_eq!(
            plan_rename_plugin(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/skills/review.md",
                "gear",
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/skills/review.md` is not a plugin's own entry note"
        );
    }

    #[test]
    fn a_marketplace_rename_checks_the_kind() {
        let vfs = one_plugin();
        assert_eq!(
            plan_rename_marketplace(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/tools.md",
                "shop",
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/tools.md` is not a marketplace's own entry note"
        );
    }

    /// A marketplace's entry note is `marketplaces/<m>/skillwright.md`, and a
    /// rename or delete of it works on the whole folder.
    #[test]
    fn a_marketplace_folder_renames_and_deletes_through_its_root_file() {
        let vfs = one_plugin();
        write(
            &vfs,
            "marketplaces/house/skillwright.md",
            &note("house", ""),
        );
        let entry = "marketplaces/house/skillwright.md";
        let AuthoringOp::Rename { to, bundle, .. } =
            plan_rename_marketplace(&vfs, vault(), &sw(), entry, "shop").unwrap()
        else {
            panic!("expected a rename");
        };
        assert_eq!(to, "marketplaces/shop/skillwright.md");
        assert!(bundle);
        let AuthoringOp::Delete { paths, .. } =
            plan_delete_container(&vfs, vault(), &sw(), entry, EntityKind::Marketplace).unwrap()
        else {
            panic!("expected a delete");
        };
        assert_eq!(paths, vec!["marketplaces/house"]);
    }

    // ── Convert ─────────────────────────────────────────────────────────────

    #[test]
    fn a_convert_moves_into_the_new_kinds_folder() {
        let vfs = one_plugin();
        let AuthoringOp::ConvertKind {
            to,
            frontmatter,
            scaffold,
            ..
        } = plan_convert_kind(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
            EntityKind::Command,
            &[],
        )
        .unwrap()
        else {
            panic!("expected a conversion");
        };
        assert_eq!(to, "marketplaces/house/plugins/tools/commands/review.md");
        assert!(frontmatter.is_empty());
        assert!(scaffold.is_empty());
    }

    #[test]
    fn a_convert_to_a_container_scaffolds_its_member_folders() {
        let vfs = one_plugin();
        let AuthoringOp::ConvertKind { to, scaffold, .. } = plan_convert_kind(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
            EntityKind::Plugin,
            &[],
        )
        .unwrap() else {
            panic!("expected a conversion");
        };
        // A plugin sits a level above its members, so the plugin level drops.
        assert_eq!(to, "marketplaces/house/plugins/review/review.md");
        assert_eq!(
            scaffold,
            vec![
                "marketplaces/house/plugins/review/skills",
                "marketplaces/house/plugins/review/commands",
                "marketplaces/house/plugins/review/agents",
                "marketplaces/house/plugins/review/hooks",
            ]
        );
    }

    #[test]
    fn a_convert_refuses_hook() {
        let vfs = one_plugin();
        assert_eq!(
            plan_convert_kind(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/skills/review.md",
                EntityKind::Hook,
                &[],
            )
            .unwrap_err(),
            "converting to or from `hook` is not offered here"
        );
    }

    #[test]
    fn a_convert_refuses_a_container_that_still_holds_members() {
        let vfs = one_plugin();
        assert_eq!(
            plan_convert_kind(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/tools.md",
                EntityKind::Skill,
                &[],
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/tools.md` still holds a component: \
             marketplaces/house/plugins/tools/skills/review.md"
        );
    }

    #[test]
    fn a_convert_never_drops_name_or_description() {
        let vfs = one_plugin();
        for key in UNDROPPABLE_KEYS {
            assert_eq!(
                plan_convert_kind(
                    &vfs,
                    vault(),
                    &sw(),
                    "marketplaces/house/plugins/tools/skills/review.md",
                    EntityKind::Command,
                    &[format!(" {key} ")],
                )
                .unwrap_err(),
                format!("` {key} ` is carried by every kind and is not dropped by a conversion")
            );
        }
    }

    #[test]
    fn a_convert_drops_the_keys_it_is_given_once_each() {
        let vfs = one_plugin();
        let AuthoringOp::ConvertKind { frontmatter, .. } = plan_convert_kind(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/skills/review.md",
            EntityKind::Command,
            &[
                "allowed-tools".to_owned(),
                " allowed-tools ".to_owned(),
                String::new(),
            ],
        )
        .unwrap() else {
            panic!("expected a conversion");
        };
        assert_eq!(frontmatter, vec![FieldEdit::remove("allowed-tools"),]);
    }

    #[test]
    fn a_convert_to_the_kind_it_already_is_is_refused() {
        let vfs = one_plugin();
        assert_eq!(
            plan_convert_kind(
                &vfs,
                vault(),
                &sw(),
                "marketplaces/house/plugins/tools/skills/review.md",
                EntityKind::Skill,
                &[],
            )
            .unwrap_err(),
            "`marketplaces/house/plugins/tools/skills/review.md` is already a skill"
        );
    }

    // ── Composition ─────────────────────────────────────────────────────────

    #[test]
    fn composing_a_new_note_seeds_a_frontmatter_block() {
        let out = compose_note(None, &fields("review"), "\n# Body\n").unwrap();
        assert_eq!(
            out,
            "---\nname: review\ndescription: Use this skill to test review.\n---\n\n# Body\n"
        );
    }

    #[test]
    fn applying_edits_keeps_the_body_and_the_unnamed_keys() {
        let source = "---\nname: review\ndescription: d\n---\n\n# Body\n";
        let out = apply_field_edits(
            source,
            &[
                FieldEdit::remove("sw-target"),
                FieldEdit::set("name", Value::Str("audit".to_owned())),
            ],
        )
        .unwrap();
        // The key is absent, so removing it changes nothing: the output keeps the
        // closing fence where it was.
        assert_eq!(out, "---\nname: audit\ndescription: d\n---\n\n# Body\n");
    }

    #[test]
    fn removing_a_key_that_is_not_there_is_a_no_op() {
        let source = "---\nname: review\n---\n\n# Body\n";
        assert_eq!(
            apply_field_edits(source, &[FieldEdit::remove("sw-target")]).unwrap(),
            source
        );
    }

    // ── Regressions ─────────────────────────────────────────────────────────

    fn read(vfs: &MemVfs, rel: &str) -> String {
        String::from_utf8(vfs.read_file(&Path::new(VAULT).join(rel)).unwrap()).unwrap()
    }

    /// Carry out a single-note op the way a host does: apply `edits` to the
    /// note at `from`, write the result at `to`, and remove `from`.
    fn apply_note(vfs: &MemVfs, from: &str, to: &str, edits: &[FieldEdit]) {
        let edited = apply_field_edits(&read(vfs, from), edits).unwrap();
        if from != to {
            vfs.remove(&Path::new(VAULT).join(from)).unwrap();
        }
        write(vfs, to, &edited);
    }

    fn rescan(vfs: &MemVfs) -> crate::discover::DiscoveryReport {
        discover_components(vfs, vault(), None, &sw()).unwrap()
    }

    #[test]
    fn a_note_named_after_its_kind_folder_is_a_lone_note() {
        let vfs = MemVfs::new();
        write(&vfs, "skills/skills.md", &note("skills", ""));
        write(&vfs, "skills/other.md", &note("other", ""));
        assert_eq!(bundle_dir("skills/skills.md"), None);
        assert_eq!(
            bundle_dir("marketplaces/m/plugins/p/skills/skills.md"),
            None
        );
        assert_eq!(bundle_dir("commands/commands.md"), None);

        let AuthoringOp::Delete { paths, .. } =
            plan_delete_component(&vfs, vault(), &sw(), "skills/skills.md").unwrap()
        else {
            panic!("expected a delete");
        };
        assert_eq!(paths, vec!["skills/skills.md"]);

        let AuthoringOp::Rename { to, bundle, .. } =
            plan_rename_component(&vfs, vault(), &sw(), "skills/skills.md", "renamed").unwrap()
        else {
            panic!("expected a rename");
        };
        assert_eq!(to, "skills/renamed.md");
        assert!(!bundle);
    }

    #[test]
    fn a_lone_skill_md_in_a_kind_folder_is_not_a_bundle() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "marketplaces/m/plugins/p/skills/SKILL.md",
            "---\ndescription: lone\n---\n",
        );
        write(
            &vfs,
            "marketplaces/m/plugins/p/skills/other/other.md",
            &note("other", ""),
        );
        let AuthoringOp::Delete { paths, .. } = plan_delete_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/m/plugins/p/skills/SKILL.md",
        )
        .unwrap() else {
            panic!("expected a delete");
        };
        assert_eq!(paths, vec!["marketplaces/m/plugins/p/skills/SKILL.md"]);

        let AuthoringOp::Rename { to, bundle, .. } = plan_rename_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/m/plugins/p/skills/SKILL.md",
            "lone",
        )
        .unwrap() else {
            panic!("expected a rename");
        };
        assert_eq!(to, "marketplaces/m/plugins/p/skills/lone.md");
        assert!(!bundle);
    }

    #[test]
    fn an_uppercase_md_bundle_is_still_a_bundle() {
        let vfs = MemVfs::new();
        write(&vfs, "skills/bund/bund.MD", &note("bund", ""));
        write(&vfs, "skills/bund/scripts/a.py", "print(1)\n");
        let AuthoringOp::Delete { paths, .. } =
            plan_delete_component(&vfs, vault(), &sw(), "skills/bund/bund.MD").unwrap()
        else {
            panic!("expected a delete");
        };
        assert_eq!(paths, vec!["skills/bund"]);

        // A container whose only member is an `.MD` note still holds it.
        write(&vfs, "marketplaces/m/plugins/p/p.md", &note("p", ""));
        write(
            &vfs,
            "marketplaces/m/plugins/p/skills/Only.MD",
            &note("only", ""),
        );
        let refusal = plan_convert_kind(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/m/plugins/p/p.md",
            EntityKind::Skill,
            &[],
        )
        .unwrap_err();
        assert!(refusal.contains("still holds a component"), "{refusal}");
    }

    #[test]
    fn a_container_with_uppercase_md_members_refuses_a_plain_delete() {
        let vfs = MemVfs::new();
        write(&vfs, "marketplaces/m/plugins/p/p.md", &note("p", ""));
        write(
            &vfs,
            "marketplaces/m/plugins/p/skills/Review.MD",
            "---\nname: review\ndescription: r\n---\n",
        );
        assert_eq!(rescan(&vfs).components.len(), 2);
        let refusal = plan_delete_component(&vfs, vault(), &sw(), "marketplaces/m/plugins/p/p.md")
            .unwrap_err();
        assert!(refusal.contains("still holds a component"), "{refusal}");
    }

    #[test]
    fn renaming_an_uppercase_md_bundle_renames_its_folder() {
        let vfs = MemVfs::new();
        write(&vfs, "skills/foo/foo.MD", "---\ndescription: f\n---\n");
        write(&vfs, "skills/foo/scripts/a.py", "print(1)\n");
        let AuthoringOp::Rename { to, bundle, .. } =
            plan_rename_component(&vfs, vault(), &sw(), "skills/foo/foo.MD", "bar").unwrap()
        else {
            panic!("expected a rename");
        };
        assert_eq!(to, "skills/bar/bar.md");
        assert!(bundle);
    }

    #[test]
    fn a_convert_writes_the_kind_into_a_nested_block() {
        let vfs = MemVfs::new();
        let from = "marketplaces/m/plugins/p/skills/a.md";
        write(
            &vfs,
            from,
            "---\nname: a\ndescription: d\nsw:\n  targets: [claude]\n---\n",
        );
        let AuthoringOp::ConvertKind {
            to, frontmatter, ..
        } = plan_convert_kind(&vfs, vault(), &sw(), from, EntityKind::Command, &[]).unwrap()
        else {
            panic!("expected a conversion");
        };
        apply_note(&vfs, from, &to, &frontmatter);
        let report = rescan(&vfs);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert_eq!(report.components.len(), 1);
        assert_eq!(report.components[0].kind, EntityKind::Command);
        assert_eq!(
            report.components[0].targets,
            Some(vec!["claude".to_owned()])
        );
    }

    #[test]
    fn a_container_delete_is_refused_when_the_scan_cannot_run() {
        let vfs = one_plugin_unscannable();

        let refusal = plan_delete_component(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/plugins/tools/tools.md",
        )
        .unwrap_err();
        assert!(refusal.contains("still holds a component"), "{refusal}");

        write(
            &vfs,
            "marketplaces/house/skills/deploy/deploy.md",
            &note("deploy", ""),
        );
        let refusal = plan_delete_container(
            &vfs,
            vault(),
            &sw(),
            "marketplaces/house/skills/deploy/deploy.md",
            EntityKind::Plugin,
        )
        .unwrap_err();
        assert!(
            refusal.contains("is not a plugin's own entry note"),
            "{refusal}"
        );
    }

    #[test]
    fn a_note_the_scratch_scan_skips_is_refused_whatever_its_kind() {
        let vfs = MemVfs::new();
        let bogus = "---\nname: c\ndescription: ok\nsw-targets: [bogus]\n---\n";
        let refusal = refusal_for(&vfs, bogus, "commands/c.md", vault(), &sw());
        assert!(
            refusal.as_deref().is_some_and(|r| r.contains("bogus")),
            "{refusal:?}"
        );
        let long = format!("---\nname: c\ndescription: {}\n---\n", "d".repeat(1100));
        assert!(refusal_for(&vfs, &long, "commands/c.md", vault(), &sw()).is_some());
    }

    #[test]
    fn a_version_in_a_nested_block_is_read_and_written_in_place() {
        let source = "---\nname: review\ndescription: d\nsw:\n  version: 2.3.0\n---\n\n# Body\n";
        assert_eq!(
            read_ns_field(source, &sw(), "version").unwrap().as_deref(),
            Some("2.3.0")
        );
        let edits = ns_field_edits(
            source,
            &sw(),
            &[("version", Some(Value::Str("2.3.1".to_owned())))],
        )
        .unwrap();
        let out = apply_field_edits(source, &edits).unwrap();
        let vfs = MemVfs::new();
        write(&vfs, "marketplaces/m/plugins/review/review.md", &out);
        let report = rescan(&vfs);
        assert!(report.skipped.is_empty(), "{out}\n{:?}", report.skipped);
        assert_eq!(report.components.len(), 1);
        assert_eq!(
            read_ns_field(&out, &sw(), "version").unwrap().as_deref(),
            Some("2.3.1")
        );

        // The flat shape still reads and writes as itself.
        let flat = "---\nname: review\nsw-version: 0.2.0\n---\n";
        assert_eq!(
            read_ns_field(flat, &sw(), "version").unwrap().as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            ns_field_edits(flat, &sw(), &[("version", None)]).unwrap(),
            vec![FieldEdit::remove("sw-version")]
        );
    }

    #[test]
    fn a_bundled_note_outside_the_component_tree_is_still_a_bundle() {
        assert_eq!(bundle_dir("notes/foo/foo.md"), Some("notes/foo"));
        assert_eq!(bundle_dir("notes/foo/SKILL.md"), Some("notes/foo"));
        assert_eq!(bundle_dir("notes/foo/bar.md"), None);
        assert_eq!(
            rename_destination("notes/foo/foo.md", "bar"),
            "notes/bar/bar.md"
        );
    }

    /// Under a kind folder, a path the shape rule gives no shape is a lone
    /// note, not a bundle owning the folder it sits in.
    #[test]
    fn an_unshaped_note_inside_a_kind_folder_is_not_a_bundle() {
        assert_eq!(bundle_dir("plugins/p/SKILL.md"), None);
        assert_eq!(bundle_dir("skills/x/y/y.md"), None);
        assert_eq!(
            rename_destination("plugins/p/SKILL.md", "q"),
            "plugins/p/q.md"
        );
    }

    #[test]
    fn a_skill_bundle_with_markdown_resources_deletes_when_the_scan_cannot_run() {
        let vfs = one_plugin_unscannable();

        let entry = "marketplaces/house/skills/deploy/deploy.md";
        write(&vfs, entry, &note("deploy", ""));
        write(
            &vfs,
            "marketplaces/house/skills/deploy/references/guide.md",
            "# Guide\n",
        );
        let AuthoringOp::Delete { paths, .. } =
            plan_delete_component(&vfs, vault(), &sw(), entry).unwrap()
        else {
            panic!("expected a delete");
        };
        assert_eq!(paths, vec!["marketplaces/house/skills/deploy"]);
    }

    #[test]
    fn a_convert_drops_our_own_keys_from_a_nested_block() {
        let vfs = MemVfs::new();
        let from = "marketplaces/m/plugins/p/p.md";
        write(
            &vfs,
            from,
            "---\nname: p\ndescription: d\nsw:\n  version: 1.0.0\n  targets: [claude]\n---\n",
        );
        let AuthoringOp::ConvertKind {
            to, frontmatter, ..
        } = plan_convert_kind(
            &vfs,
            vault(),
            &sw(),
            from,
            EntityKind::Skill,
            &["sw-version".to_owned()],
        )
        .unwrap()
        else {
            panic!("expected a conversion");
        };
        apply_note(&vfs, from, &to, &frontmatter);
        let out = read(&vfs, &to);
        assert!(!out.contains("version"), "{out}");
        let report = rescan(&vfs);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert_eq!(report.components[0].kind, EntityKind::Skill);

        // The block itself holds the kind the conversion writes, so it is not
        // a key a conversion can drop.
        write(
            &vfs,
            from,
            "---\nname: p\ndescription: d\nsw:\n  version: 1.0.0\n---\n",
        );
        let refusal = plan_convert_kind(
            &vfs,
            vault(),
            &sw(),
            from,
            EntityKind::Skill,
            &["sw".to_owned()],
        )
        .unwrap_err();
        assert!(refusal.contains("`sw`"), "{refusal}");
    }

    #[test]
    fn converting_an_uppercase_md_note_to_a_container_names_its_folder_by_the_stem() {
        let vfs = MemVfs::new();
        let from = "marketplaces/m/skills/tools.MD";
        write(&vfs, from, &note("tools", ""));
        let AuthoringOp::ConvertKind { to, .. } =
            plan_convert_kind(&vfs, vault(), &sw(), from, EntityKind::Plugin, &[]).unwrap()
        else {
            panic!("expected a conversion");
        };
        assert_eq!(to, "marketplaces/m/plugins/tools/tools.md");
    }

    /// A plugin's assets are not its members, wherever the plugin folder
    /// sits: they are matched by their path from the vault root.
    #[test]
    fn a_plugins_assets_are_not_members() {
        let vfs = crate::vfs::MemVfs::new();
        for (path, text) in [
            ("/v/plugins/p/p.md", "---\nname: p\n---\n"),
            ("/v/plugins/p/skills/s.md", "---\ndescription: S.\n---\n"),
            ("/v/plugins/p/assets/guide.md", "Plain.\n"),
        ] {
            let path = Path::new(path);
            vfs.create_dir_all(path.parent().unwrap()).unwrap();
            vfs.write_file(path, text.as_bytes()).unwrap();
        }
        let members =
            container_members(&vfs, Path::new("/v"), "plugins/p", "plugins/p/p.md", &sw());
        assert_eq!(members, vec!["plugins/p/skills/s.md".to_owned()]);
    }
}
