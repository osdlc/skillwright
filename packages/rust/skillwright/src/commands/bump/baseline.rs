//! `--changed-since <ref>`: the ref's build inputs checked out into a
//! temporary folder and built there, or a built folder used as it is.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tradecraft_core::layout::MARKETPLACES_DIR;
use tradecraft_core::{
    LoadedVault, NativeVfs, ROOT_FILE, VaultRequest, build_vault, dist_dir, is_vault_root,
};

use crate::ctx::CommandContext;
use crate::exit::{refused, usage_error};
use crate::resolve::resolve_at;

/// What `--changed-since` compares against: one built marketplace in a
/// directory as it is, or a git ref checked out and built in a temporary
/// folder that is removed on drop.
pub(super) enum Baseline {
    Dir(PathBuf),
    Built(BuiltRef),
}

impl Baseline {
    /// The built tree of the marketplace `market`.
    pub(super) fn root(&self, market: &str) -> PathBuf {
        match self {
            Baseline::Dir(dir) => dir.clone(),
            Baseline::Built(built) => dist_dir(&built.base_root, market),
        }
    }

    /// A directory that exists is a built baseline; anything else is a git
    /// ref, built from the ref's own root file and `marketplaces/`.
    ///
    /// A directory holds one marketplace, so a run over several is refused.
    pub(super) fn resolve(
        since: &str,
        loaded: &LoadedVault,
        harnesses: &BTreeSet<String>,
        ctx: &mut CommandContext<'_>,
    ) -> Result<Baseline, i32> {
        let as_dir = crate::config::resolve_against_cwd(since, &ctx.cwd);
        if as_dir.is_dir() {
            if let [_, _, ..] = loaded.marketplaces.as_slice() {
                return Err(usage_error(
                    ctx.err,
                    &format!(
                        "--changed-since `{since}` is one built marketplace, and this vault holds several; name one with --marketplace"
                    ),
                ));
            }
            return Ok(Baseline::Dir(as_dir));
        }
        BuiltRef::build(since, loaded, harnesses, ctx).map(Baseline::Built)
    }
}

/// The folder under the system temp dir that holds every baseline build:
/// `<id>.lock` beside `<id>/`, the lock held for the run's lifetime.
const BASELINES_DIR: &str = "skillwright-bump-baselines";

/// The file in [`BASELINES_DIR`] locked while a run prunes and claims.
const GUARD_FILE: &str = "guard";

/// Keeps two claims in one process and one clock tick apart.
static NEXT_CLAIM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A git ref's build inputs checked out into a temporary folder, and built
/// there: each marketplace into the checkout's `dist/<m>/`. No worktree is
/// registered and the repository's own index is not touched.
pub(super) struct BuiltRef {
    temp: PathBuf,
    checkout: PathBuf,
    /// The ref's vault root inside `checkout`.
    base_root: PathBuf,
    lock_path: PathBuf,
    /// Held until drop, so a concurrent run's prune leaves this one alone.
    _lock: std::fs::File,
}

impl BuiltRef {
    /// Checks out `since`'s build inputs and builds each marketplace of
    /// `loaded` there. A failure is printed as this flag's refusal.
    fn build(
        since: &str,
        loaded: &LoadedVault,
        harnesses: &BTreeSet<String>,
        ctx: &mut CommandContext<'_>,
    ) -> Result<BuiltRef, i32> {
        Self::try_build(since, loaded, harnesses)
            .map_err(|why| refused(ctx.err, &format!("--changed-since `{since}`: {why}")))
    }

