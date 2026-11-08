//! skillwright's own frontmatter keys, read out of a note and typed.
//!
//! A note's frontmatter splits three ways:
//!
//! | Class | Where it goes | At publish |
//! |---|---|---|
//! | skillwright's — the [`crate::keys`] table's owned keys for the note's kind | [`VaultFrontmatter`], typed | **dropped** |
//! | The spec's and the harness's fields | [`crate::SkillContent`]'s modeled fields | emitted |
//! | Anything else | `SkillContent::annotations`, with a warning | under `metadata:` on a skill |
//!
//! # Bare or prefixed
//!
//! Keys are bare by default, and the key table says which bare keys are
//! skillwright's for each kind, so `license` is the pack's on a plugin note
//! and the spec's on a skill. Under a namespace ([`Namespace`]), every key
//! the prefix claims is skillwright's, in the flat shape (`sw-version`) or
//! the nested one (`sw: {version: …}`); a note carrying both is refused ·
//! `mixed-frontmatter-shape`. A prefixed key this build does not model is
//! consumed, so it cannot reach a destination, and listed in
//! [`VaultFrontmatter::unknown`].
//!
//! # A wrong-shaped value of ours is refused
//!
//! skillwright's keys are stripped at emit, so carrying a wrong-shaped one
//! verbatim would lose it. An absent key and a present-but-wrong key must not
//! share an outcome: `targets: claude` (a scalar where a list belongs) read
//! as "publish everywhere" would push files where the author meant not to.
//!
//! # The kind comes from the folder
//!
//! [`resolve_kind`] ranks a caller's request over the kind folder a note sits
//! in.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use agent_adapt::Argument;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::CoreError;
use crate::keys::{Owner, Shape};
use crate::namespace::Namespace;

/// Which publishable unit a note is: what the folder it sits in names.
///
/// default is for **absence only**. An unrecognised value is a typo, and
/// defaulting it would publish a `SKILL.md` at every destination for something
/// that is not a skill — silently, and as a write.
///
/// `mcp-server` is named by the decision as a later kind and is deliberately
/// not a variant yet.
///
/// # A kind in the vocabulary need not be publishable
///
/// Entering the vocabulary makes a note *discoverable* — the workbench can
/// list it and say what it is — while [`crate::publish`]'s kind gate refuses it
/// `no-renderer` until a projection exists. `Marketplace` and `Hook` are the
/// kinds sitting there now. Outside the vocabulary a note is `unknown-kind`: a typo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    /// One skill: frontmatter plus a body, published as a `SKILL.md`.
    #[default]
    Skill,
    /// A pack of components, holding them in the folders beneath it.
    Plugin,
    /// A slash command, published into the target's commands layout.
    Command,
    /// A subagent definition, published into the target's agents layout.
    Agent,
    /// One automation hook: typed frontmatter naming the event and the
    /// matcher, and a body that is the script or the prompt.
    ///
    /// A hook reaches a harness through its plugin's `hooks/hooks.json`
    /// rather than through a file of its own, so [`crate::publish`] refuses
    /// a standalone hook publish `no-renderer` while [`crate::export`]
    /// renders it into the pack.
    Hook,
    /// A distribution: the plugins beneath it, built into one marketplace.
    ///
    /// The level above [`Self::Plugin`], and the only kind with no
    /// per-destination layout at all — a runtime reads skills, never
    /// marketplaces. [`crate::export`] renders one; [`crate::publish`] refuses
    /// it `no-renderer`, and says where to go.
    Marketplace,
}

impl EntityKind {
    /// Every kind, in declaration order.
    pub const ALL: [EntityKind; 6] = [
        Self::Skill,
        Self::Plugin,
        Self::Command,
        Self::Agent,
        Self::Hook,
        Self::Marketplace,
    ];
    /// Every spelling [`EntityKind::from_str`] accepts, in the order a refusal
    /// lists them.
    pub const KNOWN: &'static [&'static str] =
        &["agent", "command", "hook", "marketplace", "plugin", "skill"];

    /// Does this kind hold other components in the folders beneath it?
    ///
    /// True for the container kinds, and it is what tells a *shipping* overlap
    /// from an ordinary nesting. Everything under a skill's own directory
    /// travels with it, so another component in there would publish twice —
    /// but a plugin's directory holds its skills by design, and a
    /// marketplace's holds its plugins, and neither ships what is beneath it
    /// as its own resources.
    pub const fn holds_components(self) -> bool {
        matches!(self, Self::Plugin | Self::Marketplace)
    }

    /// The spelling a note carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Plugin => "plugin",
            Self::Command => "command",
            Self::Agent => "agent",
            Self::Hook => "hook",
            Self::Marketplace => "marketplace",
        }
    }
}

/// A kind name this build does not publish.
///
/// A distinct type rather than a bare `CoreError` so [`EntityKind::from_str`]
/// stays usable without a path in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    /// The value as the note spelled it.
    pub value: String,
}

impl fmt::Display for UnknownKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a kind this build knows (known: {})",
            self.value,
            EntityKind::KNOWN.join(", ")
        )
    }
}

impl std::error::Error for UnknownKind {}

impl FromStr for EntityKind {
    type Err = UnknownKind;

    /// Exact match, no case folding: `Skill` is refused as loudly as `skil`.
    /// A vocabulary two surfaces write has to be one spelling, and quietly
    /// accepting a second is how a third appears.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "skill" => Ok(Self::Skill),
            "plugin" => Ok(Self::Plugin),
            "command" => Ok(Self::Command),
            "agent" => Ok(Self::Agent),
            "hook" => Ok(Self::Hook),
            "marketplace" => Ok(Self::Marketplace),
            other => Err(UnknownKind {
                value: other.to_owned(),
            }),
        }
    }
}

