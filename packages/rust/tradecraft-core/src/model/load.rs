//! Loading — a vault read into the marketplace model, in two passes.
//!
//! [`load`] reads the marketplace a build renders: one marketplace of the
//! vault, every plugin, the notes folded over the root file's shared defaults.
//! It reads through [`discover_in`] (pass 1) and resolves through
//! [`resolve_body`] (pass 2).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::cascade::{Cascade, belongs_to};
use super::{
    Asset, Body, Component, Extensions, HooksFile, Marketplace, Plugin, Resolved, Resource,
    ResourceContent, Segment, pack_kind, segments,
};
use crate::compile::{CallContext, CompileSource, IndexedSkill, SkillIndex, compile_note};
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
use crate::settings::{RootFile, SharedDefaults};
use crate::vault::{VaultNote, read_vault_note};

pub use crate::layout::CHANGELOG_FILE;
use crate::layout::{Layout, Place, SharedDir};
use crate::vfs::{Vfs, normalize};

/// What a marketplace load reads: this vault, this marketplace of it, for
/// these harnesses. The root file's shared defaults and `build.harnesses` are
/// read from the vault, never passed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct LoadRequest {
    /// Absolute path to the vault.
    pub vault_root: PathBuf,
    /// Which marketplace to load, by folder. `None` names the vault's only
    /// one.
    #[serde(default)]
    pub marketplace: Option<String>,
    /// Which harnesses to load for, by canonical name. It replaces the root
    /// file's `build.harnesses`; empty means that list. A body no harness
    /// here admits is read but not resolved.
    #[serde(default)]
    pub harnesses: BTreeSet<String>,
}

impl LoadRequest {
    /// A load of the vault at `vault_root`. Every other field starts empty;
    /// set it after.
    pub fn new(vault_root: impl Into<PathBuf>) -> Self {
        Self {
            vault_root: vault_root.into(),
            marketplace: None,
            harnesses: BTreeSet::new(),
        }
    }
}

/// The harnesses `names` selects, every one when empty.
///
/// # Errors
///
/// `refused: unknown-harness` — a name no such harness answers to. Refused
/// rather than ignored: silently building less than was asked for is the
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

/// The harnesses a build renders: those `chosen` names, else the root
/// file's `build.harnesses`, else every harness. A choice replaces the root
/// file's list; it may name a harness the list leaves out.
///
/// # Errors
///
/// * `refused: unknown-harness` — a name no harness answers to.
pub(crate) fn built_harnesses(
    root: &RootFile,
    chosen: &BTreeSet<String>,
) -> Result<Vec<&'static HarnessProfile>, CoreError> {
    if chosen.is_empty() {
        selected_harnesses(&root.harness_set())
    } else {
        selected_harnesses(chosen)
    }
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
    let root = crate::paths::read_root_file(vfs, &layout.root)?;
    let roots = built_harnesses(&root, &request.harnesses)?;
    let scan = discover_in(vfs, layout)?;
    load_for(
        vfs,
        layout,
        &scan,
        request.marketplace.as_deref(),
        &roots,
        &root.defaults,
    )
}

