//! The manifest files a pack and a marketplace carry, in each harness's
//! dialect.
//!
//! # Manifests are byte-deterministic
//!
//! Stable key order (serde struct order), plugins and skills sorted by name,
//! two-space indentation, one trailing newline — so `check` can compare
//! bytes, and two builds of one vault are identical.
//!
//! The functions here are the shapes; which harness writes which is its
//! translator's choice ([`crate::harness::HarnessTranslator`]).

use serde::Serialize;

use crate::export::{MarketplaceConfig, PluginOverrides};
use crate::frontmatter::EntityKind;
use crate::harness::PluginDialect;

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

/// The harness a dialect belongs to, as its profile and its block name it.
fn harness_of(dialect: PluginDialect) -> &'static str {
    match dialect {
        PluginDialect::Claude => "claude",
        PluginDialect::Codex => "codex",
        PluginDialect::Cursor => "cursor",
        PluginDialect::Pi => "pi",
    }
}

/// The harness's plugin format, for its manifest key lists.
fn format_of(harness: &str) -> &'static crate::harness::PluginFormat {
    crate::harness::profile_named(harness)
        .expect("every dialect has a profile")
        .format()
}

type Block = serde_json::Map<String, serde_json::Value>;

/// One plugin's harness block for `harness`: the marketplace's block, less
/// the keys that belong to the marketplace manifest's own top level and the
/// keys that say which plugin this is, overlaid by the plugin note's block key
/// by key.
pub(crate) fn plugin_block(config: &MarketplaceConfig, plugin: &str, harness: &str) -> Block {
    let top = format_of(harness).marketplace_fields;
    let mut block: Block = config
        .blocks
        .get(harness)
        .into_iter()
        .flatten()
        .filter(|_| crate::keys::inherits(EntityKind::Plugin, harness))
        .filter(|(key, _)| {
            !top.contains(&key.as_str())
                && !crate::keys::BLOCK_IDENTITY_KEYS.contains(&key.as_str())
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if let Some(own) = overrides_for(config, plugin).and_then(|o| o.blocks.get(harness)) {
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
fn add_links(map: &mut Block, config: &MarketplaceConfig, plugin: &str, harness: &str) {
    let fields = format_of(harness).manifest_fields;
    let links = [
        (
            "repository",
            inherited(
                config,
                plugin,
                "repository",
                |own| &own.repository,
                |market| &market.repository,
            ),
        ),
        (
            "homepage",
            inherited(
                config,
                plugin,
                "homepage",
                |own| &own.homepage,
                |market| &market.homepage,
            ),
        ),
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
    harness: &str,
) -> String {
    let mut value = serde_json::to_value(generated).expect("manifest types serialize infallibly");
    if let serde_json::Value::Object(map) = &mut value {
        add_links(map, config, plugin, harness);
        merge_into(
            map,
            &plugin_block(config, plugin, harness),
            format_of(harness).manifest_fields,
        );
    }
    stable_json(&value)
}

/// The finished marketplace manifest: the marketplace block's top-level keys
/// over the top, and each plugin's block over its entry.
fn finish_marketplace<T: Serialize>(
    generated: &T,
    config: &MarketplaceConfig,
    harness: &str,
) -> String {
    let format = format_of(harness);
    let empty = Block::new();
    let top = config.blocks.get(harness).unwrap_or(&empty);
    let mut value = merged(generated, top, format.marketplace_fields);
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
                    format.entry_fields,
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

/// `plugin`'s value for `key`: its own, then the marketplace's where the key
/// table says the key inherits. The one reading of a plugin's inherited field.
pub(crate) fn inherited<T: Clone>(
    config: &MarketplaceConfig,
    plugin: &str,
    key: &str,
    own: impl Fn(&PluginOverrides) -> &Option<T>,
    market: impl Fn(&MarketplaceConfig) -> &Option<T>,
) -> Option<T> {
    overrides_for(config, plugin)
        .and_then(|overrides| own(overrides).clone())
        .or_else(|| {
            crate::keys::inherits(EntityKind::Plugin, key)
                .then(|| market(config).clone())
                .flatten()
        })
}

/// The version a plugin ships as when nothing states one.
pub(crate) const DEFAULT_PLUGIN_VERSION: &str = "0.1.0";

pub(crate) fn plugin_version(config: &MarketplaceConfig, plugin: &str) -> String {
    overrides_for(config, plugin)
        .and_then(|o| o.version.clone())
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
    let own = overrides_for(config, plugin);
    EntryListing {
        category: own.and_then(|o| o.category.clone()),
        tags: own.and_then(|o| o.keywords.clone()).unwrap_or_default(),
    }
}

fn plugin_category(config: &MarketplaceConfig, plugin: &str) -> String {
    overrides_for(config, plugin)
        .and_then(|o| o.category.clone())
        .unwrap_or_else(|| "Productivity".to_owned())
}

fn plugin_license(config: &MarketplaceConfig, plugin: &str) -> String {
    inherited(
        config,
        plugin,
        "license",
        |own| &own.license,
        |market| &market.license,
    )
    .unwrap_or_else(|| "UNLICENSED".to_owned())
}

fn plugin_author(config: &MarketplaceConfig, plugin: &str) -> Option<serde_json::Value> {
    inherited(
        config,
        plugin,
        "author",
        |own| &own.author,
        |market| &market.author,
    )
    .or_else(|| config.owner.clone())
    .map(normalize_author)
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

/// One plugin's pack manifest, `members` its skills' names.
///
/// One function for every layout a pack is written in — a built marketplace
/// and a destination — so there is one answer to "what is a pack manifest".
///
/// `hooks` is the manifest's `hooks` entry, written as given; `None` writes
/// none.
pub fn render_plugin_manifest(
    dialect: PluginDialect,
    plugin: &str,
    members: &[&str],
    config: &MarketplaceConfig,
    hooks: Option<&str>,
) -> String {
    if dialect == PluginDialect::Pi {
        return pi_package_manifest(plugin, members, config);
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Interface {
        display_name: String,
        category: String,
    }
    #[derive(Serialize)]
    struct Manifest<'a> {
        name: String,
        version: String,
        description: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<serde_json::Value>,
        license: String,
        keywords: Vec<String>,
        skills: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        hooks: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interface: Option<Interface>,
    }
    let interface = (dialect == PluginDialect::Codex).then(|| Interface {
        display_name: overrides_for(config, plugin)
            .and_then(|o| o.display_name.clone())
            .unwrap_or_else(|| plugin.to_owned()),
        category: plugin_category(config, plugin),
    });
    let manifest = Manifest {
        name: plugin.to_owned(),
        version: plugin_version(config, plugin),
        description: plugin_description(config, plugin, members),
        author: plugin_author(config, plugin),
        license: plugin_license(config, plugin),
        keywords: overrides_for(config, plugin)
            .and_then(|o| o.keywords.clone())
            .unwrap_or_default(),
        skills: "./skills/",
        hooks,
        interface,
    };
    finish_plugin(&manifest, config, plugin, harness_of(dialect))
}

/// Pi's pack manifest: a `package.json` naming the pack's `skills/`
/// directory. Pi itself needs only `pi.skills`; `name`, `version`,
/// `description`, `author` and `license` are the same fields the other
/// dialects carry, in their npm places.
pub(crate) fn pi_package_manifest(
    plugin: &str,
    members: &[&str],
    config: &MarketplaceConfig,
) -> String {
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
        pi: Pi,
    }
    let manifest = Manifest {
        name: plugin.to_owned(),
        version: plugin_version(config, plugin),
        description: plugin_description(config, plugin, members),
        author: plugin_author(config, plugin),
        license: plugin_license(config, plugin),
        pi: Pi {
            skills: ["./skills"],
        },
    };
    finish_plugin(&manifest, config, plugin, "pi")
}

/// Codex's index: a `interface.displayName` beside the name, and an entry
/// carrying an installation policy rather than a description and a version.
pub fn codex_marketplace_manifest(
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
        finish_marketplace(&manifest, config, "codex")
    }
}

/// Claude's index: a **required** owner object, an optional top-level
/// description, and an entry carrying the description and version the pack
/// manifest also holds.
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
pub fn claude_marketplace_manifest(
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
            version: String,
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
                .owner
                .clone()
                .map(normalize_author)
                .unwrap_or_else(|| serde_json::json!({ "name": name })),
            description: config.description.clone(),
            version: config.version.clone(),
            plugins: listed
                .iter()
                .map(|(plugin, members)| Entry {
                    name: (*plugin).to_owned(),
                    source: format!("./{packs}/{plugin}"),
                    description: plugin_description(config, plugin, members),
                    version: plugin_version(config, plugin),
                    listing: entry_listing(config, plugin),
                })
                .collect(),
        };
        finish_marketplace(&manifest, config, "claude")
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
pub fn cursor_marketplace_manifest(
    listed: &[(&str, Vec<&str>)],
    packs: &str,
    name: &str,
    config: &MarketplaceConfig,
) -> String {
    #[derive(Serialize)]
    struct Entry {
        name: String,
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
            .owner
            .clone()
            .map(normalize_author)
            .unwrap_or_else(|| serde_json::json!({ "name": name })),
        metadata: (config.description.is_some() || config.version.is_some()).then(|| Metadata {
            description: config.description.clone(),
            version: config.version.clone(),
        }),
        plugins: listed
            .iter()
            .map(|(plugin, members)| Entry {
                name: (*plugin).to_owned(),
                source: format!("{packs}/{plugin}"),
                description: plugin_description(config, plugin, members),
                version: plugin_version(config, plugin),
                listing: entry_listing(config, plugin),
            })
            .collect(),
    };
    finish_marketplace(&manifest, config, "cursor")
}
