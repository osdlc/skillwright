//! The marketplace renderer — every note a pack ships, built into one
//! marketplace directory serving every harness, and the drift check over it.
//!
//! The engine behind the `build` and `check` verbs.
//! Skills group by the plugin folder they sit in, each group
//! becomes a pack directory with a manifest, and the groups assemble into a
//! marketplace.
//!
//! # One directory, one tree per harness
//!
//! `build` writes each marketplace into `<root>/dist/<m>/`
//! ([`crate::model::vault::dist_dir`]); nothing moves it. Each harness gets its own
//! top-level tree inside it: `.claude-plugin/`,
//! `.agents/`, `.cursor-plugin/` and `.pi/`. Each tree carries that harness's
//! marketplace manifest (Pi has none), its pack manifests in its own dialect,
//! and every skill, at `<packs>/<plugin>/skills/<name>/`.
//!
//! The trees are disjoint by construction: the harnesses translate a skill's
//! tool names and frontmatter differently, so the same skill is one file per
//! harness.
//!
//! A plugin's commands render alongside its skills, at
//! `<packs>/<plugin>/commands/<name>.<ext>`, on each harness whose plugin
//! format has a place for them
//! ([`crate::harness::PluginFormat::carries`]). A command narrowed
//! out this way has no file and no manifest mention there, and the report
//! names it as [`ExportUnit::NotCarried`] for that harness.
//!
//! Which harnesses a build renders is [`BuildOptions::harnesses`] when it
//! names any, else the root file's `build.harnesses`, else all of them. A
//! name no harness answers to is refused rather than ignored — publishing
//! other than was asked for, quietly, is the failure this crate is built
//! around.
//!
//! # One model, one tree per harness
//!
//! The vault is loaded once into the marketplace model ([`crate::model`]) —
//! every note read, every body resolved — and the translator writes that
//! model as each harness's tree.
//! What a note means is decided once, harness blocks included.
//!
//! # One render, two verbs
//!
//! `check` is not a second reading of `build`'s rules. Both modes translate
//! the whole marketplace once, into a `FileTree` per
//! harness, and diff those bytes against the destination. `check` returns
//! that diff, having written nothing under any outcome. `build` acts on it:
//! an up-to-date destination is reported unchanged and nothing is written or
//! renamed; a drifted harness is staged from the very bytes the diff compared
//! and swapped in. The two verdicts cannot
//! disagree, because there is only one render and one comparison.
//!
//! # The commit is stage-then-swap, one path at a time, with backup
//!
//! A changed harness is written under `<out>/.skillwright-stage/new/`, each
//! path it owns is renamed aside to `…/backup/<path>`, and the staged path is
//! renamed into place — the temp-and-rename [`crate::vfs::Vfs::rename`] and
//! [`crate::vfs::Vfs::remove`] exist for. The stage lives under the output
//! folder itself, because a rename is only atomic within one filesystem and the one
//! directory guaranteed to share a filesystem with the thing being replaced
//! is its own parent. A failure mid-swap rolls back every path already
//! swapped, restores the failing path's own backup, and surfaces as
//! [`CoreError::SwapFailed`] saying so. [`VaultPlan::commit`] stages every
//! marketplace before it swaps any, and a failure rolls back every
//! marketplace already swapped.
//!
//! # A build replaces only what it owns
//!
//! The swap unit is what [`owned_paths`] names: a harness's
//! packs folder and its marketplace manifest. Those are generated output,
//! replaced on every build. Anything inside them the render did not produce —
//! a stale pack, a stray file, an empty directory — goes with the swap, and
//! `check` reports exactly that set as stale, which is what keeps its verdict
//! honest about what `build` would change.
//!
//! The folders around them are not swapped: `.agents/` and `.pi/` also hold
//! skills and config people author. Anything outside the owned paths is never
//! looked at.
//!
//! # Manifests are byte-deterministic
//!
//! Stable key order (serde struct order), plugins and skills sorted by name,
//! two-space indentation, one trailing newline — so `check` can compare
//! bytes, and two builds of one vault are identical. The root file's shared
//! defaults are read from the vault's `skillwright.md` through the [`Vfs`],
//! like every note; a request cannot carry them.
//!
//! # Plan once, then commit
//!
//! [`plan`] renders and compares every marketplace of a loaded vault, and
//! writes nothing. [`VaultPlan::report`] says what it found, and
//! [`VaultPlan::commit`] swaps in what drifted, or writes nothing when any
//! marketplace was refused. [`build_vault`] does all three in one call.

mod dest;
mod report;
mod swap;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::discover::DiscoveryReport;
use crate::error::CoreError;
use crate::harness::{Harness, Translator};
use crate::model::load::{built_harnesses, load_for};
use crate::model::vault::{BuildScope, LoadedVault, VaultMarketplace, load_vault};
use crate::model::{Component, Marketplace};
use crate::refusal;
use crate::settings::RootFile;
use crate::vfs::{PreviewWrite, Vfs, normalize};
use dest::{Snapshot, admitted_plugins, diff_root};
pub use report::{ExportReport, ExportUnit, StaleKind, UnitStatus, VersionBumpSuggestion};
use swap::{Swap, commit};

/// Whether an export writes the marketplace, or only reports its drift.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportMode {
    /// Render, and swap any drifted root into place.
    #[default]
    Build,
    /// Render and compare, and write nothing under any outcome.
    Check,
}

/// The harnesses a build renders under the vault's root file `root`, each
/// with every path it owns in the marketplace directory. `harnesses` is
/// [`BuildOptions::harnesses`].
///
/// So a caller can say what a build is about to replace, before it builds,
/// without owning a second copy of the roots table or of the harness
/// selection.
///
/// # Errors
///
/// `refused: unknown-harness`.
pub fn owned_paths(
    root: &RootFile,
    harnesses: &BTreeSet<String>,
) -> Result<Vec<(Harness, &'static str)>, CoreError> {
    Ok(built_harnesses(root, harnesses)?
        .into_iter()
        .flat_map(|root| root.owned_paths().into_iter().map(move |path| (root, path)))
        .collect())
}

/// One marketplace rendered and compared with its output folder, with
/// nothing written yet.
struct ExportPlan {
    report: ExportReport,
    swap: Swap,
}

/// A build or check of every marketplace in a vault, or of the one named.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct VaultRequest {
    /// Absolute path to the vault root.
    pub vault_root: PathBuf,
    /// Which marketplace to build, by folder. Absent builds every one.
    #[serde(default, rename = "marketplace")]
    pub scope: BuildScope,
    /// How to build it.
    #[serde(flatten)]
    pub options: BuildOptions,
}

impl VaultRequest {
    /// A build of every marketplace of the vault at `vault_root`. Set the
    /// other fields after.
    pub fn new(vault_root: impl Into<PathBuf>) -> Self {
        Self {
            vault_root: vault_root.into(),
            scope: BuildScope::Vault,
            options: BuildOptions::default(),
        }
    }

    /// A build of the marketplace `name` of the vault at `vault_root`.
    pub fn for_marketplace(vault_root: impl Into<PathBuf>, name: &str) -> Self {
        let mut request = Self::new(vault_root);
        request.scope = BuildScope::Marketplace(name.to_owned());
        request
    }
}

/// How a build or check runs, whatever part of the vault it covers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BuildOptions {
    /// Which harnesses to build, by name. It replaces the root file's
    /// `build.harnesses` for this run; empty means that list, and with no
    /// list, every harness.
    ///
    /// A name no harness answers to is refused rather than ignored: silently
    /// publishing other than was asked for is the failure this crate is
    /// built around.
    #[serde(default)]
    pub harnesses: BTreeSet<String>,
    /// Build, or check. Absent on the wire means [`ExportMode::Build`].
    #[serde(default)]
    pub mode: ExportMode,
    /// Check only: the built marketplace to compare the render with, in place
    /// of `<root>/dist/<m>/`. `bump --changed-since` compares with a baseline
    /// this way. It names one marketplace's output, so the request must
    /// cover one marketplace. A build refuses it: a build always writes
    /// `dist/<m>/`.
    #[serde(default)]
    pub compare_with: Option<PathBuf>,
}

/// What [`build_vault`] did, or would do, per marketplace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VaultReport {
    /// One report per marketplace, in folder order.
    pub marketplaces: Vec<MarketplaceReport>,
    /// The refused marketplaces, in folder order. A build that meets one
    /// writes nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refused: Vec<MarketplaceRefusal>,
}

/// One marketplace's report in a [`VaultReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketplaceReport {
    /// The marketplace's folder name.
    pub marketplace: String,
    /// Its export report.
    pub report: ExportReport,
}

/// A marketplace that could not be planned, in a [`VaultReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketplaceRefusal {
    /// The marketplace's folder name.
    pub marketplace: String,
    /// The refusal's token, or `None` for an error that is not a refusal.
    pub token: Option<String>,
    /// What refused it, after the token: [`CoreError::detail`].
    pub detail: String,
}

impl MarketplaceRefusal {
    /// What refused it, as [`CoreError::reason`] gives it.
    pub fn reason(&self) -> String {
        crate::error::reason(self.token.as_deref(), &self.detail)
    }
}

/// [`VaultPlan::outcomes`], the reports and the refusals listed apart.
impl From<Vec<Result<MarketplaceReport, MarketplaceRefusal>>> for VaultReport {
    fn from(outcomes: Vec<Result<MarketplaceReport, MarketplaceRefusal>>) -> Self {
        let (marketplaces, refused): (Vec<_>, Vec<_>) =
            outcomes.into_iter().partition(Result::is_ok);
        Self {
            marketplaces: marketplaces.into_iter().flatten().collect(),
            refused: refused.into_iter().filter_map(Result::err).collect(),
        }
    }
}

/// Every marketplace of a run planned, with nothing written: what [`plan`]
/// returns. Read it with [`Self::report`], then write it with
/// [`Self::commit`].
pub struct VaultPlan {
    mode: ExportMode,
    /// Each marketplace by name with its plan, or what refused it, in folder
    /// order.
    marketplaces: Vec<(String, Result<ExportPlan, CoreError>)>,
}

impl VaultPlan {
    /// Each marketplace's report, or what refused it, in folder order.
    pub fn outcomes(&self) -> Vec<Result<MarketplaceReport, MarketplaceRefusal>> {
        self.marketplaces
            .iter()
            .map(|(marketplace, plan)| match plan {
                Ok(plan) => Ok(MarketplaceReport {
                    marketplace: marketplace.clone(),
                    report: plan.report.clone(),
                }),
                Err(err) => Err(MarketplaceRefusal {
                    marketplace: marketplace.clone(),
                    token: err.token().map(|token| token.to_string()),
                    detail: err.detail(),
                }),
            })
            .collect()
    }

    /// [`Self::outcomes`] as a [`VaultReport`].
    pub fn report(&self) -> VaultReport {
        self.outcomes().into()
    }

    /// Writes a build: stages every marketplace, then swaps them all in. A
    /// failed swap rolls back every marketplace already swapped. A check
    /// writes nothing.
    ///
    /// # Errors
    ///
    /// A build only: the first marketplace's refusal, with nothing written.
    /// `refused: output-changed` when an owned path in a `dist/<m>/` changed
    /// since [`plan`] read it, with nothing written. Then
    /// [`CoreError::SwapFailed`] and the I/O errors of the swap.
    pub fn commit(self, vfs: &dyn Vfs) -> Result<(), CoreError> {
        if self.mode == ExportMode::Check {
            return Ok(());
        }
        let mut swaps = Vec::with_capacity(self.marketplaces.len());
        for (_, plan) in self.marketplaces {
            swaps.push(plan?.swap);
        }
        commit(vfs, &swaps)
    }
}

/// Plans every marketplace `loaded` covers for `options`, and writes
/// nothing. The harnesses are resolved and the vault scanned once for all of
/// them. A marketplace that refuses does not stop the others' plans.
///
/// # Errors
///
/// `refused: compare-with` for a build that names `compare_with`, or a
/// vault of several marketplaces that does. Each marketplace's own refusal
/// is in its entry; a refused harness name or scan is in every entry.
pub fn plan(
    vfs: &dyn Vfs,
    loaded: &LoadedVault,
    options: &BuildOptions,
) -> Result<VaultPlan, CoreError> {
    if options.mode == ExportMode::Build && options.compare_with.is_some() {
        return Err(CoreError::refused(
            refusal::COMPARE_WITH,
            "a build always writes `dist/<m>/`; `compare_with` is for a check. Nothing was \
             written",
        ));
    }
    if options.compare_with.is_some() && loaded.marketplaces.len() > 1 {
        return Err(CoreError::refused(
            refusal::COMPARE_WITH,
            "`compare_with` names one built marketplace, and this vault holds several; name \
             one with `marketplace`",
        ));
    }
    let shared = built_harnesses(&loaded.settings, &options.harnesses).and_then(|roots| {
        let scan = crate::discover::discover_in(vfs, &loaded.layout)?;
        Ok((roots, scan))
    });
    let marketplaces = loaded
        .marketplaces
        .iter()
        .map(|market| {
            let plan = match &shared {
                Ok((roots, scan)) => plan_with(vfs, loaded, scan, roots, market, options),
                Err(err) => Err(err.duplicate()),
            };
            (market.name.clone(), plan)
        })
        .collect();
    Ok(VaultPlan {
        mode: options.mode,
        marketplaces,
    })
}

/// Builds or checks every marketplace of the vault, or the one the request
/// names, in one call: [`load_vault`], [`plan`], [`VaultPlan::report`] and
/// [`VaultPlan::commit`]. A check writes nothing, and reports each
/// marketplace that refused in [`VaultReport::refused`].
///
/// # Errors
///
/// * What [`load_vault`] refuses: the layout, the root file, an unknown
///   marketplace.
/// * What [`plan`] refuses: `refused: compare-with`.
/// * A build only: the first marketplace's refusal, among them
///   `refused: empty-marketplace` (no plugin), `unknown-harness` (a harness
///   name no harness answers to), `foreign-tree` (output this vault did not
///   build) and `path-collision`. Nothing is written.
/// * A build only: [`CoreError::SwapFailed`] and the I/O errors of the swap.
pub fn build_vault(vfs: &dyn Vfs, request: &VaultRequest) -> Result<VaultReport, CoreError> {
    let loaded = load_vault(vfs, &request.vault_root, &request.scope)?;
    let plan = plan(vfs, &loaded, &request.options)?;
    let report = plan.report();
    plan.commit(vfs)?;
    Ok(report)
}

