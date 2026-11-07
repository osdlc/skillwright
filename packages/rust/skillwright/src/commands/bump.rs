//! `skillwright bump` — raise a plugin's `version` by a semver level,
//! rewriting only that frontmatter key (or, for a note that uses the nested
//! `sw:` block, only that block). Contract: the CLI spec's `bump` section.
//!
//! Two forms. `bump <plugin>` raises one plugin. `bump --changed-since
//! <ref|dir>` raises every plugin whose rendered members differ from a
//! baseline and whose version has not moved since: a directory is a built
//! marketplace as it stands, and anything else is a git ref, built in a
//! temporary worktree with the ref's own copy of the root file.
//!
//! A version that does not parse as plain `MAJOR.MINOR.PATCH` is refused
//! rather than guessed at ([`tradecraft_core::next_version`]); an absent
//! `version` starts from the core's own default, `0.1.0` — so a note that
//! has never carried the key is bumped, not refused. The root file's
//! `build.bump.patch` picks how the patch number is written ([`PatchScheme`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tradecraft_core::harness::with_plugin_format;
use tradecraft_core::{
    BumpLevel, DiscoveredComponent, EntityKind, ExportMode, ExportRequest, ExportUnit,
    MarketplaceConfig, Namespace, NativeVfs, PatchScheme, ROOT_FILE, UnitStatus, Vfs,
    apply_field_edits, check_packs, discover_components, export_skills, in_plugin_folder,
    next_version, ns_field_edits, ns_field_label, owned_paths, packs_path, parse_semver,
    plugin_folder_in, plugin_manifest_path, read_ns_field,
};

use crate::commands::marketplace::configured_request;
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, usage_error};
use crate::prompt::confirm_write;
use crate::report::normalize_thrown_reason;
use crate::resolve::{CommonFlags, ResolvedRun, refuse_workspace, resolve_run, workspace_runs};

/// The resolved flags of `skillwright bump`.
pub struct BumpFlags {
    /// The plugin to bump, by its frontmatter `name`. `None` with
    /// `changed_since`.
    pub plugin: Option<String>,
    /// `--changed-since`: bump every plugin that changed since this git ref
    /// or built directory.
    pub changed_since: Option<String>,
    /// `--marketplace`: the marketplace to work in.
    pub marketplace: Option<String>,
    /// How much of the version to bump.
    pub level: BumpLevel,
    /// The global `--vault`, `--config` and `--namespace` flags.
    pub common: CommonFlags,
    /// `--json`: print the report as one JSON document.
    pub json: bool,
    /// `--dry-run`: classify and report; write nothing.
    pub dry_run: bool,
    /// `--yes`: accept the plan without prompting.
    pub yes: bool,
    /// `--quiet`: drop progress and the tally from stderr.
    pub quiet: bool,
}

/// The `--json` row: the same facts the report line carries. Not one of
/// the CLI spec's worked transcripts — those are human-only — so the shape
/// here is this file's own judgement call, kept minimal on purpose.
#[derive(Serialize)]
struct BumpRow<'a> {
    path: &'a str,
    status: &'a str,
    current: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<&'a str>,
}

/// `--changed-since`'s `--json` document: the baseline, and one row per
/// plugin the render holds.
#[derive(Serialize)]
struct ChangedReport<'a> {
    baseline: &'a str,
    plugins: Vec<ChangedRow<'a>>,
}

/// One plugin of a `--changed-since` run. `status` is `written`,
/// `dry-run`, `unchanged`, `skipped` or `refused`.
#[derive(Serialize)]
struct ChangedRow<'a> {
    plugin: &'a str,
    path: &'a str,
    status: &'a str,
    current: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

/// One report row, printed as the text line in human mode or as one JSON
/// object in `--json` mode — [`crate::commands::marketplace::run_export`]'s
/// own `--json` branch, applied to a single-unit verb. `Err` is the exit code
/// a report that would not serialize earns; every other outcome is `Ok(())`.
fn write_row(
    ctx: &mut CommandContext<'_>,
    json: bool,
    path: &str,
    status: &str,
    current: &str,
    new: Option<&str>,
    line: &str,
) -> Result<(), i32> {
    if json {
        let row = BumpRow {
            path,
            status,
            current,
            new,
        };
        print_json(ctx, &row)
    } else {
        let _ = writeln!(ctx.out, "{line}");
        Ok(())
    }
}

fn print_json(ctx: &mut CommandContext<'_>, value: &impl Serialize) -> Result<(), i32> {
    match serde_json::to_string_pretty(value) {
        Ok(document) => {
            let _ = writeln!(ctx.out, "{document}");
            Ok(())
        }
        Err(err) => Err(blocked(
            ctx.err,
            &format!("the report would not serialize: {err}"),
        )),
    }
}

fn bump_failed(err: &mut dyn std::io::Write, e: impl std::fmt::Display) -> i32 {
    blocked(
        err,
        &format!("bump failed: {}", normalize_thrown_reason(&e.to_string())),
    )
}

