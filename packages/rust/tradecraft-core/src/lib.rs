//! The Tradecraft core: read a vault, model what is in it, and project a
//! skill onto a target's format.
//!
//! The markdown tree under the vault root is the system of record and git is
//! its history. There is no store, and no file here is a projection of
//! one.
//!
//! | Module | Does |
//! |---|---|
//! | [`layout`] | The vault layout: find the root, check every folder, place a path |
//! | [`discover`] | Read every note of a vault by its position and vet it |
//! | [`skill`] | The frontmatter split and the small readers every note path shares |
//! | [`keys`] | The closed key table: every frontmatter key, per kind |
//! | [`frontmatter`] | skillwright's keys read out of a note, typed |
//! | [`vault`] | The *vault* skill: `<name>/<name>.md` and the folder it owns |
//! | [`content`] | The skill shape projection renders, and its peer resources |
//! | [`compile`] | Embeds expanded, links rewritten, references collected |
//! | [`projection`] | One skill to one target's files, via `agent-adapt` |
//! | [`export`] | The vault built into one marketplace tree per harness, and checked |
//! | [`harness`] | Each harness as a profile row (data) and a translator (code), and the target names |
//! | [`settings`] | `skillwright.md`, the root file, as a build config |
//! | [`paths`] | Load a vault: its root file and its marketplaces ([`load_vault`]) |
//! | [`field_edit`] | Read and set a note's `version`, keeping every other byte |
//! | [`status`] | The version-bump rule a build and `bump` share |
//! | [`guard`] | Containment resolved through links |
//! | [`vfs`] | The filesystem seam every I/O-touching function takes |
//! | [`model`] | The marketplace model: a vault loaded once, every body resolved |
//! | [`hooks_file`] | A plugin's `hooks/hooks.json`, parsed and localized per harness |
//! | [`error`] | The crate's error type and its refusal tokens |
//! | [`semver`] | The semver-bump arithmetic |
//!
//! # Specs named in comments
//!
//! `spec section N` and `decision Dn` are sections of
//! `docs/spec/structure-v2.md`.
//!
//! Comments cite a few rule sets by short name. Each is the behavior the
//! listed module implements and tests; those module docs are the reference.
//!
//! | Name | Rule set | Where |
//! |---|---|---|
//! | vault spec | A skill is a folder `<s>/` holding its note `<s>.md` and the files it ships | [`vault`], [`export`] |
//! | marketplace layout spec | How one marketplace tree per harness is laid out and checked | [`export`] |
//! | CLI spec | Exit codes, stdout versus stderr, `--dry-run`, confirmation before writes | the `skillwright` crate |
//! | Agent Skills spec | The public Agent Skills standard: `SKILL.md` frontmatter fields, name rules, and length caps on `description` and `compatibility` | [`skill`], [`discover`] |
//!
//! # No `std::fs` above [`vfs`]
//!
//! Every function here that reaches a filesystem takes a `&dyn Vfs` first.
//! That is what lets one core serve a native shell and a
//! `wasm32-unknown-unknown` build of itself, where there is no filesystem to
//! reach and a `std::fs` call compiles cleanly and then fails at runtime.

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
pub mod paths;
pub mod projection;
pub mod semver;
pub mod settings;
pub mod skill;
pub mod status;
mod text;
pub mod vault;
pub mod vfs;

pub use agent_adapt::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticScope, Severity};
pub use compile::{
    CallContext, CompileSource, CompiledNote, IndexedSkill, SkillIndex, compile_note,
};
pub use content::{SkillContent, SkillProjection};
pub use discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, MISSING_DESCRIPTION_REASON,
    SkippedNote, UNKNOWN_KEY_REASON, default_name, discover_components, refuse_undescribed,
};
pub use error::*;
pub use export::{
    ExportMode, ExportReport, ExportUnit, MarketplaceRefusal, MarketplaceReport, SkillNaming,
    StaleKind, UnitStatus, VaultReport, VaultRequest, build_vault, marketplace_manifest_path,
    owned_paths, packs_path, plugin_manifest_path, published_skill_name, rendered_skill_call,
};
pub use field_edit::{read_version, set_version};
pub use frontmatter::{ContainerIdentity, EntityKind, VaultFrontmatter};
pub use harness::KNOWN_TARGETS;
pub use keys::{HARNESS_BLOCKS, KEYS, KeySpec, Owner, Shape};
pub use layout::{
    KIND_DIRS, Layout, LayoutFile, MARKETPLACES_DIR, Place, Position, RESERVED_NAMES, SharedDir,
    check_vault_root, find_vault_root, is_vault_root, name_problem, read_layout,
};
pub use paths::{
    LoadedVault, VaultMarketplace, dist_dir, load_vault, read_root_file, resolve_path,
};
pub use semver::{BumpLevel, next_version, parse_semver};
pub use settings::{
    BuildSettings, BumpSettings, PatchScheme, ROOT_FILE, RootFile, SettingsError, SharedDefaults,
    compose_root_file, parse_root_file,
};
pub use vault::{VaultNote, read_vault_note};
pub use vfs::{DirEntry, MemVfs, NativeVfs, PreviewWrite, Vfs};
