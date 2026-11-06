//! `skillwright bump` — raise a plugin's `sw-version` by a semver level,
//! rewriting only that frontmatter key (or, for a note that uses the nested
//! `sw:` block, only that block). Contract: the CLI spec's `bump` section.
//!
//! Two forms. `bump <plugin>` raises one plugin. `bump --changed-since
//! <ref|dir>` raises every plugin whose rendered members differ from a
//! baseline and whose version has not moved since: a directory is a built
//! marketplace as it stands, and anything else is a git ref, built into a
//! temporary worktree with this run's project file.
//!
//! A version that does not parse as plain `MAJOR.MINOR.PATCH` is refused
//! rather than guessed at ([`tradecraft_core::next_version`]); an absent
//! `sw-version` starts from the core's own default, `0.1.0` — so a note that
//! has never carried the key is bumped, not refused. The project file's
//! `bump.patch` picks how the patch number is written ([`PatchScheme`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tradecraft_core::{
    BumpLevel, DiscoveredComponent, EntityKind, ExportMode, ExportUnit, Namespace, NativeVfs,
    UnitStatus, Vfs, apply_field_edits, discover_components, export_skills, next_version,
    ns_field_edits, ns_field_label, parse_semver, read_ns_field,
};

use crate::commands::marketplace::configured_request;
use crate::config::PatchScheme;
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, usage_error};
use crate::prompt::confirm_write;
use crate::report::normalize_thrown_reason;
use crate::resolve::{CommonFlags, ResolvedRun, resolve_run, resolve_skills_root};

/// The resolved flags of `skillwright bump`.
pub struct BumpFlags {
    /// The plugin to bump, by its frontmatter `name`. `None` with
    /// `changed_since`.
    pub plugin: Option<String>,
    /// `--changed-since`: bump every plugin that changed since this git ref
    /// or built directory.
    pub changed_since: Option<String>,
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
}

fn plan_bump(
    component: &DiscoveredComponent,
    namespace: &Namespace,
    level: BumpLevel,
    scheme: PatchScheme,
    now: u64,
) -> Result<Planned, String> {
    let bytes = NativeVfs
        .read_file(&component.entry_path)
        .map_err(|e| e.to_string())?;
    let source = String::from_utf8(bytes)
        .map_err(|e| format!("{} is not valid UTF-8: {e}", component.vault_relative_path))?;
    // Either shape the note writes it in: flat `sw-version`, or `version`
    // inside a nested `sw:` block. Messages name the key as the note spells it.
    let key = ns_field_label(&source, namespace, "version").map_err(|e| e.to_string())?;
    let found = read_ns_field(&source, namespace, "version").map_err(|e| e.to_string())?;
    let versioned = found.is_some();
    // No version key in either shape is not a refusal — the core's stated
    // default is what a bump starts from.
    let current = found.unwrap_or_else(|| "0.1.0".to_string());
    let new = bumped_version(&current, level, scheme, now);
    Ok(Planned {
        rel: component.vault_relative_path.clone(),
        entry_path: component.entry_path.clone(),
        source,
        key,
        current,
        versioned,
        new,
    })
}