/// The version a bump sets, or `None` for a current version that is not
/// plain semver or has no room at `level`. `now` is the UTC time as
/// `YYYYMMDDHHMMSS`, read only by [`PatchScheme::Timestamp`].
pub(crate) fn bumped_version(
    current: &str,
    level: BumpLevel,
    scheme: PatchScheme,
    now: u64,
) -> Option<String> {
    match scheme {
        PatchScheme::Increment => next_version(current, level),
        PatchScheme::Timestamp => {
            let (major, minor, patch) = parse_semver(current)?;
            Some(match level {
                BumpLevel::Patch => format!("{major}.{minor}.{}", now.max(patch.checked_add(1)?)),
                BumpLevel::Minor => format!("{major}.{}.{now}", minor.checked_add(1)?),
                BumpLevel::Major => format!("{}.0.{now}", major.checked_add(1)?),
            })
        }
    }
}

fn utc_now_stamp() -> u64 {
    chrono::Utc::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
        .parse()
        .unwrap_or_default()
}

/// One plugin note read and its bump worked out; nothing written yet.
struct Planned {
    rel: String,
    entry_path: PathBuf,
    source: String,
    /// The version key as the note spells it.
    key: String,
    current: String,
    /// Whether the note carries a version key at all.
    versioned: bool,
    /// `None`: the current version is not plain semver.
    new: Option<String>,
    /// How this note spells its keys.
    namespace: Namespace,
}

/// A note that holds a plugin's version.
struct VersionNote {
    path: PathBuf,
    rel: String,
    namespace: Namespace,
}

fn plan_bump(
    note: VersionNote,
    level: BumpLevel,
    scheme: PatchScheme,
    now: u64,
) -> Result<Planned, String> {
    let VersionNote {
        path,
        rel,
        namespace,
    } = note;
    let bytes = NativeVfs.read_file(&path).map_err(|e| e.to_string())?;
    let source = String::from_utf8(bytes).map_err(|e| format!("{rel} is not valid UTF-8: {e}"))?;
    // Either shape the note writes it in: flat `version`, or `version`
    // inside a nested `sw:` block. Messages name the key as the note spells it.
    let key = ns_field_label(&source, &namespace, "version").map_err(|e| e.to_string())?;
    let found = read_ns_field(&source, &namespace, "version").map_err(|e| e.to_string())?;
    let versioned = found.is_some();
    // No version key in either shape is not a refusal — the core's stated
    // default is what a bump starts from.
    let current = found.unwrap_or_else(|| "0.1.0".to_string());
    let new = bumped_version(&current, level, scheme, now);
    Ok(Planned {
        rel,
        entry_path: path,
        source,
        key,
        current,
        versioned,
        new,
        namespace,
    })
}

fn write_bump(planned: &Planned, new: &str) -> Result<(), String> {
    let edited = ns_field_edits(
        &planned.source,
        &planned.namespace,
        &[("version", Some(yaml_splice::Value::Str(new.to_owned())))],
    )
    .and_then(|edits| apply_field_edits(&planned.source, &edits))
    .map_err(|e| e.to_string())?;
    NativeVfs
        .write_file(&planned.entry_path, edited.as_bytes())
        .map_err(|e| e.to_string())
}

/// Run `skillwright bump` and return the exit code.
pub fn run_bump(flags: &BumpFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    if !run.is_workspace() {
        return bump_in(flags, ctx, &run, flags.marketplace.as_deref());
    }
    // At a workspace root `--marketplace` picks the member, a vault of its own.
    let Some(only) = flags.marketplace.as_deref() else {
        return refuse_workspace("bump", &run, ctx.err)
            .err()
            .unwrap_or(EXIT_OK);
    };
    match workspace_runs(&run, &flags.common, Some(only), ctx.err) {
        Ok(members) => members
            .first()
            .map_or(EXIT_OK, |member| bump_in(flags, ctx, member, None)),
        Err(code) => code,
    }
}

/// The marketplace folders a scan holds: each marketplace note's, and each
/// component's under `marketplaces/`, as the build counts them.
pub(crate) fn marketplaces_in(components: &[DiscoveredComponent]) -> BTreeSet<&str> {
    components
        .iter()
        .filter_map(|c| match c.kind {
            EntityKind::Marketplace => Some(c.folder()),
            _ => c.marketplace.as_deref(),
        })
        .collect()
}

