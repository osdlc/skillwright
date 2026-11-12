//! Containment resolved through the filesystem: `within`, so a symlinked
//! alias cannot walk past a check.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

use path_guard::{Guard, Resolver};

use crate::error::CoreError;
use crate::refusal;
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
        self.failure
            .borrow_mut()
            .get_or_insert_with(|| unresolvable(p, &message));
        io::Error::other(message)
    }
}

/// The refusal for a path the host could not resolve, with its `message`.
fn unresolvable(p: &Path, message: &str) -> CoreError {
    CoreError::refused(
        refusal::UNSAFE_PATH,
        format!(
            "{}: cannot tell where this path leads — {message}. Nothing was written",
            p.display()
        ),
    )
}

/// The real path of `p`, which exists, with every symlink resolved.
///
/// # Errors
///
/// `refused: unsafe-path` when the host cannot resolve it, as [`within`]
/// refuses.
pub(crate) fn real_path(vfs: &dyn Vfs, p: &Path) -> Result<PathBuf, CoreError> {
    vfs.canonicalize(p)
        .map_err(|err| unresolvable(p, &CoreError::from(err).to_string()))
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
            refusal::UNSAFE_PATH,
            format!(
                "cannot compare {} with {}: {err}. Nothing was written",
                inner.display(),
                outer.display()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_within_itself_and_its_ancestors_only() {
        let vfs = crate::vfs::MemVfs::new();
        let within = |outer: &str, inner: &str| within(&vfs, Path::new(outer), Path::new(inner));
        assert!(within("/vault", "/vault").unwrap());
        assert!(within("/vault", "/vault/dist/m").unwrap());
        assert!(!within("/vault/dist", "/vault").unwrap());
        assert!(!within("/vault", "/elsewhere").unwrap());
    }

    fn canonical_tempdir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        (tmp, root)
    }

    #[cfg(unix)]
    #[test]
    fn a_tree_reached_through_a_symlink_is_within_it() {
        let (_tmp, root) = canonical_tempdir();
        let tree = root.join("repo/marketplaces");
        std::fs::create_dir_all(&tree).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&tree, &alias).unwrap();
        let vfs = crate::vfs::NativeVfs;
        assert!(within(&vfs, &tree, &alias.join("m")).unwrap());
        assert!(!within(&vfs, &tree, &root.join("elsewhere")).unwrap());
    }

    /// A host that fails to resolve a path for a reason other than its being
    /// missing.
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
    fn a_host_error_that_is_not_a_missing_path_refuses() {
        let vfs = DeniedVfs(crate::vfs::MemVfs::new());
        let message = within(&vfs, Path::new("/vault"), Path::new("/locked/dest"))
            .unwrap_err()
            .to_string();
        assert!(message.starts_with("refused: unsafe-path: "), "{message}");
        assert!(message.contains("/locked"), "{message}");
        // A missing path is an answer, not a failure.
        assert!(!within(&vfs, Path::new("/vault"), Path::new("/gone")).unwrap());
    }
}
