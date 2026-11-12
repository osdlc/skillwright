//! The manifest files a pack and a marketplace carry, in each harness's
//! dialect.
//!
//! # Manifests are byte-deterministic
//!
//! Stable key order (serde struct order), plugins and skills sorted by name,
//! two-space indentation, one trailing newline — so `check` can compare
//! bytes, and two builds of one vault are identical.
//!
//! The functions here are the shapes; which harness writes which is a
//! `match` on its [`Harness`].

use serde::Serialize;

use crate::error::CoreError;
use crate::frontmatter::{ContainerIdentity, EntityKind};
use crate::harness::{Harness, SkillNaming};
use crate::model::config::{MarketplaceConfig, PluginOverrides};
use crate::refusal;

/// Two-space indentation, stable key order, one trailing newline — the byte
/// form both manifests share, and the reason `check` can compare bytes.
pub(crate) fn stable_json<T: Serialize>(value: &T) -> String {
    let mut buffer = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    value
        .serialize(&mut serializer)
        .expect("manifest types serialize infallibly");
    let mut out = String::from_utf8(buffer).expect("serde_json emits UTF-8");
    out.push('\n');
    out
}

type Block = serde_json::Map<String, serde_json::Value>;

/// The harness-block key choosing how a harness whose profile allows it
/// ([`crate::harness::profile::PluginPolicy::naming_choice`]) names a published skill.
pub const NAMING_KEY: &str = "naming";

/// Files an operating system leaves in any folder a person opens.
const OS_FILES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

/// Whether `relative` is one of [`OS_FILES`]: not anyone's work.
pub(crate) fn is_os_file(relative: &str) -> bool {
    OS_FILES.contains(&relative.rsplit('/').next().unwrap_or(relative))
}

/// Refuses a key in a container's block whose value is not the kind that
/// harness reads, such as a `metadata` that is not a mapping. Only the keys
/// this kind of container accepts are checked: any other key warns as
/// `unknown-key` and is left out, whatever it holds. A key with no known type
/// is not checked either.
///
/// # Errors
///
/// `refused: malformed`.
pub(crate) fn check_block_values(config: &MarketplaceConfig) -> Result<(), CoreError> {
    let market = container_owner(EntityKind::Marketplace, config.name.as_deref());
    let plugins = config.plugins.iter().map(|(name, plugin)| {
        (
            container_owner(EntityKind::Plugin, Some(name)),
            EntityKind::Plugin,
            &plugin.blocks,
        )
    });
    let containers = std::iter::once((market, EntityKind::Marketplace, &config.blocks));
    for (owner, kind, blocks) in containers.chain(plugins) {
        let as_map = blocks
            .iter()
            .map(|(harness, block)| (harness.clone(), serde_json::Value::Object(block.clone())))
            .collect();
        check_container_blocks(&owner, kind, &as_map)?;
    }
    Ok(())
}

/// How a refusal names a container: `plugin `p``, `marketplace `m``, or
/// `the marketplace` when it has no name.
pub(crate) fn container_owner(kind: EntityKind, name: Option<&str>) -> String {
    match (kind, name.filter(|name| !name.is_empty())) {
        (EntityKind::Plugin, Some(name)) => format!("plugin `{name}`"),
        (EntityKind::Plugin, None) => "the plugin".to_owned(),
        (_, Some(name)) => format!("marketplace `{name}`"),
        (_, None) => "the marketplace".to_owned(),
    }
}

/// [`check_block_values`] for one container's blocks, keyed by harness.
/// `owner` names the container in the refusal: `plugin `p``.
///
/// # Errors
///
/// `refused: malformed`.
pub(crate) fn check_container_blocks(
    owner: &str,
    kind: EntityKind,
    blocks: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), CoreError> {
    let unknown = crate::discover::unknown_container_block_keys(blocks, kind);
    for (harness, block) in blocks {
        for (key, value) in block.as_object().into_iter().flatten() {
            if let Some(ty) = Harness::from_name(harness)
                .and_then(|harness| crate::harness::field_types::container_type(harness, key))
                && !unknown.contains(&format!("{harness}.{key}"))
                && !ty.accepts(value)
            {
                return Err(CoreError::refused(
                    refusal::MALFORMED,
                    format!("`{harness}.{key}` of {owner} {}", ty.refusal(value)),
                ));
            }
        }
    }
    Ok(())
}