/// Which layer decided a note's kind.
///
/// Carried alongside the kind because the answer to "why is this a plugin"
/// is a different sentence per layer, and a report that only had the kind
/// could not write any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KindSource {
    /// The caller said so explicitly.
    Override,
    /// The kind folder the note sits in (`skills/`, `plugins/` and so on;
    /// see [`crate::discover`]).
    Directory,
    /// No layer applied. Absent means `skill`.
    Default,
}

impl KindSource {
    /// The spelling a report prints, matching the serialized form so a caller
    /// that renders this and a caller that deserializes it cannot disagree.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Override => "override",
            Self::Directory => "directory",
            Self::Default => "default",
        }
    }
}

/// A kind and the layer that decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedKind {
    /// The resolved kind.
    pub kind: EntityKind,
    /// The layer that decided the kind.
    pub source: KindSource,
}

/// What the caller knows about a note's kind. A note never says its own kind:
/// the folder decides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindHints {
    /// An explicit instruction — "treat this note as a skill" — from a flag
    /// or a conversion. Wins outright.
    pub requested: Option<EntityKind>,
    /// What the note's kind folder said, already resolved by the caller
    /// against the note's path. `None` when no folder matched.
    pub folder: Option<EntityKind>,
}

/// Resolves a note's kind: the caller's request, then the kind folder, then
/// [`EntityKind::Skill`].
pub fn resolve_kind(hints: KindHints) -> ResolvedKind {
    if let Some(kind) = hints.requested {
        return ResolvedKind {
            kind,
            source: KindSource::Override,
        };
    }
    if let Some(kind) = hints.folder {
        return ResolvedKind {
            kind,
            source: KindSource::Directory,
        };
    }
    ResolvedKind {
        kind: EntityKind::default(),
        source: KindSource::Default,
    }
}

/// Where adoption read a note from: the `adopted` block, written by adoption
/// and by nothing else.
///
/// Every field is optional and a note carrying none of them is hand-authored
/// ([`Provenance::is_hand_authored`]). It is data in the user's file rather
/// than a lock: a hand-edited value is authoritative, and a wrong one produces
/// a refusal rather than a clobber.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// `adopted.source` — the path adoption read the skill from.
    pub source: Option<String>,
    /// `adopted.format` — the runtime layout it was read in (`claude`,
    /// `codex`, `agents`, …), or `none`.
    pub format: Option<String>,
    /// `adopted.source_hash` — hash over the source folder as read. Answers
    /// "has the source moved on".
    pub source_hash: Option<String>,
    /// `adopted.hash` — hash over what adoption wrote. Answers "has the note
    /// been edited since".
    pub adopted_hash: Option<String>,
    /// `adopted.at` — when adoption wrote it.
    pub adopted_at: Option<String>,
    /// `adopted.also_at` — other paths the same content was found at.
    pub also_at: Option<Vec<String>>,
}

impl Provenance {
    /// True when adoption never touched this note.
    pub fn is_hand_authored(&self) -> bool {
        self == &Self::default()
    }
}

/// The fields of the `adopted` block, in the order adoption writes them.
pub const ADOPTED_FIELDS: &[&str] = &["source", "format", "hash", "source_hash", "at", "also_at"];

/// What a container note says about the thing it names — the manifest fields
/// a marketplace or a plugin publishes under.
///
/// Read only on a container: the same words on a skill are the skill's own
/// (`license`) or unknown.
///
/// **Every field is optional and every absence has a default**, resolved by
/// [`crate::export`] rather than here, because the defaults are a property of
/// the manifest dialect being written and not of the note.
///
/// `owner` and `author` are [`Value`] rather than `String` because an author
/// may spell either as `{name, email, url}`, and narrowing them here would
/// refuse a shape the manifest accepts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerIdentity {
    /// `owner` — the marketplace's owner, a name or a mapping.
    pub owner: Option<Value>,
    /// `author` — the author a plugin publishes under, a name or a mapping. On a
    /// marketplace note it is the default for every plugin under it.
    pub author: Option<Value>,
    /// `display_name` — the human-facing name where a dialect carries one
    /// separately from the slug.
    pub display_name: Option<String>,
    /// `license` — an SPDX identifier, unread and passed through.
    pub license: Option<String>,
    /// `keywords`.
    pub keywords: Option<Vec<String>>,
    /// `category` — the plugin's marketplace category.
    pub category: Option<String>,
    /// `version` — the plugin's or marketplace's own version.
    pub version: Option<String>,
    /// `repository` — the source repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// `homepage` — the documentation URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

impl ContainerIdentity {
    /// True when the note said nothing about its identity, and every manifest
    /// field will come from a default or from configuration.
    pub fn is_silent(&self) -> bool {
        self == &Self::default()
    }
}

/// Everything skillwright's own keys said about one note, read out of its
/// frontmatter and typed.
///
/// Deliberately **not** part of [`crate::SkillContent`]: these keys are
/// configuration about *this* vault, and keeping them off the type
/// [`crate::projection::project_skill`] renders makes the strip structural.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VaultFrontmatter {
    /// `targets`. **Three outcomes, not two:** `None` is absent and means
    /// every destination (or what the plugin says); `Some(vec![])` is an
    /// empty list and means nowhere; `Some(names)` narrows to those.
    pub targets: Option<Vec<String>>,
    /// `arguments` — what this skill or command's invocation takes,
    /// validated: every name is a slug, every name is unique, no `values`
    /// list is empty, and every `default` is one of its own `values` when
    /// `values` is present. Empty when the key is absent.
    ///
    /// [`crate::content::SkillContent::apply_arguments`] carries it onto the
    /// published content; the raw key never reaches a destination.
    pub arguments: Vec<Argument>,
    /// The `adopted` block.
    pub provenance: Provenance,
    /// The manifest fields a container note publishes under. Empty on any
    /// other note.
    pub identity: ContainerIdentity,
    /// The harness blocks — `claude:`, `codex:`, `cursor:`, `pi:` — keyed by
    /// harness and then by that harness's own key names. Empty when the note
    /// has none.
    ///
    /// Read and shape-checked here; validated against the harness's key list
    /// by [`crate::publish::overrides_for`] for a component, and by the
    /// manifest writer for a container.
    pub blocks: Map<String, Value>,
    /// Every key on the note that the key table does not list for its kind,
    /// sorted. Each gets a warning. On a skill they publish under
    /// `metadata:`; on a container they are ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown: Vec<String>,
    /// Every key the note sets to a value, in its bare spelling. An inherited
    /// block key yields to these ([`crate::model::Cascade::apply`]).
    #[serde(skip)]
    pub own_keys: std::collections::BTreeSet<String>,
}

