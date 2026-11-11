//! Loading — a vault read into the marketplace model, in two passes.
//!
//! [`load`] reads the marketplace a build renders: one marketplace of the
//! vault, every plugin, the request's configuration folded under the notes'.
//! It reads through [`read_notes`] (pass 1) and resolves through
//! [`resolve_body`] (pass 2).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use markdown_link::LinkIndex;
use serde::{Deserialize, Serialize};

use super::cascade::{Cascade, belongs_to};
use super::{
    Asset, Body, Component, Extensions, HooksFile, Marketplace, Plugin, Resolved, Resource,
    ResourceContent, Segment, pack_kind, segments,
};
use crate::compile::{
    CallContext, CompileSource, EXCLUDED_ASSET_DIRECTORIES, IndexedSkill, Recorded, SkillIndex,
    SkillSource, compile_projection,
};
use crate::content::{SkillContent, SkillProjection};
use crate::discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, SkippedNote, discover_in,
    refuse_invalid_names, refuse_undescribed,
};
use crate::error::CoreError;
use crate::export::{AssetSpec, HookFileSpec, MarketplaceConfig};
use crate::frontmatter::{EntityKind, VaultFrontmatter};
use crate::harness::manifest::plugin_version;
use crate::harness::{HarnessProfile, SkillNaming, with_plugin_format};
use crate::namespace::Namespace;
use crate::projection::parse_runtime_target;
use crate::publish::{PLUGIN_HOOKS_PATH, canonical_target, refuse_carried_peers};
use crate::vault::{VaultNote, read_vault_note};

pub use crate::layout::CHANGELOG_FILE;
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
    /// Files copied verbatim into a plugin's pack.
    #[serde(default)]
    pub assets: Vec<AssetSpec>,
    /// Canonical hooks files.
    #[serde(default)]
    pub hooks: Vec<HookFileSpec>,
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
            assets: Vec::new(),
            hooks: Vec::new(),
        }
    }
}