/// How `plugin`'s skills are named on `harness`: its block's `naming`, or the
/// harness default. A harness without
/// [`naming_choice`](crate::harness::profile::PluginPolicy::naming_choice)
/// returns its default and never reads the key.
///
/// # Errors
///
/// `refused: unknown-naming` for a value other than `bare` and `prefixed`.
pub(crate) fn naming_of(
    config: &MarketplaceConfig,
    plugin: &str,
    harness: Harness,
) -> Result<SkillNaming, CoreError> {
    let format = harness.format();
    if !format.policy.naming_choice {
        return Ok(format.naming);
    }
    match plugin_block(config, plugin, harness).get(NAMING_KEY) {
        None => Ok(format.naming),
        Some(value) => match value.as_str() {
            Some("bare") => Ok(SkillNaming::Bare),
            Some("prefixed") => Ok(SkillNaming::PluginPrefixed),
            _ => Err(CoreError::refused(
                refusal::UNKNOWN_NAMING,
                format!(
                    "`{harness}.{NAMING_KEY}` of plugin `{plugin}` must be `bare` or `prefixed`, not {value}"
                ),
            )),
        },
    }
}

/// One plugin's harness block for `harness`: the marketplace's block, less
/// every key it does not pass down
/// ([`Harness::passed_down_keys`]) and every key the plugin note sets
/// as plain frontmatter, overlaid by the plugin note's block key by key.
pub(crate) fn plugin_block(config: &MarketplaceConfig, plugin: &str, harness: Harness) -> Block {
    let passed = harness.passed_down_keys();
    let own = overrides_for(config, plugin);
    let plain = own
        .map(|o| crate::model::cascade::plain_inherited(&o.identity, EntityKind::Plugin))
        .unwrap_or_default();
    let mut block: Block = config
        .blocks
        .get(harness.as_str())
        .into_iter()
        .flatten()
        .filter(|_| crate::keys::inherits(EntityKind::Plugin, harness.as_str()))
        .filter(|(key, _)| passed.contains(key.as_str()))
        .filter(|(key, _)| !plain.contains(key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if let Some(own) = own.and_then(|o| o.blocks.get(harness.as_str())) {
        for (key, value) in own {
            block.insert(key.clone(), value.clone());
        }
    }
    block
}

/// `generated` as JSON with `block`'s keys from `allowed` merged over it. A
/// mapping merges one level deep, so `interface: {displayName: …}` keeps the
/// generated `category`; anything else replaces the generated value.
fn merged<T: Serialize>(generated: &T, block: &Block, allowed: &[&str]) -> serde_json::Value {
    let mut value = serde_json::to_value(generated).expect("manifest types serialize infallibly");
    if let serde_json::Value::Object(map) = &mut value {
        merge_into(map, block, allowed);
    }
    value
}

/// [`merged`] over a map already in hand.
fn merge_into(map: &mut Block, block: &Block, allowed: &[&str]) {
    for (key, over) in block {
        if !allowed.contains(&key.as_str()) {
            continue;
        }
        match (map.get_mut(key), over) {
            (Some(serde_json::Value::Object(into)), serde_json::Value::Object(over)) => {
                for (inner, value) in over {
                    into.insert(inner.clone(), value.clone());
                }
            }
            _ => {
                map.insert(key.clone(), over.clone());
            }
        }
    }
}

/// Insert `key` right after `after` in `map`, or at the end when `after` is
/// absent, so a generated field keeps its place in the byte form.
fn insert_after(map: &mut Block, after: &str, key: &str, value: serde_json::Value) {
    let mut out = Block::new();
    let mut placed = false;
    for (k, v) in std::mem::take(map) {
        let here = k == after;
        out.insert(k, v);
        if here {
            out.insert(key.to_owned(), value.clone());
            placed = true;
        }
    }
    if !placed {
        out.insert(key.to_owned(), value);
    }
    *map = out;
}

/// A plugin's `repository` and `homepage`, its own or the marketplace's,
/// added after `license` when the dialect's manifest has the field.
fn add_links(map: &mut Block, config: &MarketplaceConfig, plugin: &str, harness: Harness) {
    let fields = harness.format().policy.manifest_fields;
    let identity = plugin_identity(config, plugin);
    let links = [
        ("repository", identity.repository),
        ("homepage", identity.homepage),
    ];
    let mut after = "license".to_owned();
    for (key, value) in links {
        if let Some(value) = value
            && fields.contains(&key)
        {
            insert_after(map, &after, key, value.into());
            after = key.to_owned();
        }
    }
}

/// The finished manifest, after the plugin's links and block.
fn finish_plugin<T: Serialize>(
    generated: &T,
    config: &MarketplaceConfig,
    plugin: &str,
    harness: Harness,
) -> String {
    let mut value = serde_json::to_value(generated).expect("manifest types serialize infallibly");
    if let serde_json::Value::Object(map) = &mut value {
        add_links(map, config, plugin, harness);
        merge_into(
            map,
            &plugin_block(config, plugin, harness),
            harness.format().policy.manifest_fields,
        );
    }
    stable_json(&value)
}

/// The finished marketplace manifest: the marketplace block's top-level keys
/// over the top, and each plugin's block over its entry.
fn finish_marketplace<T: Serialize>(
    generated: &T,
    config: &MarketplaceConfig,
    harness: Harness,
) -> String {
    let policy = harness.format().policy;
    let empty = Block::new();
    let top = config.blocks.get(harness.as_str()).unwrap_or(&empty);
    let mut value = merged(generated, top, policy.marketplace_fields);
    if let Some(serde_json::Value::Array(entries)) = value.get_mut("plugins") {
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let serde_json::Value::Object(map) = entry {
                merge_into(
                    map,
                    &plugin_block(config, &name, harness),
                    policy.entry_fields,
                );
            }
        }
    }
    stable_json(&value)
}

pub(crate) fn overrides_for<'a>(
    config: &'a MarketplaceConfig,
    plugin: &str,
) -> Option<&'a PluginOverrides> {
    config.plugins.get(plugin)
}