    fn try_build(
        since: &str,
        loaded: &LoadedVault,
        harnesses: &BTreeSet<String>,
    ) -> Result<BuiltRef, String> {
        let root = loaded.root.canonicalize().unwrap_or(loaded.root.clone());
        let (repo, sha) = commit_of(&root, since)?;
        let Ok(vault_rel) = root.strip_prefix(&repo).map(Path::to_path_buf) else {
            return Err(format!("{} is outside {}", root.display(), repo.display()));
        };
        let built = Self::claim(&repo, &vault_rel)?;
        let index = built.temp.join("index");
        path_set(&repo, &sha, vault_candidates(&repo, &root))
            .and_then(|set| check_out_paths(&repo, &sha, &index, &built.checkout, &set))
            .map_err(|err| format!("checking it out failed: {err}"))?;
        if !is_vault_root(&NativeVfs, &built.base_root) {
            return Err(format!(
                "{since} holds no `{ROOT_FILE}` and `{MARKETPLACES_DIR}/` at {}, so it cannot be built. \
                 Compare against a built folder instead",
                if vault_rel.as_os_str().is_empty() {
                    "the repository top".to_owned()
                } else {
                    vault_rel.display().to_string()
                }
            ));
        }
        built.build_marketplaces(loaded, harnesses)?;
        Ok(built)
    }

    /// A claimed baseline folder for `repo`, with an empty `checkout/` in
    /// it, the vault at `vault_rel` inside that.
    fn claim(repo: &Path, vault_rel: &Path) -> Result<BuiltRef, String> {
        let baselines = std::env::temp_dir().join(BASELINES_DIR);
        let (id, lock_path, lock) = claim_baseline(&baselines, repo)?;
        let temp = baselines.join(&id);
        if let Err(err) = std::fs::create_dir_all(&temp) {
            let _ = std::fs::remove_file(&lock_path);
            return Err(format!("cannot create {}: {err}", temp.display()));
        }
        // Canonical: on macOS the temp dir sits behind the `/var` link.
        let temp = temp.canonicalize().unwrap_or(temp);
        let built = BuiltRef {
            checkout: temp.join("checkout"),
            base_root: temp.join("checkout").join(vault_rel),
            temp,
            lock_path,
            _lock: lock,
        };
        // Git refuses a work tree that is not there.
        std::fs::create_dir(&built.checkout)
            .map_err(|err| format!("cannot create {}: {err}", built.checkout.display()))?;
        Ok(built)
    }

    /// Builds each marketplace of `loaded` from the checkout into its
    /// `dist/<m>/`. One the ref does not hold gets an empty folder.
    fn build_marketplaces(
        &self,
        loaded: &LoadedVault,
        harnesses: &BTreeSet<String>,
    ) -> Result<(), String> {
        let base_root = &self.base_root;
        // The ref's refusal names the temporary checkout, so it is said as
        // this flag's failure.
        let mut said: Vec<u8> = Vec::new();
        let Ok(base) = resolve_at(base_root, None, &mut said) else {
            let said = String::from_utf8_lossy(&said);
            return Err(format!("building it failed: {}", said.trim_end()));
        };
        for market in &loaded.marketplaces {
            let out = dist_dir(base_root, &market.name);
            let Some(base_market) = base.marketplace(&market.name) else {
                std::fs::create_dir_all(&out)
                    .map_err(|err| format!("cannot create {}: {err}", out.display()))?;
                continue;
            };
            let mut request = VaultRequest::for_marketplace(&base.root, &base_market.name);
            request.options.harnesses.clone_from(harnesses);
            build_vault(&NativeVfs, &request)
                .map_err(|err| format!("building it failed: {}", err.reason()))?;
        }
        Ok(())
    }
}

/// The repository top holding `root`, and the commit `since` names there.
fn commit_of(root: &Path, since: &str) -> Result<(PathBuf, String), String> {
    let top = typed_git::git()
        .dir(root)
        .args(&["rev-parse", "--show-toplevel"])
        .run()
        .map_err(|_| {
            format!(
                "is not a directory, and {} is not in a git repository",
                root.display()
            )
        })?;
    let repo = PathBuf::from(top);
    let repo = repo.canonicalize().unwrap_or(repo);
    let sha = typed_git::query::resolve_commit(&repo, since)
        .map_err(|_| format!("is neither a directory nor a commit in {}", repo.display()))?;
    Ok((repo, sha))
}

/// The path set that stands for the whole tree.
const WHOLE_TREE: &str = ".";

