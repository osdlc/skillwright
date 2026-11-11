//! The Tradecraft core: read a source root, model what is in it, and project
//! a skill onto a target's format.
//!
//! The markdown tree under the source root is the system of record and git is
//! its history. There is no store, and no file here is a projection of
//! one.
//!
//! | Module | Does |
//! |---|---|
//! | [`layout`] | The vault layout: find the root, check every folder, place a path |
//! | [`discover`] | Read every note of a vault by its position and vet it |
//! | [`skill`] | The frontmatter split and the small readers every note path shares |
//! | [`keys`] | The closed key table: every frontmatter key, per kind |
//! | [`namespace`] | Which prefix skillwright's keys carry — none by default |
//! | [`frontmatter`] | skillwright's keys read out of a note, typed |
//! | [`vault`] | The *vault* skill: `<name>/<name>.md` and the folder it owns |
//! | [`content`] | The skill shape projection renders, and its peer resources |
//! | [`compile`] | Embeds expanded, links rewritten, references collected |
//! | [`projection`] | One skill to one target's files, via `agent-adapt` |
//! | [`publish`] | The target names, reason tokens and harness-block fold a build shares |
//! | [`export`] | The vault built into one marketplace tree per harness, and checked |
//! | [`harness`] | Each harness as a profile row (data) and a translator (code) |
//! | [`settings`] | `skillwright.md`, the root file, as a build config |
//! | [`paths`] | Load a vault: its root file and its marketplaces ([`load_vault`]) |
//! | [`field_edit`] | Frontmatter field edits that keep every other byte |
//! | [`status`] | The version-bump rule a build and `bump` share |
//! | [`guard`] | Containment resolved through links |
//! | [`vfs`] | The filesystem seam every I/O-touching function takes |
//!
//! # Specs named in comments
//!
//! Comments cite a few rule sets by short name. Each is the behavior the
//! listed module implements and tests; those module docs are the reference.
//!
//! | Name | Rule set | Where |
//! |---|---|---|
//! | refusal tokens | The reason word a refusal leads with, `targets` exclusion, output overlap | [`publish`], [`export`], [`guard`] |
//! | vault spec | A skill is a note plus a sibling folder of peers; building is five steps (embeds, links, references, peers, target dialect) | [`vault`], [`export`] |
//! | compile spec | Embeds are expanded and links rewritten; an unresolved link is not a blocker | [`compile`] |
//! | marketplace layout spec | How one marketplace tree per harness is laid out and checked | [`export`] |
//! | namespace spec | The keys the key table lists as skillwright's belong to this tool and are stripped on build; a configured prefix moves them under it | [`keys`], [`namespace`], [`frontmatter`] |
//! | write-guard spec | A build never writes into `marketplaces/` | [`export`] |
//! | CLI spec | Exit codes, stdout versus stderr, `--dry-run`, confirmation before writes | the `skillwright` crate |
//! | Agent Skills spec | The public Agent Skills standard: `SKILL.md` frontmatter fields, name rules, and length caps on `description` and `compatibility` | [`skill`], [`discover`] |
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

pub mod compile;
pub mod content;
pub mod discover;
pub mod error;
pub mod export;
pub mod field_edit;
pub mod frontmatter;
pub mod guard;
pub mod harness;
pub mod hooks_file;
pub mod keys;
pub mod layout;
pub mod model;
pub mod namespace;
pub mod paths;
pub mod projection;
pub mod publish;
pub mod semver;
pub mod settings;
pub mod skill;
mod source_view;
pub mod status;
mod text;
pub mod vault;
pub mod vfs;
mod walk;

pub use agent_adapt::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticScope, Severity};
pub use compile::{
    CallContext, CompileSource, CompiledNote, IndexedSkill, RelatedSplit, SkillIndex, SkillSource,
    compile_note, compile_projection, related_of,
};
pub use content::{SkillContent, SkillProjection};
pub use discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, INVALID_NAME_REASON,
    MISSING_DESCRIPTION_REASON, SkippedNote, UNKNOWN_KEY_REASON, default_name, discover_components,
    discover_roster, refuse_invalid_names, refuse_undescribed,
};
pub use error::*;
pub use export::{
    AssetSpec, ExportMode, ExportReport, ExportRequest, ExportUnit, HookFileSpec,
    MarketplaceConfig, PluginOverrides, SkillNaming, StaleKind, UnitStatus, export_skills,
    marketplace_manifest_path, owned_paths, packs_path, plugin_manifest_path, published_skill_name,
    rendered_skill_call,
};
pub use field_edit::{
    FieldEdit, apply_field_edits, compose_note, ns_field_edits, ns_field_label, read_ns_field,
};
pub use frontmatter::{
    ContainerIdentity, EntityKind, KindHints, KindSource, ResolvedKind, VaultFrontmatter,
    resolve_kind,
};
pub use harness::manifest::{check_packs, packs_of};
pub use keys::{HARNESS_BLOCKS, KEYS, KeySpec, Owner, Shape};
pub use layout::{
    KIND_DIRS, Layout, MARKETPLACES_DIR, Position, RESERVED_NAMES, check_vault_root, classify,
    find_vault_root, is_vault_root, read_layout,
};
/// The vault's file index, for [`CompileSource::links`]: re-exported so a
/// consumer needs no `markdown-link` dependency of its own.
pub use markdown_link::LinkIndex;
pub use namespace::{InvalidStem, Namespace, StemProblem};
pub use paths::{
    LoadedVault, MarketplaceInputs, VaultMarketplace, dist_dir, load_vault, marketplace_inputs,
    resolve_path,
};
pub use publish::{KNOWN_TARGETS, canonical_target};
pub use semver::{BumpLevel, next_version, parse_semver};
pub use settings::{
    AssetSetting, BuildSettings, BumpSettings, HookSetting, MarketplaceFields, PatchScheme,
    ROOT_FILE, RootFile, SettingsError, compose_root_file, parse_marketplace_note, parse_root_file,
};
pub use vault::{VaultNote, bundle_root, read_vault_note, read_vault_skill};
pub use vfs::{DirEntry, MemVfs, NativeVfs, PlannedOp, PreviewVfs, PreviewWrite, Vfs};
pub use walk::built_paths;
/// The value of a frontmatter field, for [`FieldEdit::set`]: re-exported so a
/// consumer needs no `yaml-splice` dependency of its own.
pub use yaml_splice::Value as FieldValue;