/// `plugin`'s identity: its own fields, then the marketplace's where the key
/// table says a plugin inherits them. The one reading of a plugin's inherited
/// identity.
pub(crate) fn plugin_identity(config: &MarketplaceConfig, plugin: &str) -> ContainerIdentity {
    let mut identity = overrides_for(config, plugin)
        .map(|overrides| overrides.identity.clone())
        .unwrap_or_default();
    identity.fill_unset_from(&config.identity, EntityKind::Plugin);
    identity
}

/// The version a plugin ships as when nothing states one.
pub(crate) const DEFAULT_PLUGIN_VERSION: &str = "0.1.0";

/// The plugin's own `version`. The load gives the marketplace's own plugin the
/// marketplace's, so this is the one answer for every plugin.
pub(crate) fn plugin_version(config: &MarketplaceConfig, plugin: &str) -> String {
    overrides_for(config, plugin)
        .and_then(|o| o.identity.version.clone())
        .unwrap_or_else(|| DEFAULT_PLUGIN_VERSION.to_owned())
}

/// The description names the root's *own* members. A narrowed skill is
/// absent from the root, so its manifest does not name it.
fn plugin_description(config: &MarketplaceConfig, plugin: &str, members: &[&str]) -> String {
    overrides_for(config, plugin)
        .and_then(|o| o.description.clone())
        .unwrap_or_else(|| format!("Skills: {}.", members.join(", ")))
}