/// Loads, renders and compares one marketplace with the harnesses `roots`
/// and the pass 1 `scan` of `loaded`, and writes nothing.
///
/// # Errors
///
/// * Everything [`crate::model::load::load_for`] refuses, among them
///   `refused: empty-marketplace` (no plugin) and `path-collision`.
/// * `refused: foreign-tree` — output this vault did not build.
fn plan_with(
    vfs: &dyn Vfs,
    loaded: &LoadedVault,
    scan: &DiscoveryReport,
    roots: &[Harness],
    market: &VaultMarketplace,
    options: &BuildOptions,
) -> Result<ExportPlan, CoreError> {
    // Load once, for every harness this build renders: the marketplace
    // chosen and validated, every body resolved. Everything a build refuses
    // before it renders, it refuses here.
    let out = match &options.compare_with {
        Some(path) => normalize(path),
        None => market.dist.clone(),
    };
    let scope = BuildScope::Marketplace(market.name.clone());
    let defaults = &loaded.settings.defaults;
    let market = load_for(vfs, &loaded.layout, scan, &scope, roots, defaults)?;
    let warnings = market.warnings.clone();

    // One tree per harness, translated from the one model, at the final
    // destination paths. Both modes read this and nothing else; `build`
    // even writes from it.
    let mut rendered = Vec::with_capacity(roots.len());
    for &spec in roots {
        rendered.push(Translator(spec).translate(&market)?.files);
    }

    // The components no root could take, listed once rather than per root:
    // the reason is the layout's, not any one root's. A component only some
    // roots cannot take is named under each of those roots.
    let mut units: Vec<ExportUnit> = market
        .uncarried
        .iter()
        .map(|c| ExportUnit::NotCarried {
            root: None,
            path: c.path().to_owned(),
            kind: c.kind(),
        })
        .collect();
    // Taken before the comparison, so `commit` sees a change made after it.
    let all_owned = roots.iter().flat_map(|spec| spec.owned_paths()).collect();
    let seen = Snapshot::take(vfs, &out, all_owned)?;
    let mut drifted = Vec::with_capacity(roots.len());
    for (&spec, files) in roots.iter().zip(&rendered) {
        let (root_units, equal) = compare_root(vfs, &out, spec, &market, files)?;
        units.extend(root_units);
        drifted.push(!equal);
    }
    let mut swap = Swap {
        base: out,
        owned: Vec::new(),
        files: BTreeMap::new(),
        seen,
    };
    for ((spec, files), drifted) in roots.iter().zip(rendered).zip(drifted) {
        if drifted {
            swap.owned
                .extend(spec.owned_paths().into_iter().map(str::to_owned));
            swap.files.extend(files);
        }
    }
    Ok(ExportPlan {
        report: ExportReport {
            mode: options.mode,
            checked: market.checked,
            changed: !swap.owned.is_empty(),
            units,
            warnings,
        },
        swap,
    })
}