/// `bump` in one vault, narrowed to the marketplace folder `market`.
fn bump_in(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    run: &ResolvedRun,
    market: Option<&str>,
) -> i32 {
    let namespace = run.core_namespace();

    let mut scan = match discover_components(&run.source_vfs(None), &run.vault, None, &namespace) {
        Ok(scan) => scan,
        Err(err) => {
            let message = err.to_string();
            return blocked(
                ctx.err,
                &format!(
                    "the vault scan could not complete: {}",
                    normalize_thrown_reason(&message)
                ),
            );
        }
    };
    if let Some(market) = market {
        let markets: Vec<&str> = marketplaces_in(&scan.components).into_iter().collect();
        // As `build` takes it: a vault with no marketplaces has no set to
        // pick from, so the name selects everything.
        if !markets.is_empty() && !markets.contains(&market) {
            return usage_error(
                ctx.err,
                &format!(
                    "no marketplace `{market}` in this vault: it holds {}",
                    markets.join(", ")
                ),
            );
        }
        scan.components.retain(|c| {
            c.marketplace.as_deref().is_none_or(|m| m == market)
                && (c.kind != EntityKind::Marketplace || c.folder() == market)
        });
    }
    let scheme = run
        .config
        .build
        .bump
        .as_ref()
        .map(|bump| bump.patch)
        .unwrap_or_default();

    match (&flags.plugin, &flags.changed_since) {
        (Some(plugin), None) => run_one(flags, ctx, run, &scan.components, plugin, scheme),
        (None, Some(since)) => {
            run_changed(flags, ctx, run, market, &scan.components, since, scheme)
        }
        _ => usage_error(
            ctx.err,
            "bump takes a plugin name or --changed-since, and not both",
        ),
    }
}

/// The plugin notes named `name`.
fn plugin_notes<'a>(
    components: &'a [DiscoveredComponent],
    name: &str,
) -> Vec<&'a DiscoveredComponent> {
    components
        .iter()
        .filter(|c| c.kind == EntityKind::Plugin && c.name == name)
        .collect()
}

/// The note holding the version of the marketplace's own plugin: `name` is the
/// marketplace's and no folder holds such a plugin. It is a `skillwright.md`,
/// so its keys are bare under any namespace.
fn marketplace_note(
    run: &ResolvedRun,
    components: &[DiscoveredComponent],
    name: &str,
) -> Option<VersionNote> {
    if components.iter().any(|c| in_plugin_folder(c, name)) {
        return None;
    }
    let own = components
        .iter()
        .find(|c| c.kind == EntityKind::Marketplace && c.name == name);
    let (path, rel) = match own {
        Some(note) => (note.entry_path.clone(), note.vault_relative_path.clone()),
        None if run.root_config().name.as_deref() == Some(name) => {
            let file = run.config_path.as_ref()?;
            let rel = crate::config::relative_path(&run.vault, file)
                .to_string_lossy()
                .replace('\\', "/");
            (file.clone(), rel)
        }
        None => return None,
    };
    Some(VersionNote {
        path,
        rel,
        namespace: Namespace::bare(),
    })
}

/// The version note of the plugin note `component`.
fn plugin_version_note(component: &DiscoveredComponent, run: &ResolvedRun) -> VersionNote {
    VersionNote {
        path: component.entry_path.clone(),
        rel: component.vault_relative_path.clone(),
        namespace: run.core_namespace(),
    }
}

fn run_one(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    run: &ResolvedRun,
    components: &[DiscoveredComponent],
    plugin: &str,
    scheme: PatchScheme,
) -> i32 {
    let matches = plugin_notes(components, plugin);
    let note = match matches.as_slice() {
        [] => match marketplace_note(run, components, plugin) {
            Some(note) => note,
            None => {
                let folder = components
                    .iter()
                    .filter(|c| c.plugin.as_deref() == Some(plugin))
                    .find_map(|c| plugin_folder_in(&c.vault_relative_path, plugin))
                    .unwrap_or_else(|| format!("plugins/{plugin}"));
                return usage_error(
                    ctx.err,
                    &format!(
                        "no plugin note named `{plugin}` in this vault. A folder-only plugin has no note to bump: add `{folder}/{plugin}.md` with a `version`"
                    ),
                );
            }
        },
        [one] => plugin_version_note(one, run),
        many => {
            let paths: Vec<&str> = many
                .iter()
                .map(|c| c.vault_relative_path.as_str())
                .collect();
            return usage_error(
                ctx.err,
                &format!(
                    "`{plugin}` names more than one plugin: {} — name one with --marketplace",
                    paths.join(", ")
                ),
            );
        }
    };

    let planned = match plan_bump(note, flags.level, scheme, utc_now_stamp()) {
        Ok(planned) => planned,
        Err(err) => return bump_failed(ctx.err, err),
    };
    let (rel, version_key, current) = (
        planned.rel.as_str(),
        planned.key.as_str(),
        planned.current.as_str(),
    );

    let Some(new) = planned.new.as_deref() else {
        let line = format!("{rel}  refused · non-semver: `{version_key}` reads `{current}`");
        if let Err(code) = write_row(ctx, flags.json, rel, "refused", current, None, &line) {
            return code;
        }
        if !flags.quiet {
            let _ = writeln!(ctx.err, "1 refused");
        }
        return EXIT_REFUSED;
    };

    if flags.dry_run {
        let line = format!("{rel}  {current} \u{2192} {new}");
        if let Err(code) = write_row(ctx, flags.json, rel, "dry-run", current, Some(new), &line) {
            return code;
        }
        let _ = writeln!(
            ctx.err,
            "dry run — reporting the bump this run would make; nothing will be written"
        );
        return EXIT_OK;
    }

    if !flags.yes {
        let plan = vec![format!("{rel}  {version_key} {current} -> {new}")];
        let gate = confirm_write("bump", &plan, ctx.err);
        if gate != EXIT_OK {
            return gate;
        }
    }

    if let Err(err) = write_bump(&planned, new) {
        return bump_failed(ctx.err, err);
    }

    let line = format!("{rel}  written · {version_key} {current} -> {new}");
    if let Err(code) = write_row(ctx, flags.json, rel, "written", current, Some(new), &line) {
        return code;
    }
    if !flags.quiet {
        let _ = writeln!(ctx.err, "1 written");
    }
    EXIT_OK
}