/// The `category` and `tags` a Claude or Cursor marketplace entry carries:
/// what the plugin note states, with `tags` read from its `keywords`. Neither
/// has a default here, so a plugin that states none adds nothing to the index.
#[derive(Serialize)]
struct EntryListing {
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

fn entry_listing(config: &MarketplaceConfig, plugin: &str) -> EntryListing {
    EntryListing {
        category: overrides_for(config, plugin).and_then(|o| o.identity.category.clone()),
        tags: plugin_keywords(config, plugin),
    }
}

fn plugin_keywords(config: &MarketplaceConfig, plugin: &str) -> Vec<String> {
    overrides_for(config, plugin)
        .and_then(|o| o.identity.keywords.clone())
        .unwrap_or_default()
}

fn plugin_category(config: &MarketplaceConfig, plugin: &str) -> String {
    overrides_for(config, plugin)
        .and_then(|o| o.identity.category.clone())
        .unwrap_or_else(|| "Productivity".to_owned())
}

fn plugin_license(config: &MarketplaceConfig, plugin: &str) -> String {
    plugin_identity(config, plugin)
        .license
        .unwrap_or_else(|| "UNLICENSED".to_owned())
}

fn plugin_author(config: &MarketplaceConfig, plugin: &str) -> Option<serde_json::Value> {
    plugin_identity(config, plugin).author.map(normalize_author)
}

/// Claude's plugin schema requires `author` as `{ name, email? }`, but a
/// marketplace note may still write `author: <name>` as a bare string.
/// A string is promoted to `{ "name": <string> }`; anything else (already an
/// object, or a type the schema does not expect) passes through unchanged.
/// The one call site both dialects render through, so both get it.
fn normalize_author(author: serde_json::Value) -> serde_json::Value {
    match author {
        serde_json::Value::String(name) => serde_json::json!({ "name": name }),
        other => other,
    }
}

/// A person as Cursor reads one: its docs and schema list `name` and `email`,
/// and the schema closes the object.
fn without_url(mut person: serde_json::Value) -> serde_json::Value {
    if let Some(fields) = person.as_object_mut() {
        fields.remove("url");
    }
    person
}

/// A plugin's Cursor display name: its own note's `cursor.displayName` when
/// that is a string, else its `display_name`. A marketplace's
/// `cursor.displayName` never applies, because it names the marketplace
/// ([`crate::keys::BLOCK_IDENTITY_KEYS`]).
fn cursor_display_name(config: &MarketplaceConfig, plugin: &str) -> Option<String> {
    plugin_block(config, plugin, Harness::Cursor)
        .get("displayName")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| overrides_for(config, plugin)?.identity.display_name.clone())
}