/// One root's units against `out`, and whether what it owns there already
/// equals `rendered`.
///
/// # Errors
///
/// `refused: foreign-tree` — output this vault did not build.
fn compare_root(
    vfs: &dyn Vfs,
    out: &Path,
    spec: Harness,
    market: &Marketplace,
    rendered: &BTreeMap<String, PreviewWrite>,
) -> Result<(Vec<ExportUnit>, bool), CoreError> {
    let admitted = admitted_plugins(Translator(spec), market);
    let diff = diff_root(vfs, out, spec, &admitted, rendered, &market.config)?;
    // The swap replaces this root's owned paths. Holding work this build
    // did not produce, they are far likelier to be a misconfiguration than
    // an invitation. A rebuild is told from a stranger by the rule in
    // [`DestTree::built_here`]. Refused on `check` too: it exists to say what
    // a build would do.
    if diff.foreign {
        return Err(CoreError::refused(
            refusal::FOREIGN_TREE,
            format!(
                "{} already holds `{}`, and it holds files this build does not produce \
                 (and none carries this tool's generated-file marker): rendering `{spec}` here \
                 would replace it wholesale. Move it aside. Nothing was written",
                out.display(),
                spec.owned_paths().join("` and `"),
            ),
        ));
    }
    let mut units = diff.units;
    let mut uncarried: Vec<&Component> = market
        .plugins
        .values()
        .flat_map(|plugin| &plugin.commands)
        .filter(|c| !spec.format().carries(c.kind()))
        .collect();
    uncarried.sort_by(|a, b| a.path().cmp(b.path()));
    units.extend(uncarried.into_iter().map(|c| ExportUnit::NotCarried {
        root: Some(spec),
        path: c.path().to_owned(),
        kind: c.kind(),
    }));
    Ok((units, diff.equal))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    /// One marketplace's build or check, as `build_vault` runs it.
    fn export_skills(vfs: &dyn Vfs, request: &VaultRequest) -> Result<ExportReport, CoreError> {
        let loaded = load_vault(vfs, &request.vault_root, &request.scope)?;
        let planned = plan(vfs, &loaded, &request.options)?
            .marketplaces
            .remove(0)
            .1?;
        if request.options.mode == ExportMode::Build {
            commit(vfs, std::slice::from_ref(&planned.swap))?;
        }
        Ok(planned.report)
    }

    use serde_json::json;

    use super::*;
    use crate::frontmatter::{ContainerIdentity, EntityKind};
    use crate::harness::translator::GENERATED_MARKER;
    use crate::refusal;
    use crate::vfs::{MemVfs, NativeVfs};

    /// Writes `content` at `path`. For a path under a `marketplaces/` folder it
    /// first adds, when missing, the root file beside that folder and the note
    /// of each marketplace and plugin folder the path passes through, so a
    /// test writes only the notes it cares about.
    fn write(vfs: &dyn Vfs, path: &str, content: &str) {
        if let Some((root, rel)) = path.split_once("/marketplaces/") {
            let parts: Vec<&str> = rel.split('/').collect();
            let mut containers = vec![format!("{root}/skillwright.md")];
            if parts.len() > 2 {
                containers.push(format!("{root}/marketplaces/{0}/{0}.md", parts[0]));
            }
            if parts.len() > 4 && parts[1] == "plugins" {
                containers.push(format!(
                    "{root}/marketplaces/{0}/plugins/{1}/{1}.md",
                    parts[0], parts[2]
                ));
            }
            for container in containers {
                let container = Path::new(&container);
                if !vfs.exists(container) {
                    vfs.create_dir_all(container.parent().unwrap()).unwrap();
                    vfs.write_file(container, b"---\n---\n").unwrap();
                }
            }
        }
        let path = Path::new(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(path, content.as_bytes()).unwrap();
    }

    /// A skill note, with a caller-supplied
    /// body — for a test that needs to control what a skill's rendered text
    /// actually says (e.g. whether it calls the CLI at all).
    fn note_with_body(name: &str, body: &str) -> String {
        format!("---\ndescription: Use {name}.\n---\n\n{body}")
    }

    /// A skill note.
    fn note(name: &str) -> String {
        note_with_body(name, &format!("# {name}\n"))
    }

    /// The marketplace layout spec example 4: two plugins.
    fn example_vault() -> MemVfs {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper/commit-helper.md",
            &note("commit-helper"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/rebase-guide/rebase-guide.md",
            &note("rebase-guide"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            &note("review-guide"),
        );
        vfs
    }

    fn request(mode: ExportMode) -> VaultRequest {
        let mut value = VaultRequest::new("/vault");
        value.options.mode = mode;
        value
    }

    fn request_for(marketplace: &str, mode: ExportMode) -> VaultRequest {
        let mut value = request(mode);
        value.scope = BuildScope::Marketplace(marketplace.to_owned());
        value
    }

    fn build(vfs: &dyn Vfs) -> ExportReport {
        export_skills(vfs, &request(ExportMode::Build)).unwrap()
    }

    /// A build whose root file holds `frontmatter`.
    fn build_with(vfs: &dyn Vfs, frontmatter: &str) -> ExportReport {
        vfs.write_file(
            Path::new("/vault/skillwright.md"),
            format!("---\n{frontmatter}\n---\n").as_bytes(),
        )
        .unwrap();
        build(vfs)
    }

    fn check(vfs: &dyn Vfs) -> ExportReport {
        export_skills(vfs, &request(ExportMode::Check)).unwrap()
    }

    /// Every file under `root`: relative path, bytes, execute bit — sorted.
    fn files_under(vfs: &dyn Vfs, root: &Path) -> Vec<(String, Vec<u8>, bool)> {
        let options = vfs_walk::WalkOptions {
            skip_names: vec![".DS_Store".to_owned(), ".git".to_owned()],
            prune_dirs: Vec::new(),
            filter: None,
            symlinks: vfs_walk::Symlinks::List,
            errors: vfs_walk::Errors::Skip,
        };
        let mut out: Vec<_> = vfs_walk::walk(vfs, root, &options)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| {
                (
                    entry.rel,
                    vfs.read_file(&entry.abs).unwrap(),
                    vfs.is_executable(&entry.abs),
                )
            })
            .collect();
        out.sort();
        out
    }

    fn paths_under(vfs: &dyn Vfs, root: &Path) -> Vec<String> {
        files_under(vfs, root)
            .into_iter()
            .map(|(path, _, _)| path)
            .collect()
    }

    fn text_at(vfs: &dyn Vfs, path: &str) -> String {
        String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap()
    }

    /// Which (root, unit-name) pairs a report says differ — `drift` for a
    /// check, `written` for a build. Manifest units count under the name
    /// `marketplace.json`.
    fn differing(report: &ExportReport) -> BTreeSet<(String, String)> {
        report
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExportUnit::Plugin {
                    root,
                    name,
                    status: UnitStatus::Differs,
                    ..
                } => Some((root.to_string(), name.clone())),
                ExportUnit::Manifest {
                    root,
                    status: UnitStatus::Differs,
                } => Some((root.to_string(), "marketplace.json".to_owned())),
                _ => None,
            })
            .collect()
    }

    fn stale_set(report: &ExportReport) -> BTreeSet<(String, String, StaleKind)> {
        report
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExportUnit::Stale { root, path, kind } => {
                    Some((root.to_string(), path.clone(), *kind))
                }
                _ => None,
            })
            .collect()
    }

    fn plugin_status(report: &ExportReport, root: &str, name: &str) -> UnitStatus {
        report
            .units
            .iter()
            .find_map(|unit| match unit {
                ExportUnit::Plugin {
                    root: r,
                    name: n,
                    status,
                    ..
                } if r.as_str() == root && n == name => Some(*status),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no plugin unit for {root}/{name}: {report:?}"))
    }

    fn plugin_version_bump(
        report: &ExportReport,
        root: &str,
        name: &str,
    ) -> Option<VersionBumpSuggestion> {
        report
            .units
            .iter()
            .find_map(|unit| match unit {
                ExportUnit::Plugin {
                    root: r,
                    name: n,
                    version_bump_suggestion,
                    ..
                } if r.as_str() == root && n == name => Some(version_bump_suggestion.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no plugin unit for {root}/{name}: {report:?}"))
    }

    // A build swaps whole roots and sweeps what the vault no longer produces,
    // so a note it could not read would look exactly like a note that was
    // deleted. It refuses instead, and `check` refuses with it — one path,
    // one verdict.
    #[test]
    fn a_note_the_scan_could_not_read_stops_the_build_and_the_check() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/draft/draft.md",
            "---\ndescription: [half written\n---\n\nHalf written.\n",
        );

        for mode in [ExportMode::Build, ExportMode::Check] {
            let err = export_skills(&vfs, &request(mode)).unwrap_err();
            let message = err.to_string();
            assert!(message.contains("unreadable-note"), "{message}");
            assert!(message.contains("skills/draft/draft.md"), "{message}");
        }
        // The refusal lands before anything is staged.
        assert!(paths_under(&vfs, Path::new("/vault/dist/house")).is_empty());
    }

    #[test]
    fn build_produces_the_standalone_trees_and_nothing_else() {
        let vfs = example_vault();
        let report = build(&vfs);

        assert_eq!(report.checked, 3);
        assert!(report.changed);
        // The whole output, byte-listed: one tree per harness, each with its
        // own manifest dialect and, where the harness has one, its own
        // marketplace manifest — and no shared tree, no root-level manifests,
        // no stage residue.
        assert_eq!(
            paths_under(&vfs, Path::new("/vault/dist/house")),
            vec![
                ".agents/packs/git-tools/.codex-plugin/plugin.json",
                ".agents/packs/git-tools/skills/commit-helper/SKILL.md",
                ".agents/packs/git-tools/skills/rebase-guide/SKILL.md",
                ".agents/packs/tools/.codex-plugin/plugin.json",
                ".agents/packs/tools/skills/review-guide/SKILL.md",
                ".agents/plugins/marketplace.json",
                ".claude-plugin/marketplace.json",
                ".claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
                ".claude-plugin/packs/git-tools/skills/commit-helper/SKILL.md",
                ".claude-plugin/packs/git-tools/skills/rebase-guide/SKILL.md",
                ".claude-plugin/packs/tools/.claude-plugin/plugin.json",
                ".claude-plugin/packs/tools/skills/review-guide/SKILL.md",
                ".cursor-plugin/marketplace.json",
                ".cursor-plugin/packs/git-tools/.cursor-plugin/plugin.json",
                ".cursor-plugin/packs/git-tools/skills/git-tools-commit-helper/SKILL.md",
                ".cursor-plugin/packs/git-tools/skills/git-tools-rebase-guide/SKILL.md",
                ".cursor-plugin/packs/tools/.cursor-plugin/plugin.json",
                ".cursor-plugin/packs/tools/skills/tools-review-guide/SKILL.md",
                ".pi/packs/git-tools/package.json",
                ".pi/packs/git-tools/skills/git-tools-commit-helper/SKILL.md",
                ".pi/packs/git-tools/skills/git-tools-rebase-guide/SKILL.md",
                ".pi/packs/tools/package.json",
                ".pi/packs/tools/skills/tools-review-guide/SKILL.md",
            ]
        );
        // Each root counts both of git-tools' skills.
        let git_claude = report.units.iter().any(|u| {
            matches!(u, ExportUnit::Plugin { root, name, skills, .. }
                if *root == Harness::Claude && name == "git-tools" && *skills == 2)
        });
        let git_codex = report.units.iter().any(|u| {
            matches!(u, ExportUnit::Plugin { root, name, skills, .. }
                if *root == Harness::Codex && name == "git-tools" && *skills == 2)
        });
        assert!(git_claude && git_codex, "{report:?}");
    }

    #[test]
    fn a_plugin_folder_s_assets_and_changelog_land_at_its_pack_root() {
        let vfs = example_vault();
        let folder = "/vault/marketplaces/house/plugins/tools";
        write(&vfs, &format!("{folder}/assets/bin/tool"), "#!/bin/sh\n");
        write(
            &vfs,
            &format!("{folder}/CHANGELOG.md"),
            "# Changelog\n\n## 0.9.0\n",
        );
        write(
            &vfs,
            &format!("{folder}/tools.md"),
            "---\ndescription: Tools.\nversion: 1.0.0\n---\n",
        );
        let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();

        let paths = paths_under(&vfs, Path::new("/vault/dist/house"));
        for path in [
            ".claude-plugin/packs/tools/assets/bin/tool",
            ".claude-plugin/packs/tools/CHANGELOG.md",
            ".pi/packs/tools/CHANGELOG.md",
        ] {
            assert!(paths.contains(&path.to_owned()), "{path}: {paths:?}");
        }
        // Nothing is flattened onto the pack root.
        assert!(
            !paths.iter().any(|p| p.ends_with("tools/bin/tool")),
            "{paths:?}"
        );
        // 1.0.0 has no heading in the changelog.
        let warned: Vec<&str> = report.warnings.iter().map(|w| w.reason.as_str()).collect();
        assert!(warned.contains(&"changelog-missing-version"), "{warned:?}");
    }

    /// `assets/` ships as `assets/`: not read as notes, and with nothing left
    /// out, whatever the folders inside it are called.
    #[test]
    fn a_plugin_folder_s_assets_land_whole_and_are_not_notes() {
        let vfs = example_vault();
        let assets = "/vault/marketplaces/house/plugins/tools/assets";
        write(&vfs, &format!("{assets}/notes/helper.md"), &note("helper"));
        write(&vfs, &format!("{assets}/docs/guide.txt"), "Guide.\n");
        write(&vfs, &format!("{assets}/tests/case.test.txt"), "Case.\n");
        let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();

        assert_eq!(report.checked, 3, "the asset is not a fourth note");
        let paths = paths_under(&vfs, Path::new("/vault/dist/house"));
        for path in [
            ".claude-plugin/packs/tools/assets/notes/helper.md",
            ".claude-plugin/packs/tools/assets/docs/guide.txt",
            ".claude-plugin/packs/tools/assets/tests/case.test.txt",
        ] {
            assert!(paths.contains(&path.to_owned()), "{path}: {paths:?}");
        }
    }

    /// The warnings a build with this `CHANGELOG.md` and plugin note gives.
    fn changelog_reasons(changelog: &str, plugin_note: Option<&str>) -> Vec<String> {
        let vfs = example_vault();
        let folder = "/vault/marketplaces/house/plugins/tools";
        write(&vfs, &format!("{folder}/CHANGELOG.md"), changelog);
        if let Some(plugin_note) = plugin_note {
            write(&vfs, &format!("{folder}/tools.md"), plugin_note);
        }
        let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
        report.warnings.into_iter().map(|w| w.reason).collect()
    }

    #[test]
    fn a_changelog_heading_names_a_version_only_as_a_whole_token() {
        let plugin = "---\ndescription: Tools.\nversion: 1.0.0\n---\n";
        let warned = |changelog: &str| {
            changelog_reasons(changelog, Some(plugin))
                .contains(&"changelog-missing-version".to_owned())
        };
        for changelog in ["## 1.0.0\n", "## [1.0.0] - 2026-01-02\n", "## v1.0.0\n"] {
            assert!(!warned(changelog), "{changelog}");
        }
        for changelog in ["## 11.0.0\n", "## 1.0.0-rc.1\n", "1.0.0\n"] {
            assert!(warned(changelog), "{changelog}");
        }
    }

    #[test]
    fn a_plugin_with_no_note_is_checked_at_the_version_it_ships() {
        let warned = |changelog: &str| {
            changelog_reasons(changelog, None).contains(&"changelog-missing-version".to_owned())
        };
        assert!(warned("# Changelog\n"));
        assert!(!warned("## 0.1.0\n"));
    }

    /// A build writes `dist/<m>/` under the vault root. A build reads only
    /// `marketplaces/`, so what the first build writes there is not read back
    /// as notes by the second.
    #[test]
    fn output_in_dist_is_not_read_back_as_notes() {
        let vfs = example_vault();
        let request = request(ExportMode::Build);
        export_skills(&vfs, &request).unwrap();
        assert!(vfs.exists(Path::new(
            "/vault/dist/house/.claude-plugin/marketplace.json"
        )));
        let again = export_skills(&vfs, &check_of(&request)).unwrap();
        assert!(!again.changed, "{:?}", again.units);
        assert_eq!(again.checked, 3);
    }

    /// `<plugin>-<skill>` is not injective: plugin `a` skill `b-c` and plugin
    /// `a-b` skill `c` both publish as `a-b-c` on pi, so the build refuses.
    #[test]
    fn two_plugins_whose_prefixed_skill_names_meet_are_refused() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/a/skills/b-c/b-c.md",
            &note("b-c"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/a-b/skills/c/c.md",
            &note("c"),
        );
        let message = export_skills(&vfs, &request(ExportMode::Build))
            .unwrap_err()
            .to_string();
        assert!(message.contains("path-collision"), "{message}");
        assert!(message.contains("a-b-c"), "{message}");
        assert!(message.contains("plugins/a/skills/b-c/b-c.md"), "{message}");
        assert!(message.contains("plugins/a-b/skills/c/c.md"), "{message}");
        assert!(!vfs.exists(Path::new("/vault/dist/house")));
    }

    /// The swap replaces each tree whole, so a rendered path that turns from
    /// a file into a directory (or back) between builds must rebuild, and
    /// `check` must report it rather than fail.
    #[test]
    fn a_rendered_path_that_switches_between_file_and_directory_rebuilds() {
        const ASSETS: &str = "/vault/marketplaces/house/plugins/tools/assets";
        // Each case writes the first source, then the second, and carries the
        // path the second build lands and its bytes.
        for (first, second, landed, bytes) in [
            ("docs", "docs/x.md", "assets/docs/x.md", &b"second\n"[..]),
            ("docs/x.md", "docs", "assets/docs", &b"second\n"[..]),
        ] {
            let vfs = example_vault();
            write(&vfs, &format!("{ASSETS}/{first}"), "first\n");
            let mut request = request(ExportMode::Build);
            assert!(export_skills(&vfs, &request).unwrap().changed);

            vfs.remove(Path::new(ASSETS)).unwrap();
            write(&vfs, &format!("{ASSETS}/{second}"), "second\n");
            request.options.mode = ExportMode::Check;
            let report = export_skills(&vfs, &request).unwrap();
            assert!(report.changed, "{report:?}");
            request.options.mode = ExportMode::Build;
            assert!(export_skills(&vfs, &request).unwrap().changed);

            let path = Path::new("/vault/dist/house/.claude-plugin/packs/tools").join(landed);
            assert_eq!(vfs.read_file(&path).unwrap(), bytes);
        }
    }

    // --- Skill calls ---------------------------------------------------------

    const CALLS_SKILL_A: &str = "/vault/marketplaces/house/plugins/tools/skills/a/a.md";

    /// Plugin `tools` holds skills `a` and `b` and the reference
    /// `references/plain.md`, which is not a skill; plugin `other` holds `c`.
    /// `a`'s body is the caller's.
    fn calls_vault(a_body: &str) -> MemVfs {
        let vfs = MemVfs::new();
        write(&vfs, CALLS_SKILL_A, &note_with_body("a", a_body));
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/b/b.md",
            &note_with_body("b", "# b\n\n## Steps\n\nB's own prose.\n"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/other/skills/c/c.md",
            &note("c"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/references/plain.md",
            "# plain\n",
        );
        vfs
    }

    fn skill_md(vfs: &dyn Vfs, tree: &str, skill: &str) -> String {
        text_at(
            vfs,
            &format!("/vault/dist/house/{tree}/packs/tools/skills/{skill}/SKILL.md"),
        )
    }

    fn export_refusal(vfs: &dyn Vfs) -> String {
        export_skills(vfs, &request(ExportMode::Build))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn a_call_renders_each_harness_spelling_and_copies_nothing() {
        let vfs = calls_vault(
            "# a\n\nNext, run /[[b]] to finish.\n\nUsage: /[[b|the b skill]] ISSUE-1 --flag\n\n\
             ```sh\n/[[b]] now\n```\n\nThen /[[c]].\n",
        );
        let report = build(&vfs);

        for (tree, a, b, c) in [
            (".claude-plugin", "a", "/tools:b", "/other:c"),
            (".agents", "a", "$tools:b", "$other:c"),
            (".cursor-plugin", "tools-a", "/tools-b", "/other-c"),
        ] {
            let body = skill_md(&vfs, tree, a);
            assert!(
                body.contains(&format!("Next, run `{b}` to finish.")),
                "{tree}: {body}"
            );
            assert!(
                body.contains(&format!("Usage: the b skill (`{b}`) ISSUE-1 --flag")),
                "{tree}: {body}"
            );
            // Code is literal.
            assert!(body.contains("```sh\n/[[b]] now\n```"), "{tree}: {body}");
            assert!(body.contains(&format!("Then `{c}`.")), "{tree}: {body}");
            assert!(!body.contains("B's own prose"), "{tree}: {body}");
            let copied: Vec<String> = paths_under(
                &vfs,
                Path::new(&format!("/vault/dist/house/{tree}/packs/tools/skills/{a}")),
            );
            assert_eq!(copied, vec!["SKILL.md".to_owned()], "{tree}");
        }

        assert!(
            !report.warnings.iter().any(|w| w.detail.contains("[[b")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_skills_link_to_itself_is_a_link_to_its_skill_md_in_each_harness() {
        let vfs = MemVfs::new();
        let folder = "/vault/marketplaces/house/plugins/tools/skills/a";
        write(
            &vfs,
            &format!("{folder}/a.md"),
            &note_with_body("a", "# a\n\nPart of [[a]].\n"),
        );
        write(
            &vfs,
            &format!("{folder}/references/x.md"),
            "Part of [[a]].\n",
        );
        let check = |vfs: &MemVfs, tree: &str, skill: &str| {
            let body = skill_md(vfs, tree, skill);
            assert!(body.contains("Part of [a](SKILL.md)."), "{tree}: {body}");
            let reference = text_at(
                vfs,
                &format!("/vault/dist/house/{tree}/packs/tools/skills/{skill}/references/x.md"),
            );
            assert_eq!(reference, "Part of [a](../SKILL.md).\n", "{tree}");
        };
        let report = build(&vfs);
        for (tree, skill) in [
            (".claude-plugin", "a"),
            (".agents", "a"),
            (".cursor-plugin", "tools-a"),
            (".pi", "tools-a"),
        ] {
            check(&vfs, tree, skill);
        }
        assert!(
            report
                .warnings
                .iter()
                .all(|w| w.reason == "missing-version"),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn an_escaped_double_bracket_ships_without_its_backslash_in_each_harness() {
        let vfs = calls_vault("# a\n\nSee \\[[TODO later.\n");
        build(&vfs);
        for (tree, skill) in [
            (".claude-plugin", "a"),
            (".agents", "a"),
            (".cursor-plugin", "tools-a"),
            (".pi", "tools-a"),
        ] {
            let body = skill_md(&vfs, tree, skill);
            assert!(body.contains("See [[TODO later.\n"), "{tree}: {body}");
        }
    }

    #[test]
    fn a_call_naming_nothing_or_a_non_skill_is_refused() {
        let vfs = calls_vault("# a\n\nRun /[[nope]].\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("skill-call-unresolved"), "{err}");

        // A call names a skill by name, so a note that is not one names
        // nothing.
        let vfs = calls_vault("# a\n\nRun /[[plain]].\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("skill-call-unresolved"), "{err}");
    }

    #[test]
    fn a_call_fragment_is_refused() {
        let vfs = calls_vault("# a\n\nRun /[[b#Steps]].\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("skill-call-fragment"), "{err}");
        assert!(err.contains("`/[[b#Steps]]`"), "{err}");
    }

    #[test]
    fn an_escaped_sigil_shows_the_notation() {
        let vfs = calls_vault(
            "# a\n\nWrite \\/[[plain]] in prose and `\\/[[b]]` in code.\n\nCite \\/[[b]] too.\n",
        );
        build(&vfs);
        let body = skill_md(&vfs, ".claude-plugin", "a");
        // In prose the notation is written out, even when it names a skill.
        // Code is literal.
        assert!(body.contains("Write /[[plain]] in prose"), "{body}");
        assert!(body.contains("and `\\/[[b]]` in code."), "{body}");
        assert!(body.contains("Cite /[[b]] too."), "{body}");
        assert!(!body.contains("/tools:b"), "{body}");
    }

    #[test]
    fn a_literal_call_to_a_known_skill_is_refused() {
        let vfs = calls_vault("# a\n\nRun $other:c now.\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("literal-skill-call"), "{err}");
        assert!(err.contains("`$other:c`, write `/[[c]]`"), "{err}");

        let vfs = calls_vault("# a\n\nRun /tools:b now.\n");
        assert!(export_refusal(&vfs).contains("`/tools:b`"));

        // A plugin or skill this vault does not hold is left alone: it may be
        // another vault's. So is a bare name, a path, and code.
        let vfs = calls_vault(
            "# a\n\nRun /tools:nope, /elsewhere:task-work, /b, `/tools:b`, or see src/b and /b/x.\n",
        );
        build(&vfs);
    }

    #[test]
    fn the_pi_root_writes_a_package_per_plugin_with_prefixed_names() {
        let vfs = calls_vault("# a\n\nRun /[[b]]. See [[b]].\n");
        let mut request = request(ExportMode::Build);
        let report = export_skills(&vfs, &request).unwrap();

        assert_eq!(
            paths_under(&vfs, Path::new("/vault/dist/house/.pi")),
            vec![
                "packs/other/package.json",
                "packs/other/skills/other-c/SKILL.md",
                "packs/tools/package.json",
                "packs/tools/references/plain.md",
                "packs/tools/skills/tools-a/SKILL.md",
                "packs/tools/skills/tools-b/SKILL.md",
            ]
        );
        assert_eq!(
            text_at(&vfs, "/vault/dist/house/.pi/packs/tools/package.json"),
            "{\n  \"name\": \"tools\",\n  \"version\": \"0.1.0\",\n  \"description\": \
             \"Skills: a, b.\",\n  \"license\": \"UNLICENSED\",\n  \"keywords\": [\n    \
             \"pi-package\"\n  ],\n  \"pi\": {\n    \"skills\": [\n      \"./skills\"\n    \
             ]\n  }\n}\n"
        );
        let b = text_at(
            &vfs,
            "/vault/dist/house/.pi/packs/tools/skills/tools-b/SKILL.md",
        );
        assert!(b.contains("name: tools-b\n"), "{b}");
        let a = text_at(
            &vfs,
            "/vault/dist/house/.pi/packs/tools/skills/tools-a/SKILL.md",
        );
        assert!(
            a.contains("Run `/skill:tools-b`. See [b](../tools-b/SKILL.md)."),
            "{a}"
        );
        // No marketplace index, so no manifest unit either.
        assert!(!report.units.iter().any(|u| matches!(
            u,
            ExportUnit::Manifest { root, .. } if *root == Harness::Pi
        )));
        // A rebuild finds the tree its own, and unchanged.
        request.options.mode = ExportMode::Check;
        assert!(!export_skills(&vfs, &request).unwrap().changed);
    }

    #[test]
    fn a_prefixed_name_over_the_limit_refuses_the_build() {
        let plugin = "p".repeat(33);
        let skill = "s".repeat(31);
        let vfs = MemVfs::new();
        let path = format!("/vault/marketplaces/house/plugins/{plugin}/skills/{skill}/{skill}.md");
        write(&vfs, &path, &note(&skill));
        let err = export_refusal(&vfs);
        assert!(err.contains("published-name-invalid"), "{err}");
        assert!(paths_under(&vfs, Path::new("/vault/dist/house")).is_empty());

        // Built for Claude Code alone, which keeps the bare name, it builds.
        let mut claude = request(ExportMode::Build);
        claude.options.harnesses = BTreeSet::from(["claude".to_owned()]);
        export_skills(&vfs, &claude).unwrap();
    }

    #[test]
    fn a_build_applies_each_harness_its_own_override_block() {
        // Intended change: a build renders a note the way a publish does, so a
        // harness's block reaches that harness's tree and no
        // other.
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            "---\ndescription: Use review-guide.\nclaude:\n  \
             permissionMode: plan\n---\n\n# review-guide\n",
        );
        build(&vfs);
        let read = |path: &str| String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap();
        let claude =
            read("/vault/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md");
        assert!(claude.contains("permissionMode: plan"), "{claude}");
        let codex = read("/vault/dist/house/.agents/packs/tools/skills/review-guide/SKILL.md");
        assert!(!codex.contains("permissionMode"), "{codex}");
    }

    #[test]
    fn a_build_drops_an_override_key_the_harness_does_not_read_with_a_warning() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            "---\ndescription: Use review-guide.\ncodex:\n  \
             argument-hint: x\n---\n\n# review-guide\n",
        );
        let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.reason == "unknown-key" && w.detail.contains("codex.argument-hint")),
            "{:?}",
            report.warnings
        );
        let text = text_at(
            &vfs,
            "/vault/dist/house/.agents/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(!text.contains("argument-hint"), "{text}");
    }

    #[test]
    fn check_reports_drift_when_an_asset_changes_on_disk() {
        let vfs = example_vault();
        let tool = "/vault/marketplaces/house/plugins/tools/assets/tool";
        write(&vfs, tool, "v1\n");
        let request = request(ExportMode::Build);
        export_skills(&vfs, &request).unwrap();

        // Unchanged: `check` agrees with what was just built.
        let mut check_request = request.clone();
        check_request.options.mode = ExportMode::Check;
        assert!(!export_skills(&vfs, &check_request).unwrap().changed);

        // The source moves, out from under the build; `check` catches it the
        // same way it catches a rendered skill drifting.
        vfs.write_file(Path::new(tool), b"v2\n").unwrap();
        assert!(export_skills(&vfs, &check_request).unwrap().changed);
    }

    #[test]
    fn a_plugin_a_root_carries_nothing_of_is_absent_from_it_entirely() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/commands/rebase/rebase.md",
            &note("rebase"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            &note("review-guide"),
        );
        build(&vfs);

        // No directory, no plugin manifest, no marketplace listing.
        assert!(!vfs.exists(Path::new("/vault/dist/house/.agents/packs/git-tools")));
        let manifest = text_at(&vfs, "/vault/dist/house/.agents/plugins/marketplace.json");
        assert!(!manifest.contains("git-tools"), "{manifest}");
        // And in the root that admits it, all three are present.
        assert!(vfs.exists(Path::new(
            "/vault/dist/house/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json"
        )));
        let manifest = text_at(&vfs, "/vault/dist/house/.claude-plugin/marketplace.json");
        assert!(manifest.contains("git-tools"), "{manifest}");
    }

    #[test]
    fn the_emitted_skill_md_is_compiled_stamped_and_stripped() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            "---\ndescription: Reviews things.\n---\n\n\
             # Review Guide\n\n![[steps]]\n\nSee [[style-guide]].\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/references/steps.md",
            "Step prose to inline.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/references/style-guide.md",
            "# Style\n",
        );
        build(&vfs);

        let text = text_at(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(text.contains("name: review-guide"), "{text}");
        assert!(text.contains("description: Reviews things."), "{text}");
        // The provenance marker, naming the vault-relative source.
        assert!(
            text.contains(
                "<!-- Generated by skillwright from \
                 marketplaces/house/plugins/tools/skills/review-guide/review-guide.md; do not edit. -->"
            ),
            "{text}"
        );
        // Compiled: the embed inlined, the link rewritten, nothing left to
        // resolve.
        assert!(text.contains("Step prose to inline."), "{text}");
        assert!(text.contains("(references/style-guide.md)"), "{text}");
        assert!(!text.contains("[["), "{text}");
        // The linked file travelled as a resource, in the claude and codex roots.
        for packs in [".claude-plugin/packs", ".agents/packs"] {
            assert_eq!(
                vfs.read_file(
                    &Path::new("/vault/dist/house")
                        .join(packs)
                        .join("tools/skills/review-guide/references/style-guide.md")
                )
                .unwrap(),
                b"# Style\n",
                "{packs}"
            );
        }
    }

    /// A plugin note with a `name` and no `description`, over one described
    /// skill.
    fn silent_plugin_vault() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/said/said.md",
            "---\ndescription: d\n---\n",
        );
        vfs
    }

    #[test]
    fn a_skill_with_a_name_and_no_description_refuses_the_build() {
        let vfs = silent_plugin_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/quiet/quiet.md",
            "---\n---\n\n# Quiet\n",
        );
        for mode in [ExportMode::Check, ExportMode::Build] {
            let message = export_skills(&vfs, &request(mode)).unwrap_err().to_string();
            assert!(message.contains("missing-description"), "{message}");
            assert!(message.contains("skills/quiet/quiet.md"), "{message}");
            assert!(!message.contains("tools/tools.md"), "{message}");
        }
        assert!(!vfs.exists(Path::new("/vault/dist/house")));
    }

    /// An agent ships from no build, but one with no description is still
    /// refused by name.
    #[test]
    fn an_undescribed_agent_alone_is_refused_by_name() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/agents/helper/helper.md",
            "---\n---\n",
        );
        let message = export_skills(&vfs, &request(ExportMode::Check))
            .unwrap_err()
            .to_string();
        assert!(message.contains("missing-description"), "{message}");
        assert!(message.contains("agents/helper/helper.md"), "{message}");
    }

    /// A container need not describe itself: a silent plugin or marketplace
    /// note carries no warning, and its manifest takes the default.
    #[test]
    fn a_silent_container_note_has_no_warning() {
        let vfs = silent_plugin_vault();
        write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");
        let report = build(&vfs);
        assert!(
            report
                .warnings
                .iter()
                .all(|w| w.reason != refusal::MISSING_DESCRIPTION),
            "{:?}",
            report.warnings
        );
        let plugin: serde_json::Value = serde_json::from_str(&text_at(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        ))
        .unwrap();
        assert_eq!(plugin["description"], "Skills: said.");
    }

    /// A version in the plugin note silences the warning.
    #[test]
    fn a_plugin_without_a_version_warns_until_one_is_stated() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\ndescription: Tools.\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/said/said.md",
            "---\ndescription: d\n---\n",
        );
        let request = request(ExportMode::Check);
        let report = export_skills(&vfs, &request).unwrap();
        let reasons: Vec<&str> = report.warnings.iter().map(|w| w.reason.as_str()).collect();
        assert_eq!(reasons, ["missing-version"]);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\ndescription: Tools.\nversion: 2.0.0\n---\n",
        );
        let report = export_skills(&vfs, &request).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// YAML reads `version: 0.3` as a number. Check and build refuse it and
    /// say to quote it, rather than ship the marketplace without the plugin.
    #[test]
    fn a_plugin_with_an_unquoted_float_version_is_refused() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\ndescription: Tools.\nversion: 0.3\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/said/said.md",
            "---\ndescription: d\n---\n",
        );
        for mode in [ExportMode::Check, ExportMode::Build] {
            let err = export_skills(&vfs, &request(mode)).unwrap_err().to_string();
            assert!(err.contains("version"), "{err}");
            assert!(err.contains("quote it"), "{err}");
        }
        assert!(!vfs.exists(Path::new("/vault/dist")));
    }

    #[test]
    fn another_marketplaces_missing_description_neither_refuses_nor_warns() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/said/said.md",
            "---\ndescription: d\n---\n",
        );
        write(&vfs, "/vault/marketplaces/other/other.md", "---\n---\n");
        write(
            &vfs,
            "/vault/marketplaces/other/plugins/kit/skills/quiet/quiet.md",
            "---\n---\n",
        );
        let mut request = request(ExportMode::Build);
        request.scope = BuildScope::Marketplace("house".to_owned());
        let report = export_skills(&vfs, &request).unwrap();
        assert!(
            report
                .warnings
                .iter()
                .all(|w| w.reason == "missing-version"),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_description_holding_a_colon_space_publishes_as_valid_yaml() {
        let vfs = MemVfs::new();
        let description = "Turn a review into a page: diagrams of the flows. Use when asked.";
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/flow/flow.md",
            &format!("---\ndescription: \"{description}\"\n---\n\n# Flow\n"),
        );
        build(&vfs);
        let text = text_at(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/tools/skills/flow/SKILL.md",
        );
        let frontmatter = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(fm, _)| fm)
            .unwrap();
        let parsed: serde_yaml::Mapping = serde_yaml::from_str(frontmatter).unwrap();
        assert_eq!(
            parsed.get("description").and_then(|v| v.as_str()),
            Some(description)
        );
    }

    // `read_vault_note` never fills `SkillContent::arguments` — the load has
    // to call `apply_arguments`, or a marketplace-published skill would carry
    // no hint and no section at all.
    #[test]
    fn a_marketplace_skill_derives_its_hint_and_renders_the_section() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            "---\ndescription: Reviews things.\n\
             arguments:\n  - name: pr-number\n    required: true\n    description: The PR.\n\
             ---\n\n# Review Guide\n",
        );
        build(&vfs);

        // Claude Code's dialect keeps `argument-hint`; Codex's dialect omits
        // every extension field — see
        // `agent_adapt::AGENT_SKILLS_EXTENSION_FIELDS` — so only the body
        // section, not the frontmatter hint, is common to both.
        let claude = text_at(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(
            claude.contains("argument-hint: \"<pr-number>\""),
            "{claude}"
        );
        assert!(claude.contains("arguments:\n  - pr-number"), "{claude}");
        assert!(claude.contains("$ARGUMENTS"), "{claude}");

        for tree in [".claude-plugin", ".agents"] {
            let text = text_at(
                &vfs,
                &format!("/vault/dist/house/{tree}/packs/tools/skills/review-guide/SKILL.md"),
            );
            assert!(text.contains("## Arguments"), "{tree}: {text}");
            assert!(
                text.contains("- `pr-number` (required): The PR."),
                "{tree}: {text}"
            );
        }

        let codex = text_at(
            &vfs,
            "/vault/dist/house/.agents/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(!codex.contains("argument-hint"), "{codex}");
        assert!(!codex.contains("arguments:"), "{codex}");
        assert!(!codex.contains("$ARGUMENTS"), "{codex}");
    }

    #[test]
    fn manifests_are_byte_deterministic() {
        let vfs = example_vault();
        build(&vfs);

        // Golden bytes: stable key order, plugins sorted by name, two-space
        // indentation, trailing newline.
        //
        // This vault's note names no owner, so the marketplace's own name
        // stands in. Claude marks `owner` required and rejects a bare string,
        // so an absent one is not an option — see `claude_marketplace_manifest`.
        assert_eq!(
            text_at(&vfs, "/vault/dist/house/.claude-plugin/marketplace.json"),
            r#"{
  "name": "house",
  "owner": {
    "name": "house"
  },
  "plugins": [
    {
      "name": "git-tools",
      "source": "./.claude-plugin/packs/git-tools",
      "description": "Skills: commit-helper, rebase-guide."
    },
    {
      "name": "tools",
      "source": "./.claude-plugin/packs/tools",
      "description": "Skills: review-guide."
    }
  ]
}
"#
        );
        assert_eq!(
            text_at(
                &vfs,
                "/vault/dist/house/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json"
            ),
            r#"{
  "name": "git-tools",
  "version": "0.1.0",
  "description": "Skills: commit-helper, rebase-guide.",
  "license": "UNLICENSED",
  "keywords": [],
  "skills": "./skills/"
}
"#
        );
        // The codex output describes its own members, in its own dialect.
        assert_eq!(
            text_at(
                &vfs,
                "/vault/dist/house/.agents/packs/git-tools/.codex-plugin/plugin.json"
            ),
            r#"{
  "name": "git-tools",
  "version": "0.1.0",
  "description": "Skills: commit-helper, rebase-guide.",
  "license": "UNLICENSED",
  "keywords": [],
  "skills": "./skills/",
  "interface": {
    "displayName": "git-tools",
    "category": "Productivity"
  }
}
"#
        );
        assert_eq!(
            text_at(&vfs, "/vault/dist/house/.agents/plugins/marketplace.json"),
            r#"{
  "name": "house",
  "interface": {
    "displayName": "house"
  },
  "plugins": [
    {
      "name": "git-tools",
      "source": {
        "source": "local",
        "path": "./.agents/packs/git-tools"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    },
    {
      "name": "tools",
      "source": {
        "source": "local",
        "path": "./.agents/packs/tools"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    }
  ]
}
"#
        );

        // And the whole output is byte-stable: a second vault, same content,
        // builds to identical bytes.
        let again = example_vault();
        build(&again);
        assert_eq!(
            files_under(&vfs, Path::new("/vault/dist/house")),
            files_under(&again, Path::new("/vault/dist/house"))
        );
    }

    #[test]
    fn container_notes_reach_the_manifests_and_the_owner_is_not_an_author() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/house.md",
            "---\nowner: {name: Kelly}\ndisplay_name: Demo Market\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/git-tools.md",
            "---\ndescription: Git helpers.\nversion: 2.0.0\nlicense: MIT\n\
             keywords: [git]\ncategory: Development\ndisplay_name: Git Tools\n---\n",
        );
        build(&vfs);

        let plugin = text_at(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
        );
        assert!(plugin.contains("\"version\": \"2.0.0\""), "{plugin}");
        assert!(
            plugin.contains("\"description\": \"Git helpers.\""),
            "{plugin}"
        );
        assert!(plugin.contains("\"license\": \"MIT\""), "{plugin}");
        assert!(plugin.contains("\"git\""), "{plugin}");
        // No author anywhere: `owner` is not inherited, so none is written.
        assert!(!plugin.contains("\"author\""), "{plugin}");
        assert!(!plugin.contains("Kelly"), "{plugin}");

        let codex = text_at(
            &vfs,
            "/vault/dist/house/.agents/packs/git-tools/.codex-plugin/plugin.json",
        );
        assert!(codex.contains("\"displayName\": \"Git Tools\""), "{codex}");
        assert!(codex.contains("\"category\": \"Development\""), "{codex}");

        let market = text_at(&vfs, "/vault/dist/house/.agents/plugins/marketplace.json");
        assert!(
            market.contains("\"displayName\": \"Demo Market\""),
            "{market}"
        );
        assert!(market.contains("\"category\": \"Development\""), "{market}");

        let market = text_at(&vfs, "/vault/dist/house/.claude-plugin/marketplace.json");
        assert!(market.contains("\"owner\""), "{market}");
        assert!(
            market.contains("\"description\": \"Git helpers.\""),
            "{market}"
        );
        // `plugin.json` holds the version: with both, `claude plugin validate` warns.
        assert!(!market.contains("\"version\": \"2.0.0\""), "{market}");
    }

    #[test]
    fn an_up_to_date_destination_is_reported_unchanged_and_untouched() {
        let vfs = example_vault();
        build(&vfs);
        let before = files_under(&vfs, Path::new("/vault/dist/house"));

        let report = build(&vfs);

        assert!(!report.changed, "{report:?}");
        assert!(
            report.units.iter().all(|unit| matches!(
                unit,
                ExportUnit::Plugin {
                    status: UnitStatus::Same,
                    ..
                } | ExportUnit::Manifest {
                    status: UnitStatus::Same,
                    ..
                }
            )),
            "{report:?}"
        );
        // Not one byte, not one rename — no stage was even created.
        assert_eq!(files_under(&vfs, Path::new("/vault/dist/house")), before);
        assert!(!vfs.exists(Path::new("/vault/dist/house/.skillwright-stage")));
        assert!(!vfs.exists(Path::new("/vault/dist/house/.skillwright-stage")));
    }

    // --- Version bump suggestions -----------------------------------------

    /// One plugin with its own container note — carrying `version`, which
    /// [`crate::model::Cascade`] folds into `MarketplaceConfig.plugins` — and one
    /// member skill.
    fn vault_with_versioned_plugin() -> MemVfs {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/house/plugins/git-tools";
        write(
            &vfs,
            &format!("{base}/git-tools.md"),
            "---\ndescription: Git helpers.\nversion: 2.1.0\n---\n\nThe pack.\n",
        );
        write(
            &vfs,
            &format!("{base}/skills/commit-helper/commit-helper.md"),
            "---\ndescription: Writes commits.\n---\n\nCommit well.\n",
        );
        vfs
    }

    // The case the callout exists for: a member changed and nothing moved
    // `version` to say so.
    #[test]
    fn a_changed_member_with_an_unmoved_version_suggests_a_bump() {
        let vfs = vault_with_versioned_plugin();
        build(&vfs);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper/commit-helper.md",
            "---\ndescription: Writes better commits.\n---\n\nCommit well.\n",
        );

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Differs
        );
        assert_eq!(
            plugin_version_bump(&report, "claude", "git-tools"),
            Some(VersionBumpSuggestion {
                current_version: "2.1.0".to_owned(),
                published_version: "2.1.0".to_owned(),
            })
        );
    }

    // The author already answered the drift, so there is nothing left to
    // suggest.
    #[test]
    fn a_changed_member_with_a_moved_version_suggests_nothing() {
        let vfs = vault_with_versioned_plugin();
        build(&vfs);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper/commit-helper.md",
            "---\ndescription: Writes better commits.\n---\n\nCommit well.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/git-tools.md",
            "---\ndescription: Git helpers.\nversion: 2.2.0\n---\n\nThe pack.\n",
        );

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Differs
        );
        assert_eq!(plugin_version_bump(&report, "claude", "git-tools"), None);
    }

    // Nothing changed, so the plugin is `ok` and there is nothing to suggest
    // either.
    #[test]
    fn an_unchanged_plugin_suggests_nothing() {
        let vfs = vault_with_versioned_plugin();
        build(&vfs);

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Same
        );
        assert_eq!(plugin_version_bump(&report, "claude", "git-tools"), None);
    }

    // `example_vault`'s plugins have no container note at all, so there is
    // no `version` anywhere to compare a bump against.
    #[test]
    fn a_plugin_with_no_container_note_never_suggests_a_bump() {
        let vfs = example_vault();
        build(&vfs);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper/commit-helper.md",
            "---\ndescription: Use commit-helper, but better.\n---\n\n# commit-helper\n",
        );

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Differs
        );
        assert_eq!(plugin_version_bump(&report, "claude", "git-tools"), None);
    }

    #[test]
    fn check_writes_nothing_under_any_outcome() {
        // Outcome one: everything drifted (no destination at all).
        let vfs = example_vault();
        let report = check(&vfs);
        assert!(report.changed);
        assert!(
            !vfs.exists(Path::new("/vault/dist/house")),
            "check created the destination"
        );

        // Outcome two: clean.
        build(&vfs);
        let before = files_under(&vfs, Path::new("/vault/dist/house"));
        let report = check(&vfs);
        assert!(!report.changed, "{report:?}");
        assert_eq!(files_under(&vfs, Path::new("/vault/dist/house")), before);

        // Outcome three: drifted over an existing destination.
        vfs.write_file(
            Path::new("/vault/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md"),
            b"tampered\n",
        )
        .unwrap();
        let before = files_under(&vfs, Path::new("/vault/dist/house"));
        let report = check(&vfs);
        assert!(report.changed);
        assert_eq!(files_under(&vfs, Path::new("/vault/dist/house")), before);
    }

    #[test]
    fn check_reports_ok_and_drift_per_plugin_per_root() {
        let vfs = example_vault();
        build(&vfs);
        // One plugin, one root: everything else must stay OK.
        vfs.write_file(
            Path::new("/vault/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md"),
            b"tampered\n",
        )
        .unwrap();

        let report = check(&vfs);

        assert!(report.changed);
        assert_eq!(
            plugin_status(&report, "claude", "tools"),
            UnitStatus::Differs
        );
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Same
        );
        assert_eq!(plugin_status(&report, "codex", "tools"), UnitStatus::Same);
        assert_eq!(
            plugin_status(&report, "codex", "git-tools"),
            UnitStatus::Same
        );
        assert!(
            report.units.iter().all(|u| !matches!(
                u,
                ExportUnit::Manifest {
                    status: UnitStatus::Differs,
                    ..
                }
            )),
            "{report:?}"
        );
    }

    #[test]
    fn stale_content_is_reported_by_check_and_swept_by_build() {
        let vfs = example_vault();
        build(&vfs);
        // A stale plugin and an empty directory among the packs. Beside the
        // packs, nothing is stale.
        write(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/legacy/junk.md",
            "junk\n",
        );
        vfs.create_dir_all(Path::new("/vault/dist/house/.agents/packs/empty"))
            .unwrap();
        write(&vfs, "/vault/dist/house/.claude-plugin/README.md", "kept\n");
        vfs.create_dir_all(Path::new("/vault/dist/house/.agents/empty"))
            .unwrap();

        let report = check(&vfs);
        assert!(report.changed);
        assert_eq!(
            stale_set(&report),
            BTreeSet::from([
                (
                    "claude".to_owned(),
                    ".claude-plugin/packs/legacy".to_owned(),
                    StaleKind::Plugin,
                ),
                (
                    "codex".to_owned(),
                    ".agents/packs/empty".to_owned(),
                    StaleKind::Plugin,
                ),
            ]),
            "one unit per stale thing — the stale plugin's files do not \
             double-report: {report:?}"
        );
        // The vault's own plugins are still OK; staleness is the only drift.
        assert_eq!(plugin_status(&report, "claude", "tools"), UnitStatus::Same);

        let report = build(&vfs);
        assert!(report.changed);
        assert_eq!(stale_set(&report).len(), 2, "{report:?}");
        assert!(!vfs.exists(Path::new("/vault/dist/house/.claude-plugin/packs/legacy")));
        assert!(!vfs.exists(Path::new("/vault/dist/house/.agents/packs/empty")));
        assert!(vfs.exists(Path::new("/vault/dist/house/.claude-plugin/README.md")));
        assert!(vfs.exists(Path::new("/vault/dist/house/.agents/empty")));
        assert!(!check(&vfs).changed);
    }

    #[test]
    fn check_build_check_verdicts_agree() {
        let vfs = example_vault();
        build(&vfs);
        // Three kinds of drift at once: tampered bytes in one root, a missing
        // plugin manifest in the other, and a stale plugin.
        vfs.write_file(
            Path::new("/vault/dist/house/.agents/packs/tools/skills/review-guide/SKILL.md"),
            b"tampered\n",
        )
        .unwrap();
        vfs.remove(Path::new(
            "/vault/dist/house/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
        ))
        .unwrap();
        write(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/legacy/junk.md",
            "junk\n",
        );

        let verdict = check(&vfs);
        assert!(verdict.changed);

        let built = build(&vfs);
        assert!(built.changed);
        // What check called drift is exactly what build wrote, unit for unit,
        // and the stale sets are the same set.
        assert_eq!(differing(&verdict), differing(&built));
        assert_eq!(stale_set(&verdict), stale_set(&built));
        assert_eq!(
            differing(&verdict),
            BTreeSet::from([
                ("claude".to_owned(), "git-tools".to_owned()),
                ("codex".to_owned(), "tools".to_owned()),
            ])
        );

        // And after the build, the same check finds nothing.
        let after = check(&vfs);
        assert!(!after.changed, "{after:?}");
        assert!(differing(&after).is_empty());
        assert!(stale_set(&after).is_empty());
    }

    #[test]
    fn a_stale_execute_bit_is_drift_and_build_repairs_it() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/tool-pack/tool-pack.md",
            &note("tool-pack"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/tool-pack/scripts/run.sh",
            "#!/bin/sh\n",
        );
        vfs.set_executable(
            Path::new("/vault/marketplaces/house/plugins/tools/skills/tool-pack/scripts/run.sh"),
            true,
        )
        .unwrap();
        build(&vfs);

        let published = Path::new(
            "/vault/dist/house/.claude-plugin/packs/tools/skills/tool-pack/scripts/run.sh",
        );
        assert!(vfs.is_executable(published), "the bit travelled");

        vfs.set_executable(published, false).unwrap();
        let report = check(&vfs);
        assert!(report.changed, "{report:?}");
        assert_eq!(
            plugin_status(&report, "claude", "tools"),
            UnitStatus::Differs
        );
        assert_eq!(plugin_status(&report, "codex", "tools"), UnitStatus::Same);

        build(&vfs);
        assert!(vfs.is_executable(published), "the bit was repaired");
    }

    /// A wrapper that fails the next `remaining` renames onto one path — the
    /// injection a mid-swap failure test needs.
    struct FailingRename<'a> {
        inner: &'a MemVfs,
        to: PathBuf,
        remaining: Cell<u32>,
    }

    impl Vfs for FailingRename<'_> {
        fn read_file(&self, path: &Path) -> Result<Vec<u8>, crate::vfs::VfsError> {
            self.inner.read_file(path)
        }
        fn read_dir(&self, path: &Path) -> Result<Vec<crate::vfs::DirEntry>, crate::vfs::VfsError> {
            self.inner.read_dir(path)
        }
        fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), crate::vfs::VfsError> {
            self.inner.write_file(path, bytes)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), crate::vfs::VfsError> {
            self.inner.create_dir_all(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.inner.exists(path)
        }
        fn metadata(&self, path: &Path) -> Result<crate::vfs::Metadata, crate::vfs::VfsError> {
            self.inner.metadata(path)
        }
        fn is_executable(&self, path: &Path) -> bool {
            self.inner.is_executable(path)
        }
        fn set_executable(&self, path: &Path, yes: bool) -> Result<(), crate::vfs::VfsError> {
            self.inner.set_executable(path, yes)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), crate::vfs::VfsError> {
            if normalize(to) == self.to && self.remaining.get() > 0 {
                self.remaining.set(self.remaining.get() - 1);
                return Err(crate::vfs::VfsError::io(
                    to,
                    std::io::Error::other("injected failure"),
                ));
            }
            self.inner.rename(from, to)
        }
        fn remove(&self, path: &Path) -> Result<(), crate::vfs::VfsError> {
            self.inner.remove(path)
        }
    }

    #[test]
    fn a_failed_swap_rolls_back_every_root_already_swapped() {
        let vfs = example_vault();
        build(&vfs);
        // Drift every root, so the swap has several units and a later one can fail
        // after the first landed.
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper/commit-helper.md",
            "---\ndescription: Use commit-helper.\n---\n\nRevised body.\n",
        );
        let before = files_under(&vfs, Path::new("/vault/dist/house"));

        // The swap unit is an entry, so the injected failure names one.
        let failing = FailingRename {
            inner: &vfs,
            to: PathBuf::from("/vault/dist/house/.agents/packs"),
            remaining: Cell::new(1),
        };
        let err = export_skills(&failing, &request(ExportMode::Build)).unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, CoreError::SwapFailed(_)), "{message}");
        assert!(message.contains("rolled back"), "{message}");
        // Claude had already swapped; the rollback put it back. Byte for
        // byte, bit for bit, the destination is as it was — and the stage is
        // gone.
        assert_eq!(files_under(&vfs, Path::new("/vault/dist/house")), before);
        assert!(!vfs.exists(Path::new("/vault/dist/house/.skillwright-stage")));
        assert!(!vfs.exists(Path::new("/vault/dist/house/.skillwright-stage")));

        // The failure was transient, not structural: the same build succeeds
        // once the filesystem cooperates.
        let report = build(&vfs);
        assert!(report.changed);
        assert!(!check(&vfs).changed);
    }

    #[test]
    fn a_failed_rollback_keeps_the_backup_and_names_it() {
        let vfs = example_vault();
        build(&vfs);
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper/commit-helper.md",
            "---\ndescription: Use commit-helper.\n---\n\nRevised body.\n",
        );

        // Two failures on the same path: the swap-in, then the restore.
        let failing = FailingRename {
            inner: &vfs,
            to: PathBuf::from("/vault/dist/house/.agents/packs"),
            remaining: Cell::new(2),
        };
        let err = export_skills(&failing, &request(ExportMode::Build)).unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, CoreError::SwapFailed(_)), "{message}");
        // The message names what could not be restored and where its bytes
        // survive; the stage is deliberately not deleted.
        assert!(message.contains(".agents"), "{message}");
        assert!(message.contains("backup survives"), "{message}");
        assert!(
            vfs.exists(Path::new(
                "/vault/dist/house/.skillwright-stage/backup/.agents/packs/git-tools/.codex-plugin/plugin.json"
            )),
            "the old entry's bytes are still in the backup"
        );
        // The root that did swap was still rolled back.
        assert!(vfs.exists(Path::new(
            "/vault/dist/house/.claude-plugin/marketplace.json"
        )));
    }

    /// A vault at a repository root, with a `.git` and a file of its own.
    fn repo_vault(vfs: &MemVfs) -> VaultRequest {
        write(
            vfs,
            "/repo/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            &note("review-guide"),
        );
        write(vfs, "/repo/README.md", "# A repository\n");
        write(vfs, "/repo/dist/house/.git/HEAD", "ref: refs/heads/main\n");
        let mut request = request(ExportMode::Build);
        request.vault_root = PathBuf::from("/repo");
        request
    }

    #[test]
    fn the_repository_root_is_a_marketplace_directory_like_any_other() {
        // The whole point: every harness's manifest at the path its harness
        // reads, so a bare repository URL registers with all of them.
        let vfs = MemVfs::new();
        let report = export_skills(&vfs, &repo_vault(&vfs)).unwrap();
        assert!(report.changed);

        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.claude-plugin/marketplace.json"
        )));
        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.agents/plugins/marketplace.json"
        )));
        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md"
        )));
        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.agents/packs/tools/skills/review-guide/SKILL.md"
        )));
    }

    #[test]
    fn a_build_at_a_repository_root_touches_nothing_it_did_not_render() {
        // A root owns one top-level entry per harness and nothing else, so
        // the swap can never name `.git`, the vault, or a file of the
        // repository's own — the render produces no path beginning with one.
        let vfs = MemVfs::new();
        export_skills(&vfs, &repo_vault(&vfs)).unwrap();

        assert_eq!(
            vfs.read_file(Path::new("/repo/README.md")).unwrap(),
            b"# A repository\n"
        );
        assert!(vfs.exists(Path::new("/repo/dist/house/.git/HEAD")));
        assert!(vfs.exists(Path::new(
            "/repo/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md"
        )));
    }

    #[test]
    fn the_two_harnesses_never_collide_at_one_directory() {
        // Each renders the same skill, and they cannot share the file: the
        // two harnesses spell a skill's tool names differently. Separate
        // trees is what makes one directory possible at all.
        let vfs = MemVfs::new();
        export_skills(&vfs, &repo_vault(&vfs)).unwrap();
        let claude = "/repo/dist/house/.claude-plugin/packs/tools/skills/review-guide/SKILL.md";
        let codex = "/repo/dist/house/.agents/packs/tools/skills/review-guide/SKILL.md";
        assert_ne!(claude, codex);
        assert!(vfs.exists(Path::new(claude)) && vfs.exists(Path::new(codex)));
        // And each manifest points at its own tree.
        let manifest = text_at(&vfs, "/repo/dist/house/.claude-plugin/marketplace.json");
        assert!(
            manifest.contains("./.claude-plugin/packs/tools"),
            "{manifest}"
        );
        let manifest = text_at(&vfs, "/repo/dist/house/.agents/plugins/marketplace.json");
        assert!(manifest.contains("./.agents/packs/tools"), "{manifest}");
    }

    #[test]
    fn a_repository_marketplace_is_clean_on_the_check_that_follows() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        export_skills(&vfs, &request).unwrap();

        let mut checking = request.clone();
        checking.options.mode = ExportMode::Check;
        let report = export_skills(&vfs, &checking).unwrap();
        assert!(!report.changed, "{:?}", report.units);
        // The repository around it is not drift, and never reported.
        assert!(
            !report
                .units
                .iter()
                .any(|unit| matches!(unit, ExportUnit::Stale { .. })),
            "{:?}",
            report.units
        );
    }

    #[test]
    fn a_pack_removed_from_the_vault_disappears_from_every_tree() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        write(
            &vfs,
            "/repo/marketplaces/house/plugins/extra/skills/spare/spare.md",
            &note("spare"),
        );
        export_skills(&vfs, &request).unwrap();
        assert!(vfs.exists(Path::new("/repo/dist/house/.claude-plugin/packs/extra")));

        vfs.remove(Path::new("/repo/marketplaces/house/plugins/extra"))
            .unwrap();
        let report = export_skills(&vfs, &request).unwrap();
        assert!(!vfs.exists(Path::new("/repo/dist/house/.claude-plugin/packs/extra")));
        assert!(!vfs.exists(Path::new("/repo/dist/house/.agents/packs/extra")));
        assert!(!vfs.exists(Path::new("/repo/dist/house/.cursor-plugin/packs/extra")));
        assert!(!vfs.exists(Path::new("/repo/dist/house/.pi/packs/extra")));
        // Once per harness, each naming its own path.
        let stale: Vec<&str> = report
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExportUnit::Stale { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            stale,
            vec![
                ".claude-plugin/packs/extra",
                ".agents/packs/extra",
                ".cursor-plugin/packs/extra",
                ".pi/packs/extra",
            ]
        );
    }

    #[test]
    fn a_harness_can_be_selected_and_the_others_are_not_rendered() {
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        request.options.harnesses.insert("claude".to_owned());

        export_skills(&vfs, &request).unwrap();
        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.claude-plugin/marketplace.json"
        )));
        assert!(!vfs.exists(Path::new("/repo/dist/house/.agents")));
    }

    #[test]
    fn a_harness_this_build_cannot_render_is_refused_by_name() {
        // Silently publishing less than was asked for is the failure this
        // crate exists to prevent.
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        request.options.harnesses.insert("windsurf".to_owned());

        let err = export_skills(&vfs, &request).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused { .. }), "{message}");
        assert!(message.contains("unknown-harness"), "{message}");
        assert!(message.contains("windsurf"), "{message}");
        assert!(!vfs.exists(Path::new("/repo/dist/house/.claude-plugin")));
    }

    /// A file someone adds beside the packs in `dist/<m>/` survives a rebuild:
    /// a build replaces only the paths it owns.
    #[test]
    fn a_hand_added_file_beside_the_packs_survives_a_rebuild() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        export_skills(&vfs, &request).unwrap();
        let mine = "/repo/dist/house/.agents/skills/mine/SKILL.md";
        write(&vfs, mine, "---\nname: mine\n---\n\nBy hand.\n");
        write(&vfs, "/repo/dist/house/.pi/settings.json", "{}\n");
        write(&vfs, "/repo/dist/house/.claude-plugin/notes.txt", "mine\n");

        // Nothing reports them, and a rebuild that changes the trees keeps them.
        assert!(!export_skills(&vfs, &check_of(&request)).unwrap().changed);
        write(
            &vfs,
            "/repo/marketplaces/house/plugins/tools/skills/review-guide/review-guide.md",
            &note_with_body("review-guide", "Revised.\n"),
        );
        let report = export_skills(&vfs, &request).unwrap();

        assert!(report.changed);
        assert!(stale_set(&report).is_empty(), "{:?}", stale_set(&report));
        assert_eq!(text_at(&vfs, mine), "---\nname: mine\n---\n\nBy hand.\n");
        assert!(vfs.exists(Path::new("/repo/dist/house/.pi/settings.json")));
        assert!(vfs.exists(Path::new("/repo/dist/house/.claude-plugin/notes.txt")));
    }

    /// `request` as a check.
    fn check_of(request: &VaultRequest) -> VaultRequest {
        let mut check = request.clone();
        check.options.mode = ExportMode::Check;
        check
    }

    #[test]
    fn packs_this_vault_did_not_build_are_refused() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        let hand = "/repo/dist/house/.claude-plugin/packs/hand/plugin.json";
        write(&vfs, hand, "{}\n");

        let err = export_skills(&vfs, &request).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused { .. }), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        assert!(message.contains(".claude-plugin/packs"), "{message}");
        // And it stopped before writing anything.
        assert_eq!(vfs.read_file(Path::new(hand)).unwrap(), b"{}\n");
    }

    #[test]
    fn check_refuses_packs_this_vault_did_not_build_and_writes_nothing() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        let hand = "/repo/dist/house/.claude-plugin/packs/hand/plugin.json";
        write(&vfs, hand, "{}\n");
        let before = files_under(&vfs, Path::new("/repo"));

        let err = export_skills(&vfs, &check_of(&request)).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused { .. }), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        assert_eq!(files_under(&vfs, Path::new("/repo")), before);
    }

    /// The case the fixture above does not reach, and the one that matters:
    /// a repository that already *is* a marketplace. Its manifest sits at the
    /// path this build renders one to, so every rendered-path test says
    /// "ours" and the swap takes it without a word. Provenance is what tells
    /// them apart.
    #[test]
    fn a_hand_written_marketplace_at_the_same_path_is_refused_not_replaced() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        let manifest = "{\n  \"name\": \"hand\",\n  \"plugins\": []\n}\n";
        write(
            &vfs,
            "/repo/dist/house/.claude-plugin/marketplace.json",
            manifest,
        );
        write(
            &vfs,
            "/repo/dist/house/.claude-plugin/packs/hand/skills/by-hand/SKILL.md",
            "---\nname: by-hand\n---\n\nWritten by a person.\n",
        );

        let err = export_skills(&vfs, &request).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused { .. }), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        // Not one byte of it moved.
        assert_eq!(
            vfs.read_file(Path::new(
                "/repo/dist/house/.claude-plugin/marketplace.json"
            ))
            .unwrap(),
            manifest.as_bytes()
        );
        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.claude-plugin/packs/hand/skills/by-hand/SKILL.md"
        )));
    }

    /// The residue, pinned so it is a decision rather than a surprise. Only a
    /// `marketplace.json`, with no marker to read, is claimed as ours on the
    /// strength of the path alone, because a commands-only plugin leaves
    /// exactly that for the harness whose plugin format carries no commands.
    /// A hand-written Claude marketplace laid out the documented way looks
    /// identical, and is replaced. See [`DestTree::built_here`].
    #[test]
    fn a_lone_manifest_is_claimed_on_its_path_alone() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        write(
            &vfs,
            "/repo/dist/house/.claude-plugin/marketplace.json",
            "{\n  \"name\": \"someone-elses\",\n  \"plugins\": []\n}\n",
        );

        // No refusal: the one file present is a path this render produces.
        let report = export_skills(&vfs, &request).unwrap();
        assert!(report.changed);
    }

    #[test]
    fn a_harness_that_renders_nothing_loses_only_its_packs() {
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        export_skills(&vfs, &request).unwrap();
        write(&vfs, "/repo/dist/house/.pi/settings.json", "{}\n");
        vfs.remove(Path::new("/repo/marketplaces/house/plugins/tools"))
            .unwrap();
        write(
            &vfs,
            "/repo/marketplaces/house/plugins/gear/commands/run/run.md",
            "---\ndescription: Run.\n---\n\nBody.\n",
        );
        request.options.harnesses = BTreeSet::from(["pi".to_owned()]);

        export_skills(&vfs, &request).unwrap();
        assert!(!vfs.exists(Path::new("/repo/dist/house/.pi/packs")));
        assert!(vfs.exists(Path::new("/repo/dist/house/.pi/settings.json")));
    }

    /// A link above an owned path would carry the swap out of the directory.
    #[cfg(unix)]
    #[test]
    fn a_link_above_an_owned_path_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let guide = root.join("vault/marketplaces/house/plugins/tools/skills/guide/guide.md");
        write(&NativeVfs, guide.to_str().unwrap(), &note("guide"));
        let out = root.join("vault/dist/house");
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, out.join(".agents")).unwrap();

        let mut request = request(ExportMode::Build);
        request.vault_root = root.join("vault");
        let message = export_skills(&NativeVfs, &request).unwrap_err().to_string();

        assert!(message.contains("foreign-tree"), "{message}");
        assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn a_tree_that_built_here_before_is_not_mistaken_for_a_foreign_one() {
        // Every rebuild finds its tree occupied. What tells the two cases
        // apart is that a previous build's manifest is still there, at a path
        // this render names — even after every pack has been renamed.
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        export_skills(&vfs, &request).unwrap();

        vfs.remove(Path::new("/repo/marketplaces/house/plugins/tools"))
            .unwrap();
        write(
            &vfs,
            "/repo/marketplaces/house/plugins/gear/skills/review-guide/review-guide.md",
            &note("review-guide"),
        );

        let report = export_skills(&vfs, &request).unwrap();
        assert!(report.changed);
        assert!(vfs.exists(Path::new(
            "/repo/dist/house/.claude-plugin/packs/gear/.claude-plugin/plugin.json"
        )));
        assert!(!vfs.exists(Path::new("/repo/dist/house/.claude-plugin/packs/tools")));
    }

    /// Build `/repo`, then rewrite the generated-file marker in every built
    /// file to the one an earlier release of the tool wrote.
    fn built_tree_with_an_earlier_marker(vfs: &MemVfs, request: &VaultRequest) {
        export_skills(vfs, request).unwrap();
        for (relative, bytes, _) in files_under(vfs, Path::new("/repo/dist/house")) {
            let text = String::from_utf8_lossy(&bytes)
                .replace(GENERATED_MARKER, "<!-- Generated by tradecraft from ");
            write(vfs, &format!("/repo/dist/house/{relative}"), &text);
        }
    }

    #[test]
    fn a_tree_with_an_earlier_marker_and_the_same_file_set_is_rebuilt() {
        // No marker counts as ours, but every file is at a path this render
        // produces, so it is still claimed.
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        built_tree_with_an_earlier_marker(&vfs, &request);

        let report = export_skills(&vfs, &request).unwrap();
        assert!(report.changed);
    }

    #[test]
    fn packs_with_an_earlier_marker_and_an_extra_file_are_refused() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        built_tree_with_an_earlier_marker(&vfs, &request);
        let stray = "/repo/dist/house/.claude-plugin/packs/tools/stray.txt";
        write(&vfs, stray, "left over\n");

        let err = export_skills(&vfs, &request).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused { .. }), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        assert!(vfs.exists(Path::new(stray)));
    }

    /// Two marketplaces, each holding a `core` plugin holding a `review`:
    /// legal in the vault, and one output root cannot hold both.
    fn two_marketplaces() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/acme/plugins/core/skills/review/review.md",
            &note("review"),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/skills/review/review.md",
            &note("review"),
        );
        vfs
    }

    /// A vault that names its own marketplace and its own pack.
    fn vault_with_container_notes() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/demo-market/demo-market.md",
            "---\ndescription: The house marketplace.\n\
             owner:\n  name: kelly/house\nauthor: house-team\n\
             display_name: The House\n---\n\nProse.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/tools.md",
            "---\ndescription: Everyday tools.\nversion: 2.1.0\nlicense: MIT\nkeywords:\n  - editing\n\
             category: Writing\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/skills/review/review.md",
            &note("review"),
        );
        vfs
    }

    /// The whole point: a vault that says who owns it publishes that, and a
    /// build flag is not where a marketplace's identity lives.
    #[test]
    fn the_marketplace_note_supplies_the_identity_the_config_used_to() {
        let vfs = vault_with_container_notes();
        build(&vfs);

        let manifest = text_at(
            &vfs,
            "/vault/dist/demo-market/.claude-plugin/marketplace.json",
        );
        assert!(manifest.contains(r#""name": "demo-market""#), "{manifest}");
        assert!(manifest.contains(r#""name": "kelly/house""#), "{manifest}");

        // Codex takes the display name; Claude's dialect has no field for it.
        let codex = text_at(
            &vfs,
            "/vault/dist/demo-market/.agents/plugins/marketplace.json",
        );
        assert!(codex.contains("The House"), "{codex}");
    }

    /// The vault behind every license scenario: a marketplace note whose
    /// `license` line is the variable, one plugin that says nothing about
    /// its license, and one that licenses itself differently.
    fn vault_with_licenses(market_license: Option<&str>) -> MemVfs {
        let vfs = MemVfs::new();
        let line = market_license
            .map(|spdx| format!("license: {spdx}\n"))
            .unwrap_or_default();
        write(
            &vfs,
            "/vault/marketplaces/demo-market/demo-market.md",
            &format!(
                "---\ndescription: The house marketplace.\n\
                 {line}---\n"
            ),
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/skills/review/review.md",
            &note("review"),
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/vendor/vendor.md",
            "---\ndescription: Vendored work.\nlicense: Apache-2.0\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/vendor/skills/other/other.md",
            &note("other"),
        );
        vfs
    }

    /// The license one pack manifest published.
    fn license_of(vfs: &dyn Vfs, plugin: &str) -> String {
        let manifest = text_at(
            vfs,
            &format!(
                "/vault/dist/demo-market/.claude-plugin/packs/{plugin}/.claude-plugin/plugin.json"
            ),
        );
        serde_json::from_str::<serde_json::Value>(&manifest)
            .unwrap_or_else(|err| panic!("{plugin} manifest is not JSON: {err}\n{manifest}"))["license"]
            .as_str()
            .unwrap_or_else(|| panic!("{plugin} manifest carries no license\n{manifest}"))
            .to_owned()
    }

    /// The point of the rung: a distribution licensed as a whole says so once.
    /// Before it existed, a note that licensed itself still published
    /// `UNLICENSED` into every pack manifest it shipped.
    #[test]
    fn a_plugin_inherits_the_marketplace_note_license() {
        let vfs = vault_with_licenses(Some("MIT"));
        build(&vfs);
        assert_eq!(license_of(&vfs, "tools"), "MIT");
    }

    /// Inheritance is a default, not a policy: one plugin under a differently
    /// licensed marketplace is the case a vendored dependency makes.
    #[test]
    fn a_plugin_licensing_itself_overrides_its_marketplace() {
        let vfs = vault_with_licenses(Some("MIT"));
        build(&vfs);
        assert_eq!(license_of(&vfs, "vendor"), "Apache-2.0");
    }

    /// Nothing anywhere still means the documented default, so adding the rung
    /// did not quietly license an unlicensed distribution.
    #[test]
    fn a_silent_vault_and_a_silent_project_file_stay_unlicensed() {
        let vfs = vault_with_licenses(None);
        build(&vfs);
        assert_eq!(license_of(&vfs, "tools"), "UNLICENSED");
        // The plugin that licensed itself is unaffected by the marketplace's
        // silence — its own note was always the first rung.
        assert_eq!(license_of(&vfs, "vendor"), "Apache-2.0");
    }

    /// A vault with no marketplace note at all still has somewhere to say it.
    #[test]
    fn the_project_file_licenses_a_marketplace_no_note_licenses() {
        let vfs = vault_with_licenses(None);
        build_with(&vfs, "license: MIT");
        assert_eq!(license_of(&vfs, "tools"), "MIT");
    }

    /// The vault is the source of truth the root file falls back to, the
    /// same precedence `owner` and `author` already follow.
    #[test]
    fn the_marketplace_note_beats_the_root_file() {
        let vfs = vault_with_licenses(Some("MIT"));
        build_with(&vfs, "license: GPL-3.0-only");
        assert_eq!(license_of(&vfs, "tools"), "MIT");
    }

    /// The note's `description` reached the pack manifests and stopped there,
    /// so a marketplace that described itself published an index that did not
    /// — `claude plugin validate` warns about the absence.
    #[test]
    fn a_marketplace_note_describes_its_own_index() {
        let vfs = vault_with_container_notes();
        build(&vfs);

        // Claude takes a top-level `description`; its documentation prefers
        // that to `metadata.description`, which it keeps for compatibility.
        let claude = text_at(
            &vfs,
            "/vault/dist/demo-market/.claude-plugin/marketplace.json",
        );
        assert!(
            claude.contains(r#""description": "The house marketplace.""#),
            "{claude}"
        );

        // Cursor documents no top-level description — `metadata` is the
        // object holding it.
        let cursor = text_at(
            &vfs,
            "/vault/dist/demo-market/.cursor-plugin/marketplace.json",
        );
        assert!(
            cursor.contains(r#""metadata": {"#) && cursor.contains("The house marketplace."),
            "{cursor}"
        );

        // Codex documents no description field at all, so it gets none rather
        // than one written to a key invented for the occasion.
        let codex = text_at(
            &vfs,
            "/vault/dist/demo-market/.agents/plugins/marketplace.json",
        );
        assert!(!codex.contains("The house marketplace."), "{codex}");
    }

    /// `new marketplace --owner kelly` writes `owner: kelly`, a bare
    /// string, and Claude rejects it outright — `owner: Invalid input`. The
    /// note's spelling is a convenience; the object is the manifest's shape.
    #[test]
    fn a_bare_owner_name_becomes_the_object_both_dialects_require() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/demo-market/demo-market.md",
            "---\ndescription: The house marketplace.\n\
             owner: kelly\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/skills/review/review.md",
            &note("review"),
        );
        build(&vfs);

        for tree in [
            ".claude-plugin/marketplace.json",
            ".cursor-plugin/marketplace.json",
        ] {
            let manifest = text_at(&vfs, &format!("/vault/dist/demo-market/{tree}"));
            assert!(
                manifest.contains("\"owner\": {\n    \"name\": \"kelly\"\n  }"),
                "{tree}: {manifest}"
            );
        }
    }

    /// A plugin note describes its own pack, in the same `description` a skill
    /// note uses for itself — and the generated `Skills: …` list is the
    /// fallback for a pack with no note, not an override of one.
    #[test]
    fn the_plugin_note_supplies_its_manifest_rather_than_the_generated_default() {
        let vfs = vault_with_container_notes();
        build(&vfs);

        let manifest = text_at(
            &vfs,
            "/vault/dist/demo-market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        );
        assert!(
            manifest.contains(r#""description": "Everyday tools.""#),
            "{manifest}"
        );
        assert!(!manifest.contains("Skills: review"), "{manifest}");
        assert!(manifest.contains(r#""version": "2.1.0""#), "{manifest}");
        assert!(manifest.contains(r#""license": "MIT""#), "{manifest}");
        assert!(manifest.contains("editing"), "{manifest}");
        // The author falls through the marketplace note, which is the only
        // place it was said — a bare string in the note, promoted to the
        // object Claude's schema requires.
        assert!(manifest.contains(r#""author": {"#), "{manifest}");
        assert!(manifest.contains(r#""name": "house-team""#), "{manifest}");

        let codex = text_at(
            &vfs,
            "/vault/dist/demo-market/.agents/packs/tools/.codex-plugin/plugin.json",
        );
        assert!(codex.contains("Writing"), "{codex}");
    }

    /// An author already shaped as an object passes through untouched,
    /// `email` included — normalization only promotes a bare string.
    #[test]
    fn an_object_author_passes_through_with_its_email() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/git-tools.md",
            "---\nauthor: {name: Ada, email: ada@example.com}\n---\n",
        );
        build(&vfs);

        let plugin = text_at(
            &vfs,
            "/vault/dist/house/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
        );
        assert!(plugin.contains(r#""author": {"#), "{plugin}");
        assert!(plugin.contains(r#""name": "Ada""#), "{plugin}");
        assert!(plugin.contains(r#""email": "ada@example.com""#), "{plugin}");
    }

    /// A marketplace note that sets no `name`. The folder names the
    /// distribution: a marketplace called `house` on disk needs no second
    /// place to say so.
    #[test]
    fn a_marketplace_is_named_by_its_folder() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review/review.md",
            &note("review"),
        );
        build(&vfs);
        let manifest = text_at(&vfs, "/vault/dist/house/.claude-plugin/marketplace.json");
        assert!(manifest.contains(r#""name": "house""#), "{manifest}");
    }

    /// A load of one marketplace from a vault of several must name it.
    #[test]
    fn two_marketplaces_are_refused_rather_than_flattened_onto_one_tree() {
        let vfs = two_marketplaces();
        let err = crate::model::load(&vfs, &crate::model::LoadRequest::new("/vault"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("ambiguous-marketplace"), "{err}");
        assert!(err.contains("`acme`"), "{err}");
        assert!(err.contains("`beta`"), "{err}");
    }

    #[test]
    fn naming_a_marketplace_builds_that_one_and_leaves_the_other_out() {
        let vfs = two_marketplaces();
        let request = request_for("acme", ExportMode::Build);
        let report = export_skills(&vfs, &request).unwrap();
        assert_eq!(report.checked, 1);
        assert!(
            text_at(
                &vfs,
                "/vault/dist/acme/.claude-plugin/packs/core/skills/review/SKILL.md"
            )
            .contains("Use review.")
        );
    }

    /// A typo must not quietly load or build something else.
    #[test]
    fn an_unknown_marketplace_is_refused_naming_the_ones_there_are() {
        let vfs = two_marketplaces();
        let mut load = crate::model::LoadRequest::new("/vault");
        load.scope = BuildScope::Marketplace("gamma".to_owned());
        let err = crate::model::load(&vfs, &load).unwrap_err().to_string();
        assert!(err.contains("unknown-marketplace"), "{err}");
        assert!(err.contains("it holds `acme` and `beta`"), "{err}");
        let request = request_for("gamma", ExportMode::Build);
        let err = export_skills(&vfs, &request).unwrap_err().to_string();
        assert!(err.contains("unknown-marketplace"), "{err}");
        assert!(err.contains("it holds `acme`, `beta`"), "{err}");
    }

    /// A marketplace whose one plugin holds only an agent builds the plugin's
    /// manifests, and names the agent as carried by no root rather than
    /// dropping it.
    #[test]
    fn a_marketplace_holding_only_an_agent_builds_and_names_it_uncarried() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/acme/plugins/core/skills/review/review.md",
            &note("review"),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/agents/reviewer/reviewer.md",
            &note("reviewer"),
        );

        let request = request_for("beta", ExportMode::Build);
        let report = export_skills(&vfs, &request).unwrap();
        assert!(vfs.exists(Path::new(
            "/vault/dist/beta/.claude-plugin/packs/core/.claude-plugin/plugin.json"
        )));
        assert!(!vfs.exists(Path::new(
            "/vault/dist/beta/.claude-plugin/packs/core/agents"
        )));
        assert!(report.units.iter().any(|unit| matches!(
            unit,
            ExportUnit::NotCarried { root: None, path, .. }
                if path.ends_with("agents/reviewer/reviewer.md")
        )));
    }

    /// A plugin with no skills at all, only a command, still builds — on the
    /// Claude root, which has a place for one — and still gets a marketplace
    /// index entry, or nothing could ever install it.
    #[test]
    fn a_commands_only_plugin_still_gets_a_marketplace_entry() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship/ship.md",
            &note("ship"),
        );

        let request = request_for("beta", ExportMode::Build);
        let report = export_skills(&vfs, &request).unwrap();
        assert_eq!(report.mode, ExportMode::Build);
        assert!(
            vfs.exists(Path::new(
                "/vault/dist/beta/.claude-plugin/packs/core/commands/ship.md"
            )),
            "the command file itself"
        );
        assert!(
            vfs.exists(Path::new(
                "/vault/dist/beta/.claude-plugin/packs/core/.claude-plugin/plugin.json"
            )),
            "the plugin still gets a manifest"
        );
        let marketplace_manifest =
            text_at(&vfs, "/vault/dist/beta/.claude-plugin/marketplace.json");
        assert!(
            marketplace_manifest.contains("\"core\""),
            "the plugin is listed even with no skills: {marketplace_manifest}"
        );
        // Codex has no place for a command, so the plugin is absent from
        // that root entirely: no directory, no manifest, no listing.
        assert!(
            !vfs.exists(Path::new("/vault/dist/beta/.agents/packs/core")),
            "codex has no layout for a commands-only plugin"
        );
    }

    /// A command's embed of a plugin file is a link to where the file ships
    /// in the pack: nothing is copied beside the command.
    #[test]
    fn a_command_embedding_a_plugin_image_links_it() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship/ship.md",
            "---\ndescription: Cuts a release.\n---\n\n             Cut it.\n\n![[team-logo.png]]\n",
        );
        let references = Path::new("/vault/marketplaces/beta/plugins/core/references");
        vfs.create_dir_all(references).unwrap();
        vfs.write_file(&references.join("team-logo.png"), b"PNG")
            .unwrap();

        let request = request_for("beta", ExportMode::Build);
        export_skills(&vfs, &request).unwrap();
        let ship = text_at(
            &vfs,
            "/vault/dist/beta/.claude-plugin/packs/core/commands/ship.md",
        );
        assert!(
            ship.contains("![team-logo.png](../references/team-logo.png)"),
            "{ship}"
        );
    }

    /// A plugin carrying no skills is still a plugin at a root that has a
    /// place for its commands: the diff admits it, so a build reports it and
    /// a following check does not call its own correct output stale.
    #[test]
    fn a_commands_only_plugin_is_reported_and_never_stale() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship/ship.md",
            &note("ship"),
        );

        let build = request_for("beta", ExportMode::Build);
        let report = export_skills(&vfs, &build).unwrap();
        assert!(
            report.units.iter().any(|unit| matches!(
                unit,
                ExportUnit::Plugin { root, name, .. } if *root == Harness::Claude && name == "core"
            )),
            "the build reports the plugin: {:?}",
            report.units
        );

        let checked = export_skills(&vfs, &request_for("beta", ExportMode::Check)).unwrap();
        assert!(!checked.changed, "{:?}", checked.units);
        assert!(
            !checked
                .units
                .iter()
                .any(|unit| matches!(unit, ExportUnit::Stale { .. })),
            "nothing is stale: {:?}",
            checked.units
        );
    }

    /// A hook note is not a component: `hooks/` holds `hooks.json` only, so
    /// the build and the check refuse the layout before writing anything.
    #[test]
    fn a_hook_note_is_refused_as_a_layout_error() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/skills/review/review.md",
            &note("review"),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/lint-on-write.md",
            "---\ndescription: Lints.\nevent: PreToolUse\n---\n\njust lint\n",
        );

        for mode in [ExportMode::Check, ExportMode::Build] {
            let request = request_for("beta", mode);
            let message = export_skills(&vfs, &request).unwrap_err().to_string();
            assert!(message.contains("refused: layout"), "{message}");
            assert!(message.contains("hooks/lint-on-write.md"), "{message}");
        }
        assert!(!vfs.exists(Path::new("/vault/dist/beta")));
    }

    /// A plugin with both a skill and a command: the Claude root carries
    /// both, the Codex root carries the skill alone — its own plugin format
    /// simply has no place for the command.
    #[test]
    fn a_mixed_plugin_carries_its_command_only_where_the_root_has_a_layout() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/skills/review/review.md",
            &note("review"),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship/ship.md",
            &note("ship"),
        );

        let request = request_for("beta", ExportMode::Build);
        export_skills(&vfs, &request).unwrap();

        assert!(vfs.exists(Path::new(
            "/vault/dist/beta/.claude-plugin/packs/core/skills/review/SKILL.md"
        )));
        assert!(vfs.exists(Path::new(
            "/vault/dist/beta/.claude-plugin/packs/core/commands/ship.md"
        )));
        assert!(vfs.exists(Path::new(
            "/vault/dist/beta/.agents/packs/core/skills/review/SKILL.md"
        )));
        assert!(
            !vfs.exists(Path::new("/vault/dist/beta/.agents/packs/core/commands")),
            "codex's own plugin has no commands/ directory at all"
        );
    }

    /// A marketplace whose one plugin holds no component builds: every root
    /// gets the marketplace manifest and the plugin's manifest, and nothing
    /// else. This is the skeleton `init` writes.
    #[test]
    fn a_marketplace_of_empty_plugins_builds_their_manifests() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/draft/draft.md",
            "---\n---\n",
        );

        let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
        assert!(report.changed);
        for (index, pack) in [
            (
                "/vault/dist/house/.claude-plugin/marketplace.json",
                "/vault/dist/house/.claude-plugin/packs/draft/.claude-plugin/plugin.json",
            ),
            (
                "/vault/dist/house/.agents/plugins/marketplace.json",
                "/vault/dist/house/.agents/packs/draft/.codex-plugin/plugin.json",
            ),
            (
                "/vault/dist/house/.cursor-plugin/marketplace.json",
                "/vault/dist/house/.cursor-plugin/packs/draft/.cursor-plugin/plugin.json",
            ),
        ] {
            let listing = text_at(&vfs, index);
            assert!(listing.contains("\"draft\""), "{index}: {listing}");
            assert!(vfs.exists(Path::new(pack)), "{pack}");
        }
        assert!(vfs.exists(Path::new("/vault/dist/house/.pi/packs/draft/package.json")));
        for root in [".claude-plugin", ".agents", ".cursor-plugin", ".pi"] {
            assert!(
                !vfs.exists(Path::new(&format!(
                    "/vault/dist/house/{root}/packs/draft/skills"
                ))),
                "{root} holds no skills"
            );
        }
    }

    /// A plugin with no component builds its manifest for every harness
    /// built.
    #[test]
    fn an_empty_plugin_builds_for_every_harness_built() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/draft/draft.md",
            "---\n---\n",
        );

        build(&vfs);
        for root in [".claude-plugin", ".agents", ".cursor-plugin", ".pi"] {
            assert!(
                vfs.exists(Path::new(&format!("/vault/dist/house/{root}/packs/draft"))),
                "{root} carries the empty plugin"
            );
        }
    }

    /// A plugin holding only agents is an empty pack, so it too builds for
    /// every harness built.
    #[test]
    fn an_agents_only_plugin_builds_for_every_harness_built() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/crew/crew.md",
            "---\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/crew/agents/reviewer/reviewer.md",
            &note("reviewer"),
        );

        let report = build(&vfs);
        for root in [".claude-plugin", ".agents", ".cursor-plugin", ".pi"] {
            assert!(
                vfs.exists(Path::new(&format!("/vault/dist/house/{root}/packs/crew"))),
                "{root} carries the agents-only plugin"
            );
        }
        assert!(report.units.iter().any(|unit| matches!(
            unit,
            ExportUnit::NotCarried { path, .. } if path.ends_with("agents/reviewer/reviewer.md")
        )));
    }

    /// A marketplace with no plugin folder has nothing to write.
    #[test]
    fn a_marketplace_with_no_plugin_is_refused_not_built() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");

        let message = export_skills(&vfs, &request(ExportMode::Build))
            .unwrap_err()
            .to_string();
        assert!(message.contains("empty-marketplace"), "{message}");
        assert!(message.contains("`house`"), "{message}");
        assert!(!vfs.exists(Path::new("/vault/dist/house")));
    }

    #[test]
    fn the_wire_forms_are_kebab_case() {
        let report = ExportReport {
            mode: ExportMode::Check,
            checked: 3,
            changed: true,
            units: vec![
                ExportUnit::Plugin {
                    root: Harness::Claude,
                    name: "tools".to_owned(),
                    skills: 2,
                    status: UnitStatus::Differs,
                    version_bump_suggestion: None,
                },
                ExportUnit::Manifest {
                    root: Harness::Codex,
                    status: UnitStatus::Same,
                },
                ExportUnit::Stale {
                    root: Harness::Claude,
                    path: "plugins/old".to_owned(),
                    kind: StaleKind::Plugin,
                },
            ],
            warnings: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"mode":"check","checked":3,"changed":true,"units":[{"unit":"plugin","root":"claude","name":"tools","skills":2,"status":"differs","version_bump_suggestion":null},{"unit":"manifest","root":"codex","status":"same"},{"unit":"stale","root":"claude","path":"plugins/old","kind":"plugin"}],"warnings":[]}"#
        );
        // And a request with no mode means build, the write.
        let request: VaultRequest = serde_json::from_str(r#"{"vault_root":"/v"}"#).unwrap();
        assert_eq!(request.options.mode, ExportMode::Build);
    }

    /// A build that quietly omitted an agent would be the failure this crate
    /// is built around. It has no place for one — a pack declares only
    /// `skills/` — so it names it instead, once, with no root.
    #[test]
    fn a_component_no_pack_can_hold_is_named_rather_than_dropped() {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/house/plugins/core";
        write(
            &vfs,
            &format!("{base}/skills/review/review.md"),
            "---\ndescription: Reviews.\n---\n\nRead it.\n",
        );
        write(
            &vfs,
            &format!("{base}/agents/triage/triage.md"),
            "---\ndescription: Triages.\n---\n\nSort it.\n",
        );

        let report = check(&vfs);

        let named: Vec<&ExportUnit> = report
            .units
            .iter()
            .filter(|u| matches!(u, ExportUnit::NotCarried { .. }))
            .collect();
        assert_eq!(named.len(), 1, "{:?}", report.units);
        let ExportUnit::NotCarried { root, path, kind } = named[0] else {
            unreachable!()
        };
        assert_eq!(*root, None);
        assert_eq!(
            path,
            "marketplaces/house/plugins/core/agents/triage/triage.md"
        );
        assert_eq!(*kind, EntityKind::Agent);
        // And the skill still built.
        assert!(
            report
                .units
                .iter()
                .any(|u| matches!(u, ExportUnit::Plugin { .. })),
            "{:?}",
            report.units
        );
    }

    /// A command on a root whose pack format has no place for one is named
    /// on that root, not dropped from it. The root that carries it reports
    /// nothing extra.
    #[test]
    fn a_command_a_root_cannot_carry_is_named_on_that_root() {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/house/plugins/core";
        write(
            &vfs,
            &format!("{base}/skills/review/review.md"),
            &note("review"),
        );
        write(
            &vfs,
            &format!("{base}/commands/ship/ship.md"),
            &note("ship"),
        );

        let report = check(&vfs);

        let named: Vec<(Option<&str>, &str, &str)> = report
            .units
            .iter()
            .filter_map(|u| match u {
                ExportUnit::NotCarried { root, path, kind } => {
                    Some((root.map(Harness::as_str), path.as_str(), kind.as_str()))
                }
                _ => None,
            })
            .collect();
        let ship = "marketplaces/house/plugins/core/commands/ship/ship.md";
        assert_eq!(
            named,
            vec![
                (Some("codex"), ship, "command"),
                (Some("cursor"), ship, "command"),
                (Some("pi"), ship, "command"),
            ],
            "{:?}",
            report.units
        );
    }

    /// A recursive copy. `fs::copy` keeps each file's permissions.
    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    /// The committed fixture, end to end on a real filesystem: a
    /// bundled skill with an embed, built into every root and clean on the
    /// check that follows. A copy, since a build writes the vault's `dist/`.
    #[test]
    fn the_committed_fixture_builds_on_a_real_filesystem() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent-skills-vault");
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        copy_tree(&fixture, &vault);
        let request = VaultRequest::new(vault);

        let report = export_skills(&NativeVfs, &request).unwrap();
        assert_eq!(report.checked, 1);
        assert!(report.changed);

        let skill = dir
            .path()
            .join("vault/dist/house/.claude-plugin/packs/tools/skills/workflow-checklist");
        let text = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert!(text.contains("name: workflow-checklist"), "{text}");
        assert!(
            text.contains(
                "marketplaces/house/plugins/tools/skills/workflow-checklist/workflow-checklist.md; \
                 do not edit."
            ),
            "{text}"
        );
        // The embed inlined its body; the bundle file still travels.
        assert!(text.contains("Understand the requested outcome."), "{text}");
        assert!(!text.contains("![["), "{text}");
        assert!(skill.join("references/steps.md").exists());
        assert!(
            dir.path()
                .join("vault/dist/house/.agents/packs/tools/skills/workflow-checklist/SKILL.md")
                .exists()
        );

        let check = check_of(&request);
        let report = export_skills(&NativeVfs, &check).unwrap();
        assert!(!report.changed, "{report:?}");
    }

    /// Unset identity fields are absent on the wire, not `null`.
    #[test]
    fn an_empty_identity_serializes_to_an_empty_object() {
        assert_eq!(
            serde_json::to_value(ContainerIdentity::default()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn an_undescribed_command_is_refused_by_name() {
        let vfs = silent_plugin_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/commands/ship/ship.md",
            "---\n---\n",
        );
        for mode in [ExportMode::Check, ExportMode::Build] {
            let message = export_skills(&vfs, &request(mode)).unwrap_err().to_string();
            assert!(message.contains("missing-description"), "{message}");
            assert!(message.contains("commands/ship/ship.md"), "{message}");
        }
        assert!(!vfs.exists(Path::new("/vault/dist/house")));
    }

    #[test]
    fn check_warns_when_the_changelog_lacks_the_version() {
        let vfs = example_vault();
        let folder = "/vault/marketplaces/house/plugins/tools";
        write(&vfs, &format!("{folder}/CHANGELOG.md"), "## 0.9.0\n");
        write(
            &vfs,
            &format!("{folder}/tools.md"),
            "---\ndescription: Tools.\nversion: 1.0.0\n---\n",
        );
        export_skills(&vfs, &request(ExportMode::Build)).unwrap();
        let report = export_skills(&vfs, &request(ExportMode::Check)).unwrap();
        let warned: Vec<&str> = report.warnings.iter().map(|w| w.reason.as_str()).collect();
        assert!(warned.contains(&"changelog-missing-version"), "{warned:?}");
    }
}