impl VaultFrontmatter {
    /// Takes skillwright's keys for a note of `kind` out of `fields`, leaving
    /// the note's own keys.
    ///
    /// `fields` is the note's frontmatter as a JSON object, untouched yet.
    /// `ns` is the vault's namespace, and must be the one the emit side uses.
    /// `path` names the note in a refusal.
    ///
    /// # Errors
    ///
    /// * `refused: mixed-frontmatter-shape` — under a prefixed namespace, the
    ///   note carries the nested block *and* at least one flat key.
    /// * A content error naming the key and the path when one of
    ///   skillwright's keys is the wrong shape.
    pub fn take_from(
        fields: &mut Map<String, Value>,
        path: &Path,
        ns: &Namespace,
        kind: EntityKind,
    ) -> Result<Self, CoreError> {
        let mut ours = claim(fields, path, ns, kind)?;
        let own_keys = fields
            .iter()
            .chain(ours.iter())
            .filter(|(_, value)| !value.is_null())
            .map(|(key, _)| ns.field(key).to_owned())
            .collect();

        check_shapes(&ours, path, ns, kind)?;

        let targets = take_string_list(&mut ours, &ns.key("targets"), path)?;
        // A key the table does not list for this kind stays in `ours`, to be
        // reported as unknown.
        let reads = |key: &str| crate::keys::is_owned(kind, key);
        let arguments = if reads("arguments") {
            take_arguments(&mut ours, &ns.key("arguments"), path)?
        } else {
            Vec::new()
        };
        let mut overrides = Map::new();
        for harness in crate::keys::HARNESS_BLOCKS.iter().filter(|h| reads(h)) {
            if let Some(block) = take_harness_block(&mut ours, &ns.key(harness)) {
                overrides.insert((*harness).to_owned(), Value::Object(block));
            }
        }
        // A key a block reads set to nothing is unset and dropped. One it does
        // not read stays, so it still warns.
        let unknown_in_blocks = crate::discover::unknown_block_keys(&overrides, kind);
        for (harness, block) in &mut overrides {
            if let Value::Object(block) = block {
                *block = crate::harness::field_types::without_unset(
                    harness,
                    std::mem::take(block),
                    &unknown_in_blocks,
                );
            }
        }
        let mut unknown: Vec<String> = Vec::new();
        let provenance = if reads("adopted") {
            take_adopted(&mut ours, &ns.key("adopted"), path, &mut unknown)?
        } else {
            Provenance::default()
        };

        let identity = if kind.holds_components() {
            ContainerIdentity {
                owner: ours.remove(&ns.key("owner")).filter(|v| !v.is_null()),
                author: ours.remove(&ns.key("author")).filter(|v| !v.is_null()),
                display_name: take_string(&mut ours, &ns.key("display_name"), path)?,
                license: take_string(&mut ours, &ns.key("license"), path)?,
                keywords: take_string_list(&mut ours, &ns.key("keywords"), path)?,
                category: take_string(&mut ours, &ns.key("category"), path)?,
                version: take_string(&mut ours, &ns.key("version"), path)?,
                repository: take_string(&mut ours, &ns.key("repository"), path)?,
                homepage: take_string(&mut ours, &ns.key("homepage"), path)?,
            }
        } else {
            ContainerIdentity::default()
        };

        unknown.extend(ours.keys().cloned());
        unknown.extend(
            fields
                .iter()
                .filter(|(key, value)| !reads_as_written(kind, key, value, ns))
                .map(|(key, _)| key.clone()),
        );
        unknown.sort();
        unknown.dedup();

        Ok(Self {
            targets,
            arguments,
            provenance,
            identity,
            blocks: overrides,
            unknown,
            own_keys,
        })
    }
}

/// Splits skillwright's keys off `fields` and normalises them onto the flat
/// spelling.
///
/// Under a prefixed namespace, every key the prefix claims moves, and a
/// nested `sw: {version: 1.0.0}` comes back as `{"sw-version": "1.0.0"}`.
/// Bare, the key table decides: the keys it lists as skillwright's for `kind`
/// move. `arguments` holding names stays: that is Claude's own key.
fn claim(
    fields: &mut Map<String, Value>,
    path: &Path,
    ns: &Namespace,
    kind: EntityKind,
) -> Result<Map<String, Value>, CoreError> {
    if ns.is_bare() {
        let owned: Vec<String> = fields
            .keys()
            .filter(|key| crate::keys::is_owned(kind, key))
            .filter(|key| {
                key.as_str() != "arguments" || !is_claude_arguments(&fields[key.as_str()])
            })
            .cloned()
            .collect();
        let mut ours = Map::new();
        for key in owned {
            if let Some(value) = fields.remove(&key) {
                ours.insert(key, value);
            }
        }
        return Ok(ours);
    }

    let block_key = ns.block_key();
    let flat: Vec<String> = fields
        .keys()
        .filter(|k| ns.is_ours(k) && k.as_str() != block_key)
        .cloned()
        .collect();
    let nested = fields.contains_key(block_key);

    // The shape check comes first and is not a reconciliation: it refuses
    // whether or not the two shapes carry the same fields.
    if nested && !flat.is_empty() {
        return Err(refuse(
            "mixed-frontmatter-shape",
            path,
            format!(
                "the note carries a nested `{block_key}:` block and {}. \
                 Move the flat keys into the block, or remove the block",
                listed(&flat)
            ),
        ));
    }

    if !nested {
        let mut ours = Map::new();
        for key in flat {
            if let Some(value) = fields.remove(&key) {
                ours.insert(key, value);
            }
        }
        return Ok(ours);
    }

    match fields.remove(block_key).unwrap_or(Value::Null) {
        // `sw:` with nothing under it is an empty block, not a broken one.
        Value::Null => Ok(Map::new()),
        Value::Object(map) => Ok(map
            .into_iter()
            .map(|(key, value)| (ns.key(&key), value))
            .collect()),
        other => Err(CoreError::wrong_kind(
            path,
            format!(
                "`{block_key}:` must be a mapping of our fields, not {}",
                type_name(&other)
            ),
        )),
    }
}

