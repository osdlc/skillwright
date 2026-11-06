//! What more than one integration binary here needs.

use std::sync::Mutex;

#[allow(dead_code)]
static GIT_LOCK: Mutex<()> = Mutex::new(());

/// Run one `--from-git` case at a time.
///
/// In a cargo test binary `tiered-dirs` resolves the checkout cache under its
/// own per-process scratch root, so nothing here reads or writes a real
/// `$HOME`. The cases share that cache, so they run one at a time.
#[allow(dead_code)]
pub fn with_git_cache<T>(f: impl FnOnce() -> T) -> T {
    let _guard = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}