/// Pass 1: every note in the vault, read into a node — its frontmatter, kind,
/// plugin, marketplace and files.
///
/// The one function here that walks the vault. Beside the report it returns
/// the vault's link index, built from the same walk, so pass 2 resolves every
/// link without walking again.
///
/// # Errors
///
/// What [`crate::discover::discover_components`] refuses.
pub fn read_notes(
    vfs: &dyn Vfs,
    vault_root: &Path,
    ns: &Namespace,
) -> Result<(DiscoveryReport, LinkIndex), CoreError> {
    let layout = crate::layout::read_layout(vfs, vault_root)?;
    let report = discover_in(vfs, &layout, ns)?;
    let files: Vec<PathBuf> = layout
        .files
        .iter()
        .filter(|path| {
            path.strip_prefix(vault_root).is_ok_and(|rel| {
                !crate::walk::in_plugin_assets(&crate::skill::forward_slashed(rel))
            })
        })
        .cloned()
        .collect();
    let links = crate::compile::link_index(vault_root, &files)?;
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
/// * `refused: missing-description` — a member with a `name` and no
///   `description`.
/// * `refused: empty-marketplace` — the marketplace holds no plugin.
/// * `refused: path-collision` — two components of one name in one plugin.
/// * `refused: unsafe-asset-dest` / `asset-over-manifest` /
///   `unknown-asset-plugin` — an asset that cannot land where it says.
/// * `refused: unknown-hooks-plugin` / `duplicate-hooks-file` and what
///   [`crate::hooks_file::parse_hooks_file`] refuses.
/// * `refused: unknown-target` — a harness block named for no known harness.
/// * `refused: malformed` — a container block key holding the wrong
///   kind of value.
/// * Everything the resolve pass refuses; see [`resolve_body`].
pub fn load(
    vfs: &dyn Vfs,
    request: &LoadRequest,
    ns: &Namespace,
) -> Result<Marketplace, CoreError> {
    let vault_root = normalize(&request.vault_root);
    let roots = selected_harnesses(&request.harnesses)?;

    // Pass 1.
    let (scan, links) = read_notes(vfs, &vault_root, ns)?;
    // The root file's own entry is configuration, not a note, and
    // `check_block_values` below refuses it naming the marketplace.
    let skipped: Vec<&SkippedNote> = scan
        .skipped
        .iter()
        .filter(|note| note.path != crate::settings::ROOT_FILE)
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
    refuse_invalid_names(belonging.iter().copied())?;
    let plugin_notes: Vec<&str> = belonging
        .iter()
        .filter(|c| c.kind == EntityKind::Plugin)
        .filter_map(|c| c.plugin.as_deref())
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
    let folder_assets = folder_assets(vfs, &vault_root, &market, &plugins);
    warnings.extend(changelog_warnings(vfs, &vault_root, &folder_assets, config));
    load_assets(
        vfs,
        &folder_assets,
        &request.assets,
        &roots,
        &mut plugins,
        config,
    )?;
    load_hook_files(vfs, &request.hooks, &mut plugins)?;

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
        let launcher = plugin.launcher.clone();
        let plugin_name = plugin.name.clone();
        for component in plugin
            .skills
            .iter_mut()
            .chain(plugin.commands.iter_mut())
            .chain(plugin.hooks.iter_mut())
        {
            if !roots.iter().any(|root| admits(&component.discovered, root)) {
                continue;
            }
            let mut note =
                read_vault_note(vfs, &component.discovered.entry_path, ns, component.kind)?;
            cascade.apply(&mut note, component.kind, Some(&plugin_name), ns);
            component.extensions = extensions(&note.frontmatter, &component.discovered, ns)?;
            let (resolved, content) = match component.kind {
                // A hook's body is a script or a prompt inside a JSON string:
                // an embed expanded into it would rewrite what runs, so it is
                // taken verbatim. The hooks document is one file, with nowhere
                // for a peer to travel.
                EntityKind::Hook => {
                    refuse_carried_peers(&note.projection, "hook")?;
                    (None, note.projection.content.clone())
                }
                kind => {
                    let (resolved, content) = resolve_body(
                        vfs,
                        &vault_root,
                        &component.discovered,
                        &note,
                        kind,
                        &calls,
                        &index,
                        launcher.as_deref(),
                    )?;
                    (Some(resolved), content)
                }
            };
            component.content = Some(content);
            component.source = Some(note.projection.resources);
            component.resolved = resolved;
        }
    }
    for warning in calls.warnings.into_inner() {
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
    warnings.sort_by(|a, b| a.path.cmp(&b.path));
    apply_naming(&mut plugins, config)?;
    crate::harness::manifest::check_packs(config)?;

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
#[allow(clippy::too_many_arguments)]
pub fn resolve_body(
    vfs: &dyn Vfs,
    vault_root: &Path,
    component: &DiscoveredComponent,
    note: &VaultNote,
    kind: EntityKind,
    calls: &CallContext<'_>,
    index: &Index,
    launcher: Option<&str>,
) -> Result<(Resolved, SkillContent), CoreError> {
    let record = calls
        .record
        .expect("a load compiles through a recording context");
    let (mut resolved, content) = resolve_compiled(
        vfs,
        vault_root,
        component,
        note,
        kind,
        calls,
        record,
        launcher,
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

/// The renderer a recording compile never calls.
fn no_render(_: &IndexedSkill, _: bool) -> Result<String, CoreError> {
    Ok(String::new())
}

#[allow(clippy::too_many_arguments)]
fn resolve_compiled(
    vfs: &dyn Vfs,
    vault_root: &Path,
    component: &DiscoveredComponent,
    note: &VaultNote,
    kind: EntityKind,
    calls: &CallContext<'_>,
    record: &RefCell<Recorded>,
    launcher: Option<&str>,
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
            vault_root,
            entry_path: &component.entry_path,
            bundle_root: component.bundle_root.as_deref(),
            skill_name: &component.name,
            calls: Some(calls),
            links,
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
    let body = segments(&projection.content.body, &record, launcher);
    let mut content = projection.content;
    content.body = note.projection.content.body.clone();
    let resources = projection
        .resources
        .into_iter()
        .map(|(path, bytes, executable)| {
            let content = match String::from_utf8(bytes) {
                Ok(text) if path.to_lowercase().ends_with(".md") => ResourceContent::Body {
                    body: segments(&text, &record, None),
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
        let members = plugin
            .skills
            .iter_mut()
            .chain(&mut plugin.commands)
            .chain(&mut plugin.hooks);
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
        source: None,
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
/// What [`crate::publish::overrides_for`] refuses.
fn extensions(
    frontmatter: &VaultFrontmatter,
    component: &DiscoveredComponent,
    ns: &Namespace,
) -> Result<Extensions, CoreError> {
    let blocks =
        crate::publish::overrides_for(frontmatter, &component.entry_path, component.kind, ns)?;
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
        EntityKind::Hook => spec.format().carries_hooks,
        _ => true,
    }
}

/// Gate 2 of [`admits`]: whether the member's `targets` names this
/// destination. Absent is every destination, empty is none — matched on the
/// destination a name resolves to, so `claude-code` admits `claude`.
pub(crate) fn targets_admit(component: &DiscoveredComponent, canonical: &str) -> bool {
    targets_name(component.targets.as_deref(), canonical)
}

/// [`targets_admit`] over a bare list.
pub(crate) fn targets_name(targets: Option<&[String]>, canonical: &str) -> bool {
    match targets {
        None => true,
        Some(names) => names
            .iter()
            .any(|name| canonical_target(name) == Some(canonical)),
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
    notes: &[&str],
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
        .map(|name| {
            let plugin = Plugin {
                name: (*name).to_owned(),
                ..Plugin::default()
            };
            ((*name).to_owned(), plugin)
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

/// The plugins a build holds, for a refusal that names a missing one.
fn plugin_list(plugins: &BTreeMap<String, Plugin>) -> String {
    if plugins.is_empty() {
        return "no plugins".to_owned();
    }
    plugins
        .keys()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every asset, checked and read into the plugin it lands in.
///
/// A plugin an asset names but the vault never produces is refused up front,
/// the same way an unknown `--marketplace` is: writing nothing rather than
/// silently dropping bytes an author declared.
///
/// A `configured` directory source lands file by file, with
/// [`EXCLUDED_ASSET_DIRECTORIES`] pruned at every depth and every `*.test.*`
/// file skipped, so a shared tree of reference docs does not carry its own
/// test suite into the build. A `folder` asset, from the convention, lands
/// whole. An asset is never compiled: its whole point is to ship exactly the
/// bytes on disk.
fn load_assets(
    vfs: &dyn Vfs,
    folder: &[AssetSpec],
    configured: &[AssetSpec],
    roots: &[&HarnessProfile],
    plugins: &mut BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    let mut checked: Vec<(&AssetSpec, String, bool)> = Vec::new();
    let assets = folder
        .iter()
        .map(|asset| (asset, true))
        .chain(configured.iter().map(|asset| (asset, false)));
    for (asset, whole) in assets {
        if !asset_dest_is_relative(&asset.dest) {
            return Err(CoreError::refused(
                "unsafe-asset-dest",
                format!(
                    "`{}`'s `dest` (`{}`) is not a plain relative path — `AssetSpec::dest` must \
                     stay under the plugin root it is joined onto, so an absolute path or a `..` \
                     component is refused rather than silently writing outside \
                     `<packs>/<plugin>/`. Nothing was written",
                    asset.source.display(),
                    asset.dest,
                ),
            ));
        }
        if let Some(spec) = roots
            .iter()
            .find(|spec| asset_covers(vfs, asset, spec.format().plugin_manifest))
        {
            return Err(CoreError::refused(
                "asset-over-manifest",
                format!(
                    "`{}` lands at `{}`, the {} root's pack manifest, which the build writes \
                     itself. Move the asset's `dest`. Nothing was written",
                    asset.source.display(),
                    spec.format().plugin_manifest,
                    spec.name,
                ),
            ));
        }
        let plugin = asset.plugin.as_deref().unwrap_or_default();
        if !plugins.contains_key(plugin) {
            return Err(CoreError::refused(
                "unknown-asset-plugin",
                format!(
                    "`{}` names plugin `{plugin}`, which this build holds no skill, command or \
                     hook for — this vault publishes {}. Nothing was written",
                    asset.source.display(),
                    plugin_list(plugins),
                ),
            ));
        }
        checked.push((asset, plugin.to_owned(), whole));
    }
    for (asset, plugin, whole) in checked {
        let files = read_asset(vfs, asset, whole)?;
        let plugin = plugins
            .get_mut(&plugin)
            .expect("the plugin was checked above");
        // Launcher calls are rewritten only for a plugin that ships the
        // launcher, by `asset_covers`'s rule: a `./`-spelled dest or a
        // directory asset holding it counts.
        if let Some(launcher) = &config.launcher_path
            && asset_covers(vfs, asset, launcher)
        {
            plugin.launcher = Some(launcher.clone());
        }
        plugin.assets.extend(files);
    }
    Ok(())
}

/// The folder inside a plugin folder whose contents land at the plugin's
/// root in every pack.
pub const ASSETS_DIR: &str = "assets";

/// The assets each plugin folder supplies: its `assets/`, copied to the
/// plugin root, and its `CHANGELOG.md`, copied beside the manifest.
fn folder_assets(
    vfs: &dyn Vfs,
    vault_root: &Path,
    market: &str,
    plugins: &BTreeMap<String, Plugin>,
) -> Vec<AssetSpec> {
    let mut specs = Vec::new();
    for name in plugins.keys() {
        let folder = vault_root
            .join(crate::layout::MARKETPLACES_DIR)
            .join(market)
            .join(crate::layout::PLUGINS_DIR)
            .join(name);
        let assets = folder.join(ASSETS_DIR);
        if is_directory(vfs, &assets) {
            specs.push(AssetSpec {
                source: assets,
                dest: String::new(),
                plugin: Some(name.clone()),
            });
        }
        let changelog = folder.join(CHANGELOG_FILE);
        if vfs.exists(&changelog) && !is_directory(vfs, &changelog) {
            specs.push(AssetSpec {
                source: changelog,
                dest: CHANGELOG_FILE.to_owned(),
                plugin: Some(name.clone()),
            });
        }
    }
    specs
}

/// A warning for each plugin whose `CHANGELOG.md` has no heading naming the
/// version the plugin ships, so a release does not go out undescribed.
fn changelog_warnings(
    vfs: &dyn Vfs,
    vault_root: &Path,
    assets: &[AssetSpec],
    config: &MarketplaceConfig,
) -> Vec<DiscoveryWarning> {
    let mut warnings = Vec::new();
    for asset in assets.iter().filter(|a| a.dest == CHANGELOG_FILE) {
        let Some(plugin) = asset.plugin.as_deref() else {
            continue;
        };
        let version = plugin_version(config, plugin);
        let Ok(bytes) = vfs.read_file(&asset.source) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let named = text.lines().any(|line| heading_names(line, &version));
        if !named {
            warnings.push(DiscoveryWarning {
                path: asset
                    .source
                    .strip_prefix(vault_root)
                    .map(crate::skill::forward_slashed)
                    .unwrap_or_else(|_| asset.source.display().to_string()),
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

/// One asset's files, read verbatim. A directory asset is filtered unless
/// `whole`.
fn read_asset(vfs: &dyn Vfs, asset: &AssetSpec, whole: bool) -> Result<Vec<Asset>, CoreError> {
    let copy = |source: &Path, dest: String| -> Result<Asset, CoreError> {
        Ok(Asset {
            dest,
            bytes: vfs.read_file(source)?,
            executable: vfs.is_executable(source),
        })
    };
    if !is_directory(vfs, &asset.source) {
        return Ok(vec![copy(&asset.source, asset.dest.clone())?]);
    }
    let mut files = Vec::new();
    let excluded = if whole {
        &[]
    } else {
        EXCLUDED_ASSET_DIRECTORIES
    };
    for source in crate::walk::files(vfs, &asset.source, excluded)? {
        if !whole && is_test_file(&source) {
            continue;
        }
        let relative = source.strip_prefix(&asset.source).map_err(|_| {
            CoreError::Projection(format!(
                "asset walk under `{}` yielded `{}`, outside it",
                asset.source.display(),
                source.display()
            ))
        })?;
        let dest = Path::new(&asset.dest).join(relative);
        files.push(copy(&source, crate::skill::forward_slashed(&dest))?);
    }
    Ok(files)
}

/// Would `asset` write the file at `manifest` (relative to the pack root)?
/// A file asset does when its `dest` is that path; a directory asset does
/// when its tree holds the file at the matching place.
fn asset_covers(vfs: &dyn Vfs, asset: &AssetSpec, manifest: &str) -> bool {
    let dest: PathBuf = Path::new(&asset.dest)
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    let Ok(rest) = Path::new(manifest).strip_prefix(&dest) else {
        return false;
    };
    if rest.as_os_str().is_empty() {
        return true;
    }
    is_directory(vfs, &asset.source) && vfs.exists(&asset.source.join(rest))
}

/// Is `dest` a plain relative path with no `..` component and no root/prefix
/// (an absolute Unix path, or a Windows drive/UNC prefix)? `PathBuf::join`
/// silently honors both an absolute replacement and a lexical `..` escape, so
/// an unsafe `dest` refuses the whole build before anything is written.
fn asset_dest_is_relative(dest: &str) -> bool {
    use std::path::Component;
    Path::new(dest)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Is this a co-located test file (`<name>.test.<ext>`)? The file-level half
/// of a directory asset's dev-only exclude.
fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().contains(".test."))
}

/// Is `path` a directory? Asked of its parent's listing, the one directory
/// signal every [`Vfs`] reports the same way — a symlinked directory reads as
/// a non-directory there, so it is copied as a file (and fails loudly on
/// read) rather than walked out of the tree it appears to sit in.
fn is_directory(vfs: &dyn Vfs, path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let name = name.to_string_lossy();
    vfs.read_dir(parent)
        .is_ok_and(|entries| entries.iter().any(|e| e.is_dir && e.name == name))
}

/// Every [`HookFileSpec`], read and parsed into the plugin it belongs to.
///
/// The gap report covers every harness with a plugin format, not only the
/// requested ones, so it is the same bytes whatever a build selects. A
/// harness's `file` is set only when its pack carries some of the file's
/// hooks: the format carries hooks, the harness admits the plugin, and at
/// least one hook is carried.
///
/// # Errors
///
/// * `refused: unknown-hooks-plugin` — the plugin holds no skill or command,
///   so there is no pack to carry the file.
/// * `refused: duplicate-hooks-file` — two files for one plugin.
/// * What [`crate::hooks_file::parse_hooks_file`] refuses, and a read failure.
fn load_hook_files(
    vfs: &dyn Vfs,
    specs: &[HookFileSpec],
    plugins: &mut BTreeMap<String, Plugin>,
) -> Result<(), CoreError> {
    for spec in specs {
        let plugin = spec.plugin.as_deref().unwrap_or_default();
        let list = plugin_list(plugins);
        let Some(group) = plugins.get_mut(plugin) else {
            return Err(CoreError::refused(
                "unknown-hooks-plugin",
                format!(
                    "hooks file `{}` names plugin `{plugin}`, which this build holds no skill, \
                     command or hook for — this vault publishes {list}. Nothing was written",
                    spec.from,
                ),
            ));
        };
        if group.hooks_file.is_some() {
            return Err(CoreError::refused(
                "duplicate-hooks-file",
                format!(
                    "plugin `{plugin}` has more than one hooks file; `{}` is the second. Merge \
                     them into one. Nothing was written",
                    spec.from
                ),
            ));
        }
        let bytes = vfs.read_file(&spec.source)?;
        let text = String::from_utf8(bytes).map_err(|_| {
            CoreError::refused("malformed-hooks-file", format!("{}: not UTF-8", spec.from))
        })?;
        let hooks = crate::hooks_file::parse_hooks_file(&text, &spec.from)?;
        let harnesses = with_plugin_format()
            .map(|root| {
                let runtime = parse_runtime_target(root.runtime)?;
                let carries = root.format().carries_hooks
                    && group.carried_by(|c| admits(&c.discovered, root))
                    && !crate::hooks_file::carried_hooks(&hooks, runtime).is_empty();
                Ok(crate::hooks_file::GapHarness {
                    name: root.name,
                    runtime,
                    plugin_root_var: root.format().plugin_root_var,
                    file: carries.then_some(PLUGIN_HOOKS_PATH),
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        let gap_report = crate::hooks_file::render_gap_report(&spec.from, &hooks, &harnesses);
        group.hooks_file = Some(HooksFile { hooks, gap_report });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;

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
        let (report, links) = read_notes(&vfs, Path::new("/vault"), &Namespace::sw()).unwrap();
        assert_eq!(report.components.len(), 3);
        assert_eq!(
            links.resolve("guide"),
            Some("marketplaces/m/plugins/p/references/guide.md")
        );
        assert_eq!(links.resolve("elsewhere"), None);
    }
}
