//! The three questions every verb asks before it does anything: which root
//! file, which frontmatter namespace, which vault.
//!
//! Said once here rather than at the top of each command, so a second copy
//! in a command module cannot drift on `--config` precedence.
//!
//! Nothing here decides anything the core decides. The namespace rule is
//! asked of [`tradecraft_core::Namespace`]; this module only knows *when*
//! to ask, and what to say about where the offending value came from —
//! which is the one thing the core cannot know.

use std::io::Write;
use std::path::{Path, PathBuf};

use tradecraft_core::Namespace;

use crate::config::{
    ROOT_FILE, RootFile, RootLoad, WORKSPACE_DIR, find_root_file, is_convention_vault,
    print_warnings, read_root_file, resolve_against_cwd, resolve_configured_path, user_skills_dir,
    workspace_above, workspace_members,
};
use crate::exit::{blocked, no_config};

/// The globals every verb declares, as far as this module cares.
#[derive(Debug, Clone, Default)]
pub struct CommonFlags {
    /// `--vault`.
    pub vault: Option<String>,
    /// `--config`.
    pub config: Option<String>,
    /// `--namespace`.
    pub namespace: Option<String>,
}

/// What a run resolved to, once the three questions are answered.
#[derive(Debug, Clone)]
pub struct ResolvedRun {
    /// The root file, with a workspace's shared defaults folded in. Every
    /// default when the vault has no root file.
    pub config: RootFile,
    /// The folder relative paths inside the root file resolve against: the
    /// vault.
    pub config_dir: PathBuf,
    /// The root file that was read, or `None` when the vault has none.
    pub config_path: Option<PathBuf>,
    /// The frontmatter namespace for this run, validated at [`resolve_run`].
    /// Absent means the core's `sw`.
    pub namespace: Option<Namespace>,
    /// The vault root: the folder holding the root file.
    pub vault: PathBuf,
}

impl ResolvedRun {
    /// This run's namespace as the core takes it.
    pub fn core_namespace(&self) -> Namespace {
        self.namespace.clone().unwrap_or_default()
    }

    /// Whether this run's root file is a workspace's.
    pub fn is_workspace(&self) -> bool {
        self.config.workspace
    }