/// What the comparison against the baseline says about one plugin.
#[derive(Debug, Default)]
struct Verdict<'a> {
    /// Its members differ and its version has not moved, in some root.
    bump: bool,
    /// Anything of it differs, in some root.
    drift: bool,
    /// The harness roots the render holds it in.
    roots: Vec<&'a str>,
}

/// Why a drifted, versioned plugin is not bumped: the `reason` token of a
/// `skipped` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Skip {
    /// Its version already differs from the baseline's.
    VersionMoved,
    /// No configured harness root of the baseline holds its manifest.
    NotInBaseline,
    /// Its members match; only its manifest differs.
    ManifestOnly,
}

impl Skip {
    fn token(self) -> &'static str {
        match self {
            Skip::VersionMoved => "version-moved",
            Skip::NotInBaseline => "not-in-baseline",
            Skip::ManifestOnly => "manifest-only",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Skip::VersionMoved => "its version already moved since the baseline",
            Skip::NotInBaseline => "the baseline does not hold this plugin",
            Skip::ManifestOnly => "only its manifest changed",
        }
    }

    /// Read the baseline's own manifests of `plugin` in `roots` to tell the
    /// three causes apart.
    fn of(
        baseline: &Path,
        config: &MarketplaceConfig,
        roots: &[&str],
        plugin: &str,
        current: &str,
    ) -> Skip {
        let mut held = false;
        for root in roots {
            let (Some(packs), Some(manifest)) =
                (packs_path(config, root), plugin_manifest_path(root))
            else {
                continue;
            };
            let Ok(bytes) = std::fs::read(baseline.join(packs).join(plugin).join(manifest)) else {
                continue;
            };
            held = true;
            let version = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|doc| doc.get("version")?.as_str().map(str::to_owned));
            if version.as_deref() != Some(current) {
                return Skip::VersionMoved;
            }
        }
        if held {
            Skip::ManifestOnly
        } else {
            Skip::NotInBaseline
        }
    }
}

/// Whether `dir` holds a plugin manifest for any of `roots`.
fn holds_a_plugin_manifest(dir: &Path, config: &MarketplaceConfig, roots: &[&str]) -> bool {
    roots.iter().any(|root| {
        let (Some(packs), Some(manifest)) = (packs_path(config, root), plugin_manifest_path(root))
        else {
            return false;
        };
        std::fs::read_dir(dir.join(packs)).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().join(manifest).is_file())
        })
    })
}

