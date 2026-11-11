//! The safety rails: what keeps a build from writing over its own sources.
//!
//! The rule: a build's output may not be, contain, or be contained by a
//! protected source tree. Every containment check is resolved through the
//! filesystem, so a symlinked alias cannot walk past it.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

use path_guard::{Guard, Resolver};

use crate::error::CoreError;
use crate::vfs::Vfs;

/// A [`Vfs`] as `path-guard`'s [`Resolver`], so every containment check reads
/// the filesystem the build writes to.
///
/// A host on the lexical [`Vfs::canonicalize`] default resolves every path,
/// existing or not, so its checks reduce to the lexical rule.
///
/// `path-guard` reads any error as "missing" or "not a link". Only some
/// errors mean that, so the rest are kept in `failure`, and [`within`] turns
/// them into a refusal instead of an answer.
struct VfsResolver<'a> {
    vfs: &'a dyn Vfs,
    failure: RefCell<Option<CoreError>>,
}

impl VfsResolver<'_> {
    /// Passes `err` on to `path-guard` when its kind is one of `expected`,
    /// and otherwise keeps the first such error as the check's failure.
    fn classify(&self, p: &Path, err: CoreError, expected: &[io::ErrorKind]) -> io::Error {
        if let CoreError::Io { source, .. } = &err
            && expected.contains(&source.kind())
        {
            return io::Error::new(source.kind(), err.to_string());
        }
        let message = err.to_string();
        self.failure.borrow_mut().get_or_insert_with(|| {
            CoreError::refused(
                "unsafe-path",
                format!(
                    "{}: cannot tell where this path leads — {message}. Nothing was written",
                    p.display()
                ),
            )
        });
        io::Error::other(message)
    }
}

impl Resolver for VfsResolver<'_> {
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
        self.vfs.canonicalize(p).map_err(|e| {
            // A missing path, or one with a file where a directory should be.
            self.classify(
                p,
                e.into(),
                &[io::ErrorKind::NotFound, io::ErrorKind::NotADirectory],
            )
        })
    }

    fn read_link(&self, p: &Path) -> io::Result<PathBuf> {
        self.vfs.read_link(p).map_err(|e| {
            // Missing, or there but not a link (`EINVAL`), or a host with no links.
            self.classify(
                p,
                e.into(),
                &[
                    io::ErrorKind::NotFound,
                    io::ErrorKind::NotADirectory,
                    io::ErrorKind::InvalidInput,
                    io::ErrorKind::Unsupported,
                ],
            )
        })
    }
}

/// Is `inner` the same directory as `outer`, or beneath it, once both are
/// resolved through `vfs`? Paths need not exist: `path-guard` resolves the
/// part that does and follows a dangling symlink to where its target lands.
/// `false` when either side passes through more than
/// `path_guard::MAX_LINK_HOPS` symlinks.
///
/// # Errors
///
/// `refused: unsafe-path` when the host fails to resolve a path for any
/// reason other than its being missing or not a link — a permission error, a
/// loop, a host that answered with nothing usable — and when either path is
/// empty or relative. The check fails closed: neither `true` nor `false`
/// would be safe for every caller.
pub(crate) fn within(vfs: &dyn Vfs, outer: &Path, inner: &Path) -> Result<bool, CoreError> {
    let resolver = VfsResolver {
        vfs,
        failure: RefCell::new(None),
    };
    let answer = Guard::new(&resolver).contains(outer, inner);
    if let Some(failure) = resolver.failure.into_inner() {
        return Err(failure);
    }
    answer.map_err(|err| {
        CoreError::refused(
            "unsafe-path",
            format!(
                "cannot compare {} with {}: {err}. Nothing was written",
                inner.display(),
                outer.display()
            ),
        )
    })
}