/// One plugin's pack manifest for `harness`, `members` its skills' names.
///
/// Claude, Codex and Cursor share one shape and differ in the display name:
/// Codex carries it in an `interface` block, Cursor as `displayName`, which
/// the cursor/plugins schema allows, and Claude not at all. Pi's is a
/// `package.json`. Field order is fixed.
pub(crate) fn render_plugin_manifest(
    harness: Harness,
    plugin: &str,
    members: &[&str],
    config: &MarketplaceConfig,
) -> String {
    if harness == Harness::Pi {
        return pi_package_manifest(plugin, members, config);
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Interface {
        display_name: String,
        category: String,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        version: String,
        description: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<serde_json::Value>,
        license: String,
        keywords: Vec<String>,
        skills: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        interface: Option<Interface>,
    }
    let display_name = overrides_for(config, plugin).and_then(|o| o.identity.display_name.clone());
    let interface = (harness == Harness::Codex).then(|| Interface {
        display_name: display_name.clone().unwrap_or_else(|| plugin.to_owned()),
        category: plugin_category(config, plugin),
    });
    let manifest = Manifest {
        name: plugin.to_owned(),
        display_name: (harness == Harness::Cursor)
            .then(|| cursor_display_name(config, plugin))
            .flatten(),
        version: plugin_version(config, plugin),
        description: plugin_description(config, plugin, members),
        author: plugin_author(config, plugin).map(|author| match harness {
            Harness::Cursor => without_url(author),
            _ => author,
        }),
        license: plugin_license(config, plugin),
        keywords: plugin_keywords(config, plugin),
        skills: "./skills/",
        interface,
    };
    finish_plugin(&manifest, config, plugin, harness)
}

/// The keyword the pi.dev gallery lists an npm package by.
const PI_PACKAGE_KEYWORD: &str = "pi-package";

/// Pi's pack manifest: a `package.json` naming the pack's `skills/`
/// directory. Pi itself needs only `pi.skills`; `name`, `version`,
/// `description`, `author`, `license` and `keywords` are the same fields the
/// other dialects carry, in their npm places. `keywords` gains
/// [`PI_PACKAGE_KEYWORD`], so the pack is listed once it is on npm; a
/// `pi.keywords` block key replaces the whole list, as any block key does.
fn pi_package_manifest(plugin: &str, members: &[&str], config: &MarketplaceConfig) -> String {
    #[derive(Serialize)]
    struct Pi {
        skills: [&'static str; 1],
    }
    #[derive(Serialize)]
    struct Manifest {
        name: String,
        version: String,
        description: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<serde_json::Value>,
        license: String,
        keywords: Vec<String>,
        pi: Pi,
    }
    let mut keywords = plugin_keywords(config, plugin);
    if !keywords.iter().any(|k| k == PI_PACKAGE_KEYWORD) {
        keywords.push(PI_PACKAGE_KEYWORD.to_owned());
    }
    let manifest = Manifest {
        name: plugin.to_owned(),
        version: plugin_version(config, plugin),
        description: plugin_description(config, plugin, members),
        author: plugin_author(config, plugin),
        license: plugin_license(config, plugin),
        keywords,
        pi: Pi {
            skills: ["./skills"],
        },
    };
    finish_plugin(&manifest, config, plugin, Harness::Pi)
}

/// Codex's index: a `interface.displayName` beside the name, and an entry
/// carrying an installation policy rather than a description and a version.
pub(crate) fn codex_marketplace_manifest(
    listed: &[(&str, Vec<&str>)],
    packs: &str,
    name: &str,
    config: &MarketplaceConfig,
) -> String {
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Interface {
            display_name: String,
        }
        #[derive(Serialize)]
        struct Source {
            source: &'static str,
            path: String,
        }
        #[derive(Serialize)]
        struct Policy {
            installation: &'static str,
            authentication: &'static str,
        }
        #[derive(Serialize)]
        struct Entry {
            name: String,
            source: Source,
            policy: Policy,
            category: String,
        }
        #[derive(Serialize)]
        struct Manifest {
            name: String,
            interface: Interface,
            plugins: Vec<Entry>,
        }
        let manifest = Manifest {
            name: name.to_owned(),
            interface: Interface {
                display_name: config
                    .identity
                    .display_name
                    .clone()
                    .unwrap_or_else(|| name.to_owned()),
            },
            plugins: listed
                .iter()
                .map(|(plugin, _)| Entry {
                    name: (*plugin).to_owned(),
                    source: Source {
                        source: "local",
                        path: format!("./{packs}/{plugin}"),
                    },
                    policy: Policy {
                        installation: "AVAILABLE",
                        authentication: "ON_INSTALL",
                    },
                    category: plugin_category(config, plugin),
                })
                .collect(),
        };
        finish_marketplace(&manifest, config, Harness::Codex)
    }
}

/// Claude's index: a **required** owner object, an optional top-level
/// description, and an entry carrying the description. The version stays in
/// `plugin.json` alone: with both, that one wins and `claude plugin validate`
/// warns.
///
/// `owner` is marked required and typed as an object — `name` required,
/// `email` and `url` optional — and a bare string is rejected outright
/// (`owner: Invalid input` from `claude plugin validate`). So a string is
/// normalised here rather than passed through, and an absent owner falls back
/// to the marketplace's own name, for the reason
/// [`cursor_marketplace_manifest`] gives: a manifest the harness loads beats a
/// field-perfect one it rejects, and there is no other name to reach for.
///
/// `description` is accepted both at the top level and under `metadata` for
/// backward compatibility, and the documentation recommends the top level
/// (<https://code.claude.com/docs/en/plugin-marketplaces> retrieved
/// 2026-09-20).
pub(crate) fn claude_marketplace_manifest(
    listed: &[(&str, Vec<&str>)],
    packs: &str,
    name: &str,
    config: &MarketplaceConfig,
) -> String {
    {
        #[derive(Serialize)]
        struct Entry {
            name: String,
            source: String,
            description: String,
            #[serde(flatten)]
            listing: EntryListing,
        }
        #[derive(Serialize)]
        struct Manifest {
            name: String,
            owner: serde_json::Value,
            #[serde(skip_serializing_if = "Option::is_none")]
            description: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            version: Option<String>,
            plugins: Vec<Entry>,
        }
        let manifest = Manifest {
            name: name.to_owned(),
            owner: config
                .identity
                .owner
                .clone()
                .map(normalize_author)
                .unwrap_or_else(|| serde_json::json!({ "name": name })),
            description: config.description.clone(),
            version: config.identity.version.clone(),
            plugins: listed
                .iter()
                .map(|(plugin, members)| Entry {
                    name: (*plugin).to_owned(),
                    source: format!("./{packs}/{plugin}"),
                    description: plugin_description(config, plugin, members),
                    listing: entry_listing(config, plugin),
                })
                .collect(),
        };
        finish_marketplace(&manifest, config, Harness::Claude)
    }
}

/// Cursor's index: Claude's entry shape under a **required** owner, a source
/// that is a plain relative path rather than a `./`-prefixed one, and a
/// description under `metadata` rather than at the top level.
///
/// All three are Cursor's documentation, not a preference
/// (<https://cursor.com/docs/reference/plugins> retrieved 2026-09-20): `owner`
/// is marked required, the worked example's `"source": "plugin-one"` carries
/// no prefix, and the field table gives `metadata` as the object holding
/// `description`, `version` and `pluginRoot` — Cursor documents no top-level
/// `description`, where Claude accepts one. When the configuration names no
/// owner, the marketplace's own name stands in — a manifest Cursor loads beats
/// a field-perfect one it rejects, and there is no other name to reach for.
pub(crate) fn cursor_marketplace_manifest(
    listed: &[(&str, Vec<&str>)],
    packs: &str,
    name: &str,
    config: &MarketplaceConfig,
) -> String {
    #[derive(Serialize)]
    struct Entry {
        name: String,
        /// Ours, not a documented entry field (#123): the plugin's own
        /// `displayName`, so the catalog and `plugin.json` agree.
        #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        source: String,
        description: String,
        version: String,
        #[serde(flatten)]
        listing: EntryListing,
    }
    #[derive(Serialize)]
    struct Metadata {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    }
    #[derive(Serialize)]
    struct Manifest {
        name: String,
        owner: serde_json::Value,
        /// Absent entirely when nothing describes the marketplace: an empty
        /// `metadata` object would be a field Cursor reads as present.
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Metadata>,
        plugins: Vec<Entry>,
    }
    let manifest = Manifest {
        name: name.to_owned(),
        owner: config
            .identity
            .owner
            .clone()
            .map(normalize_author)
            .map(without_url)
            .unwrap_or_else(|| serde_json::json!({ "name": name })),
        metadata: (config.description.is_some() || config.identity.version.is_some()).then(|| {
            Metadata {
                description: config.description.clone(),
                version: config.identity.version.clone(),
            }
        }),
        plugins: listed
            .iter()
            .map(|(plugin, members)| Entry {
                name: (*plugin).to_owned(),
                display_name: cursor_display_name(config, plugin),
                source: format!("{packs}/{plugin}"),
                description: plugin_description(config, plugin, members),
                version: plugin_version(config, plugin),
                listing: entry_listing(config, plugin),
            })
            .collect(),
    };
    finish_marketplace(&manifest, config, Harness::Cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The match in `facts_for` has no wildcard, so a new harness does not
    /// compile until it has a table. Each table holds the keys its manifests
    /// carry.
    #[test]
    fn facts_for_covers_every_harness() {
        for harness in Harness::ALL {
            let facts = crate::harness::field_types::facts_for(harness);
            assert!(facts.iter().any(|(key, _)| *key == "name"), "{harness}");
        }
    }

    #[test]
    fn a_marketplace_block_passes_down_only_its_passed_down_keys() {
        let mut config = MarketplaceConfig::default();
        let block = serde_json::json!({
            "naming": "prefixed",
            "logo": "logo.png",
            "owner": {"name": "o"},
            "name": "m",
            "zz-unknown": 1,
        });
        config
            .blocks
            .insert("cursor".to_owned(), block.as_object().unwrap().clone());
        let block = plugin_block(&config, "p", Harness::Cursor);
        assert_eq!(block.get("naming"), Some(&serde_json::json!("prefixed")));
        assert_eq!(block.get("logo"), Some(&serde_json::json!("logo.png")));
        for key in ["owner", "name", "zz-unknown"] {
            assert!(!block.contains_key(key), "{key}");
        }
    }

    /// A config whose plugin `t` has `display_name` and a `cursor` block of
    /// `cursor`.
    fn display_config(display_name: Option<&str>, cursor: serde_json::Value) -> MarketplaceConfig {
        let mut config = MarketplaceConfig::default();
        let mut overrides = PluginOverrides::default();
        overrides.identity.display_name = display_name.map(str::to_owned);
        if let serde_json::Value::Object(block) = cursor {
            overrides.blocks.insert("cursor".to_owned(), block);
        }
        config.plugins.insert("t".to_owned(), overrides);
        config
    }

    fn cursor_entry(config: &MarketplaceConfig) -> serde_json::Map<String, serde_json::Value> {
        let catalog: serde_json::Value = serde_json::from_str(&cursor_marketplace_manifest(
            &[("t", vec![])],
            ".cursor-plugin/packs",
            "m",
            config,
        ))
        .unwrap();
        catalog["plugins"][0].as_object().unwrap().clone()
    }

    fn cursor_plugin(config: &MarketplaceConfig) -> serde_json::Value {
        serde_json::from_str(&render_plugin_manifest(Harness::Cursor, "t", &[], config)).unwrap()
    }

    #[test]
    fn a_cursor_catalog_entry_carries_the_plugin_s_display_name() {
        let config = display_config(Some("Ticket workflow"), serde_json::Value::Null);
        let entry = cursor_entry(&config);
        let keys: Vec<&str> = entry.keys().map(String::as_str).collect();
        assert_eq!(keys[..2], ["name", "displayName"], "{entry:?}");
        assert_eq!(entry["displayName"], "Ticket workflow");

        let config = display_config(
            Some("Ticket workflow"),
            serde_json::json!({"displayName": "Ticket workflow (t)"}),
        );
        assert_eq!(cursor_entry(&config)["displayName"], "Ticket workflow (t)");
        assert_eq!(cursor_plugin(&config)["displayName"], "Ticket workflow (t)");

        let config = display_config(None, serde_json::Value::Null);
        assert!(!cursor_entry(&config).contains_key("displayName"));
    }

    #[test]
    fn a_marketplace_cursor_display_name_never_reaches_a_plugin() {
        let mut config = display_config(None, serde_json::Value::Null);
        config.blocks.insert(
            "cursor".to_owned(),
            serde_json::json!({"displayName": "Market"})
                .as_object()
                .unwrap()
                .clone(),
        );
        assert!(!cursor_entry(&config).contains_key("displayName"));
        assert!(cursor_plugin(&config).get("displayName").is_none());
    }

    #[test]
    fn cursor_source_has_no_dot_slash() {
        let entry = cursor_entry(&display_config(None, serde_json::Value::Null));
        assert_eq!(entry["source"], ".cursor-plugin/packs/t");
    }
}