fn run_changed(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    run: &ResolvedRun,
    market: Option<&str>,
    components: &[DiscoveredComponent],
    since: &str,
    scheme: PatchScheme,
) -> i32 {
    // The build's own `packs` check, before a ref is built and its packs are
    // moved to where this config keeps them.
    let config = configured_request(run, market, run.vault.clone(), ExportMode::Check).config;
    if let Err(err) = check_packs(&config) {
        return blocked(
            ctx.err,
            &format!(
                "--changed-since `{since}`: {}",
                normalize_thrown_reason(&err.to_string())
            ),
        );
    }
    let baseline = match Baseline::resolve(since, flags, run, market, ctx) {
        Ok(baseline) => baseline,
        Err(code) => return code,
    };
    let request = configured_request(
        run,
        market,
        baseline.root().to_path_buf(),
        ExportMode::Check,
    );
    let mut roots: Vec<&str> = match owned_paths(&request) {
        Ok(owned) => owned.into_iter().map(|(root, _)| root).collect(),
        Err(err) => return blocked(ctx.err, normalize_thrown_reason(&err.to_string())),
    };
    roots.dedup();
    if let Baseline::Dir(dir) = &baseline
        && !holds_a_plugin_manifest(dir, &request.config, &roots)
    {
        let expected: Vec<String> = roots
            .iter()
            .filter_map(|root| {
                Some(format!(
                    "{}/<plugin>/{}",
                    packs_path(&request.config, root)?,
                    plugin_manifest_path(root)?
                ))
            })
            .collect();
        return blocked(
            ctx.err,
            &format!(
                "--changed-since `{since}`: {} holds no plugin manifest for any configured harness; a built marketplace holds one at {}",
                dir.display(),
                expected.join(" or ")
            ),
        );
    }
    let namespace = run.core_namespace();
    let report = match export_skills(&run.config_vfs(), &request, &namespace) {
        Ok(report) => report,
        Err(err) => {
            return blocked(
                ctx.err,
                &format!(
                    "comparing against `{since}` failed: {}",
                    normalize_thrown_reason(&err.to_string())
                ),
            );
        }
    };

    let mut verdicts: BTreeMap<&str, Verdict> = BTreeMap::new();
    for unit in &report.units {
        if let ExportUnit::Plugin {
            root,
            name,
            status,
            version_bump_suggestion,
            ..
        } = unit
        {
            let verdict = verdicts.entry(name.as_str()).or_default();
            verdict.bump |= version_bump_suggestion.is_some();
            verdict.drift |= *status == UnitStatus::Drift;
            verdict.roots.push(root.as_str());
        }
    }

    let now = utc_now_stamp();
    let mut rows: Vec<Row> = Vec::new();
    for (name, verdict) in verdicts {
        let note = match plugin_notes(components, name).as_slice() {
            [one] => Some(plugin_version_note(one, run)),
            [] => marketplace_note(run, components, name),
            _ => {
                rows.push(Row::refused(
                    name,
                    "",
                    "",
                    format!("`{name}` names more than one plugin in this vault"),
                ));
                continue;
            }
        };
        let Some(note) = note else {
            let reason = format!("`{name}` has no note to bump; add one with a `version`");
            let mut row = Row::refused(name, "", "", reason);
            if !verdict.drift {
                row.status = "unchanged";
                row.reason = None;
            }
            rows.push(row);
            continue;
        };
        let planned = match plan_bump(note, flags.level, scheme, now) {
            Ok(planned) => planned,
            Err(err) => return bump_failed(ctx.err, err),
        };
        // A note with no version key has no version to have moved, so any
        // drift is a change.
        let changed = verdict.bump || (verdict.drift && !planned.versioned);
        let row = if !verdict.drift {
            Row::plain(name, &planned, "unchanged", None)
        } else if !changed {
            let skip = Skip::of(
                baseline.root(),
                &request.config,
                &verdict.roots,
                name,
                &planned.current,
            );
            let mut row = Row::plain(name, &planned, "skipped", Some(skip.token().to_owned()));
            row.explain = Some(skip.text());
            row
        } else if planned.new.is_none() {
            let reason = format!("non-semver: `{}` reads `{}`", planned.key, planned.current);
            Row::refused(name, &planned.rel, &planned.current, reason)
        } else {
            Row::bump(name, planned)
        };
        rows.push(row);
    }
    drop(baseline);

    let pending: Vec<&Row> = rows.iter().filter(|r| r.planned.is_some()).collect();
    if !flags.dry_run && !flags.yes && !pending.is_empty() {
        let plan: Vec<String> = pending
            .iter()
            .filter_map(|r| {
                let p = r.planned.as_ref()?;
                Some(format!(
                    "{}  {} {} -> {}",
                    p.rel,
                    p.key,
                    p.current,
                    p.new.as_deref()?
                ))
            })
            .collect();
        let gate = confirm_write("bump", &plan, ctx.err);
        if gate != EXIT_OK {
            return gate;
        }
    }

    for row in &mut rows {
        let Some(planned) = &row.planned else {
            continue;
        };
        let Some(new) = planned.new.clone() else {
            continue;
        };
        if flags.dry_run {
            row.status = "dry-run";
        } else {
            // A failed write is reported on its row; the bumps already
            // written still reach the report.
            if let Err(err) = write_bump(planned, &new) {
                row.status = "refused";
                row.reason = Some(format!(
                    "the write failed: {}",
                    normalize_thrown_reason(&err)
                ));
                continue;
            }
            row.status = "written";
        }
        row.new = Some(new);
    }

    if flags.json {
        let document = ChangedReport {
            baseline: since,
            plugins: rows.iter().map(Row::json).collect(),
        };
        if let Err(code) = print_json(ctx, &document) {
            return code;
        }
    } else {
        for row in &rows {
            let _ = writeln!(ctx.out, "{}", row.line());
        }
    }
    if flags.dry_run {
        let _ = writeln!(
            ctx.err,
            "dry run — reporting the bumps this run would make; nothing will be written"
        );
    }
    if !flags.quiet {
        let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &rows {
            *tally.entry(row.status).or_default() += 1;
        }
        let parts: Vec<String> = tally.iter().map(|(s, n)| format!("{n} {s}")).collect();
        let _ = writeln!(
            ctx.err,
            "{} checked against `{since}` — {}",
            rows.len(),
            if parts.is_empty() {
                "no plugins".to_owned()
            } else {
                parts.join(", ")
            }
        );
    }
    if rows.iter().any(|r| r.status == "refused") {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

/// One plugin's line in a `--changed-since` report.
struct Row {
    plugin: String,
    rel: String,
    status: &'static str,
    current: String,
    new: Option<String>,
    reason: Option<String>,
    /// What `reason` means, for the text line; `None` when `reason` says it.
    explain: Option<&'static str>,
    /// The bump to make; `None` for a row that bumps nothing.
    planned: Option<Planned>,
}

impl Row {
    fn plain(plugin: &str, planned: &Planned, status: &'static str, reason: Option<String>) -> Row {
        Row {
            plugin: plugin.to_owned(),
            rel: planned.rel.clone(),
            status,
            current: planned.current.clone(),
            new: None,
            reason,
            explain: None,
            planned: None,
        }
    }

    fn refused(plugin: &str, rel: &str, current: &str, reason: String) -> Row {
        Row {
            plugin: plugin.to_owned(),
            rel: rel.to_owned(),
            status: "refused",
            current: current.to_owned(),
            new: None,
            reason: Some(reason),
            explain: None,
            planned: None,
        }
    }

    fn bump(plugin: &str, planned: Planned) -> Row {
        Row {
            plugin: plugin.to_owned(),
            rel: planned.rel.clone(),
            status: "pending",
            current: planned.current.clone(),
            new: None,
            reason: None,
            explain: None,
            planned: Some(planned),
        }
    }

    fn line(&self) -> String {
        let label = if self.rel.is_empty() {
            &self.plugin
        } else {
            &self.rel
        };
        match (self.status, &self.new, &self.reason) {
            ("written", Some(new), _) => {
                let key = self.planned.as_ref().map_or("version", |p| p.key.as_str());
                format!("{label}  written · {key} {} -> {new}", self.current)
            }
            ("dry-run", Some(new), _) => format!("{label}  {} \u{2192} {new}", self.current),
            (status, _, Some(reason)) => match self.explain {
                Some(explain) => format!("{label}  {status} · {reason}: {explain}"),
                None => format!("{label}  {status} · {reason}"),
            },
            (status, _, None) => format!("{label}  {status}"),
        }
    }

    fn json(&self) -> ChangedRow<'_> {
        ChangedRow {
            plugin: &self.plugin,
            path: &self.rel,
            status: self.status,
            current: &self.current,
            new: self.new.as_deref(),
            reason: self.reason.as_deref(),
        }
    }
}

/// What `--changed-since` compares against: a built directory as it is, or
/// a git ref built into a temporary directory that is removed on drop.
enum Baseline {
    Dir(PathBuf),
    Built(BuiltRef),
}

impl Baseline {
    fn root(&self) -> &Path {
        match self {
            Baseline::Dir(dir) => dir,
            Baseline::Built(built) => &built.out,
        }
    }

    /// A directory that exists is a built baseline; anything else is a git
    /// ref, built with the ref's own copy of the root file.
    fn resolve(
        since: &str,
        flags: &BumpFlags,
        run: &ResolvedRun,
        market: Option<&str>,
        ctx: &mut CommandContext<'_>,
    ) -> Result<Baseline, i32> {
        let as_dir = crate::config::resolve_against_cwd(since, &ctx.cwd);
        if as_dir.is_dir() {
            return Ok(Baseline::Dir(as_dir));
        }
        BuiltRef::build(since, flags, run, market, ctx).map(Baseline::Built)
    }
}

/// The folder under the system temp dir that holds every baseline build:
/// `<id>.lock` beside `<id>/`, the lock held for the run's lifetime.
const BASELINES_DIR: &str = "skillwright-bump-baselines";

/// The file in [`BASELINES_DIR`] locked while a run prunes and claims.
const GUARD_FILE: &str = "guard";

/// Keeps two claims in one process and one clock tick apart.
static NEXT_CLAIM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A git ref checked out into a temporary worktree and built there.
struct BuiltRef {
    repo: PathBuf,
    temp: PathBuf,
    checkout: PathBuf,
    out: PathBuf,
    lock_path: PathBuf,
    /// Held until drop, so a concurrent run's prune leaves this one alone.
    _lock: std::fs::File,
}

impl BuiltRef {
    fn build(
        since: &str,
        flags: &BumpFlags,
        run: &ResolvedRun,
        market: Option<&str>,
        ctx: &mut CommandContext<'_>,
    ) -> Result<BuiltRef, i32> {
        let fail = |ctx: &mut CommandContext<'_>, why: String| {
            Err(blocked(
                ctx.err,
                &format!("--changed-since `{since}`: {why}"),
            ))
        };
        let Some(config_path) = run.config_path.as_ref() else {
            return fail(
                ctx,
                "is not a directory, and a git ref needs a root file to build it with".to_owned(),
            );
        };
        let config_path = config_path.canonicalize().unwrap_or(config_path.clone());
        let config_dir = config_path.parent().unwrap_or(Path::new("."));
        let repo = match typed_git::git()
            .dir(config_dir)
            .args(&["rev-parse", "--show-toplevel"])
            .run()
        {
            Ok(top) => PathBuf::from(top),
            Err(_) => {
                return fail(
                    ctx,
                    format!(
                        "is not a directory, and {} is not in a git repository",
                        config_path.display()
                    ),
                );
            }
        };
        let repo = repo.canonicalize().unwrap_or(repo);
        let Ok(sha) = typed_git::query::resolve_commit(&repo, since) else {
            return fail(
                ctx,
                format!("is neither a directory nor a commit in {}", repo.display()),
            );
        };
        let Ok(config_rel) = config_path.strip_prefix(&repo).map(Path::to_path_buf) else {
            return fail(
                ctx,
                format!("{} is outside {}", config_path.display(), repo.display()),
            );
        };
        // A `--vault` flag names a folder of this checkout; the ref's own copy
        // of it is the same path inside the worktree.
        let vault_rel = match &flags.common.vault {
            None => None,
            Some(_) => {
                let vault = run.vault.canonicalize().unwrap_or(run.vault.clone());
                match vault.strip_prefix(&repo) {
                    Ok(rel) => Some(rel.to_path_buf()),
                    Err(_) => {
                        return fail(
                            ctx,
                            format!(
                                "--vault {} is outside {}",
                                run.vault.display(),
                                repo.display()
                            ),
                        );
                    }
                }
            }
        };

        let baselines = std::env::temp_dir().join(BASELINES_DIR);
        let (id, lock_path, lock) = match claim_baseline(&baselines, &repo) {
            Ok(claimed) => claimed,
            Err(why) => return fail(ctx, why),
        };
        let temp = baselines.join(&id);
        if let Err(err) = std::fs::create_dir_all(&temp) {
            let _ = std::fs::remove_file(&lock_path);
            return fail(ctx, format!("cannot create {}: {err}", temp.display()));
        }
        // Canonical: on macOS the temp dir sits behind the `/var` link.
        let temp = temp.canonicalize().unwrap_or(temp);
        let built = BuiltRef {
            repo: repo.clone(),
            checkout: temp.join("checkout"),
            out: temp.join("out"),
            temp,
            lock_path,
            _lock: lock,
        };
        // Hooks off and LFS left as pointers: the checkout runs nothing of the
        // repository's and fetches nothing.
        if let Err(err) = typed_git::git()
            .dir(&repo)
            .env("GIT_LFS_SKIP_SMUDGE", "1")
            .args(&[
                "-c",
                "core.hooksPath=/dev/null",
                "worktree",
                "add",
                "--detach",
                "--quiet",
            ])
            .arg(built.checkout.to_string_lossy())
            .arg(&sha)
            .run()
        {
            return fail(ctx, format!("checking it out failed: {err}"));
        }

        let Some(base_config) = ref_root_file(&built.checkout, &config_rel) else {
            return fail(
                ctx,
                format!(
                    "{since} holds no {} here, so it cannot be built. A ref from before \
                     {ROOT_FILE} cannot be a baseline: compare against a built folder instead",
                    config_rel.display()
                ),
            );
        };
        let common = CommonFlags {
            vault: vault_rel.map(|rel| built.checkout.join(rel).display().to_string()),
            config: Some(base_config.display().to_string()),
            namespace: flags.common.namespace.clone(),
        };
        let base_cwd = base_config
            .parent()
            .unwrap_or(&built.checkout)
            .to_path_buf();
        let base_run = resolve_run(&common, &base_cwd, ctx.err)?;
        let request = configured_request(&base_run, market, built.out.clone(), ExportMode::Build);
        if let Err(err) =
            export_skills(&base_run.config_vfs(), &request, &base_run.core_namespace())
        {
            return fail(
                ctx,
                format!(
                    "building it failed: {}",
                    normalize_thrown_reason(&err.to_string())
                ),
            );
        }
        let current = configured_request(run, market, built.out.clone(), ExportMode::Check);
        if let Err(err) = align_packs(&built.out, &request, &current) {
            return fail(ctx, format!("moving its packs failed: {err}"));
        }
        Ok(built)
    }
}

