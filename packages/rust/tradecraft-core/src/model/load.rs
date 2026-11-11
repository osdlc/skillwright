//! Loading — a vault read into the marketplace model, in two passes.
//!
//! [`load`] reads the marketplace a build renders: one marketplace of the
//! vault, every plugin, the request's configuration folded under the notes'.
//! It reads through [`read_notes`] (pass 1) and resolves through
//! [`resolve_body`] (pass 2).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use markdown_link::LinkIndex;
use serde::{Deserialize, Serialize};

use super::cascade::{Cascade, belongs_to};
use super::{
    Asset, Body, Component, Extensions, HooksFile, Marketplace, Plugin, Resolved, Resource,
    ResourceContent, Segment, pack_kind, segments,
};
use crate::compile::{
    CallContext, CompileSource, IndexedSkill, Recorded, SkillIndex, SkillSource, compile_projection,
};
use crate::content::{SkillContent, SkillProjection};
use crate::discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, SkippedNote, discover_in,
    refuse_undescribed,
};
use crate::error::CoreError;
use crate::export::MarketplaceConfig;
use crate::frontmatter::{EntityKind, VaultFrontmatter};
use crate::harness::PLUGIN_HOOKS_PATH;
use crate::harness::manifest::plugin_version;
use crate::harness::{HarnessProfile, SkillNaming, with_plugin_format};
use crate::vault::{VaultNote, read_vault_note};

pub use crate::layout::CHANGELOG_FILE;
use crate::layout::{Layout, Place, SharedDir};
use crate::vfs::{Vfs, normalize};

/// What a marketplace load reads: this vault, this marketplace of it, for
/// these harnesses, under this configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LoadRequest {
    /// Absolute path to the vault.
    pub vault_root: PathBuf,
    /// Which marketplace to load, by folder. `None` names the vault's only
    /// one.
    #[serde(default)]
    pub marketplace: Option<String>,
    /// Which harnesses the model is for, by canonical name. Empty means every
    /// harness with a plugin format. A body no harness here admits is read
    /// but not resolved.
    #[serde(default)]
    pub harnesses: BTreeSet<String>,
    /// The marketplace's identity and per-plugin overrides, under what the
    /// vault's own notes say.
    pub config: MarketplaceConfig,
}

impl LoadRequest {
    /// A load of the vault at `vault_root` under `config`. Every other field
    /// starts empty; set it after.
    pub fn new(vault_root: impl Into<PathBuf>, config: MarketplaceConfig) -> Self {
        Self {
            vault_root: vault_root.into(),
            marketplace: None,
            harnesses: BTreeSet::new(),
            config,
        }
    }
}

/// Pass 1: every note of the layout, read into a node — its frontmatter,
/// kind, plugin, marketplace and files.
///
/// Beside the report it returns the vault's link index, built from the
/// layout's files, so pass 2 resolves every link without walking again. A
/// plugin's own `assets/` is left out of the index.
///
/// # Errors
///
/// What [`crate::discover::discover_components`] refuses.
pub fn read_notes(
    vfs: &dyn Vfs,
    layout: &Layout,
) -> Result<(DiscoveryReport, LinkIndex), CoreError> {
    let report = discover_in(vfs, layout)?;
    let files: Vec<PathBuf> = layout
        .files
        .iter()
        .filter(|file| {
            !matches!(
                file.place,
                Place::Shared {
                    skill: None,
                    dir: SharedDir::Assets,
                    ..
                }
            )
        })
        .map(|file| file.path.clone())
        .collect();
    let links = crate::compile::link_index(&layout.root, &files)?;
    Ok((report, links))
}

