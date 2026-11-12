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
//! | [`keys`] | The closed key table: every frontmatter key, per kind |
//! | [`frontmatter`] | skillwright's keys read out of a note, typed |
//! | [`note`] | One note read as the vault stores it, and the frontmatter split every note path shares |
//! | [`content`] | The skill shape projection renders, and its peer resources |
//! | [`compile`] | Embeds expanded, links rewritten, references collected |
//! | [`export`] | The vault built into one marketplace tree per harness, and checked |
//! | [`harness`] | The [`Harness`] names, each harness's row of data, and the translator |
//! | [`settings`] | `skillwright.md`, the root file, as a build config |
//! | [`field_edit`] | Read and set a note's `version`, keeping every other byte |
//! | [`guard`] | Containment resolved through links |
//! | [`vfs`] | The filesystem seam every I/O-touching function takes |
//! | [`model`] | The marketplace model: a vault loaded once ([`load_vault`]), every body resolved |
//! | [`hooks_file`] | A plugin's `hooks/hooks.json`, parsed and localized per harness |
//! | [`error`] | The crate's error type |
//! | [`refusal`] | Every refusal token |
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
//! | vault spec | A skill is a folder `<s>/` holding its note `<s>.md` and the files it ships | [`note`], [`export`] |
//! | marketplace layout spec | How one marketplace tree per harness is laid out and checked | [`export`] |
//! | CLI spec | Exit codes, stdout versus stderr, `--dry-run`, confirmation before writes | the `skillwright` crate |
//! | Agent Skills spec | The public Agent Skills standard: `SKILL.md` frontmatter fields, name rules, and length caps on `description` and `compatibility` | [`note`], [`discover`] |
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
pub mod note;
pub(crate) mod projection;
pub mod refusal;
pub mod settings;
pub mod vfs;

pub use agent_adapt::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticScope, Severity};
pub use compile::{CallContext, CompileSource, CompiledNote, SkillIndex, compile_note};
pub use content::{SkillContent, SkillProjection};
pub use discover::{
    DiscoveredComponent, DiscoveryReport, DiscoveryWarning, SkippedNote, UNKNOWN_KEY_REASON,
    default_name, discover_components, refuse_undescribed,
};
pub use error::*;
pub use export::{
    BuildOptions, ExportMode, ExportReport, ExportUnit, MarketplaceRefusal, MarketplaceReport,
    StaleKind, UnitStatus, VaultPlan, VaultReport, VaultRequest, build_vault, owned_paths, plan,
};
pub use field_edit::{read_version, set_version};
pub use frontmatter::{ContainerIdentity, EntityKind, VaultFrontmatter};
pub use harness::{Harness, KNOWN_TARGETS, SkillNaming, published_skill_name, rendered_skill_call};
pub use keys::{KEYS, KeySpec, Owner, Shape};
pub use layout::{
    KIND_DIRS, Layout, LayoutFile, MARKETPLACES_DIR, Place, Position, RESERVED_NAMES, SharedDir,
    check_vault_root, find_vault_root, is_vault_root, name_problem, read_layout,
};
pub use model::vault::{
    BuildScope, LoadedVault, VaultMarketplace, dist_dir, load_vault, read_root_file,
};
pub use note::{VaultNote, read_vault_note};
pub use settings::{
    BuildSettings, BumpSettings, PatchScheme, ROOT_FILE, RootFile, SettingsError, SharedDefaults,
    compose_root_file, parse_root_file,
};
pub use vfs::{DirEntry, MemVfs, NativeVfs, PreviewWrite, Vfs};