/// Moves each harness's packs in a baseline built by `base` to where `current`
/// keeps them, so a `packs` setting that changed since the ref still compares
/// each plugin with itself.
fn align_packs(out: &Path, base: &ExportRequest, current: &ExportRequest) -> std::io::Result<()> {
    let side = out.join(".moving");
    let moved: Vec<(&str, &str, PathBuf)> = with_plugin_format()
        .filter_map(|spec| {
            let from = packs_path(&base.config, spec.name)?;
            let to = packs_path(&current.config, spec.name)?;
            (from != to && out.join(from).is_dir()).then(|| (from, to, side.join(spec.name)))
        })
        .collect();
    // Through a side folder: one harness's new folder can be another's old one.
    for (from, _, aside) in &moved {
        std::fs::create_dir_all(&side)?;
        std::fs::rename(out.join(from), aside)?;
    }
    for (_, to, aside) in &moved {
        let target = out.join(to);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(aside, target)?;
    }
    if !moved.is_empty() {
        std::fs::remove_dir(&side)?;
    }
    Ok(())
}

/// Claims a baseline under `baselines`: an `<id>.lock`, created and locked.
/// [`GUARD_FILE`] is held across the prune and the claim, so one run's prune
/// never takes a lock another run has created and not yet locked.
///
/// # Errors
///
/// What failed, as the line to print.
fn claim_baseline(
    baselines: &Path,
    repo: &Path,
) -> Result<(String, PathBuf, std::fs::File), String> {
    std::fs::create_dir_all(baselines)
        .map_err(|err| format!("cannot create {}: {err}", baselines.display()))?;
    let guard_path = baselines.join(GUARD_FILE);
    let guard = std::fs::File::create(&guard_path)
        .and_then(|file| file.lock().map(|()| file))
        .map_err(|err| format!("cannot lock {}: {err}", guard_path.display()))?;
    prune_stale_baselines(baselines, repo);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let claim = NEXT_CLAIM.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{}-{stamp}-{claim}", std::process::id());
    let lock_path = baselines.join(format!("{id}.lock"));
    let lock = std::fs::File::create(&lock_path)
        .and_then(|file| {
            file.try_lock().map_err(std::io::Error::from)?;
            Ok(file)
        })
        .map_err(|err| format!("cannot lock {}: {err}", lock_path.display()))?;
    drop(guard);
    Ok((id, lock_path, lock))
}