/// Whether a key left in the note's own fields is one the read takes as
/// written. `arguments` holding names is Claude's key under any namespace.
fn reads_as_written(kind: EntityKind, key: &str, value: &Value, ns: &Namespace) -> bool {
    match key {
        "arguments" if is_claude_arguments(value) => crate::keys::lookup(kind, key).is_some(),
        _ => crate::keys::is_known(kind, key, ns),
    }
}

/// Claude's own `arguments` key: a space-separated string, or a list of
/// strings.
fn is_claude_arguments(value: &Value) -> bool {
    match value {
        Value::String(_) => true,
        Value::Array(items) => items.iter().all(Value::is_string),
        _ => false,
    }
}

/// Refuses a key of ours whose value is not the [`crate::keys::Shape`] the
/// table lists for `kind`. `ours` holds flat spellings, so a prefixed key the
/// table does not list is left to be reported in [`VaultFrontmatter::unknown`].
fn check_shapes(
    ours: &Map<String, Value>,
    path: &Path,
    ns: &Namespace,
    kind: EntityKind,
) -> Result<(), CoreError> {
    for (key, value) in ours {
        if let Some(spec) = crate::keys::lookup(kind, ns.field(key))
            && spec.owner == Owner::Skillwright
            && !spec.shape.accepts(value)
        {
            return Err(CoreError::wrong_kind(
                path,
                shape_refusal(spec.shape, key, value),
            ));
        }
    }
    Ok(())
}

/// What a value of the wrong shape is told, naming the key as the note
/// spelled it.
fn shape_refusal(shape: Shape, key: &str, value: &Value) -> String {
    let found = type_name(value);
    match shape {
        Shape::String => format!(
            "`{key}` must be a string, not {found}{}",
            quoting_hint(value)
        ),
        Shape::Url => match value.as_str() {
            Some(text) => format!("`{key}` must be an http or https URL, not `{text}`"),
            None => format!("`{key}` must be an http or https URL, not {found}"),
        },
        Shape::StringList => match value {
            Value::Array(items) => format!(
                "`{key}` must be a list of strings; one entry is {}",
                items
                    .iter()
                    .find(|item| !item.is_string())
                    .map_or("not a string", type_name)
            ),
            _ => format!(
                "`{key}` must be a list, not {found}. \
                 An empty list means nowhere; removing the key means everywhere"
            ),
        },
        Shape::Person => {
            format!("`{key}` must be a name or a `{{name, email, url}}` mapping, not {found}")
        }
        Shape::Arguments => format!("`{key}` must be a list of argument objects, not {found}"),
        Shape::HarnessBlock => {
            format!("`{key}` must be a mapping of that harness's own keys, not {found}")
        }
        Shape::Adopted => format!(
            "`{key}` must be a mapping of {}, not {found}",
            ADOPTED_FIELDS.join(", ")
        ),
        Shape::Bool | Shape::Number | Shape::StringMap => {
            format!("`{key}` has the wrong shape: {found}")
        }
    }
}

/// Removes a harness block and returns it. `key: null` reads as no block.
fn take_harness_block(ours: &mut Map<String, Value>, key: &str) -> Option<Map<String, Value>> {
    match ours.remove(key) {
        Some(Value::Object(block)) => Some(block),
        _ => None,
    }
}

/// Removes the `adopted` block and reads it. A field the block does not
/// define is added to `unknown` as `adopted.<field>`.
fn take_adopted(
    ours: &mut Map<String, Value>,
    key: &str,
    path: &Path,
    unknown: &mut Vec<String>,
) -> Result<Provenance, CoreError> {
    let Some(Value::Object(mut block)) = ours.remove(key) else {
        return Ok(Provenance::default());
    };
    let field = |name: &str| format!("{key}.{name}");
    let provenance = Provenance {
        source: take_string(&mut block, "source", path)
            .map_err(|_| shape(path, &field("source")))?,
        format: take_string(&mut block, "format", path)
            .map_err(|_| shape(path, &field("format")))?,
        adopted_hash: take_string(&mut block, "hash", path)
            .map_err(|_| shape(path, &field("hash")))?,
        source_hash: take_string(&mut block, "source_hash", path)
            .map_err(|_| shape(path, &field("source_hash")))?,
        adopted_at: take_string(&mut block, "at", path).map_err(|_| shape(path, &field("at")))?,
        also_at: take_string_list(&mut block, "also_at", path)?,
    };
    unknown.extend(block.keys().map(|name| field(name)));
    Ok(provenance)
}

/// The wrong-shape error for one string field of the `adopted` block.
fn shape(path: &Path, key: &str) -> CoreError {
    CoreError::wrong_kind(path, format!("`{key}` must be a string"))
}

