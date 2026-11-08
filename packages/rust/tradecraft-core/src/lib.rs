//! The Tradecraft core: read a source root, model what is in it, and project
//! a skill onto a target's format.
//!
//! The markdown tree under the source root is the system of record and git is
//! its history. There is no store, and no file here is a projection of
//! one.
//!
//! | Module | Does |
//! |---|---|
//! | [`corpus`] | What a source root is, and how one is described |
//! | [`scan`] | Walk a root and find the skills in it |
//! | [`discover`] | Walk a vault's skills root and vet every publishable note |
//! | [`skill`] | The on-disk skill: frontmatter framing, refs, detail reads |
//! | [`keys`] | The closed key table: every frontmatter key, per kind |
//! | [`namespace`] | Which prefix skillwright's keys carry — none by default |
//! | [`frontmatter`] | skillwright's keys read out of a note, typed |
//! | [`vault`] | The *vault* skill: a note plus a sibling folder of peers |
//! | [`content`] | The skill shape projection renders, and its peer resources |
//! | [`compile`] | Embeds expanded, links rewritten, references collected |
//! | [`projection`] | One skill to one target's files, via `agent-adapt` |
//! | [`publish`] | A vault note written onto a destination, idempotently |
//! | [`export`] | The vault built into one marketplace tree per harness, and checked |
//! | [`harness`] | Each harness as a profile row (data) and a translator (code) |
//! | [`settings`] | `skillwright.md`, the root file, as a build config |
//! | [`paths`] | Load the root file with its workspace folded in ([`load_marketplace`]), or its pieces |
//! | [`adopt`] | Published SKILL.md bundles imported as vault notes |
//! | [`authoring`] | Where a note goes, what it collides with, what it says |
//! | [`guard`] | The rails that keep a publish out of a source root |
//! | [`vfs`] | The filesystem seam every I/O-touching function takes |
//!
//! # Specs named in comments
//!
//! Comments cite a few rule sets by short name. Each is the behavior the
//! listed module implements and tests; those module docs are the reference.
//!
//! | Name | Rule set | Where |
//! |---|---|---|
//! | publish spec | What a publish may write: per-file statuses, the closed set of refusal tokens, `targets` exclusion, destination overlap | [`publish`], [`export`], [`guard`], [`manifest`] |
//! | vault spec | A skill is a note plus a sibling folder of peers; publishing is five steps (embeds, links, references, peers, target dialect) | [`vault`], [`publish`] |
//! | compile spec | Embeds are expanded and links rewritten; an unresolved link is not a blocker | [`compile`] |
//! | marketplace layout spec | How one marketplace tree per harness is laid out and checked | [`export`] |
//! | namespace spec | The keys the key table lists as skillwright's belong to this tool and are stripped on publish; a configured prefix moves them under it | [`keys`], [`namespace`], [`frontmatter`] |
//! | write-guard spec | A publish never writes into the source root or the vault | [`guard`] |
//! | status spec | The per-file states a status report uses, including `unknown` | [`status`] |
//! | CLI spec | Exit codes, stdout versus stderr, `--dry-run`, confirmation before writes | the `skillwright` crate |
//! | Agent Skills spec | The public Agent Skills standard: `SKILL.md` frontmatter fields, name rules, and length caps on `description` and `compatibility` | [`skill`], [`discover`], [`adopt`] |
//!
//! # Two skill layouts, deliberately
//!
//! [`skill`] reads the *published* shape — `SKILL.md` inside a directory named
//! for the skill. [`vault`] reads the *canonical* shape — `code-review.md`
//! with peers in a sibling `code-review/`. They are not two spellings of one
//! reader: the vault spec keeps the vault on note names
//! because Obsidian's quick-switcher, graph, and backlinks are keyed on them,
//! and the standard's folder layout is what a publish produces at the far end.
//!
//! # No `std::fs` above [`vfs`]
//!
//! Every function here that reaches a filesystem takes a `&dyn Vfs` first.
//! That is what lets one core serve a native shell and a
//! `wasm32-unknown-unknown` build of itself, where there is no filesystem to
//! reach and a `std::fs` call compiles cleanly and then fails at runtime.
//!
//! # No ambient configuration either, for the same reason
//!
//! Every function that consults the frontmatter namespace takes a
//! [`Namespace`] alongside its `&dyn Vfs`. A vault may configure a prefix, so
//! there is no one true spelling to read from a global — and a caller reading
//! one note bare and another under `sw` must be able to, in one process.
//! `Namespace::default()` is bare.