/// Refuses `candidate` as a destination root if it is equal to, inside, or
/// an ancestor of any protected tree.
///
/// `refused: unsafe-root`, the token the publish spec gives a
/// destination whose write paths overlap a tree that must not be written to.
///
/// Ancestor refusal is what makes registering a repo ROOT impossible while
/// any tree inside it is protected — the publish paths would be computed
/// under the repo, and one of them could be the protected tree itself.
///
/// Both sides are compared after `within` resolves them, so a symlinked
/// spelling of a protected tree is that tree. Paths that do not exist yet are
/// vetted too.
pub fn assert_destination_allowed(
    vfs: &dyn Vfs,
    candidate: &Path,
    protected: &[PathBuf],
) -> Result<(), CoreError> {
    for tree in protected {
        let why = match (within(vfs, tree, candidate)?, within(vfs, candidate, tree)?) {
            (true, true) => format!(
                "destination {} is a protected source tree",
                candidate.display()
            ),
            (true, false) => format!(
                "destination {} is inside protected source tree {}",
                candidate.display(),
                tree.display()
            ),
            (false, true) => format!(
                "destination {} contains protected source tree {}",
                candidate.display(),
                tree.display()
            ),
            (false, false) => continue,
        };
        return Err(CoreError::refused("unsafe-root", why));
    }
    Ok(())
}