/// Removes `key` and returns it as a string, refusing any other shape.
fn take_string(
    ours: &mut Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<Option<String>, CoreError> {
    match ours.remove(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(other) => Err(CoreError::wrong_kind(
            path,
            format!(
                "`{key}` must be a string, not {}{}",
                type_name(&other),
                quoting_hint(&other)
            ),
        )),
    }
}

/// Removes `key` and returns it as a list of strings, refusing any other
/// shape — including a bare scalar, which would otherwise be one destination
/// misread as none.
fn take_string_list(
    ours: &mut Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<Option<Vec<String>>, CoreError> {
    let Some(value) = ours.remove(key) else {
        return Ok(None);
    };
    match value {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(s) => out.push(s),
                    other => {
                        return Err(CoreError::wrong_kind(
                            path,
                            format!(
                                "`{key}` must be a list of strings; one entry is {}",
                                type_name(&other)
                            ),
                        ));
                    }
                }
            }
            Ok(Some(out))
        }
        other => Err(CoreError::wrong_kind(
            path,
            format!(
                "`{key}` must be a list, not {}. \
                 An empty list means nowhere; removing the key means everywhere",
                type_name(&other)
            ),
        )),
    }
}

/// Removes `key` and returns it as a validated list of [`Argument`]s.
///
/// Absent means no arguments — an ordinary note. Every other outcome is
/// either [`CoreError::wrong_kind`] for a wrong *shape* (not a list, an entry
/// not a mapping, a field not the type it must be) or `refused` ·
/// `invalid-arguments` for a value that is the right shape and still cannot
/// be an argument list — see [`validate_arguments`] for those four rules.
///
/// # Errors
///
/// See the function documentation above.
fn take_arguments(
    ours: &mut Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<Vec<Argument>, CoreError> {
    let Some(value) = ours.remove(key) else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(mut entry) = item else {
            return Err(CoreError::wrong_kind(
                path,
                format!("`{key}` entries must be mappings, not {}", type_name(&item)),
            ));
        };

        let name = match entry.remove("name") {
            Some(Value::String(s)) => s,
            Some(other) => {
                return Err(CoreError::wrong_kind(
                    path,
                    format!(
                        "`{key}` entry `name` must be a string, not {}",
                        type_name(&other)
                    ),
                ));
            }
            None => {
                return Err(CoreError::wrong_kind(
                    path,
                    format!("a `{key}` entry is missing `name`"),
                ));
            }
        };
        let description = take_entry_string(&mut entry, key, &name, "description", path)?;
        let required = match entry.remove("required") {
            None => false,
            Some(Value::Bool(b)) => b,
            Some(other) => {
                return Err(CoreError::wrong_kind(
                    path,
                    format!(
                        "`{key}` entry `{name}`'s `required` must be a boolean, not {}",
                        type_name(&other)
                    ),
                ));
            }
        };
        let values = match entry.remove("values") {
            None => None,
            Some(Value::Array(items)) => {
                let mut strings = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        Value::String(s) => strings.push(s),
                        other => {
                            return Err(CoreError::wrong_kind(
                                path,
                                format!(
                                    "`{key}` entry `{name}`'s `values` must be a list of \
                                     strings; one entry is {}",
                                    type_name(&other)
                                ),
                            ));
                        }
                    }
                }
                Some(strings)
            }
            Some(other) => {
                return Err(CoreError::wrong_kind(
                    path,
                    format!(
                        "`{key}` entry `{name}`'s `values` must be a list, not {}",
                        type_name(&other)
                    ),
                ));
            }
        };
        let default = take_entry_string(&mut entry, key, &name, "default", path)?;

        if let Some(unexpected) = entry.keys().next() {
            return Err(CoreError::wrong_kind(
                path,
                format!(
                    "`{key}` entry `{name}` carries `{unexpected}`, which is not one of \
                     `name`, `description`, `required`, `values`, `default`"
                ),
            ));
        }

        out.push(Argument {
            name,
            description,
            required,
            values,
            default,
        });
    }

    validate_arguments(&out, key, path)?;
    Ok(out)
}

/// `take_string`, scoped to one field of one `arguments` entry, so every
/// entry-level string error names the argument it is about.
fn take_entry_string(
    entry: &mut Map<String, Value>,
    key: &str,
    arg_name: &str,
    field: &str,
    path: &Path,
) -> Result<Option<String>, CoreError> {
    match entry.remove(field) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(CoreError::wrong_kind(
            path,
            format!(
                "`{key}` entry `{arg_name}`'s `{field}` must be a string, not {}",
                type_name(&other)
            ),
        )),
    }
}