/// The harnesses `names` selects, every one with a plugin format when empty.
///
/// # Errors
///
/// `refused: unknown-harness` — a name no such harness answers to. Refused
/// rather than ignored: silently publishing less than was asked for is the
/// failure this crate is built around.
pub(crate) fn selected_harnesses(
    names: &BTreeSet<String>,
) -> Result<Vec<&'static HarnessProfile>, CoreError> {
    if names.is_empty() {
        return Ok(with_plugin_format().collect());
    }
    let unknown: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !with_plugin_format().any(|spec| spec.name == *name))
        .collect();
    if !unknown.is_empty() {
        return Err(CoreError::refused(
            "unknown-harness",
            format!(
                "no harness named {} — this build renders {}. Nothing was written",
                unknown.join(", "),
                with_plugin_format()
                    .map(|spec| spec.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    Ok(with_plugin_format()
        .filter(|spec| names.contains(spec.name))
        .collect())
}

/// Loads the marketplace a build renders.
///
/// # Errors
///
/// * `refused: unreadable-note` — a note the scan could not read. A build
///   swaps whole packs, and a note it could not read would be swept as a
///   note that was deleted.
/// * `refused: ambiguous-marketplace` / `unknown-marketplace` — which
///   marketplace.
/// * `refused: missing-description` — a component with no `description`.
/// * `refused: empty-marketplace` — the marketplace holds no plugin.
/// * `refused: path-collision` — two components of one name in one plugin.
/// * What [`crate::hooks_file::parse_hooks_file`] refuses in a plugin's
///   `hooks/hooks.json`.
/// * `refused: unknown-target` — a harness block named for no known harness.
/// * `refused: malformed` — a container block key holding the wrong
///   kind of value.
/// * Everything the resolve pass refuses; see [`resolve_body`].
/// * What [`crate::layout::read_layout`] refuses.
pub fn load(vfs: &dyn Vfs, request: &LoadRequest) -> Result<Marketplace, CoreError> {
    let layout = crate::layout::read_layout(vfs, &normalize(&request.vault_root))?;
    load_in(vfs, &layout, request)
}

/// [`load`] over a layout already read, so a build of several marketplaces
/// walks the vault once.
///
/// # Errors
///
/// As [`load`], less the layout's own.
pub fn load_in(
    vfs: &dyn Vfs,
    layout: &Layout,
    request: &LoadRequest,
) -> Result<Marketplace, CoreError> {
    let vault_root = layout.root.clone();
    let roots = selected_harnesses(&request.harnesses)?;

    // Pass 1.
    let (scan, links) = read_notes(vfs, layout)?;
    // The root file's own entry is configuration, not a note, and
    // `check_block_values` below refuses it naming the marketplace. A note in
    // another marketplace does not stop a build of this one.
    let market_prefix = request
        .marketplace
        .as_deref()
        .map(|m| format!("{}/{m}/", crate::layout::MARKETPLACES_DIR));
    let skipped: Vec<&SkippedNote> = scan
        .skipped
        .iter()
        .filter(|note| note.path != crate::settings::ROOT_FILE)
        .filter(|note| {
            market_prefix
                .as_deref()
                .is_none_or(|prefix| note.path.starts_with(prefix))
        })
        .collect();
    if !skipped.is_empty() {
        let (token, which) = skipped_wording(skipped.iter().copied());
        // A note's own reason is said only when it differs from the token.
        let notes: Vec<String> = skipped
            .iter()
            .map(|note| {
                let said = if note.reason == token {
                    note.detail.clone()
                } else {
                    note.sentence()
                };
                format!("{} ({said})", note.path)
            })
            .collect();
        return Err(CoreError::refused(
            token,
            format!(
                "the vault holds {} note(s) {which}, and a marketplace built \
                 without them would sweep away what they publish: {}. Nothing was written",
                notes.len(),
                notes.join("; ")
            ),
        ));
    }
    let mut discovered = scan.components;
    // Which marketplaces the vault holds is read from every kind, not from the
    // skills alone: one holding only commands still exists, and naming it must
    // say it holds nothing publishable rather than that it is not there.
    let market = resolve_marketplace(&discovered, request.marketplace.as_deref())?;
    let cascade = Cascade::over(&discovered, Some(&market), &request.config);
    // Before members inherit a block, so a wrong value names its container.
    crate::harness::manifest::check_block_values(&cascade.config)?;
    cascade.resolve_targets(&mut discovered);
    let belonging: Vec<&DiscoveredComponent> = discovered
        .iter()
        .filter(|component| belongs_to(component, Some(&market)))
        .collect();
    // Only this marketplace's notes: another marketplace's gap neither
    // refuses this build nor appears in its report.
    let ours: BTreeSet<&str> = belonging
        .iter()
        .map(|c| c.vault_relative_path.as_str())
        .collect();
    let mut warnings: Vec<DiscoveryWarning> = scan
        .warnings
        .into_iter()
        .filter(|w| ours.contains(w.path.as_str()))
        .collect();
    // Named, never dropped: a pack has a place for skills and commands and
    // for nothing else, and a build that quietly omitted an agent would be
    // the failure this crate is built around.
    let uncarried: Vec<Component> = belonging
        .iter()
        .filter(|c| !pack_kind(c.kind))
        .map(|c| unread(c))
        .collect();
    let of_kind = |kind: EntityKind| -> Vec<&DiscoveredComponent> {
        belonging
            .iter()
            .copied()
            .filter(|component| component.kind == kind)
            .collect()
    };
    let (skills, commands) = (of_kind(EntityKind::Skill), of_kind(EntityKind::Command));
    // Before `empty-marketplace`, so an undescribed note is named.
    refuse_undescribed(
        belonging
            .iter()
            .filter(|c| c.lacks_description())
            .map(|c| c.vault_relative_path.as_str()),
    )?;
    let plugin_notes: Vec<&DiscoveredComponent> = belonging
        .iter()
        .copied()
        .filter(|c| c.kind == EntityKind::Plugin && c.plugin.is_some())
        .collect();
    if plugin_notes.is_empty() && skills.is_empty() && commands.is_empty() {
        return Err(CoreError::refused(
            "empty-marketplace",
            format!(
                "marketplace `{market}` under {} holds no plugin: add \
                 `{}/{market}/{}/<p>/<p>.md`. Nothing was written",
                vault_root.display(),
                crate::layout::MARKETPLACES_DIR,
                crate::layout::PLUGINS_DIR,
            ),
        ));
    }
    let name = cascade
        .config
        .name
        .clone()
        .unwrap_or_else(|| market.clone());
    let mut plugins = group_by_plugin(&plugin_notes, &skills, &commands)?;
    let config = &cascade.config;
    warnings.extend(missing_versions(&discovered, &market, &plugins, config));
    pack_files(vfs, layout, &market, &mut plugins)?;
    warnings.extend(changelog_warnings(vfs, layout, &market, config));
    warnings.extend(load_hooks_files(
        vfs,
        layout,
        &market,
        &roots,
        &mut plugins,
    )?);

    // Pass 2. One index for every call, over every skill in the marketplace
    // under the plugin it ships in, whether or not a given harness ships it:
    // which harness ships what is asked when the call is written.
    let checked = skills.len();
    let index = skill_index(
        plugins.values().flat_map(|plugin| {
            plugin
                .skills
                .iter()
                .map(|skill| (&skill.discovered, Some(plugin.name.as_str())))
        }),
        Some(links),
    );
    let record = RefCell::new(Recorded::default());
    let calls = recording_context(SkillSource::ready(&index.skills), true, &record);
    for plugin in plugins.values_mut() {
        let plugin_name = plugin.name.clone();
        for component in plugin.skills.iter_mut().chain(plugin.commands.iter_mut()) {
            if !roots.iter().any(|root| admits(&component.discovered, root)) {
                continue;
            }
            let mut note = read_vault_note(vfs, &component.discovered.entry_path, component.kind)?;
            cascade.apply(&mut note, component.kind, Some(&plugin_name));
            component.extensions = extensions(&note.frontmatter, &component.discovered)?;
            let (resolved, content) = resolve_body(
                vfs,
                layout,
                &component.discovered,
                &note,
                component.kind,
                &calls,
                &index,
            )?;
            component.content = Some(content);
            component.resolved = Some(resolved);
        }
    }
    for warning in calls.warnings.into_inner() {
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
    warnings.sort_by(|a, b| a.path.cmp(&b.path));
    apply_naming(&mut plugins, config)?;

    Ok(Marketplace {
        name,
        folder: Some(market),
        config: cascade.config,
        plugins,
        uncarried,
        warnings,
        checked,
        vault_root,
    })
}

/// Pass 2 for one component: its body compiled once for every harness —
/// embeds expanded, links pointed at the files that travel with it, calls at
/// the skills they name — and split into segments.
///
/// # Errors
///
/// Everything [`crate::compile::compile_note`] refuses except
/// `skill-call-not-shipped`, which depends on the harness and is asked when
/// the call is written. `refused: no-renderer` for a command carrying peers:
/// a command projects to one file, with nowhere for a peer to travel.
pub fn resolve_body(
    vfs: &dyn Vfs,
    layout: &Layout,
    component: &DiscoveredComponent,
    note: &VaultNote,
    kind: EntityKind,
    calls: &CallContext<'_>,
    index: &Index,
) -> Result<(Resolved, SkillContent), CoreError> {
    let record = calls
        .record
        .expect("a load compiles through a recording context");
    let (mut resolved, content) = resolve_compiled(
        vfs,
        layout,
        component,
        note,
        kind,
        calls,
        record,
        index.links.as_ref(),
    )?;
    fill_targets(&mut resolved, &index.targets);
    Ok((resolved, content))
}

/// The skills a call may name, each one's `targets`, and the vault's link
/// index when pass 1 built it.
#[derive(Debug, Clone, Default)]
pub struct Index {
    skills: SkillIndex,
    targets: BTreeMap<PathBuf, Option<Vec<String>>>,
    links: Option<LinkIndex>,
}

fn skill_index<'a>(
    skills: impl Iterator<Item = (&'a DiscoveredComponent, Option<&'a str>)>,
    links: Option<LinkIndex>,
) -> Index {
    let mut targets = BTreeMap::new();
    let skills = SkillIndex::new(
        skills
            .filter(|(c, _)| c.kind == EntityKind::Skill)
            .map(|(c, plugin)| {
                targets.insert(normalize(&c.entry_path), c.targets.clone());
                IndexedSkill {
                    entry_path: c.entry_path.clone(),
                    plugin: plugin.map(str::to_owned),
                    name: c.name.clone(),
                    shipped: true,
                }
            })
            .collect::<Vec<_>>(),
    );
    Index {
        skills,
        targets,
        links,
    }
}

fn recording_context<'a>(
    skills: SkillSource<'a>,
    implicit: bool,
    record: &'a RefCell<Recorded>,
) -> CallContext<'a> {
    CallContext {
        skills,
        render: &no_render,
        // Read only by the not-shipped refusal, which a recording compile
        // never raises.
        target: "",
        implicit,
        warnings: RefCell::new(Vec::new()),
        record: Some(record),
    }
}

/// Refuses a single-file kind that carries peer resources. An agent and a
/// command are one file each, so peers would be dropped; naming them lets
/// the author move them, or publish the note as a skill.
fn refuse_carried_peers(projection: &SkillProjection, kind: &str) -> Result<(), CoreError> {
    if projection.resources.is_empty() {
        return Ok(());
    }
    let carried: Vec<&str> = projection
        .resources
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect();
    Err(CoreError::refused(
        "no-renderer",
        format!(
            "a {kind} publishes as a single file and carries no peers, but {} travels with \
             this note: {}. Move them, or publish this as a skill",
            carried.len(),
            carried.join(", ")
        ),
    ))
}

/// The renderer a recording compile never calls.
fn no_render(_: &IndexedSkill, _: bool) -> Result<String, CoreError> {
    Ok(String::new())
}

#[allow(clippy::too_many_arguments)]
fn resolve_compiled(
    vfs: &dyn Vfs,
    layout: &Layout,
    component: &DiscoveredComponent,
    note: &VaultNote,
    kind: EntityKind,
    calls: &CallContext<'_>,
    record: &RefCell<Recorded>,
    links: Option<&LinkIndex>,
) -> Result<(Resolved, SkillContent), CoreError> {
    let mut projection: SkillProjection = note.projection.clone();
    // Only the two kinds an invocation actually takes arguments at.
    if matches!(kind, EntityKind::Skill | EntityKind::Command) {
        projection
            .content
            .apply_arguments(&note.frontmatter.arguments);
    }
    // Each body starts the record afresh, so its marks count from zero.
    *record.borrow_mut() = Recorded::default();
    let closures = compile_projection(
        vfs,
        &CompileSource {
            vault_root: &layout.root,
            entry_path: &component.entry_path,
            bundle_root: component.bundle_root.as_deref(),
            skill_name: &component.name,
            calls: Some(calls),
            links,
            layout: Some(layout),
        },
        &mut projection,
    )?;
    // After the compile, not before: compiling adds what the body's links and
    // embeds reach to the resources, and a single-file kind has nowhere to
    // put those either.
    match kind {
        EntityKind::Command => refuse_carried_peers(&projection, "command")?,
        EntityKind::Agent => refuse_carried_peers(&projection, "agent")?,
        _ => {}
    }
    let record = record.borrow();
    let body = segments(&projection.content.body, &record);
    let mut content = projection.content;
    content.body = note.projection.content.body.clone();
    let resources = projection
        .resources
        .into_iter()
        .map(|(path, bytes, executable)| {
            let content = match String::from_utf8(bytes) {
                Ok(text) if path.to_lowercase().ends_with(".md") => ResourceContent::Body {
                    body: segments(&text, &record),
                },
                Ok(text) => ResourceContent::Bytes {
                    bytes: text.into_bytes(),
                },
                Err(err) => ResourceContent::Bytes {
                    bytes: err.into_bytes(),
                },
            };
            Resource {
                path,
                content,
                executable,
            }
        })
        .collect();
    Ok((
        Resolved {
            body,
            resources,
            closures,
        },
        content,
    ))
}

/// Gives every call the `targets` of the skill it names.
fn fill_targets(resolved: &mut Resolved, targets: &BTreeMap<PathBuf, Option<Vec<String>>>) {
    for body in bodies_of(resolved) {
        fill_body_targets(body, targets);
    }
}

/// A resolved component's body and each resource's.
fn bodies_of(resolved: &mut Resolved) -> impl Iterator<Item = &mut Body> {
    std::iter::once(&mut resolved.body).chain(resolved.resources.iter_mut().filter_map(
        |resource| match &mut resource.content {
            ResourceContent::Body { body } => Some(body),
            ResourceContent::Bytes { .. } => None,
        },
    ))
}

/// Marks each plugin whose `<harness>: {naming: …}` names its skills against
/// that harness's default, and every call to one of its skills, wherever the
/// call sits. Only a harness whose profile allows the choice is read.
fn apply_naming(
    plugins: &mut BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    // Per plugin, the harnesses that name it bare and those that prefix it,
    // each against that harness's default.
    let mut chosen: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for profile in crate::harness::with_plugin_format().filter(|p| p.format().naming_choice) {
        let default = profile.format().naming;
        for name in plugins.keys() {
            let naming = crate::harness::manifest::naming_of(config, name, profile)?;
            if naming == default {
                continue;
            }
            let (bare, prefixed) = chosen.entry(name.clone()).or_default();
            match naming {
                SkillNaming::Bare => bare.push(profile.name.to_owned()),
                SkillNaming::PluginPrefixed => prefixed.push(profile.name.to_owned()),
                SkillNaming::HostNamespaced => {}
            }
        }
    }
    if chosen.is_empty() {
        return Ok(());
    }
    for plugin in plugins.values_mut() {
        if let Some((bare, prefixed)) = chosen.get(&plugin.name) {
            plugin.bare_on.clone_from(bare);
            plugin.prefixed_on.clone_from(prefixed);
        }
        let members = plugin.skills.iter_mut().chain(&mut plugin.commands);
        for resolved in members.filter_map(|member| member.resolved.as_mut()) {
            for body in bodies_of(resolved) {
                for segment in &mut body.segments {
                    if let Segment::Call { call } = segment
                        && let Some((bare, prefixed)) =
                            call.skill.plugin.as_ref().and_then(|p| chosen.get(p))
                    {
                        call.bare_on.clone_from(bare);
                        call.prefixed_on.clone_from(prefixed);
                    }
                }
            }
        }
    }
    Ok(())
}

fn fill_body_targets(body: &mut Body, targets: &BTreeMap<PathBuf, Option<Vec<String>>>) {
    for segment in &mut body.segments {
        if let Segment::Call { call } = segment {
            call.targets = targets
                .get(&normalize(&call.skill.entry_path))
                .cloned()
                .flatten();
        }
    }
}

/// A node read in pass 1 and not yet resolved.
fn unread(component: &DiscoveredComponent) -> Component {
    Component {
        kind: component.kind,
        name: component.name.clone(),
        path: component.vault_relative_path.clone(),
        targets: component.targets.clone(),
        extensions: Extensions::default(),
        content: None,
        resolved: None,
        discovered: component.clone(),
    }
}

/// The token and the words for notes the scan skipped: `malformed` when each
/// one parsed but holds a value of the wrong kind, else `unreadable-note`.
fn skipped_wording<'a>(
    notes: impl IntoIterator<Item = &'a SkippedNote>,
) -> (&'static str, &'static str) {
    let (mut wrong_kind, mut other) = (false, false);
    for note in notes {
        if note.reason == "malformed" {
            wrong_kind = true;
        } else {
            other = true;
        }
    }
    match (wrong_kind, other) {
        (true, false) => ("malformed", "with a value of the wrong kind"),
        (true, true) => ("unreadable-note", "this scan could not read or refused"),
        _ => ("unreadable-note", "this scan could not read"),
    }
}

/// A note's harness blocks, folded onto canonical names, each checked against
/// its own harness (D-ISOU).
///
/// # Errors
///
/// What [`super::cascade::fold_harness_blocks`] refuses.
fn extensions(
    frontmatter: &VaultFrontmatter,
    component: &DiscoveredComponent,
) -> Result<Extensions, CoreError> {
    let blocks =
        super::cascade::fold_harness_blocks(frontmatter, &component.entry_path, component.kind)?;
    Ok(Extensions { blocks })
}

// --- What the build used to decide before it rendered ------------------------

/// Does this member publish to this harness? Two gates: the harness's pack
/// has a place for the member's kind, and its `targets` names the harness.
pub(crate) fn admits(component: &DiscoveredComponent, spec: &HarnessProfile) -> bool {
    kind_carried(component, spec) && targets_admit(component, spec.name)
}

/// Gate 1 of [`admits`]: whether this harness's pack format has a place for
/// the member's kind.
pub(crate) fn kind_carried(component: &DiscoveredComponent, spec: &HarnessProfile) -> bool {
    match component.kind {
        EntityKind::Command => spec.format().carries_commands,
        _ => true,
    }
}

/// Gate 2 of [`admits`]: whether the member's `targets` names this
/// destination. Absent is every destination, empty is none.
pub(crate) fn targets_admit(component: &DiscoveredComponent, canonical: &str) -> bool {
    targets_name(component.targets.as_deref(), canonical)
}

/// [`targets_admit`] over a bare list.
pub(crate) fn targets_name(targets: Option<&[String]>, canonical: &str) -> bool {
    match targets {
        None => true,
        Some(names) => names.iter().any(|name| name == canonical),
    }
}

/// A warning for each plugin of `market` whose note states no version and
/// the config sets none. It ships as
/// [`crate::harness::manifest::DEFAULT_PLUGIN_VERSION`], and a harness
/// compares versions to decide whether an installed pack is out of date, so a
/// pack that never states one never looks updated.
fn missing_versions(
    discovered: &[DiscoveredComponent],
    market: &str,
    plugins: &BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Vec<DiscoveryWarning> {
    discovered
        .iter()
        .filter(|c| c.kind == EntityKind::Plugin && belongs_to(c, Some(market)))
        .filter(|c| plugins.contains_key(c.folder()))
        .filter(|c| {
            config
                .plugins
                .get(c.folder())
                .and_then(|o| o.identity.version.as_ref())
                .is_none()
        })
        .map(|c| DiscoveryWarning {
            path: c.vault_relative_path.clone(),
            reason: "missing-version".to_owned(),
            detail: format!(
                "plugin `{}` states no version; published as `0.1.0`. Set `version` on \
                 its note so a harness can tell an update from what it has",
                c.name
            ),
        })
        .collect()
}

/// Which marketplace this build renders, by folder.
///
/// # Errors
///
/// `refused: unknown-marketplace` when `requested` names none of them, and
/// `refused: ambiguous-marketplace` when it names none and the vault holds
/// several. `refused: empty-marketplace` for a vault with none.
fn resolve_marketplace(
    discovered: &[DiscoveredComponent],
    requested: Option<&str>,
) -> Result<String, CoreError> {
    let present: BTreeSet<&str> = discovered
        .iter()
        .filter(|c| c.kind == EntityKind::Marketplace)
        .map(DiscoveredComponent::folder)
        .collect();
    match (requested, present.len()) {
        (Some(name), _) if present.contains(name) => Ok(name.to_owned()),
        (Some(name), _) => Err(CoreError::refused(
            "unknown-marketplace",
            format!(
                "no marketplace `{name}` in this vault: it holds {}",
                if present.is_empty() {
                    "none".to_owned()
                } else {
                    named(&present)
                }
            ),
        )),
        (None, 1) => Ok(present.first().map(|m| (*m).to_owned()).unwrap_or_default()),
        (None, 0) => Err(CoreError::refused(
            "empty-marketplace",
            "this vault holds no marketplace: add `marketplaces/<m>/<m>.md`. Nothing was written",
        )),
        (None, _) => Err(CoreError::refused(
            "ambiguous-marketplace",
            format!(
                "this vault holds {} and one output root holds one of them — name the one to \
                 build",
                named(&present)
            ),
        )),
    }
}

/// `` `a`, `b` and `c` `` — for a refusal that has to name what it found.
fn named(names: &BTreeSet<&str>) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Plugins, members in discovery's name order.
///
/// # Errors
///
/// `refused: path-collision` — two members of the same kind landing on one
/// directory.
fn group_by_plugin(
    notes: &[&DiscoveredComponent],
    skills: &[&DiscoveredComponent],
    commands: &[&DiscoveredComponent],
) -> Result<BTreeMap<String, Plugin>, CoreError> {
    type Pick = fn(&mut Plugin) -> &mut Vec<Component>;
    let kinds: [(&[&DiscoveredComponent], Pick); 2] = [
        (skills, |plugin| &mut plugin.skills),
        (commands, |plugin| &mut plugin.commands),
    ];
    // Every plugin note, so a plugin with no component still builds.
    let mut plugins: BTreeMap<String, Plugin> = notes
        .iter()
        .filter_map(|note| {
            let name = note.plugin.clone()?;
            let plugin = Plugin {
                name: name.clone(),
                targets: note.targets.clone(),
                ..Plugin::default()
            };
            Some((name, plugin))
        })
        .collect();
    for (members, pick) in kinds {
        for member in members {
            let name = member.plugin.as_deref().unwrap_or_default();
            let plugin = plugins.entry(name.to_owned()).or_insert_with(|| Plugin {
                name: name.to_owned(),
                ..Plugin::default()
            });
            let siblings = pick(plugin);
            if let Some(first) = siblings.iter().find(|m| m.name == member.name) {
                return Err(CoreError::refused(
                    "path-collision",
                    format!(
                        "`{}` and `{}` both render as {} `{}` of plugin `{name}` — two \
                         components cannot publish onto one directory",
                        first.path,
                        member.vault_relative_path,
                        member.kind.as_str(),
                        member.name
                    ),
                ));
            }
            siblings.push(unread(member));
        }
    }
    Ok(plugins)
}

/// What a plugin ships at its pack root, read from the layout: its
/// `CHANGELOG.md`, and its `references/`, `scripts/` and `assets/` under
/// their own names (spec section 9). Each is copied byte for byte.
fn pack_files(
    vfs: &dyn Vfs,
    layout: &Layout,
    market: &str,
    plugins: &mut BTreeMap<String, Plugin>,
) -> Result<(), CoreError> {
    for file in layout.files_of(market) {
        let dest = match &file.place {
            Place::Changelog => CHANGELOG_FILE,
            Place::Shared {
                skill: None, path, ..
            } => path.as_str(),
            _ => continue,
        };
        let Some(plugin) = file.plugin.as_ref().and_then(|p| plugins.get_mut(p)) else {
            continue;
        };
        plugin.assets.push(Asset {
            dest: dest.to_owned(),
            bytes: vfs.read_file(&file.path)?,
            executable: vfs.is_executable(&file.path),
        });
    }
    Ok(())
}

/// A warning for each plugin whose `CHANGELOG.md` has no heading naming the
/// version the plugin ships, so a release does not go out undescribed.
fn changelog_warnings(
    vfs: &dyn Vfs,
    layout: &Layout,
    market: &str,
    config: &MarketplaceConfig,
) -> Vec<DiscoveryWarning> {
    let mut warnings = Vec::new();
    for file in layout.files_of(market) {
        let (Place::Changelog, Some(plugin)) = (&file.place, file.plugin.as_deref()) else {
            continue;
        };
        let version = plugin_version(config, plugin);
        let Ok(bytes) = vfs.read_file(&file.path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let named = text.lines().any(|line| heading_names(line, &version));
        if !named {
            warnings.push(DiscoveryWarning {
                path: file.rel.clone(),
                reason: "changelog-missing-version".to_owned(),
                detail: format!(
                    "plugin `{plugin}` ships version `{version}`, and its `{CHANGELOG_FILE}` has \
                     no heading naming it"
                ),
            });
        }
    }
    warnings
}

/// Whether `line` is a heading naming `version` as a whole token, with or
/// without a leading `v`: `## [1.2.0] - 2026-01-02` names `1.2.0`, and
/// `## 11.2.0` and `## 1.2.0-rc.1` do not.
fn heading_names(line: &str, version: &str) -> bool {
    line.trim_start().starts_with('#')
        && line
            .split(|c: char| !(c.is_alphanumeric() || "+-._".contains(c)))
            .any(|token| token.strip_prefix('v').unwrap_or(token) == version)
}

/// Each plugin's `hooks/hooks.json`, read and parsed into the plugin, and a
/// warning for each one whose Cursor pack this build writes, since that pack
/// ships no hooks (spec section 4 rule 23).
///
/// # Errors
///
/// What [`crate::hooks_file::parse_hooks_file`] refuses, and a read failure.
fn load_hooks_files(
    vfs: &dyn Vfs,
    layout: &Layout,
    market: &str,
    roots: &[&'static HarnessProfile],
    plugins: &mut BTreeMap<String, Plugin>,
) -> Result<Vec<DiscoveryWarning>, CoreError> {
    let mut warnings = Vec::new();
    for file in layout.files_of(market) {
        let (Place::HooksFile, Some(plugin)) = (&file.place, file.plugin.as_deref()) else {
            continue;
        };
        let Some(group) = plugins.get_mut(plugin) else {
            continue;
        };
        let text = String::from_utf8(vfs.read_file(&file.path)?).map_err(|_| {
            CoreError::refused("malformed-hooks-file", format!("{}: not UTF-8", file.rel))
        })?;
        let hooks = crate::hooks_file::parse_hooks_file(&text, &file.rel)?;
        group.hooks_file = Some(HooksFile { hooks });
        let cursor_built = roots.iter().any(|root| {
            root.name == "cursor" && group.carried_by(root.name, |c| admits(&c.discovered, root))
        });
        if cursor_built {
            warnings.push(DiscoveryWarning {
                path: file.rel.clone(),
                reason: "cursor-hooks-left-out".to_owned(),
                detail: format!(
                    "plugin `{plugin}`'s Cursor pack ships no hooks from `{PLUGIN_HOOKS_PATH}`: \
                     Cursor does not document its plugin-root variable for hook commands"
                ),
            });
        }
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;
    use std::path::Path;

    fn vault() -> MemVfs {
        let vfs = MemVfs::new();
        for (path, text) in [
            ("/vault/skillwright.md", "---\n---\n"),
            ("/vault/notes/elsewhere.md", "outside marketplaces/\n"),
            ("/vault/marketplaces/m/m.md", "---\n---\n"),
            ("/vault/marketplaces/m/plugins/p/p.md", "---\n---\n"),
            (
                "/vault/marketplaces/m/plugins/p/skills/demo/demo.md",
                "---\ndescription: d\n---\nSee [[guide]].\n",
            ),
            (
                "/vault/marketplaces/m/plugins/p/references/guide.md",
                "A guide.\n",
            ),
        ] {
            vfs.create_dir_all(Path::new(path).parent().unwrap())
                .unwrap();
            vfs.write_file(Path::new(path), text.as_bytes()).unwrap();
        }
        vfs
    }

    /// Pass 1 hands on an index of the files it read, `marketplaces/` only,
    /// so pass 2 resolves links without walking again.
    #[test]
    fn pass_one_indexes_what_it_read_and_nothing_outside_marketplaces() {
        let vfs = vault();
        let layout = crate::layout::read_layout(&vfs, Path::new("/vault")).unwrap();
        let (report, links) = read_notes(&vfs, &layout).unwrap();
        assert_eq!(report.components.len(), 3);
        assert_eq!(
            links.resolve("guide"),
            Some("marketplaces/m/plugins/p/references/guide.md")
        );
        assert_eq!(links.resolve("elsewhere"), None);
    }
}