/// The ref's root file: the one at this run's path, else [`ROOT_FILE`] in
/// that folder.
fn ref_root_file(checkout: &Path, config_rel: &Path) -> Option<PathBuf> {
    let dir = config_rel.parent().unwrap_or(Path::new(""));
    [config_rel.to_path_buf(), dir.join(ROOT_FILE)]
        .into_iter()
        .map(|rel| checkout.join(rel))
        .find(|path| path.is_file())
}

/// Remove the baselines a killed run left behind: each `<id>.lock` no live
/// run holds, its `<id>/` folder and that folder's worktree registration.
fn prune_stale_baselines(baselines: &Path, repo: &Path) {
    let Ok(entries) = std::fs::read_dir(baselines) else {
        return;
    };
    let mut pruned = false;
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.extension().is_none_or(|ext| ext != "lock") {
            continue;
        }
        let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) else {
            continue;
        };
        if file.try_lock().is_err() {
            continue;
        }
        let dir = path.with_extension("");
        let _ = typed_git::worktree::remove(repo, &dir.join("checkout"));
        let _ = std::fs::remove_dir_all(&dir);
        drop(file);
        let _ = std::fs::remove_file(&path);
        pruned = true;
    }
    if pruned {
        prune_worktrees(repo);
    }
}

/// `git worktree prune`: drop the registration of every worktree whose
/// folder is gone. Best-effort.
fn prune_worktrees(repo: &Path) {
    let _ = typed_git::git()
        .dir(repo)
        .args(&["worktree", "prune"])
        .run();
}