/// Refuses `candidate` as a destination root when publishing there could
/// touch the vault — the narrowed rail for a request that names its vault.
///
/// `refused: unsafe-root`, the same token the wider ancestor refusal carries.
///
/// [`assert_destination_allowed`] refuses every ancestor of a protected tree,
/// because from a destination root alone an ancestor *could* compute a write
/// path into the tree. With the vault root and the projected paths both in
/// hand that "could" is answerable, and the write-guard spec says to
/// answer it: the vault itself and anything inside it stay refused, and an
/// ancestor — `~`, a project directory the vault lives under — is refused
/// only when one of the writes it implies would actually land inside the
/// vault. That is what lets a vault at `~/notes` build into `~`: the writes
/// land under `~/.claude/skills/`, disjoint from the vault.
///
/// Every comparison here is **physical**, not lexical: both roots and every
/// projected write path are resolved by `within` first. The write
/// path is the one that must not be trusted as spelled — `~/.claude` may be
/// a link into the vault, and on a case-insensitive filesystem `.claude`
/// and a vault spelled `.Claude` are one directory — and this is the only
/// rail positioned to see that. Callers must
/// only take this narrowed rail on a host whose [`Vfs::sees_links`] is
/// honest about resolution — anywhere else the wider ancestor refusal is
/// the answer, because a link the host cannot see is exactly what this
/// check would miss.
pub fn assert_destination_allowed_for_vault<'a>(
    vfs: &dyn Vfs,
    candidate: &Path,
    vault_root: &Path,
    relative_paths: impl IntoIterator<Item = &'a str>,
) -> Result<(), CoreError> {
    if within(vfs, vault_root, candidate)? {
        let why = if within(vfs, candidate, vault_root)? {
            format!("destination {} is the vault itself", candidate.display())
        } else {
            format!(
                "destination {} is inside the vault {}",
                candidate.display(),
                vault_root.display()
            )
        };
        return Err(CoreError::refused("unsafe-root", why));
    }
    for relative in relative_paths {
        if within(vfs, vault_root, &candidate.join(relative))? {
            return Err(CoreError::refused(
                "unsafe-root",
                format!(
                    "publishing to {} would write {} inside the vault {}",
                    candidate.display(),
                    relative,
                    vault_root.display()
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protected() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/repo/repo/tools/skills"),
            PathBuf::from("/repo/.claude/skills"),
        ]
    }

    #[test]
    fn equal_to_a_protected_tree_is_refused() {
        let err = assert_destination_allowed(
            &crate::vfs::MemVfs::new(),
            Path::new("/repo/repo/tools/skills"),
            &protected(),
        );
        assert!(matches!(err, Err(CoreError::Refused(_))));
    }

    #[test]
    fn inside_a_protected_tree_is_refused() {
        let err = assert_destination_allowed(
            &crate::vfs::MemVfs::new(),
            Path::new("/repo/repo/tools/skills/task-work"),
            &protected(),
        );
        assert!(matches!(err, Err(CoreError::Refused(_))));
    }

    #[test]
    fn containing_a_protected_tree_is_refused() {
        // The repo root contains .claude/skills — registering it would put
        // protected paths inside the publish universe.
        let err = assert_destination_allowed(
            &crate::vfs::MemVfs::new(),
            Path::new("/repo"),
            &protected(),
        );
        assert!(matches!(err, Err(CoreError::Refused(_))));
    }

    /// Every refusal either rail can raise leads with `unsafe-root`, the token
    /// the publish spec gives a destination overlapping a tree we
    /// must not write to. A rail that answered in prose alone would reach a
    /// report as a sentence no script can match.
    #[test]
    fn every_rail_refusal_leads_with_the_unsafe_root_token() {
        let vfs = crate::vfs::MemVfs::new();
        let vault = Path::new("/vault");
        let ancestor: Vec<CoreError> = vec![
            assert_destination_allowed(
                &crate::vfs::MemVfs::new(),
                Path::new("/repo/repo/tools/skills"),
                &protected(),
            )
            .unwrap_err(),
            assert_destination_allowed(
                &crate::vfs::MemVfs::new(),
                Path::new("/repo/repo/tools/skills/task-work"),
                &protected(),
            )
            .unwrap_err(),
            assert_destination_allowed(
                &crate::vfs::MemVfs::new(),
                Path::new("/repo"),
                &protected(),
            )
            .unwrap_err(),
            assert_destination_allowed_for_vault(&vfs, vault, vault, []).unwrap_err(),
            assert_destination_allowed_for_vault(&vfs, Path::new("/vault/out"), vault, [])
                .unwrap_err(),
            assert_destination_allowed_for_vault(&vfs, Path::new("/"), vault, ["vault/x/SKILL.md"])
                .unwrap_err(),
        ];
        assert_eq!(ancestor.len(), 6);
        for err in ancestor {
            let message = err.to_string();
            assert!(message.starts_with("refused: unsafe-root: "), "{message}");
        }
    }

    #[test]
    fn an_unrelated_sibling_is_allowed() {
        assert_destination_allowed(
            &crate::vfs::MemVfs::new(),
            Path::new("/home/user/scratch"),
            &protected(),
        )
        .unwrap();
        // Similar name, different path — prefix comparison must be
        // component-wise, not string-wise.
        assert_destination_allowed(
            &crate::vfs::MemVfs::new(),
            Path::new("/repo/repo/tools/skills-backup"),
            &protected(),
        )
        .unwrap();
    }

    /// A canonical temporary directory, so a test compares against the same
    /// spelling the guard resolves to (macOS hands out `/var`, which is a link
    /// to `/private/var`).
    #[cfg(unix)]
    fn canonical_tempdir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        (tmp, root)
    }

    #[cfg(unix)]
    #[test]
    fn a_protected_tree_reached_through_a_symlink_is_refused() {
        let (_tmp, root) = canonical_tempdir();
        let protected = root.join("repo/.claude/skills");
        std::fs::create_dir_all(&protected).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&protected, &alias).unwrap();

        let vfs = crate::vfs::NativeVfs;
        for candidate in [alias.clone(), alias.join("task-work"), root.join("repo")] {
            let err =
                assert_destination_allowed(&vfs, &candidate, std::slice::from_ref(&protected))
                    .expect_err(&format!("{} must be refused", candidate.display()));
            assert!(err.to_string().contains("unsafe-root"), "{err}");
        }
        assert_destination_allowed(&vfs, &root.join("elsewhere"), &[protected]).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_trailing_parent_segment_behind_a_missing_directory_is_resolved_physically() {
        // `link/missing/..` is `link` on disk, and `link` is the vault.
        let (_tmp, root) = canonical_tempdir();
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&vault, &link).unwrap();

        let candidate = link.join("missing/..");
        let err = assert_destination_allowed_for_vault(
            &crate::vfs::NativeVfs,
            &candidate,
            &vault,
            [".claude/skills/x/SKILL.md"],
        )
        .expect_err("a destination that is the vault on disk must be refused");
        assert!(err.to_string().contains("unsafe-root"), "{err}");
    }

    /// A dry run and a real run reach the same rail verdict for a `..`
    /// behind a symlink: both resolve it on disk.
    #[cfg(unix)]
    #[test]
    fn a_preview_and_a_real_run_agree_on_a_parent_segment_behind_a_link() {
        let (_tmp, root) = canonical_tempdir();
        let vault = root.join("real/vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(root.join("real/deep")).unwrap();
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::os::unix::fs::symlink(root.join("real/deep"), root.join("a/link")).unwrap();

        // `a/link/../vault` is `real/vault` on disk.
        let candidate = root.join("a/link/../vault");
        let native = crate::vfs::NativeVfs;
        let preview = crate::vfs::PreviewVfs::over(&native);
        for vfs in [&native as &dyn Vfs, &preview] {
            let err = assert_destination_allowed_for_vault(
                vfs,
                &candidate,
                &vault,
                [".claude/skills/x/SKILL.md"],
            )
            .expect_err("the destination is the vault on disk");
            assert!(err.to_string().contains("unsafe-root"), "{err}");
        }
    }

    /// A host that fails to resolve a path for a reason other than its being
    /// missing: every rail refuses, whichever way its own check points.
    struct DeniedVfs(crate::vfs::MemVfs);

    impl Vfs for DeniedVfs {
        fn read_file(&self, path: &Path) -> Result<Vec<u8>, crate::vfs::VfsError> {
            self.0.read_file(path)
        }
        fn read_dir(&self, path: &Path) -> Result<Vec<crate::vfs::DirEntry>, crate::vfs::VfsError> {
            self.0.read_dir(path)
        }
        fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), crate::vfs::VfsError> {
            self.0.write_file(path, bytes)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), crate::vfs::VfsError> {
            self.0.create_dir_all(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.0.exists(path)
        }
        fn metadata(&self, path: &Path) -> Result<crate::vfs::Metadata, crate::vfs::VfsError> {
            self.0.metadata(path)
        }
        fn is_executable(&self, path: &Path) -> bool {
            self.0.is_executable(path)
        }
        fn set_executable(&self, path: &Path, yes: bool) -> Result<(), crate::vfs::VfsError> {
            self.0.set_executable(path, yes)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), crate::vfs::VfsError> {
            self.0.rename(from, to)
        }
        fn remove(&self, path: &Path) -> Result<(), crate::vfs::VfsError> {
            self.0.remove(path)
        }
        fn sees_links(&self) -> bool {
            true
        }
        /// Denied under `/locked`, missing under `/gone` and `/unreadable`,
        /// lexical elsewhere.
        fn canonicalize(&self, path: &Path) -> Result<PathBuf, crate::vfs::VfsError> {
            let kind = if path.starts_with("/locked") {
                std::io::ErrorKind::PermissionDenied
            } else if path.starts_with("/gone") || path.starts_with("/unreadable") {
                std::io::ErrorKind::NotFound
            } else {
                return Ok(crate::vfs::normalize(path));
            };
            Err(crate::vfs::VfsError::io(path, std::io::Error::from(kind)))
        }
        /// Denied for `/unreadable`; elsewhere nothing is a link.
        fn read_link(&self, path: &Path) -> Result<PathBuf, crate::vfs::VfsError> {
            let kind = if path == Path::new("/unreadable") {
                std::io::ErrorKind::PermissionDenied
            } else {
                std::io::ErrorKind::InvalidInput
            };
            Err(crate::vfs::VfsError::io(path, std::io::Error::from(kind)))
        }
    }

    #[test]
    fn a_host_error_that_is_not_a_missing_path_refuses_every_rail() {
        let vfs = DeniedVfs(crate::vfs::MemVfs::new());
        let refusals = [
            assert_destination_allowed_for_vault(
                &vfs,
                Path::new("/locked"),
                Path::new("/vault"),
                ["x/SKILL.md"],
            )
            .unwrap_err(),
            assert_destination_allowed(&vfs, Path::new("/locked/dest"), &[PathBuf::from("/vault")])
                .unwrap_err(),
        ];
        for err in refusals {
            let message = err.to_string();
            assert!(message.starts_with("refused: unsafe-path: "), "{message}");
            assert!(message.contains("/locked"), "{message}");
        }

        // `/unreadable` is missing, so path-guard asks whether it is a dangling
        // link, and the host cannot say.
        let unreadable = assert_destination_allowed_for_vault(
            &vfs,
            Path::new("/unreadable/dest"),
            Path::new("/vault"),
            ["x/SKILL.md"],
        )
        .unwrap_err()
        .to_string();
        assert!(
            unreadable.starts_with("refused: unsafe-path: /unreadable"),
            "{unreadable}"
        );

        // A missing path is an answer, not a failure.
        assert_destination_allowed_for_vault(
            &vfs,
            Path::new("/gone"),
            Path::new("/vault"),
            ["x/SKILL.md"],
        )
        .unwrap();
    }
}