/// The README's code blocks, compiled and run as doctests so they cannot drift.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

pub mod adopt;
pub mod authoring;
pub mod compile;
pub mod content;
pub mod corpus;
pub mod discover;
pub mod error;
pub mod export;
pub mod frontmatter;
pub mod guard;
pub mod harness;
pub mod hooks_file;
pub mod keys;
pub mod manifest;
pub mod model;
pub mod namespace;
pub mod paths;
pub mod patterns;
pub mod projection;
pub mod publish;
pub mod scan;
pub mod semver;
pub mod settings;
pub mod skill;
pub mod source_view;
pub mod status;
mod text;
pub mod vault;
pub mod vfs;
mod walk;

pub use adopt::{
    AdoptMode, AdoptReport, AdoptRequest, AdoptStatus, AdoptedSkill, DEFAULT_ADOPTED_PLUGIN,
    adopt_skills, rewrite_to_wikilinks,
};
pub use agent_adapt::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticScope, Severity};
pub use authoring::{
    AuthoringOp, FieldEdit, NewNote, Placement, apply_field_edits, compose_note, ns_field_edits,
    ns_field_label, plan_convert_kind, plan_create_note, plan_delete_component,
    plan_delete_container, plan_move_component, plan_rename_component, plan_rename_marketplace,
    plan_rename_plugin, read_field, read_ns_field,
};
pub use compile::{
    CallContext, CompileSource, CompiledNote, IndexedSkill, RelatedSplit, SkillIndex, SkillSource,
    compile_note, compile_projection, related_of,
};
pub use content::{SkillContent, SkillProjection};
pub use corpus::*;
pub use discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, KIND_DIRS, MISSING_DESCRIPTION,
    MISSING_DESCRIPTION_REASON, SKILL_ENTRY, SkippedNote, UNKNOWN_KEY_REASON, default_name,
    discover_components, discover_roster, folder_kind, in_plugin_folder, is_skill_entry,
    plugin_folder_in,
};
pub use error::*;
pub use export::{
    AssetSpec, ExportMode, ExportReport, ExportRequest, ExportUnit, HookFileSpec,
    MarketplaceConfig, PluginOverrides, SkillNaming, StaleKind, UnitStatus, export_skills,
    marketplace_manifest_path, owned_paths, packs_path, plugin_manifest_path, published_skill_name,
    rendered_skill_call,
};
pub use frontmatter::{
    ADOPTED_FIELDS, ContainerIdentity, EntityKind, KindHints, KindSource, Provenance, ResolvedKind,
    VaultFrontmatter, resolve_kind,
};
pub use harness::manifest::{check_packs, packs_of};
pub use keys::{HARNESS_BLOCKS, KEYS, KeySpec, Owner, Shape};
/// The vault's file index, for [`CompileSource::links`]: re-exported so a
/// consumer needs no `markdown-link` dependency of its own.
pub use markdown_link::LinkIndex;
pub use model::inherit_targets;
pub use namespace::{InvalidStem, Namespace, StemProblem};
pub use paths::{
    LoadedMarketplace, MarketplaceInputs, WORKSPACE_DIR, find_root_file, load_marketplace,
    marketplace_inputs, resolve_path, workspace_above,
};
pub use publish::{
    KNOWN_TARGETS, PublishMode, PublishOutcome, PublishReason, PublishReport, PublishRequest,
    PublishStatus, PublishedFile, ReasonToken, canonical_target, canonical_targets, publish_skill,
    publishes_to,
};
pub use semver::{BumpLevel, next_version, parse_semver};
pub use settings::{
    AssetSetting, BuildSettings, BumpSettings, DestinationSetting, HookSetting, MarketplaceFields,
    PatchScheme, ROOT_FILE, RootFile, SettingsError, compose_root_file, parse_root_file,
};
pub use source_view::{
    SourceView, VaultScope, effective_config, effective_configs, marketplace_folder, source_view,
    vault_config,
};
pub use status::{PublishState, PublishUnitStatus, StatusRequest, status_of};
pub use vault::{VaultNote, bundle_root, read_vault_note, read_vault_skill};
pub use vfs::{DirEntry, MemVfs, NativeVfs, PlannedOp, PreviewVfs, PreviewWrite, Vfs};
pub use walk::{built_paths, built_paths_for};
/// The value of a frontmatter field, for [`FieldEdit::set`]: re-exported so a
/// consumer needs no `yaml-splice` dependency of its own.
pub use yaml_splice::Value as FieldValue;