/// The four rules an `arguments` list must satisfy beyond its shape,
/// checked in this order so the first violation a caller sees is the first
/// one in declaration order that is wrong.
///
/// * Every `name` is a slug: `^[a-z0-9]+(-[a-z0-9]+)*$`, the same language
///   [`agent_adapt::naming::slugify_skill_name`] targets — an argument name
///   becomes `$name` in a Claude Code body and a word in a generated hint,
///   and neither tolerates whitespace or a leading hyphen.
/// * No `values` list is present and empty. `values: []` is not "no enum"
///   — that is what omitting the key means — it is an enum with nothing an
///   author could ever satisfy.
/// * Every `default` is one of its own `values`, when `values` is present.
///   A default outside the enum could never be assumed, so publishing it
///   would show the model a value invocation is not allowed to produce.
/// * Every `name` is unique. Two arguments named `format` would each
///   overwrite `$format` and the other's row in a rendered hint.
///
/// # Errors
///
/// `refused` · `invalid-arguments`, naming the argument and the rule.
fn validate_arguments(arguments: &[Argument], key: &str, path: &Path) -> Result<(), CoreError> {
    let mut seen = std::collections::HashSet::new();
    for arg in arguments {
        if !is_slug(&arg.name) {
            return Err(refuse(
                "invalid-arguments",
                path,
                format!(
                    "`{key}` names `{}`, which is not a slug — it must match \
                     ^[a-z0-9]+(-[a-z0-9]+)*$",
                    arg.name
                ),
            ));
        }
        if let Some(values) = &arg.values
            && values.is_empty()
        {
            return Err(refuse(
                "invalid-arguments",
                path,
                format!(
                    "`{key}` entry `{}`'s `values` is an empty list. Remove `values` for no \
                     enum, or list at least one allowed value",
                    arg.name
                ),
            ));
        }
        if let (Some(values), Some(default)) = (&arg.values, &arg.default)
            && !values.contains(default)
        {
            return Err(refuse(
                "invalid-arguments",
                path,
                format!(
                    "`{key}` entry `{}`'s `default` (`{default}`) is not one of its `values` \
                     ({})",
                    arg.name,
                    values
                        .iter()
                        .map(|v| format!("`{v}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        if !seen.insert(arg.name.as_str()) {
            return Err(refuse(
                "invalid-arguments",
                path,
                format!(
                    "`{key}` declares `{}` more than once. Names must be unique",
                    arg.name
                ),
            ));
        }
    }
    Ok(())
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$` — the Agent Skills spec's `name` language, and
/// the one an argument's own `name` is held to for the same reason: it
/// becomes a path-free identifier (`$name`, a hint token) wherever it is
/// used, and none of those readers accept whitespace or punctuation.
fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Says the quoting rule at the one moment it is actionable: a value we wanted
/// as a string arrived as a number or a boolean because YAML types a bare
/// scalar by shape.
pub(crate) fn quoting_hint(value: &Value) -> &'static str {
    if value.is_number() || value.is_boolean() {
        " — quote it. YAML reads an unquoted number, `true` or `false` as \
         that type, not as text"
    } else {
        ""
    }
}

/// What a value is, for a refusal: `a number`, `a list`, `nothing`.
pub(crate) fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a mapping",
    }
}

/// "flat key `sw-license`" / "flat keys `sw-license`, `sw-targets`" — a refusal has
/// to name every offending key, because the fix is to move each one.
fn listed(keys: &[String]) -> String {
    let quoted: Vec<String> = keys.iter().map(|k| format!("`{k}`")).collect();
    let noun = if keys.len() == 1 {
        "flat key"
    } else {
        "flat keys"
    };
    format!("{noun} {}", quoted.join(", "))
}

/// A refusal carrying one of the publish spec's closed-set reason
/// tokens, the note it is about, and the prose that says what to do.
fn refuse(token: &str, path: &Path, message: impl fmt::Display) -> CoreError {
    CoreError::refused(token, format!("{}: {message}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `EntityKind::ALL` names every kind. The match has no wildcard, so a new
    /// kind fails to compile here until it is added to the list.
    #[test]
    fn all_names_every_kind() {
        fn listed(kind: EntityKind) -> bool {
            match kind {
                EntityKind::Skill
                | EntityKind::Plugin
                | EntityKind::Command
                | EntityKind::Agent
                | EntityKind::Hook
                | EntityKind::Marketplace => EntityKind::ALL.contains(&kind),
            }
        }
        for kind in [
            EntityKind::Skill,
            EntityKind::Plugin,
            EntityKind::Command,
            EntityKind::Agent,
            EntityKind::Hook,
            EntityKind::Marketplace,
        ] {
            assert!(listed(kind), "{kind:?}");
        }
    }
    use serde_json::json;

    const NOTE: &str = "/vault/skills/code-review.md";

    /// Frontmatter as `crate::vault` hands it over: a JSON object.
    fn fields(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("test frontmatter must be a mapping, got {other}"),
        }
    }

    /// The `sw` namespace.
    fn read(value: Value) -> Result<(VaultFrontmatter, Map<String, Value>), CoreError> {
        read_as(&Namespace::sw(), value)
    }

    fn read_as(
        ns: &Namespace,
        value: Value,
    ) -> Result<(VaultFrontmatter, Map<String, Value>), CoreError> {
        let mut map = fields(value);
        let ours = VaultFrontmatter::take_from(&mut map, Path::new(NOTE), ns, EntityKind::Skill)?;
        Ok((ours, map))
    }

    fn read_kind(
        ns: &Namespace,
        kind: EntityKind,
        value: Value,
    ) -> Result<(VaultFrontmatter, Map<String, Value>), CoreError> {
        let mut map = fields(value);
        let ours = VaultFrontmatter::take_from(&mut map, Path::new(NOTE), ns, kind)?;
        Ok((ours, map))
    }

    fn bare(kind: EntityKind, value: Value) -> (VaultFrontmatter, Map<String, Value>) {
        read_kind(&Namespace::bare(), kind, value).unwrap()
    }

    fn x() -> Namespace {
        Namespace::new("x").unwrap()
    }

    #[test]
    fn harness_blocks_are_read_and_never_reach_annotations() {
        let (ours, rest) = bare(
            EntityKind::Skill,
            json!({"description": "d", "claude": {"effort": "high"}}),
        );
        assert_eq!(ours.blocks["claude"]["effort"], "high");
        assert!(!rest.contains_key("claude"));
        assert!(ours.unknown.is_empty(), "{:?}", ours.unknown);
    }

    #[test]
    fn a_wrong_shaped_harness_block_is_refused_naming_the_key() {
        let err = read_kind(&Namespace::bare(), EntityKind::Skill, json!({"codex": "x"}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("`codex`"), "{err}");
    }

    #[test]
    fn the_kind_layers_rank_the_request_then_the_folder_then_skill() {
        let resolved = resolve_kind(KindHints {
            requested: Some(EntityKind::Agent),
            folder: Some(EntityKind::Plugin),
        });
        assert_eq!(resolved.kind, EntityKind::Agent);
        assert_eq!(resolved.source, KindSource::Override);
        let resolved = resolve_kind(KindHints {
            requested: None,
            folder: Some(EntityKind::Plugin),
        });
        assert_eq!(resolved.kind, EntityKind::Plugin);
        assert_eq!(resolved.source, KindSource::Directory);
        let resolved = resolve_kind(KindHints::default());
        assert_eq!(resolved.kind, EntityKind::Skill);
        assert_eq!(resolved.source, KindSource::Default);
    }

    #[test]
    fn under_a_namespace_a_bare_unprefixed_key_stays_among_the_notes_own_keys() {
        let (_, rest) = read_kind(
            &Namespace::sw(),
            EntityKind::Skill,
            json!({"description": "d", "flavour": "mint"}),
        )
        .unwrap();
        assert_eq!(rest["flavour"], "mint");
    }

    #[test]
    fn targets_absent_empty_and_populated_are_three_outcomes() {
        let target = |value: Value| bare(EntityKind::Skill, value).0.targets;
        assert_eq!(target(json!({"description": "d"})), None);
        assert_eq!(target(json!({"targets": []})), Some(vec![]));
        assert_eq!(
            target(json!({"targets": ["claude"]})),
            Some(vec!["claude".to_owned()])
        );
    }

    #[test]
    fn a_scalar_targets_is_refused_not_read_as_everywhere() {
        let err = read_kind(
            &Namespace::bare(),
            EntityKind::Skill,
            json!({"targets": "claude"}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("`targets`"), "{err}");
    }

    #[test]
    fn the_adopted_block_round_trips_into_provenance() {
        let (ours, _) = bare(
            EntityKind::Skill,
            json!({"adopted": {
                "source": "/a", "format": "claude", "hash": "h1",
                "source_hash": "h2", "at": "2026-11-06", "also_at": ["/b"],
            }}),
        );
        let provenance = ours.provenance;
        assert_eq!(provenance.source.as_deref(), Some("/a"));
        assert_eq!(provenance.format.as_deref(), Some("claude"));
        assert_eq!(provenance.adopted_hash.as_deref(), Some("h1"));
        assert_eq!(provenance.source_hash.as_deref(), Some("h2"));
        assert_eq!(provenance.adopted_at.as_deref(), Some("2026-11-06"));
        assert_eq!(provenance.also_at, Some(vec!["/b".to_owned()]));
        assert!(!provenance.is_hand_authored());
    }

    #[test]
    fn an_unknown_field_in_the_adopted_block_is_reported() {
        let (ours, _) = bare(EntityKind::Skill, json!({"adopted": {"odd": "x"}}));
        assert_eq!(ours.unknown, vec!["adopted.odd".to_owned()]);
    }

    #[test]
    fn a_note_with_no_provenance_is_hand_authored() {
        let (ours, _) = bare(EntityKind::Skill, json!({"description": "d"}));
        assert!(ours.provenance.is_hand_authored());
    }

    #[test]
    fn license_is_the_pack_s_on_a_plugin_and_the_spec_s_on_a_skill() {
        let note = json!({"description": "d", "license": "MIT"});
        let (ours, rest) = bare(EntityKind::Plugin, note.clone());
        assert_eq!(ours.identity.license.as_deref(), Some("MIT"));
        assert!(!rest.contains_key("license"));
        let (ours, rest) = bare(EntityKind::Skill, note);
        assert_eq!(ours.identity.license, None);
        assert!(rest.contains_key("license"));
    }

    #[test]
    fn repository_and_homepage_must_be_urls() {
        let (ours, _) = bare(
            EntityKind::Plugin,
            json!({"repository": "https://example.com/r", "homepage": "http://example.com"}),
        );
        assert_eq!(
            ours.identity.repository.as_deref(),
            Some("https://example.com/r")
        );
        assert_eq!(
            ours.identity.homepage.as_deref(),
            Some("http://example.com")
        );
        let err = read_kind(
            &Namespace::bare(),
            EntityKind::Plugin,
            json!({"repository": "not a url"}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("`repository`"), "{err}");
    }

    #[test]
    fn bare_arguments_of_plain_strings_are_claude_s_own_key() {
        let (ours, rest) = bare(EntityKind::Skill, json!({"arguments": ["a", "b"]}));
        assert!(ours.arguments.is_empty());
        assert!(rest.contains_key("arguments"));
        let (ours, rest) = bare(EntityKind::Skill, json!({"arguments": [{"name": "pr"}]}));
        assert_eq!(ours.arguments.len(), 1);
        assert!(!rest.contains_key("arguments"));
    }

    #[test]
    fn unknown_keys_are_reported_and_left_in_place() {
        let (ours, rest) = bare(
            EntityKind::Skill,
            json!({"description": "d", "zeta": 1, "alpha": {"nested": true}}),
        );
        assert_eq!(ours.unknown, vec!["alpha".to_owned(), "zeta".to_owned()]);
        assert!(rest.contains_key("zeta") && rest.contains_key("alpha"));
        // A container's keys are not a skill's, and the other way round.
        let (ours, _) = bare(EntityKind::Skill, json!({"version": "1.0.0"}));
        assert_eq!(ours.unknown, vec!["version".to_owned()]);
        let (ours, _) = bare(EntityKind::Plugin, json!({"model": "opus"}));
        assert_eq!(ours.unknown, vec!["model".to_owned()]);
    }

    #[test]
    fn a_prefixed_namespace_reads_flat_and_nested_alike() {
        let flat = read_kind(
            &Namespace::sw(),
            EntityKind::Plugin,
            json!({"sw-version": "1.0.0", "sw-codex": {"interface": {}}}),
        )
        .unwrap()
        .0;
        let nested = read_kind(
            &Namespace::sw(),
            EntityKind::Plugin,
            json!({"sw": {"version": "1.0.0", "codex": {"interface": {}}}}),
        )
        .unwrap()
        .0;
        assert_eq!(flat, nested);
        assert_eq!(flat.identity.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn an_empty_nested_block_reads_as_the_defaults() {
        let (ours, _) = read(json!({"sw": null})).unwrap();
        assert_eq!(ours, VaultFrontmatter::default());
    }

    #[test]
    fn a_nested_block_that_is_not_a_mapping_is_refused() {
        let err = read(json!({"sw": "x"})).unwrap_err().to_string();
        assert!(err.contains("`sw:`"), "{err}");
    }

    #[test]
    fn both_shapes_on_one_note_is_refused_only_under_a_namespace() {
        let err = read(json!({"sw": {"targets": []}, "sw-targets": []}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("mixed-frontmatter-shape"), "{err}");
        assert!(err.contains("sw-targets"), "{err}");
    }

    #[test]
    fn an_unmodeled_prefixed_key_is_consumed_and_reported() {
        let (ours, rest) = read(json!({"sw-invented": 1})).unwrap();
        assert!(!rest.contains_key("sw-invented"));
        assert_eq!(ours.unknown, vec!["sw-invented".to_owned()]);
    }

    #[test]
    fn a_configured_stem_leaves_other_prefixes_foreign() {
        let (ours, rest) =
            read_as(&x(), json!({"x-targets": [], "sw-targets": ["claude"]})).unwrap();
        assert_eq!(ours.targets, Some(vec![]));
        assert!(rest.contains_key("sw-targets"));
    }

    #[test]
    fn absent_arguments_is_an_empty_list() {
        let (ours, _) = read(json!({"description": "d"})).unwrap();
        assert_eq!(ours.arguments, Vec::new());
    }

    #[test]
    fn a_minimal_argument_round_trips() {
        let (ours, foreign) = read(json!({
            "description": "d",
            "sw-arguments": [{"name": "pr-number"}],
        }))
        .unwrap();
        assert_eq!(
            ours.arguments,
            vec![Argument {
                name: "pr-number".into(),
                description: None,
                required: false,
                values: None,
                default: None,
            }]
        );
        assert!(
            !foreign.contains_key("sw-arguments"),
            "consumed like every key of ours"
        );
    }

    #[test]
    fn every_field_on_an_argument_round_trips() {
        let (ours, _) = read(json!({
            "description": "d",
            "sw-arguments": [{
                "name": "format",
                "description": "Output format",
                "required": true,
                "values": ["markdown", "json"],
                "default": "markdown",
            }],
        }))
        .unwrap();
        assert_eq!(
            ours.arguments,
            vec![Argument {
                name: "format".into(),
                description: Some("Output format".into()),
                required: true,
                values: Some(vec!["markdown".into(), "json".into()]),
                default: Some("markdown".into()),
            }]
        );
    }

    #[test]
    fn several_arguments_keep_declaration_order() {
        let (ours, _) = read(json!({
            "description": "d",
            "sw-arguments": [{"name": "first"}, {"name": "second"}, {"name": "third"}],
        }))
        .unwrap();
        let names: Vec<&str> = ours.arguments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn arguments_not_a_list_is_malformed() {
        let err = read(json!({"description": "d", "sw-arguments": "pr-number"})).unwrap_err();
        assert!(err.to_string().contains("sw-arguments"), "{err}");
        assert!(
            err.to_string().contains("list of argument objects"),
            "{err}"
        );
    }

    #[test]
    fn an_entry_not_a_mapping_is_malformed() {
        let err = read(json!({"description": "d", "sw-arguments": ["pr-number"]})).unwrap_err();
        assert!(err.to_string().contains("mappings"), "{err}");
    }

    #[test]
    fn an_entry_missing_name_is_malformed() {
        let err =
            read(json!({"description": "d", "sw-arguments": [{"description": "d"}]})).unwrap_err();
        assert!(err.to_string().contains("missing `name`"), "{err}");
    }

    #[test]
    fn an_unrecognised_key_on_an_entry_is_malformed() {
        let err = read(json!({
            "description": "d",
            "sw-arguments": [{"name": "a", "descripton": "typo"}],
        }))
        .unwrap_err();
        assert!(err.to_string().contains("descripton"), "{err}");
    }

    #[test]
    fn a_name_that_is_not_a_slug_is_refused() {
        for bad in [
            "PR Number",
            "pr_number",
            "-leading",
            "trailing-",
            "",
            "a--b",
        ] {
            let err = read(json!({
                "description": "d",
                "sw-arguments": [{"name": bad}],
            }))
            .unwrap_err();
            let message = err.to_string();
            assert!(message.contains("invalid-arguments"), "{bad}: {message}");
            assert!(message.contains("not a slug"), "{bad}: {message}");
        }
    }

    #[test]
    fn an_empty_values_list_is_refused() {
        let err = read(json!({
            "description": "d",
            "sw-arguments": [{"name": "format", "values": []}],
        }))
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("invalid-arguments"), "{message}");
        assert!(message.contains("empty list"), "{message}");
    }

    #[test]
    fn a_default_outside_values_is_refused() {
        let err = read(json!({
            "description": "d",
            "sw-arguments": [{
                "name": "format",
                "values": ["markdown", "json"],
                "default": "yaml",
            }],
        }))
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("invalid-arguments"), "{message}");
        assert!(message.contains("`default`"), "{message}");
        assert!(message.contains("yaml"), "{message}");
    }

    #[test]
    fn a_default_matching_one_of_values_is_accepted() {
        let (ours, _) = read(json!({
            "description": "d",
            "sw-arguments": [{
                "name": "format",
                "values": ["markdown", "json"],
                "default": "json",
            }],
        }))
        .unwrap();
        assert_eq!(ours.arguments[0].default.as_deref(), Some("json"));
    }

    #[test]
    fn a_duplicate_name_is_refused() {
        let err = read(json!({
            "description": "d",
            "sw-arguments": [{"name": "format"}, {"name": "format"}],
        }))
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("invalid-arguments"), "{message}");
        assert!(message.contains("more than once"), "{message}");
    }

    #[test]
    fn arguments_follow_the_configured_stem_like_every_key_of_ours() {
        let (ours, foreign) = read_as(
            &x(),
            json!({"description": "d", "x-arguments": [{"name": "pr-number"}]}),
        )
        .unwrap();
        assert_eq!(ours.arguments.len(), 1);
        assert!(!foreign.contains_key("x-arguments"));

        // And under the default stem, `x-arguments` is somebody else's key.
        let (ours, foreign) = read(json!({"x-arguments": [{"name": "pr-number"}]})).unwrap();
        assert!(ours.arguments.is_empty());
        assert!(foreign.contains_key("x-arguments"));
    }
}
