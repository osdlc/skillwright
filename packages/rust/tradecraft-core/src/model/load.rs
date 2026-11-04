//! Loading — a vault read into the marketplace model, in two passes.
//!
//! Three entry points, one per shape a verb asks for:
//!
//! * [`load`] — the marketplace a build renders: one vault marketplace,
//!   every plugin, the request's configuration folded under the notes'.
//! * [`load_plugin`] — one plugin as a pack published to a destination.
//! * [`load_component`] — one note published on its own.
//!
//! All three read through [`read_notes`] (pass 1) and resolve through
//! [`resolve_body`] (pass 2), and refuse what the two pipelines they replace
//! refused, in the same order and the same words.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    Asset, Body, Component, Extensions, HooksFile, Marketplace, Plugin, Resolved, Resource,
    ResourceContent, Segment, pack_kind, segments,
};
use crate::compile::{
    CallContext, CompileSource, EXCLUDED_BUNDLE_DIRECTORIES, IndexedSkill, Recorded, SkillIndex,
    SkillSource, compile_projection,
};
use crate::content::{SkillContent, SkillProjection};
use crate::discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, KIND_DIRS, MISSING_DESCRIPTION_REASON,
    discover_components,
};
use crate::error::CoreError;
use crate::export::{AssetSpec, HookFileSpec, MarketplaceConfig, PluginOverrides};
use crate::frontmatter::{ContainerIdentity, EntityKind, VaultFrontmatter};
use crate::harness::{HarnessProfile, with_plugin_format};
use crate::namespace::Namespace;
use crate::projection::parse_runtime_target;
use crate::publish::{PLUGIN_HOOKS_PATH, canonical_target, refuse_carried_peers};
use crate::vault::{VaultNote, read_vault_skill};
use crate::vfs::{Vfs, normalize};

/// What a marketplace load reads: this vault, this marketplace of it, for
/// these harnesses, under this configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    /// Absolute path to the vault.
    pub vault_root: PathBuf,
    /// The content root to scan; `None` means the vault root.
    #[serde(default)]
    pub skills_root: Option<PathBuf>,
    /// Which marketplace to load. `None` names the vault's only one.
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