fn write_bump(planned: &Planned, new: &str, namespace: &Namespace) -> Result<(), String> {
    let edited = ns_field_edits(
        &planned.source,
        namespace,
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
    let namespace = run.core_namespace();
    let skills_root = resolve_skills_root(None, &run, &ctx.cwd);

    let scan = match discover_components(&NativeVfs, &run.vault, skills_root.as_deref(), &namespace)
    {
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
    let scheme = run
        .config
        .as_ref()
        .and_then(|c| c.bump.as_ref())
        .map(|b| b.patch)
        .unwrap_or_default();
    let plugins: Vec<&DiscoveredComponent> = scan
        .components
        .iter()
        .filter(|c| c.kind == EntityKind::Plugin)
        .collect();

    match (&flags.plugin, &flags.changed_since) {
        (Some(plugin), None) => run_one(flags, ctx, &plugins, plugin, &namespace, scheme),
        (None, Some(since)) => run_changed(flags, ctx, &run, &plugins, since, scheme),
        _ => usage_error(
            ctx.err,
            "bump takes a plugin name or --changed-since, and not both",
        ),
    }
}

fn run_one(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    plugins: &[&DiscoveredComponent],
    plugin: &str,
    namespace: &Namespace,
    scheme: PatchScheme,
) -> i32 {
    let matches: Vec<&DiscoveredComponent> = plugins
        .iter()
        .copied()
        .filter(|c| c.name == plugin)
        .collect();
    let component = match matches.as_slice() {
        [] => {
            return usage_error(
                ctx.err,
                &format!("no plugin named `{plugin}` in this vault"),
            );
        }
        [one] => *one,
        many => {
            // This verb has no `--marketplace` flag to disambiguate with (a
            // known gap), so a name two marketplaces share is a usage error.
            let paths: Vec<&str> = many
                .iter()
                .map(|c| c.vault_relative_path.as_str())
                .collect();
            return usage_error(
                ctx.err,
                &format!(
                    "`{plugin}` names more than one plugin: {} — disambiguate by removing all but one from the vault",
                    paths.join(", ")
                ),
            );
        }
    };

    let planned = match plan_bump(component, namespace, flags.level, scheme, utc_now_stamp()) {
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

    if let Err(err) = write_bump(&planned, new, namespace) {
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
struct Verdict {
    /// Its members differ and its version has not moved, in some root.
    bump: bool,
    /// Anything of it differs, in some root.
    drift: bool,
}

/// Why a drifted plugin is not bumped.
const NOT_BUMPED: &str = "its version already moved since the baseline, the baseline does not hold it, or only its manifest changed";

fn run_changed(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    run: &ResolvedRun,
    plugins: &[&DiscoveredComponent],
    since: &str,
    scheme: PatchScheme,
) -> i32 {
    let baseline = match Baseline::resolve(since, flags, run, ctx) {
        Ok(baseline) => baseline,
        Err(code) => return code,
    };
    let request = match configured_request(
        run,
        &ctx.cwd,
        "bump --changed-since",
        baseline.root().to_path_buf(),
        ExportMode::Check,
    ) {
        Ok(request) => request,
        Err(err) => return blocked(ctx.err, &err),
    };
    let namespace = run.core_namespace();
    let report = match export_skills(&NativeVfs, &request, &namespace) {
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
    drop(baseline);

    let mut verdicts: BTreeMap<&str, Verdict> = BTreeMap::new();
    for unit in &report.units {
        if let ExportUnit::Plugin {
            name,
            status,
            version_bump_suggestion,
            ..
        } = unit
        {
            let verdict = verdicts.entry(name.as_str()).or_default();
            verdict.bump |= version_bump_suggestion.is_some();
            verdict.drift |= *status == UnitStatus::Drift;
        }
    }

    let now = utc_now_stamp();
    let mut rows: Vec<Row> = Vec::new();
    for (name, verdict) in verdicts {
        let matches: Vec<&DiscoveredComponent> =
            plugins.iter().copied().filter(|c| c.name == name).collect();
        let [component] = matches.as_slice() else {
            rows.push(Row::refused(
                name,
                "",
                "",
                format!("`{name}` names more than one plugin in this vault"),
            ));
            continue;
        };
        let planned = match plan_bump(component, &namespace, flags.level, scheme, now) {
            Ok(planned) => planned,
            Err(err) => return bump_failed(ctx.err, err),
        };
        // A note with no version key has no version to have moved, so any
        // drift is a change.
        let changed = verdict.bump || (verdict.drift && !planned.versioned);
        let row = if !verdict.drift {
            Row::plain(name, &planned, "unchanged", None)
        } else if !changed {
            Row::plain(name, &planned, "skipped", Some(NOT_BUMPED.to_owned()))
        } else if planned.new.is_none() {
            let reason = format!("non-semver: `{}` reads `{}`", planned.key, planned.current);
            Row::refused(name, &planned.rel, &planned.current, reason)
        } else {
            Row::bump(name, planned)
        };
        rows.push(row);
    }

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
            if let Err(err) = write_bump(planned, &new, &namespace) {
                return bump_failed(ctx.err, err);
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
            (status, _, Some(reason)) => format!("{label}  {status} · {reason}"),
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
    /// ref, built with this run's project file as the ref holds it.
    fn resolve(
        since: &str,
        flags: &BumpFlags,
        run: &ResolvedRun,
        ctx: &mut CommandContext<'_>,
    ) -> Result<Baseline, i32> {
        let as_dir = crate::config::resolve_against_cwd(since, &ctx.cwd);
        if as_dir.is_dir() {
            return Ok(Baseline::Dir(as_dir));
        }
        BuiltRef::build(since, flags, run, ctx).map(Baseline::Built)
    }
}

/// A git ref checked out into a temporary worktree and built there.
struct BuiltRef {
    repo: PathBuf,
    temp: PathBuf,
    checkout: PathBuf,
    out: PathBuf,
}

impl BuiltRef {
    fn build(
        since: &str,
        flags: &BumpFlags,
        run: &ResolvedRun,
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
                "is not a directory, and a git ref needs a project file to build it with"
                    .to_owned(),
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

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let temp =
            std::env::temp_dir().join(format!("skillwright-bump-{}-{stamp}", std::process::id()));
        if let Err(err) = std::fs::create_dir_all(&temp) {
            return fail(ctx, format!("cannot create {}: {err}", temp.display()));
        }
        let built = BuiltRef {
            repo: repo.clone(),
            checkout: temp.join("checkout"),
            out: temp.join("out"),
            temp,
        };
        if let Err(err) = typed_git::git()
            .dir(&repo)
            .args(&["worktree", "add", "--detach", "--quiet"])
            .arg(built.checkout.to_string_lossy())
            .arg(&sha)
            .run()
        {
            return fail(ctx, format!("checking it out failed: {err}"));
        }

        let base_config = built.checkout.join(&config_rel);
        if !base_config.is_file() {
            return fail(
                ctx,
                format!("{} does not hold {}", since, config_rel.display()),
            );
        }
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
        let request = match configured_request(
            &base_run,
            &base_cwd,
            "bump --changed-since",
            built.out.clone(),
            ExportMode::Build,
        ) {
            Ok(request) => request,
            Err(err) => return fail(ctx, err),
        };
        if let Err(err) = export_skills(&NativeVfs, &request, &base_run.core_namespace()) {
            return fail(
                ctx,
                format!(
                    "building it failed: {}",
                    normalize_thrown_reason(&err.to_string())
                ),
            );
        }
        Ok(built)
    }
}

impl Drop for BuiltRef {
    fn drop(&mut self) {
        let _ = typed_git::worktree::remove(&self.repo, &self.checkout);
        let _ = std::fs::remove_dir_all(&self.temp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 20_261_105_143_000;

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