    /// The root folder's own name: the marketplace's name when the root file
    /// gives none.
    pub fn folder_name(&self) -> String {
        self.vault
            .canonicalize()
            .unwrap_or_else(|_| self.vault.clone())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Where `publish` and `status` look when no `--to` is given:
    /// `build.destinations`, else each harness's user skill folder for the
    /// harnesses this vault builds for.
    pub fn destinations(&self) -> Vec<(PathBuf, String)> {
        if !self.config.build.destinations.is_empty() {
            return self
                .config
                .build
                .destinations
                .iter()
                .map(|d| {
                    (
                        resolve_configured_path(&d.root, &self.config_dir),
                        d.target.clone(),
                    )
                })
                .collect();
        }
        self.user_skill_folders()
            .into_iter()
            .map(|(dir, name)| (dir, name.to_owned()))
            .collect()
    }

    /// Each harness's user skill folder, for the harnesses this vault builds
    /// for (every one when `build.harnesses` is absent).
    pub fn user_skill_folders(&self) -> Vec<(PathBuf, &'static str)> {
        let wanted = self.config.harness_set();
        tradecraft_core::harness::with_plugin_format()
            .filter(|profile| wanted.is_empty() || wanted.contains(profile.name))
            .map(|profile| {
                (
                    user_skills_dir(profile.format().user_skills_dir),
                    profile.name,
                )
            })
            .collect()
    }

    /// The marketplace's name: the root file's `name`, else the folder's.
    pub fn marketplace_name(&self) -> String {
        self.root_config().name.unwrap_or_default()
    }

    /// The root file as a config: the top of the cascade.
    pub fn root_config(&self) -> tradecraft_core::MarketplaceConfig {
        self.config.to_config(&self.folder_name())
    }
}

/// Load the root file, validate the namespace, and resolve the vault —
/// writing any failure to `err` and handing back the exit code it earned.
///
/// The namespace is checked HERE, once, at load: a stem the core refuses is
/// a fault in the configuration, not in any note. It is a run-level
/// blocker: exit 2, nothing attempted. Not exit 4 — that means "no vault
/// here", and the person standing in one is told what is wrong with it
/// instead.
pub fn resolve_run(
    flags: &CommonFlags,
    cwd: &Path,
    err: &mut dyn Write,
) -> Result<ResolvedRun, i32> {
    let (vault, file) = if let Some(flag) = &flags.config {
        let path = resolve_against_cwd(flag, cwd);
        // Existence, not "is a file": a folder by that name is there, and
        // reading it is the failure.
        if !path.exists() {
            return Err(no_config(
                err,
                &format!(
                    "--config names a file that is not there: {}",
                    path.display()
                ),
            ));
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        (dir, Some(path))
    } else if let Some(flag) = &flags.vault {
        let dir = resolve_against_cwd(flag, cwd);
        if !dir.is_dir() {
            return Err(blocked(err, &format!("vault not found: {}", dir.display())));
        }
        let file = dir.join(ROOT_FILE);
        let file = file.is_file().then_some(file);
        (dir, file)
    } else if let Some(file) = find_root_file(cwd) {
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        (dir, Some(file))
    } else if is_convention_vault(cwd) {
        let _ = writeln!(
            err,
            "note: no {ROOT_FILE} at or above {}; reading it with every default. Add a {ROOT_FILE} to name the marketplace and set its fields.",
            cwd.display()
        );
        (cwd.to_path_buf(), None)
    } else {
        return Err(no_config(
            err,
            &format!(
                "No vault: no `{ROOT_FILE}` at or above {}, and it holds no `plugins/` or `skills/` folder.\n\
                 A vault with no `{ROOT_FILE}` is found only from its own root. Run from there, pass --vault, or add a `{ROOT_FILE}` at the vault's root.",
                cwd.display()
            ),
        ));
    };

    let mut config = match &file {
        Some(path) => load_root(path, err)?,
        None => RootFile::default(),
    };
    if !config.workspace
        && let Some(shared) = workspace_above(&vault)
    {
        let workspace = load_root(&shared, err)?;
        config.inherit(&workspace);
    }

    // The flag overrides the file for this run and is never written back —
    // the same precedence `--vault` and `--config` already follow.
    let stem = flags
        .namespace
        .clone()
        .or_else(|| config.build.namespace.clone());
    let mut namespace: Option<Namespace> = None;
    if let Some(stem) = &stem {
        match Namespace::new(stem) {
            Ok(valid) => namespace = Some(valid),
            Err(invalid) => {
                // The core's message, verbatim — it already names the stem and
                // the reason — plus the one thing the core cannot know: where
                // it came from.
                let source = if flags.namespace.is_some() {
                    "--namespace".to_string()
                } else {
                    format!(
                        "`build.namespace` in {}",
                        file.as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| ROOT_FILE.to_string())
                    )
                };
                return Err(blocked(
                    err,
                    &format!("configuration error: {invalid}\n  set by {source}"),
                ));
            }
        }
    }

    Ok(ResolvedRun {
        config,
        config_dir: vault.clone(),
        config_path: file,
        namespace,
        vault,
    })
}

/// Read one root file, printing its warnings. A file that cannot be read or
/// parsed is a run-level blocker.
fn load_root(path: &Path, err: &mut dyn Write) -> Result<RootFile, i32> {
    match read_root_file(path) {
        RootLoad::Loaded(root) => {
            print_warnings(&root, path, err);
            Ok(*root)
        }
        RootLoad::Unreadable { detail } => Err(blocked(
            err,
            &format!("configuration unreadable: {} ({detail})", path.display()),
        )),
        RootLoad::Malformed { detail } => Err(blocked(
            err,
            &format!("configuration malformed: {} ({detail})", path.display()),
        )),
    }
}

/// The marketplaces a workspace run builds: each `marketplaces/<m>/` with
/// its own root file, resolved as a run of its own with the workspace's
/// shared defaults folded in. `only` keeps the one named `<m>`.
///
/// # Errors
///
/// The exit code of a member that does not resolve, or of an `only` that
/// names no member.
pub fn workspace_runs(
    run: &ResolvedRun,
    flags: &CommonFlags,
    only: Option<&str>,
    err: &mut dyn Write,
) -> Result<Vec<ResolvedRun>, i32> {
    let mut runs = Vec::new();
    for file in workspace_members(&run.vault) {
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let folder = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if only.is_some_and(|only| only != folder) {
            continue;
        }
        let member = CommonFlags {
            vault: None,
            config: Some(file.display().to_string()),
            namespace: flags.namespace.clone(),
        };
        runs.push(resolve_run(&member, &dir, err)?);
    }
    if runs.is_empty() {
        let message = match only {
            Some(name) => format!(
                "no marketplace `{name}` in this workspace: there is no {WORKSPACE_DIR}/{name}/{ROOT_FILE} under {}",
                run.vault.display()
            ),
            None => format!(
                "the workspace at {} holds no {WORKSPACE_DIR}/<name>/{ROOT_FILE}",
                run.vault.display()
            ),
        };
        return Err(blocked(err, &message));
    }
    Ok(runs)
}

/// Refuse a verb that acts on one marketplace when this run is a
/// workspace's.
pub fn refuse_workspace(verb: &str, run: &ResolvedRun, err: &mut dyn Write) -> Result<(), i32> {
    if !run.is_workspace() {
        return Ok(());
    }
    Err(blocked(
        err,
        &format!(
            "{verb} acts on one marketplace, and {} is a workspace. Run it from inside {WORKSPACE_DIR}/<name>/, or pass --vault {WORKSPACE_DIR}/<name>",
            run.vault.display()
        ),
    ))
}

/// The skills root a `--skills-root` or `adopt --into` flag names, resolved
/// against the working directory. `None` means the vault root.
pub fn resolve_skills_root(flag: Option<&str>, cwd: &Path) -> Option<PathBuf> {
    flag.map(|flag| resolve_against_cwd(flag, cwd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::{EXIT_BLOCKED, EXIT_NO_CONFIG};

    #[test]
    fn a_folder_with_no_root_file_and_no_convention_folder_is_no_vault() {
        let dir = tempfile::tempdir().unwrap();
        let mut err: Vec<u8> = Vec::new();
        let code = resolve_run(&CommonFlags::default(), dir.path(), &mut err).unwrap_err();
        assert_eq!(code, EXIT_NO_CONFIG);
        let message = String::from_utf8(err).unwrap();
        assert!(message.contains("No vault"), "{message}");
        assert!(message.contains(ROOT_FILE), "{message}");
    }

    #[test]
    fn a_convention_folder_builds_with_every_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Tool Box/skills")).unwrap();
        let cwd = dir.path().join("Tool Box");
        let mut err: Vec<u8> = Vec::new();
        let run = resolve_run(&CommonFlags::default(), &cwd, &mut err).unwrap();
        assert_eq!(run.vault, cwd);
        assert_eq!(run.config_path, None);
        assert_eq!(run.marketplace_name(), "tool-box");
        assert!(
            String::from_utf8(err)
                .unwrap()
                .contains("note: no skillwright.md")
        );
    }

    #[test]
    fn a_member_inherits_its_workspace_s_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let member = dir.path().join("marketplaces/one");
        std::fs::create_dir_all(member.join("skills")).unwrap();
        std::fs::write(
            dir.path().join(ROOT_FILE),
            "---\nworkspace: true\nlicense: MIT\n---\n",
        )
        .unwrap();
        std::fs::write(member.join(ROOT_FILE), "---\nname: one\n---\n").unwrap();
        let mut err: Vec<u8> = Vec::new();
        let run = resolve_run(&CommonFlags::default(), &member.join("skills"), &mut err).unwrap();
        assert_eq!(run.vault, member);
        assert_eq!(run.config.marketplace.license.as_deref(), Some("MIT"));

        let top = resolve_run(&CommonFlags::default(), dir.path(), &mut err).unwrap();
        assert!(top.is_workspace());
        let runs = workspace_runs(&top, &CommonFlags::default(), None, &mut err).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].marketplace_name(), "one");
        assert_eq!(
            refuse_workspace("publish", &top, &mut err).unwrap_err(),
            EXIT_BLOCKED
        );
    }

    #[test]
    fn an_invalid_namespace_from_a_flag_is_blocked_and_names_the_flag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("skills")).unwrap();
        let flags = CommonFlags {
            namespace: Some("my-vault".to_string()),
            ..Default::default()
        };
        let mut err: Vec<u8> = Vec::new();
        let code = resolve_run(&flags, dir.path(), &mut err).unwrap_err();
        assert_eq!(code, EXIT_BLOCKED);
        let message = String::from_utf8(err).unwrap();
        assert!(
            message.contains("configuration error: `my-vault` cannot be a frontmatter namespace:"),
            "{message}"
        );
        assert!(
            message.trim_end().ends_with("set by --namespace"),
            "{message}"
        );
    }
}