/// Pass 1: every note in the vault, read into a node — its frontmatter, kind,
/// plugin, marketplace and files.
///
/// The one function here that walks the vault. A shared vault reader takes
/// its place without the passes after it noticing.
///
/// # Errors
///
/// What [`discover_components`] refuses.
pub fn read_notes(
    vfs: &dyn Vfs,
    vault_root: &Path,
    skills_root: Option<&Path>,
    ns: &Namespace,
) -> Result<DiscoveryReport, CoreError> {
    discover_components(vfs, vault_root, skills_root, ns)
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
///   swaps whole trees, and a note it could not read would be swept as a
///   note that was deleted.
/// * `refused: ambiguous-marketplace` / `unknown-marketplace` /
///   `unnamed-marketplace` — which marketplace, and what it is called.
/// * `refused: missing-description` — `require_descriptions` is set.
/// * `refused: empty-marketplace` — nothing publishable.
/// * `refused: path-collision` — two members of one kind in one plugin.
/// * `refused: unsafe-asset-dest` / `asset-over-manifest` /
///   `unknown-asset-plugin` — an asset that cannot land where it says.
/// * `refused: unknown-hooks-plugin` / `duplicate-hooks-file` and what
///   [`crate::hooks_file::parse_hooks_file`] refuses.
/// * `refused: unknown-target` / `unknown-override-key` — an `sw-overrides`
///   block one of the requested harnesses cannot take.
/// * Everything the resolve pass refuses; see [`resolve_body`].
pub fn load(
    vfs: &dyn Vfs,
    request: &LoadRequest,
    ns: &Namespace,
) -> Result<Marketplace, CoreError> {
    let vault_root = normalize(&request.vault_root);
    let roots = selected_harnesses(&request.harnesses)?;

    // Pass 1.
    let scan = read_notes(vfs, &vault_root, request.skills_root.as_deref(), ns)?;
    if !scan.skipped.is_empty() {
        let notes: Vec<String> = scan
            .skipped
            .iter()
            .map(|note| format!("{} ({})", note.path, note.detail))
            .collect();
        return Err(CoreError::refused(
            "unreadable-note",
            format!(
                "the vault holds {} note(s) this scan could not read, and a marketplace built \
                 without them would sweep away what they publish: {}. Nothing was written",
                notes.len(),
                notes.join("; ")
            ),
        ));
    }
    let discovered = scan.components;
    // Which marketplaces the vault holds is read from every kind, not from the
    // skills alone: one holding only commands still exists, and naming it must
    // say it holds nothing publishable rather than that it is not there.
    let market = resolve_marketplace(&discovered, request.marketplace.as_deref())?;
    let belonging: Vec<&DiscoveredComponent> = discovered
        .iter()
        .filter(|component| belongs_to(component, market.as_deref()))
        .collect();
    // Only this marketplace's notes: another marketplace's gap neither
    // refuses this build nor appears in its report. A marketplace note names
    // no marketplace of its own, so `belongs_to` admits every one of them.
    let ours: BTreeSet<&str> = belonging
        .iter()
        .filter(|c| c.kind != EntityKind::Marketplace || market.as_deref() == Some(c.name.as_str()))
        .map(|c| c.vault_relative_path.as_str())
        .collect();
    let mut warnings: Vec<DiscoveryWarning> = scan
        .warnings
        .into_iter()
        .filter(|w| ours.contains(w.path.as_str()))
        .collect();
    let undescribed: Vec<&str> = warnings
        .iter()
        .filter(|w| w.reason == MISSING_DESCRIPTION_REASON)
        .map(|w| w.path.as_str())
        .collect();
    if request.config.require_descriptions && !undescribed.is_empty() {
        let notes = undescribed;
        return Err(CoreError::refused(
            MISSING_DESCRIPTION_REASON,
            format!(
                "`require_descriptions` is set and {} note(s) have a `name` and no \
                 `description`: {}. Nothing was written",
                notes.len(),
                notes.join(", ")
            ),
        ));
    }
    // Named, never dropped: a pack has a place for skills, commands and hooks
    // and for nothing else, and a build that quietly omitted an agent would
    // be the failure this crate is built around.
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
    let (skills, commands, hooks) = (
        of_kind(EntityKind::Skill),
        of_kind(EntityKind::Command),
        of_kind(EntityKind::Hook),
    );
    if skills.is_empty() && commands.is_empty() && hooks.is_empty() {
        return Err(CoreError::refused(
            "empty-marketplace",
            match &market {
                Some(name) => format!(
                    "marketplace `{name}` under {} holds no publishable skills, commands or \
                     hooks: a marketplace of nothing is a configuration fault, not an empty \
                     success. Nothing was written",
                    vault_root.display()
                ),
                None => format!(
                    "no publishable skills, commands or hooks under {}: a marketplace of \
                     nothing is a configuration fault, not an empty success. Nothing was \
                     written",
                    vault_root.display()
                ),
            },
        ));
    }
    let (name, config) = resolve_config(&discovered, market.as_deref(), &request.config)?;
    let mut plugins = group_by_plugin(&skills, &commands, &hooks, &config.default_plugin)?;
    warnings.extend(missing_versions(
        &discovered,
        market.as_deref(),
        &plugins,
        &config,
    ));
    load_assets(vfs, &request.assets, &roots, &mut plugins, &config)?;
    load_hook_files(vfs, &request.hooks, &mut plugins, &config)?;

    // Pass 2. One index for every call, over every skill in the marketplace
    // under the plugin it ships in, whether or not a given harness ships it:
    // which harness ships what is asked when the call is written.
    let checked = skills.len();
    let index = skill_index(plugins.values().flat_map(|plugin| {
        plugin
            .skills
            .iter()
            .map(|skill| (&skill.discovered, Some(plugin.name.as_str())))
    }));
    let record = RefCell::new(Recorded::default());
    let calls = recording_context(SkillSource::ready(&index.skills), true, &record);
    for plugin in plugins.values_mut() {
        let shim_target = plugin.shim_target.clone();
        for component in plugin
            .skills
            .iter_mut()
            .chain(plugin.commands.iter_mut())
            .chain(plugin.hooks.iter_mut())
        {
            if !roots.iter().any(|root| admits(&component.discovered, root)) {
                continue;
            }
            let note = read_vault_skill(vfs, &component.discovered.entry_path, ns)?;
            let admitting: Vec<&HarnessProfile> = roots
                .iter()
                .copied()
                .filter(|root| admits(&component.discovered, root))
                .collect();
            component.extensions =
                extensions(&note.frontmatter, &component.discovered, &admitting, ns)?;
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
                        shim_target.as_deref(),
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

    Ok(Marketplace {
        name,
        folder: market,
        config,
        plugins,
        uncarried,
        warnings,
        checked,
        vault_root,
    })
}

/// What a single-note load is for: this note, against this tree, for this
/// destination.
#[derive(Debug, Clone)]
pub struct ComponentRequest<'a> {
    /// Absolute path to the note.
    pub path: &'a Path,
    /// The tree its links resolve against and its calls are scanned from.
    pub root: &'a Path,
    /// The note's peers, when it has a sibling folder of them.
    pub bundle_root: Option<&'a Path>,
    /// The note's kind, already resolved by the caller.
    pub kind: EntityKind,
    /// The canonical destination it is loaded for.
    pub canonical: &'a str,
    /// The name the destination was requested by, for a refusal.
    pub target: &'a str,
}

/// Loads one note published on its own: no plugin, so a call to another
/// skill is that skill's bare name.
///
/// The skills a call may name come from a scan of `request.root`, built only
/// when the body has a call or a link to check. A scan that fails blocks only
/// a call: a plain link then stays a link.
///
/// # Errors
///
/// `refused: unknown-target` / `unknown-override-key` from the note's
/// `sw-overrides`, and what [`resolve_body`] refuses.
pub fn load_component(
    vfs: &dyn Vfs,
    request: &ComponentRequest<'_>,
    note: &VaultNote,
    ns: &Namespace,
) -> Result<Component, CoreError> {
    let blocks =
        crate::publish::overrides_for(&note.frontmatter, request.path, request.target, ns)?;
    let targets: OnceCell<BTreeMap<PathBuf, Option<Vec<String>>>> = OnceCell::new();
    let build = || -> Result<SkillIndex, CoreError> {
        let scan = read_notes(vfs, request.root, None, ns)?;
        let built = skill_index(scan.components.iter().map(|c| (c, None)));
        let _ = targets.set(built.targets);
        Ok(built.skills)
    };
    let record = RefCell::new(Recorded::default());
    let implicit = matches!(request.kind, EntityKind::Skill | EntityKind::Command);
    let calls = recording_context(SkillSource::lazy(&build), implicit, &record);
    let discovered = standalone(request, note);
    let (mut resolved, content) = resolve_compiled(
        vfs,
        request.root,
        &discovered,
        note,
        request.kind,
        &calls,
        &record,
        None,
    )?;
    if let Some(targets) = targets.get() {
        fill_targets(&mut resolved, targets);
    }
    Ok(Component {
        kind: request.kind,
        name: content.name.clone(),
        path: discovered.vault_relative_path.clone(),
        targets: note.frontmatter.targets.clone(),
        extensions: Extensions { blocks },
        content: Some(content),
        source: Some(note.projection.resources.clone()),
        resolved: Some(resolved),
        discovered,
    })
}

/// A plugin loaded as a pack for one destination.
#[derive(Debug, Clone)]
pub struct Pack {
    /// The plugin, holding only the members this destination admits and
    /// carries, with the container's own files as its assets.
    pub plugin: Plugin,
    /// The pack manifest's fields: the plugin note's own frontmatter, and no
    /// marketplace above it.
    pub config: MarketplaceConfig,
    /// Members this destination admits and its pack has no place for, by
    /// vault path.
    pub not_carried: Vec<String>,
}

/// Loads one plugin as a pack for `profile`.
///
/// The members come from the vault scan rather than from the note's body,
/// which is the same rule a build follows: a component's plugin is decided by
/// the folder holding it and overruled by `sw-plugin`. A member narrowed away
/// by its own `sw-targets` is absent from the pack, exactly as it is absent
/// from a built tree.
///
/// # Errors
///
/// * `refused: unreadable-note` — a member the scan could not read. A pack
///   must not install a subset silently.
/// * `refused: empty-plugin` — nothing this destination admits.
/// * What [`resolve_body`] refuses for any member.
pub fn load_plugin(
    vfs: &dyn Vfs,
    vault_root: &Path,
    plugin_name: &str,
    identity: &ContainerIdentity,
    profile: &'static HarnessProfile,
    ns: &Namespace,
) -> Result<Pack, CoreError> {
    let canonical = profile.name;
    let carries_commands = profile.format().carries_commands;
    let scan = read_notes(vfs, vault_root, None, ns)?;
    // A note elsewhere in the vault that does not parse is not this pack's
    // problem, and a shared vault always holds one. A note this pack holds
    // is. Membership is the note's own `sw-plugin` first and its folder
    // second, the ranking a readable member follows.
    let blocked: Vec<&str> = scan
        .skipped
        .iter()
        .filter(|note| note.plugin.as_deref() == Some(plugin_name))
        .map(|note| note.path.as_str())
        .collect();
    if !blocked.is_empty() {
        return Err(CoreError::refused(
            "unreadable-note",
            format!(
                "`{plugin_name}` holds {} note(s) this scan could not read: {} — a pack must \
                 not install a subset silently",
                blocked.len(),
                blocked.join(", ")
            ),
        ));
    }
    let admitted: Vec<&DiscoveredComponent> = scan
        .components
        .iter()
        .filter(|c| c.plugin.as_deref() == Some(plugin_name) && targets_admit(c, canonical))
        .collect();
    // A member this pack layout cannot hold is named, never dropped.
    let carried = |c: &DiscoveredComponent| {
        c.kind == EntityKind::Skill || (c.kind == EntityKind::Command && carries_commands)
    };
    let not_carried: Vec<String> = admitted
        .iter()
        .filter(|c| !carried(c))
        .map(|c| c.vault_relative_path.clone())
        .collect();
    let mut plugin = Plugin {
        name: plugin_name.to_owned(),
        ..Plugin::default()
    };
    for member in admitted.iter().filter(|c| carried(c)) {
        match member.kind {
            EntityKind::Skill => plugin.skills.push(unread(member)),
            _ => plugin.commands.push(unread(member)),
        }
    }
    if plugin.skills.is_empty() && plugin.commands.is_empty() {
        return Err(CoreError::refused(
            "empty-plugin",
            format!(
                "`{plugin_name}` has no member this destination admits — every member is either \
                 absent or narrowed away by its own `sw-targets`"
            ),
        ));
    }

    // A pack's calls name each skill under its own plugin, the way the
    // harness names a plugin's skills.
    let index = skill_index(
        scan.components
            .iter()
            .filter(|c| c.kind == EntityKind::Skill)
            .map(|c| (c, c.plugin.as_deref())),
    );
    let record = RefCell::new(Recorded::default());
    let calls = recording_context(SkillSource::ready(&index.skills), true, &record);
    for member in plugin.skills.iter_mut().chain(plugin.commands.iter_mut()) {
        let note = read_vault_skill(vfs, &member.discovered.entry_path, ns)?;
        member.extensions = extensions(&note.frontmatter, &member.discovered, &[profile], ns)?;
        let (resolved, content) = resolve_body(
            vfs,
            vault_root,
            &member.discovered,
            &note,
            member.kind,
            &calls,
            &index,
            None,
        )?;
        member.resolved = Some(resolved);
        member.content = Some(content);
        member.source = Some(note.projection.resources);
    }

    // The container's own files, at the pack root beside its manifest. A
    // plugin folder holds more than its members: a licence, a readme, an
    // icon.
    if let Some(note) = scan
        .components
        .iter()
        .find(|c| c.kind == EntityKind::Plugin && c.name == plugin_name)
    {
        let source_note = note.vault_relative_path.clone();
        for relative in note.resources.iter().filter(|r| is_container_own(r)) {
            let from = vault_root
                .join(&source_note)
                .parent()
                .map(|dir| dir.join(relative));
            let Some(from) = from else { continue };
            plugin.assets.push(Asset {
                dest: relative.clone(),
                bytes: vfs.read_file(&from)?,
                executable: vfs.is_executable(&from),
            });
        }
    }

    let config = MarketplaceConfig {
        // A per-destination publish has no marketplace above it, so the
        // marketplace-level fallbacks are absent and the plugin note's own
        // frontmatter is the only source of identity there is.
        name: None,
        display_name: None,
        owner: None,
        description: None,
        author: identity.author.clone(),
        license: None,
        default_plugin: plugin_name.to_owned(),
        plugins: [(plugin_name.to_owned(), overrides_from(identity))]
            .into_iter()
            .collect(),
        require_descriptions: false,
        // Unread on this path: a pack published to a destination ships no
        // launcher asset, so no skill in it gets the shim.
        shim_target: crate::export::DEFAULT_SHIM_TARGET.to_owned(),
    };
    Ok(Pack {
        plugin,
        config,
        not_carried,
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
    shim_target: Option<&str>,
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
        shim_target,
    )?;
    fill_targets(&mut resolved, &index.targets);
    Ok((resolved, content))
}

/// The skills a call may name, and each one's `sw-targets`.
#[derive(Debug, Clone, Default)]
pub struct Index {
    skills: SkillIndex,
    targets: BTreeMap<PathBuf, Option<Vec<String>>>,
}

fn skill_index<'a>(
    skills: impl Iterator<Item = (&'a DiscoveredComponent, Option<&'a str>)>,
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
    Index { skills, targets }
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
fn no_render(_: &IndexedSkill) -> Result<String, CoreError> {
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
    shim_target: Option<&str>,
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
    let body = segments(&projection.content.body, &record, shim_target);
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

/// Gives every call the `sw-targets` of the skill it names.
fn fill_targets(resolved: &mut Resolved, targets: &BTreeMap<PathBuf, Option<Vec<String>>>) {
    let bodies =
        std::iter::once(&mut resolved.body).chain(resolved.resources.iter_mut().filter_map(
            |resource| match &mut resource.content {
                ResourceContent::Body { body } => Some(body),
                ResourceContent::Bytes { .. } => None,
            },
        ));
    for body in bodies {
        fill_body_targets(body, targets);
    }
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

/// A standalone note's record, in the shape a scan would have given it.
fn standalone(request: &ComponentRequest<'_>, note: &VaultNote) -> DiscoveredComponent {
    DiscoveredComponent {
        name: note.projection.content.name.clone(),
        description: note.projection.content.description.clone(),
        description_filled: false,
        kind: request.kind,
        kind_source: crate::frontmatter::KindSource::Default,
        folder_kind: None,
        targets: note.frontmatter.targets.clone(),
        plugin: None,
        folder_plugin: None,
        marketplace: None,
        entry_path: request.path.to_path_buf(),
        bundle_root: request.bundle_root.map(Path::to_path_buf),
        resources: Vec::new(),
        vault_relative_path: vault_relative(request.root, request.path),
        hook: None,
        identity: ContainerIdentity::default(),
    }
}

/// A vault path as the manifest spells it: relative to the root, forward
/// slashed. Falls back to the file name when the note is not under the root,
/// which only happens where no vault root was supplied.
pub(crate) fn vault_relative(vault_root: &Path, path: &Path) -> String {
    path.strip_prefix(vault_root)
        .ok()
        .map(|rest| rest.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        })
}

/// A note's `sw-overrides`, folded onto canonical names and checked for each
/// of `harnesses` (D-ISOU).
///
/// # Errors
///
/// What [`crate::publish::overrides_for`] refuses, for the first harness
/// that refuses.
fn extensions(
    frontmatter: &VaultFrontmatter,
    component: &DiscoveredComponent,
    harnesses: &[&HarnessProfile],
    ns: &Namespace,
) -> Result<Extensions, CoreError> {
    let mut blocks = BTreeMap::new();
    for harness in harnesses {
        blocks =
            crate::publish::overrides_for(frontmatter, &component.entry_path, harness.name, ns)?;
    }
    Ok(Extensions { blocks })
}

// --- What the build used to decide before it rendered ------------------------

/// Does this member publish to this harness? Two gates: the harness's pack
/// has a place for the member's kind, and its `sw-targets` names the harness.
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

/// Gate 2 of [`admits`]: whether the member's `sw-targets` names this
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

/// A plugin this build publishes from its own note, with no version from the
/// note or the config. It ships as `0.1.0`, and a harness compares versions
/// to decide whether an installed pack is out of date — so a pack that never
/// states one never looks updated.
fn missing_versions(
    discovered: &[DiscoveredComponent],
    market: Option<&str>,
    plugins: &BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Vec<DiscoveryWarning> {
    discovered
        .iter()
        .filter(|c| c.kind == EntityKind::Plugin && belongs_to(c, market))
        .filter(|c| plugins.contains_key(c.name.as_str()))
        .filter(|c| {
            config
                .plugins
                .get(&c.name)
                .and_then(|o| o.version.as_ref())
                .is_none()
        })
        .map(|c| DiscoveryWarning {
            path: c.vault_relative_path.clone(),
            reason: "missing-version".to_owned(),
            detail: format!(
                "plugin `{}` states no version; published as `0.1.0`. Set `sw-version` on \
                 its note so a harness can tell an update from what it has",
                c.name
            ),
        })
        .collect()
}

/// The manifest fields this build renders under, read from the vault first and
/// the request second.
///
/// # Why the note outranks the request
///
/// A marketplace note *is* the marketplace: its name, its owner, who authors
/// the packs under it — a fact about the vault, versioned with it and visible
/// in Obsidian. [`MarketplaceConfig`] stays required for what no note can
/// answer. A vault holding no marketplace directory at all is legal
/// ([`resolve_marketplace`] returns `None` for it); the request is the whole
/// identity there.
///
/// # Errors
///
/// `refused: unnamed-marketplace` — neither a marketplace note nor the request
/// supplies a name.
fn resolve_config(
    discovered: &[DiscoveredComponent],
    market: Option<&str>,
    requested: &MarketplaceConfig,
) -> Result<(String, MarketplaceConfig), CoreError> {
    let mut config = requested.clone();

    // A marketplace's own note is the one component that belongs to no
    // marketplace while naming one — see `DiscoveredComponent::marketplace`.
    let note = market.and_then(|name| {
        discovered
            .iter()
            .find(|c| c.kind == EntityKind::Marketplace && c.name == name)
    });
    if let Some(note) = note {
        config.name = Some(note.name.clone());
        // The note's `description` is the Agent Skills spec field every component carries,
        // not an `sw-` key — the same rule the plugin loop below follows. A
        // marketplace describes itself where a skill describes itself.
        take(&mut config.description, Some(note.description.clone()));
        take(&mut config.display_name, note.identity.display_name.clone());
        take(&mut config.owner, note.identity.owner.clone());
        take(&mut config.author, note.identity.author.clone());
        take(&mut config.license, note.identity.license.clone());
    }
    // Failing a note, the marketplace's own name is what it is called: the
    // folder named the level, and a distribution named `house` on disk should
    // not need a second place to say so.
    if config.name.is_none() {
        config.name = market.map(str::to_owned);
    }
    // The one thing nothing supplied. Everything else in this type has a
    // documented default; a name does not, because there is nothing sensible to
    // call a distribution its author never named.
    if config.name.is_none() {
        return Err(CoreError::refused(
            "unnamed-marketplace",
            match market {
                Some(name) => format!(
                    "marketplace `{name}` has no note naming it and the build supplied no name \
                     either — add a `{name}/{name}.md` carrying `name:`, or configure one"
                ),
                None => "this vault holds no marketplace to name one, and the build supplied no \
                         name — configure one, or give the vault a `marketplaces/<name>/` holding \
                         its own note"
                    .to_owned(),
            },
        ));
    }

    // Each plugin note fills its own override block. A block the request
    // already spelled is left alone only field by field: a vault naming its
    // pack's license and a request naming its version must produce a manifest
    // carrying both, not whichever side spoke last.
    for note in discovered
        .iter()
        .filter(|c| c.kind == EntityKind::Plugin)
        .filter(|c| belongs_to(c, market))
    {
        let overrides = config.plugins.entry(note.name.clone()).or_default();
        // The note's `description` is the Agent Skills spec field every component carries,
        // not an `sw-` key: a plugin note describes its pack in the same place
        // a skill note describes itself.
        take(&mut overrides.description, Some(note.description.clone()));
        take(&mut overrides.version, note.identity.version.clone());
        take(&mut overrides.author, note.identity.author.clone());
        take(&mut overrides.license, note.identity.license.clone());
        take(&mut overrides.keywords, note.identity.keywords.clone());
        take(&mut overrides.category, note.identity.category.clone());
        take(
            &mut overrides.display_name,
            note.identity.display_name.clone(),
        );
    }

    // Settled, so nothing below has to hold an `Option` it has already proved
    // is `Some`. The name and the rest travel together from here.
    let name = config.name.clone().unwrap_or_default();
    Ok((name, config))
}

/// Writes `from` into `slot` when the note supplied it, leaving a request-only
/// value in place. The vault outranks the request where both spoke; where the
/// vault is silent the request is all there is.
fn take<T>(slot: &mut Option<T>, from: Option<T>) {
    if let Some(value) = from {
        *slot = Some(value);
    }
}

/// Which marketplace this build renders, given what the vault holds.
///
/// One `out_root` holds one marketplace, so this is a choice the request
/// either makes or the vault makes for it. Discovery keys uniqueness on the
/// marketplace, so two of them may hold the same plugin holding the same
/// skill name; flattening them onto one tree would land both on one path and
/// keep whichever was written last.
///
/// `None` comes back only for a vault with no marketplaces at all — every
/// skill promoted to the vault level. That vault still builds, into an
/// unnamed marketplace whose identity comes from the config alone.
fn resolve_marketplace(
    discovered: &[DiscoveredComponent],
    requested: Option<&str>,
) -> Result<Option<String>, CoreError> {
    let present: BTreeSet<&str> = discovered
        .iter()
        .filter_map(|component| component.marketplace.as_deref())
        .collect();
    match requested {
        Some(name) => {
            if present.contains(name) {
                return Ok(Some(name.to_owned()));
            }
            // A vault holding no marketplace directory has no set to select
            // from, so naming one is not a wrong selection — it is what the
            // output gets called. Every skill in such a vault is promoted above
            // any marketplace and belongs to whichever one is built.
            if present.is_empty() {
                return Ok(Some(name.to_owned()));
            }
            Err(CoreError::refused(
                "unknown-marketplace",
                format!(
                    "no marketplace `{name}` in this vault: it holds {}",
                    named(&present)
                ),
            ))
        }
        None => match present.len() {
            0 => Ok(None),
            1 => Ok(present.iter().next().map(|name| (*name).to_owned())),
            _ => Err(CoreError::refused(
                "ambiguous-marketplace",
                format!(
                    "this vault holds {} and one output root holds one of them — name the one to \
                     build",
                    named(&present)
                ),
            )),
        },
    }
}

/// Does this component render into the marketplace being built?
///
/// Its own do. So does everything promoted above every marketplace: a
/// component at the vault level belongs to none of them and is shared by all
/// of them — the promotion rule read from the far end — so it renders into
/// each marketplace that is built, rather than into none.
fn belongs_to(component: &DiscoveredComponent, market: Option<&str>) -> bool {
    match (component.marketplace.as_deref(), market) {
        (None, _) => true,
        (Some(own), Some(market)) => own == market,
        (Some(_), None) => false,
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
/// A bare member belongs to no plugin in the vault, but every member needs
/// *some* plugin directory in a marketplace tree — so the configured default
/// is applied here rather than in the scan. Where a member lives is a fact
/// about the vault; where it publishes is a fact about this build.
///
/// # Errors
///
/// `refused: path-collision` — two members of the same kind landing on one
/// directory. This vault is legal and discovery passed it: a promoted skill
/// and a marketplace's own skill of that name are two files in two places,
/// and only this build, which renders both into one tree, can see them meet.
/// The check is per kind: the three render into disjoint subtrees, so a skill
/// and a command of one name are two directories, not one.
fn group_by_plugin(
    skills: &[&DiscoveredComponent],
    commands: &[&DiscoveredComponent],
    hooks: &[&DiscoveredComponent],
    default_plugin: &str,
) -> Result<BTreeMap<String, Plugin>, CoreError> {
    type Pick = fn(&mut Plugin) -> &mut Vec<Component>;
    let kinds: [(&[&DiscoveredComponent], Pick); 3] = [
        (skills, |plugin| &mut plugin.skills),
        (commands, |plugin| &mut plugin.commands),
        (hooks, |plugin| &mut plugin.hooks),
    ];
    let mut plugins: BTreeMap<String, Plugin> = BTreeMap::new();
    for (members, pick) in kinds {
        for member in members {
            let name = member.plugin.as_deref().unwrap_or(default_plugin);
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
/// A directory source lands file by file, with
/// [`EXCLUDED_BUNDLE_DIRECTORIES`] pruned at every depth and every `*.test.*`
/// file skipped, so a shared tree of reference docs does not carry its own
/// test suite into the build. An asset is never compiled: its whole point is
/// to ship exactly the bytes on disk.
fn load_assets(
    vfs: &dyn Vfs,
    assets: &[AssetSpec],
    roots: &[&HarnessProfile],
    plugins: &mut BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    let mut checked: Vec<(&AssetSpec, String)> = Vec::new();
    for asset in assets {
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
        let plugin = asset
            .plugin
            .as_deref()
            .unwrap_or(config.default_plugin.as_str());
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
        checked.push((asset, plugin.to_owned()));
    }
    for (asset, plugin) in checked {
        let files = read_asset(vfs, asset)?;
        let plugin = plugins
            .get_mut(&plugin)
            .expect("the plugin was checked above");
        // The shim's only job is finding this plugin's own launcher asset, so
        // it can exist at all only where that asset does.
        if asset.dest == config.shim_target {
            plugin.shim_target = Some(config.shim_target.clone());
        }
        plugin.assets.extend(files);
    }
    Ok(())
}

/// One asset's files, read verbatim.
fn read_asset(vfs: &dyn Vfs, asset: &AssetSpec) -> Result<Vec<Asset>, CoreError> {
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
    for source in crate::walk::files(vfs, &asset.source, EXCLUDED_BUNDLE_DIRECTORIES)? {
        if is_test_file(&source) {
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
/// * `refused: unknown-hooks-plugin` — the plugin holds no skill, command or
///   hook, so there is no pack to carry the file.
/// * `refused: duplicate-hooks-file` — two files for one plugin.
/// * What [`crate::hooks_file::parse_hooks_file`] refuses, and a read failure.
fn load_hook_files(
    vfs: &dyn Vfs,
    specs: &[HookFileSpec],
    plugins: &mut BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    for spec in specs {
        let plugin = spec
            .plugin
            .as_deref()
            .unwrap_or(config.default_plugin.as_str());
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
                    && group.components().any(|c| admits(&c.discovered, root))
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

/// Is this path the container's own content, rather than something living in
/// the structure it holds?
///
/// `owned_resources` gives a path to the innermost *component* directory
/// containing it, and a kind folder is not a component — so a snippet at
/// `skills/snippets/reviewer.md`, which no component claims, arrives as the
/// plugin's resource. It is not the plugin's content: it is vault-internal
/// writing that an embed inlines, and shipping it raw would put a copy of it
/// in the pack beside the skill that already contains it.
fn is_container_own(relative: &str) -> bool {
    let first = relative.split('/').next().unwrap_or("");
    !KIND_DIRS.iter().any(|(dir, _)| *dir == first)
}

/// The plugin note's own frontmatter as the build's override shape, so the two
/// paths read identity from one place.
fn overrides_from(identity: &ContainerIdentity) -> PluginOverrides {
    PluginOverrides {
        version: identity.version.clone(),
        description: None,
        author: identity.author.clone(),
        license: identity.license.clone(),
        keywords: identity.keywords.clone(),
        category: identity.category.clone(),
        display_name: identity.display_name.clone(),
    }
}