/// [`load_in`] with the root file and pass 1 already done: `scan` is
/// [`discover_in`] over `layout`, `roots` the harnesses the model is for, and
/// `defaults` the root file's shared defaults.
pub(crate) fn load_for(
    vfs: &dyn Vfs,
    layout: &Layout,
    scan: &DiscoveryReport,
    marketplace: Option<&str>,
    roots: &[&'static HarnessProfile],
    defaults: &SharedDefaults,
) -> Result<Marketplace, CoreError> {
    let vault_root = layout.root.clone();
    // The root file's own entry is configuration, not a note, and
    // `check_block_values` below refuses it naming the marketplace. A note in
    // another marketplace does not stop a build of this one.
    let market_prefix = marketplace.map(|m| format!("{}/{m}/", crate::layout::MARKETPLACES_DIR));
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
    let discovered = scan.components.clone();
    // Which marketplaces the vault holds is read from every kind, not from the
    // skills alone: one holding only commands still exists, and naming it must
    // say it holds nothing to build rather than that it is not there.
    let market = resolve_marketplace(&discovered, marketplace)?;
    let cascade = Cascade::over(&discovered, &market, defaults);
    // Before members inherit a block, so a wrong value names its container.
    crate::harness::manifest::check_block_values(&cascade.config)?;
    let belonging: Vec<&DiscoveredComponent> = discovered
        .iter()
        .filter(|component| belongs_to(component, &market))
        .collect();
    // Only this marketplace's notes: another marketplace's gap neither
    // refuses this build nor appears in its report.
    let ours: BTreeSet<&str> = belonging
        .iter()
        .map(|c| c.vault_relative_path.as_str())
        .collect();
    let mut warnings: Vec<DiscoveryWarning> = scan
        .warnings
        .iter()
        .filter(|w| ours.contains(w.path.as_str()))
        .cloned()
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
    warnings.extend(load_hooks_files(vfs, layout, &market, roots, &mut plugins)?);

    // Pass 2. One index for every call, over every skill, command and agent
    // in the marketplace under the plugin it ships in, whether or not a given
    // harness ships it: which harness ships what is asked when the call is
    // written.
    let checked = skills.len();
    let index = skill_index(
        plugins
            .values()
            .flat_map(|plugin| plugin.components())
            .chain(&uncarried)
            .map(|c| &c.discovered),
    );
    let hand_written = hand_written_forms(&plugins, roots, config)?;
    let calls = CallContext {
        skills: &index.skills,
        hand_written,
        warnings: RefCell::new(Vec::new()),
    };
    for plugin in plugins.values_mut() {
        let plugin_name = plugin.name.clone();
        for component in plugin.skills.iter_mut().chain(plugin.commands.iter_mut()) {
            if !roots
                .iter()
                .any(|root| kind_carried(&component.discovered, root))
            {
                continue;
            }
            let mut note = read_vault_note(vfs, &component.discovered.entry_path, component.kind)?;
            cascade.apply(&mut note, component.kind, Some(&plugin_name));
            component.extensions = extensions(&note.frontmatter, &component.discovered)?;
            component.resolved = Some(resolve_body(
                vfs,
                layout,
                &component.discovered,
                note,
                component.kind,
                &calls,
            )?);
        }
        plugin.references = shared_references(vfs, layout, &market, &plugin_name, &calls)?;
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
        folder: market,
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
/// the skills and commands they name — and split into segments.
///
/// # Errors
///
/// Everything [`crate::compile::compile_note`] refuses. Whether a harness
/// ships what a call names is asked when the call is written.
pub fn resolve_body(
    vfs: &dyn Vfs,
    layout: &Layout,
    component: &DiscoveredComponent,
    note: VaultNote,
    kind: EntityKind,
    calls: &CallContext<'_>,
) -> Result<Resolved, CoreError> {
    let VaultNote {
        mut content,
        frontmatter,
    } = note;
    // Only the two kinds an invocation actually takes arguments at.
    if matches!(kind, EntityKind::Skill | EntityKind::Command) {
        content.apply_arguments(&frontmatter.arguments);
    }
    let compiled = compile_note(
        vfs,
        &CompileSource::new(layout, &component.entry_path, &component.name, calls),
        &content.body,
    )?;
    let record = &compiled.recorded;
    let body = segments(&compiled.body, record);
    let resources = compiled
        .resources
        .into_iter()
        .map(|(path, bytes, executable)| {
            let content = match String::from_utf8(bytes) {
                Ok(text) if path.to_lowercase().ends_with(".md") => ResourceContent::Body {
                    body: segments(&text, record),
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
    Ok(Resolved {
        content,
        body,
        resources,
        closures: compiled.closures,
    })
}

/// The components a call may name.
#[derive(Debug, Clone, Default)]
pub struct Index {
    skills: SkillIndex,
}

fn skill_index<'a>(skills: impl Iterator<Item = &'a DiscoveredComponent>) -> Index {
    let skills = SkillIndex::new(
        skills
            .filter(|c| {
                matches!(
                    c.kind,
                    EntityKind::Skill | EntityKind::Command | EntityKind::Agent
                )
            })
            // Every component sits in a plugin (spec section 3).
            .filter_map(|c| Some((c, c.plugin.clone()?)))
            .map(|(c, plugin)| IndexedSkill {
                entry_path: c.entry_path.clone(),
                plugin,
                name: c.name.clone(),
                kind: c.kind,
            })
            .collect::<Vec<_>>(),
    );
    Index { skills }
}

/// The call strings a body may not write by hand, each with the name to
/// write as `/[[name]]` (spec section 4 rule 8): every built harness's form
/// for each skill, and for each command where the harness's pack contains
/// commands, with that harness's naming for the plugin. A form without the
/// plugin in it, such as Cursor's `/setup` under `naming: bare`, is left out:
/// a bare name is too often other text.
///
/// # Errors
///
/// What [`crate::harness::manifest::naming_of`] refuses.
fn hand_written_forms(
    plugins: &BTreeMap<String, Plugin>,
    roots: &[&'static HarnessProfile],
    config: &MarketplaceConfig,
) -> Result<BTreeMap<String, String>, CoreError> {
    let mut forms = BTreeMap::new();
    for root in roots {
        for plugin in plugins.values() {
            let naming = crate::harness::manifest::naming_of(config, &plugin.name, root)?;
            if naming == SkillNaming::Bare {
                continue;
            }
            let commands = plugin
                .commands
                .iter()
                .filter(|_| root.format().carries_commands);
            for member in plugin.skills.iter().chain(commands) {
                if let Ok(form) = crate::export::rendered_skill_call(
                    root.runtime,
                    naming,
                    &plugin.name,
                    &member.name,
                ) {
                    forms.insert(form, member.name.clone());
                }
            }
        }
    }
    Ok(forms)
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

/// Marks each plugin whose `<harness>: {naming: bare}` names its skills bare
/// against that harness's prefixing default, and every call to one of its
/// skills, wherever the call sits. Only a harness whose profile allows the
/// choice is read.
fn apply_naming(
    plugins: &mut BTreeMap<String, Plugin>,
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    // Per plugin, the harnesses that name it bare.
    let mut bare_on: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for profile in crate::harness::with_plugin_format().filter(|p| p.format().naming_choice) {
        let default = profile.format().naming;
        for name in plugins.keys() {
            let naming = crate::harness::manifest::naming_of(config, name, profile)?;
            if naming == SkillNaming::Bare && naming != default {
                bare_on
                    .entry(name.clone())
                    .or_default()
                    .push(profile.name.to_owned());
            }
        }
    }
    if bare_on.is_empty() {
        return Ok(());
    }
    for plugin in plugins.values_mut() {
        if let Some(bare) = bare_on.get(&plugin.name) {
            plugin.bare_on.clone_from(bare);
        }
        let members = plugin.skills.iter_mut().chain(&mut plugin.commands);
        for resolved in members.filter_map(|member| member.resolved.as_mut()) {
            for body in bodies_of(resolved) {
                for segment in &mut body.segments {
                    if let Segment::Call { call } = segment
                        && let Some(bare) = bare_on.get(&call.skill.plugin)
                    {
                        call.bare_on.clone_from(bare);
                    }
                }
            }
        }
    }
    Ok(())
}

/// A node read in pass 1 and not yet resolved.
fn unread(component: &DiscoveredComponent) -> Component {
    Component {
        kind: component.kind,
        name: component.name.clone(),
        path: component.vault_relative_path.clone(),
        extensions: Extensions::default(),
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

/// Whether this harness's pack carries this member: its pack format has a
/// place for the member's kind.
pub(crate) fn kind_carried(component: &DiscoveredComponent, spec: &HarnessProfile) -> bool {
    match component.kind {
        EntityKind::Command => spec.format().carries_commands,
        _ => true,
    }
}

/// A warning for each plugin of `market` whose note states no `version`. It
/// ships as [`crate::harness::manifest::DEFAULT_PLUGIN_VERSION`], and a harness
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
        .filter(|c| c.kind == EntityKind::Plugin && belongs_to(c, market))
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
                "this vault holds {}: name the marketplace to use",
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
/// their own names (spec section 9). Each is copied byte for byte, except
/// the Markdown in `references/`, which [`shared_references`] compiles.
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
            } if !shared_markdown(&file.place) => path.as_str(),
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

/// Whether `place` is Markdown in a plugin's shared `references/`.
fn shared_markdown(place: &Place) -> bool {
    matches!(
        place,
        Place::Shared {
            skill: None,
            dir: SharedDir::References,
            path,
        } if path.to_lowercase().ends_with(".md")
    )
}

/// The Markdown in plugin `plugin`'s shared `references/`, each compiled
/// as a file with no component: its links start at step 2 and are written
/// from where it lands, at the pack root (spec section 4 rule 3).
///
/// # Errors
///
/// What [`crate::compile::compile_note`] refuses.
fn shared_references(
    vfs: &dyn Vfs,
    layout: &Layout,
    market: &str,
    plugin: &str,
    calls: &CallContext<'_>,
) -> Result<Vec<Resource>, CoreError> {
    let mut out = Vec::new();
    for file in layout.files_of(market) {
        let (Place::Shared { path, .. }, Some(owner)) = (&file.place, file.plugin.as_deref())
        else {
            continue;
        };
        if owner != plugin || !shared_markdown(&file.place) {
            continue;
        }
        let raw = crate::skill::read_utf8(vfs, &file.path)?;
        // Its frontmatter ships as written; only the body is compiled.
        let split = frontmatter_lossless::split(&raw);
        let source = CompileSource::new(layout, &file.path, path, calls);
        let compiled = compile_note(vfs, &source, split.body)?;
        let text = frontmatter_lossless::compose(split.yaml, &compiled.body, split.shape);
        out.push(Resource {
            path: path.clone(),
            content: ResourceContent::Body {
                body: segments(&text, &compiled.recorded),
            },
            executable: vfs.is_executable(&file.path),
        });
    }
    Ok(out)
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
/// ships no hooks (spec section 4 rule 23). A pack that ships hooks leaves out
/// each one its harness cannot carry, with a warning per hook.
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
        for root in roots.iter().filter(|root| root.format().carries_hooks) {
            if !group.carried_by(|c| kind_carried(&c.discovered, root)) {
                continue;
            }
            for hook in &hooks {
                if let Some(why) = agent_adapt::render::hook_refusal(hook, root.runtime) {
                    warnings.push(DiscoveryWarning {
                        path: file.rel.clone(),
                        reason: "hook-left-out".to_owned(),
                        detail: format!(
                            "plugin `{plugin}`'s {} pack leaves out a `{}` hook: {why}",
                            root.label, hook.event
                        ),
                    });
                }
            }
        }
        group.hooks_file = Some(HooksFile {
            path: file.rel.clone(),
            hooks,
        });
        let cursor_built = roots.iter().any(|root| {
            root.name == "cursor" && group.carried_by(|c| kind_carried(&c.discovered, root))
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

    /// A plugin's shared Markdown reference is compiled, and its other
    /// shared files are copied.
    #[test]
    fn a_shared_markdown_reference_is_compiled_and_other_files_copied() {
        let vfs = vault();
        vfs.write_file(
            Path::new("/vault/marketplaces/m/plugins/p/references/data.txt"),
            b"[[guide]]",
        )
        .unwrap();
        let request = LoadRequest::new("/vault");
        let market = load(&vfs, &request).unwrap();
        let plugin = &market.plugins["p"];
        let paths: Vec<&str> = plugin.references.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["references/guide.md"]);
        let assets: Vec<&str> = plugin.assets.iter().map(|a| a.dest.as_str()).collect();
        assert_eq!(assets, ["references/data.txt"]);
    }
}