/// `path` relative to `top`, forward-slashed, [`WHOLE_TREE`] for `top`
/// itself. `None` for a path outside `top`, which is not a git path.
fn relative_to(top: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(top).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|part| match part {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Some(if parts.is_empty() {
        WHOLE_TREE.to_owned()
    } else {
        parts.join("/")
    })
}

/// What the build reads of the vault at `root`, relative to `repo`: the root
/// file and `marketplaces/`.
fn vault_candidates(repo: &Path, root: &Path) -> Vec<String> {
    [root.join(ROOT_FILE), root.join(MARKETPLACES_DIR)]
        .iter()
        .filter_map(|path| relative_to(repo, path))
        .collect()
}

/// What to check out for `candidates`, repo-relative paths at `sha`: the
/// candidates the ref holds, with every `.gitattributes` above them, since
/// a path checkout applies only the attributes it checks out. `[WHOLE_TREE]`
/// when a candidate is the repository top or a symlink sits in, above or
/// under one: a link can point anywhere, and the whole tree reads it as a
/// full checkout does.
///
/// # Errors
///
/// The git failure, as the line to print.
fn path_set(repo: &Path, sha: &str, candidates: Vec<String>) -> Result<Vec<String>, String> {
    let mut set: BTreeSet<String> = candidates.into_iter().collect();
    if set.contains(WHOLE_TREE) {
        return Ok(vec![WHOLE_TREE.to_owned()]);
    }
    let attributes: Vec<String> = set.iter().flat_map(|path| attribute_files(path)).collect();
    set.extend(attributes);
    // A path inside another in the set comes with it.
    let set: Vec<String> = set
        .iter()
        .filter(|path| !set.iter().any(|outer| is_under(path, outer)))
        .cloned()
        .collect();
    if has_symlink(repo, sha, &set)? {
        return Ok(vec![WHOLE_TREE.to_owned()]);
    }
    paths_at_ref(repo, sha, &set)
}

/// Whether repo-relative `path` lies strictly inside the folder `outer`.
fn is_under(path: &str, outer: &str) -> bool {
    path.strip_prefix(outer)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The `.gitattributes` in each folder above `path`, the top included.
fn attribute_files(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (0..parts.len())
        .map(|depth| match parts[..depth].join("/") {
            folder if folder.is_empty() => ".gitattributes".to_owned(),
            folder => format!("{folder}/.gitattributes"),
        })
        .collect()
}

/// `git` in `repo`, reading every pathspec as a literal path: a `*` or `[`
/// in a folder name is part of the name. `checkout` globs without it;
/// `ls-tree` reads paths literally anyway.
fn literal_git(repo: &Path) -> typed_git::GitCommand {
    let mut cmd = typed_git::git();
    cmd.dir(repo).env("GIT_LITERAL_PATHSPECS", "1");
    // Unit tests read no machine-wide git config.
    #[cfg(test)]
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd
}

/// Whether `sha` holds a symlink at, above or under any of `paths`.
/// Non-recursively, a link above a path shows as itself, since `ls-tree`
/// cannot descend through it.
fn has_symlink(repo: &Path, sha: &str, paths: &[String]) -> Result<bool, String> {
    if paths.is_empty() {
        return Ok(false);
    }
    let chain: BTreeSet<String> = paths
        .iter()
        .flat_map(|path| {
            let parts: Vec<&str> = path.split('/').collect();
            (1..=parts.len()).map(move |depth| parts[..depth].join("/"))
        })
        .collect();
    let links = |recursive: bool, paths: &mut dyn Iterator<Item = &String>| {
        let mut cmd = literal_git(repo);
        cmd.args(&["ls-tree", "-z", "--full-tree"]);
        if recursive {
            cmd.arg("-r");
        }
        cmd.arg(sha).arg("--");
        for path in paths {
            cmd.arg(path);
        }
        cmd.run()
            .map(|listing| {
                listing
                    .split('\0')
                    .any(|entry| entry.starts_with("120000 "))
            })
            .map_err(|err| err.to_string())
    };
    Ok(links(false, &mut chain.iter())? || links(true, &mut paths.iter())?)
}

/// The `candidates` that `sha` holds, as a file or a folder, in order.
fn paths_at_ref(repo: &Path, sha: &str, candidates: &[String]) -> Result<Vec<String>, String> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let mut cmd = literal_git(repo);
    cmd.args(&["ls-tree", "-z", "--name-only", "--full-tree"])
        .arg(sha)
        .arg("--");
    for candidate in candidates {
        cmd.arg(candidate);
    }
    let listing = cmd.run().map_err(|err| err.to_string())?;
    let held: Vec<&str> = listing.split('\0').collect();
    // `ls-tree` lists the deepest match, so a folder may show only as the
    // entries under it.
    Ok(candidates
        .iter()
        .filter(|candidate| {
            held.iter()
                .any(|name| name == candidate || is_under(name, candidate))
        })
        .cloned()
        .collect())
}

/// Writes `pathspecs` as they are at `sha` into `dest`, which must exist.
/// Moves no `HEAD` and registers no worktree. An empty set runs nothing,
/// since a checkout with no pathspec switches `HEAD`.
fn check_out_paths(
    repo: &Path,
    sha: &str,
    index: &Path,
    dest: &Path,
    pathspecs: &[String],
) -> Result<(), String> {
    if pathspecs.is_empty() {
        return Ok(());
    }
    let mut cmd = literal_git(repo);
    // A private index, so the repository's own is never read or written.
    cmd.env("GIT_INDEX_FILE", index.to_string_lossy())
        // LFS files stay pointers.
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args(&[
            "-c",
            // Runs none of the repository's hooks.
            "core.hooksPath=/dev/null",
            "-c",
            // `core.hooksPath` does not cover an fsmonitor command.
            "core.fsmonitor=false",
            "-c",
            // A split index writes its shared part into the repository's `.git`.
            "core.splitIndex=false",
            "-c",
            // A sparse repository does not filter the baseline.
            "core.sparseCheckout=false",
        ])
        .arg(format!("--work-tree={}", dest.to_string_lossy()))
        .args(&["checkout", "--quiet"])
        .arg(sha)
        .arg("--");
    for path in pathspecs {
        cmd.arg(path);
    }
    cmd.run().map(drop).map_err(|err| err.to_string())
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

/// Remove the baselines a killed run left behind: each `<id>.lock` no live
/// run holds, its `<id>/` folder and, for one an older version built in a
/// worktree, that worktree's registration.
fn prune_stale_baselines(baselines: &Path, repo: &Path) {
    let Ok(entries) = std::fs::read_dir(baselines) else {
        return;
    };
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
        // Removes an older run's worktree and its registration, and only that
        // one. Fails on a checkout that is not a worktree, as every new one
        // is; `remove_dir_all` does the work then.
        let _ = typed_git::worktree::remove(repo, &dir.join("checkout"));
        let _ = std::fs::remove_dir_all(&dir);
        drop(file);
        let _ = std::fs::remove_file(&path);
    }
}

impl Drop for BuiltRef {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.temp);
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

    /// Runs `git` in `repo` with no system or global config and a fixed
    /// identity. Its stdout.
    fn git(repo: &Path, args: &[&str]) -> String {
        let mut cmd = typed_git::git();
        cmd.dir(repo)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", "t@t.invalid")
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", "t@t.invalid");
        cmd.run().unwrap()
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A canonical temp folder holding a repository of `files`, committed
    /// once, and that commit's sha.
    fn repo_of(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().canonicalize().unwrap();
        for (path, text) in files {
            write(&repo.join(path), text);
        }
        git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "one"]);
        let sha = git(&repo, &["rev-parse", "HEAD"]);
        (dir, repo, sha)
    }

    /// Every file under `dir`, relative and forward-slashed, sorted.
    fn files_under(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, found);
                } else {
                    let rel = path.strip_prefix(base).unwrap();
                    found.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut found = Vec::new();
        walk(dir, dir, &mut found);
        found.sort();
        found
    }

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    /// A fresh `checkout` folder under a new temp folder, and the private
    /// index path beside it.
    fn dest() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("checkout");
        std::fs::create_dir(&dest).unwrap();
        let index = temp.path().join("index");
        (temp, dest, index)
    }

    #[test]
    fn check_out_paths_writes_only_the_named_paths() {
        let (_dir, repo, old) = repo_of(&[("a/x", "x\n"), ("b/y", "y\n"), ("c", "c\n")]);
        // The checkout is of an older commit than the index holds, so
        // writing it into the repository's own index would change it.
        std::fs::write(repo.join("a/x"), "x2\n").unwrap();
        git(&repo, &["commit", "--quiet", "-am", "two"]);
        // A modified tracked file: the repository's own index must not move.
        std::fs::write(repo.join("c"), "changed\n").unwrap();
        // The index bytes are read before any `git status` can refresh them.
        let index_before = std::fs::read(repo.join(".git/index")).unwrap();
        let config_before = std::fs::read(repo.join(".git/config")).unwrap();
        let worktrees_before = git(&repo, &["worktree", "list", "--porcelain"]);
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &old, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(
            std::fs::read(repo.join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(files_under(&dest), ["a/x"]);
        assert_eq!(std::fs::read_to_string(dest.join("a/x")).unwrap(), "x\n");
        assert_eq!(
            std::fs::read(repo.join(".git/config")).unwrap(),
            config_before
        );
        assert_eq!(
            git(&repo, &["worktree", "list", "--porcelain"]),
            worktrees_before
        );
        assert_eq!(git(&repo, &["status", "--porcelain"]), "M c");
    }

    #[cfg(unix)]
    #[test]
    fn check_out_paths_runs_no_hook() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n")]);
        let marker = repo.join("hook-ran");
        let hook = repo.join(".git/hooks/post-checkout");
        write(&hook, &format!("#!/bin/sh\ntouch '{}'\n", marker.display()));
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(files_under(&dest), ["a/x"]);
        assert!(!marker.exists(), "the post-checkout hook ran");
    }

    #[cfg(unix)]
    #[test]
    fn check_out_paths_runs_no_fsmonitor() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n")]);
        let marker = repo.join("fsmonitor-ran");
        let script = repo.join("fsmonitor.sh");
        write(
            &script,
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(
            &repo,
            &["config", "core.fsmonitor", &script.to_string_lossy()],
        );
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(files_under(&dest), ["a/x"]);
        assert!(!marker.exists(), "the fsmonitor hook ran");
    }

    #[test]
    fn check_out_paths_writes_nothing_into_a_split_index_repository() {
        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n")]);
        git(&repo, &["config", "core.splitIndex", "true"]);
        let listing = || {
            let mut names: Vec<String> = std::fs::read_dir(repo.join(".git"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        let before = listing();
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(listing(), before);
    }

    #[test]
    fn paths_at_ref_keeps_only_what_the_ref_holds() {
        // `v[1]` as a glob would match the file `v1`; `ls-tree` reads it as a
        // name, and so does the filter over its output.
        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n"), ("c", "c\n"), ("v1", "1\n")]);
        let held = paths_at_ref(&repo, &sha, &strings(&["c", "a", "missing", "v[1]"])).unwrap();
        assert_eq!(held, ["c", "a"]);
        let (_dir, repo, sha) = repo_of(&[("v[1]/f", "f\n"), ("v1", "1\n")]);
        let held = paths_at_ref(&repo, &sha, &strings(&["v[1]"])).unwrap();
        assert_eq!(held, ["v[1]"]);
    }

    #[test]
    fn an_empty_set_runs_no_checkout() {
        let (_dir, repo, sha) = repo_of(&[("c", "c\n")]);
        let head = || {
            (
                git(&repo, &["rev-parse", "HEAD"]),
                git(&repo, &["symbolic-ref", "HEAD"]),
            )
        };
        let before = head();
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &[]).unwrap();

        assert_eq!(head(), before);
        assert!(files_under(&dest).is_empty());
    }

    /// A vault: the root file, one marketplace and one plugin, under
    /// `prefix`, with `root` as the root file.
    fn vault_files(prefix: &str, root: &str) -> Vec<(String, String)> {
        vec![
            (format!("{prefix}skillwright.md"), root.to_owned()),
            (
                format!("{prefix}marketplaces/m1/m1.md"),
                "---\n---\n".to_owned(),
            ),
            (
                format!("{prefix}marketplaces/m1/plugins/p/p.md"),
                "---\nversion: 1.0.0\n---\n".to_owned(),
            ),
            (
                format!("{prefix}marketplaces/m1/plugins/p/skills/s/s.md"),
                "---\ndescription: S.\n---\n".to_owned(),
            ),
        ]
    }

    fn borrowed(files: &[(String, String)]) -> Vec<(&str, &str)> {
        files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect()
    }

    #[test]
    fn a_vault_at_the_repo_top_checks_out_only_what_the_build_reads() {
        let mut files = vault_files("", "---\n---\n");
        files.push(("unread/file".to_owned(), "u\n".to_owned()));
        let (_dir, repo, sha) = repo_of(&borrowed(&files));
        let set = path_set(&repo, &sha, vault_candidates(&repo, &repo)).unwrap();
        assert_eq!(set, ["marketplaces", "skillwright.md"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_above_the_vault_gives_the_whole_tree() {
        let files = vault_files("real/docs/vault/", "---\n---\n");
        let (_dir, repo, _) = repo_of(&borrowed(&files));
        std::os::unix::fs::symlink("real/docs", repo.join("docs")).unwrap();
        git(&repo, &["add", "docs"]);
        git(&repo, &["commit", "--quiet", "-m", "link"]);
        let sha = git(&repo, &["rev-parse", "HEAD"]);

        let root = repo.join("docs/vault");
        let set = path_set(&repo, &sha, vault_candidates(&repo, &root)).unwrap();
        assert_eq!(set, ["."]);
    }

    #[test]
    fn the_path_set_holds_ancestor_gitattributes() {
        let mut files = vault_files("sub/vault/", "---\n---\n");
        for path in [
            ".gitattributes",
            "sub/.gitattributes",
            "other/.gitattributes",
        ] {
            files.push((path.to_owned(), "*.md text\n".to_owned()));
        }
        let (_dir, repo, sha) = repo_of(&borrowed(&files));
        let root = repo.join("sub/vault");
        let set = path_set(&repo, &sha, vault_candidates(&repo, &root)).unwrap();
        assert_eq!(
            set,
            [
                ".gitattributes",
                "sub/.gitattributes",
                "sub/vault/marketplaces",
                "sub/vault/skillwright.md"
            ]
        );
    }

    #[test]
    fn a_path_with_glob_characters_is_checked_out_literally() {
        // As a glob, `v[1]` also matches the file `v1`.
        let (_dir, repo, sha) = repo_of(&[("v[1]/f", "f\n"), ("v1", "1\n")]);
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["v[1]"])).unwrap();

        assert_eq!(files_under(&dest), ["v[1]/f"]);
    }

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

    /// A prune removes only its own baselines: a worktree of the user's whose
    /// folder is missing stays registered.
    #[test]
    fn a_prune_leaves_the_users_worktrees_registered() {
        let (_dir, repo, _sha) = repo_of(&[("f", "f\n")]);
        let other = repo.join("away");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                other.to_str().unwrap(),
            ],
        );
        std::fs::remove_dir_all(&other).unwrap();
        let baselines = repo.join("baselines");
        std::fs::create_dir_all(baselines.join("stale/out")).unwrap();
        std::fs::write(baselines.join("stale.lock"), "").unwrap();

        prune_stale_baselines(&baselines, &repo);

        assert!(!baselines.join("stale").exists());
        let listed = git(&repo, &["worktree", "list", "--porcelain"]);
        assert!(listed.contains("away"), "{listed}");
    }
}