impl Drop for BuiltRef {
    fn drop(&mut self) {
        let removed = typed_git::worktree::remove(&self.repo, &self.checkout);
        let _ = std::fs::remove_dir_all(&self.temp);
        if let Err(err) = removed {
            eprintln!(
                "skillwright: removing the baseline worktree {} failed ({err}); pruning it instead",
                self.checkout.display()
            );
            prune_worktrees(&self.repo);
        }
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One run's prune never takes the lock of a claim in progress.
    #[test]
    fn concurrent_claims_never_fail_on_each_others_lock() {
        let dir = tempfile::tempdir().unwrap();
        let baselines = dir.path().join("baselines");
        let failures: usize = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        (0..200)
                            .filter(|_| match claim_baseline(&baselines, dir.path()) {
                                Ok((_, path, lock)) => {
                                    let _ = std::fs::remove_file(path);
                                    drop(lock);
                                    false
                                }
                                Err(_) => true,
                            })
                            .count()
                    })
                })
                .collect();
            runs.into_iter().map(|run| run.join().unwrap()).sum()
        });
        assert_eq!(failures, 0);
    }

    const NOW: u64 = 20_261_105_143_000;

    #[test]
    fn a_prune_removes_only_the_baselines_no_run_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let baselines = tmp.path().join("baselines");
        for id in ["stale", "live"] {
            std::fs::create_dir_all(baselines.join(id).join("out")).unwrap();
        }
        std::fs::write(baselines.join("stale.lock"), "").unwrap();
        let live = std::fs::File::create(baselines.join("live.lock")).unwrap();
        live.try_lock().unwrap();

        prune_stale_baselines(&baselines, tmp.path());

        assert!(!baselines.join("stale").exists());
        assert!(!baselines.join("stale.lock").exists());
        assert!(baselines.join("live").exists());
        assert!(baselines.join("live.lock").exists());
    }

    #[test]
    fn the_increment_scheme_is_plain_semver() {
        for (level, want) in [
            (BumpLevel::Patch, "1.2.4"),
            (BumpLevel::Minor, "1.3.0"),
            (BumpLevel::Major, "2.0.0"),
        ] {
            assert_eq!(
                bumped_version("1.2.3", level, PatchScheme::Increment, NOW).as_deref(),
                Some(want)
            );
        }
    }

    #[test]
    fn the_timestamp_scheme_writes_the_time_as_the_patch() {
        for (level, want) in [
            (BumpLevel::Patch, "1.2.20261105143000"),
            (BumpLevel::Minor, "1.3.20261105143000"),
            (BumpLevel::Major, "2.0.20261105143000"),
        ] {
            assert_eq!(
                bumped_version("1.2.3", level, PatchScheme::Timestamp, NOW).as_deref(),
                Some(want)
            );
        }
        // Twice in one second still moves the version.
        assert_eq!(
            bumped_version(
                "1.2.20261105143000",
                BumpLevel::Patch,
                PatchScheme::Timestamp,
                NOW
            )
            .as_deref(),
            Some("1.2.20261105143001")
        );
        assert_eq!(
            bumped_version("v1", BumpLevel::Patch, PatchScheme::Timestamp, NOW),
            None
        );
    }
}
