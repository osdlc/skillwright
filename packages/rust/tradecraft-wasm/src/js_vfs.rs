//! `tradecraft_core::Vfs`, implemented over a JavaScript object.
//!
//! tradecraft-core reads a vault and writes `dist/<m>/`, and on
//! `wasm32-unknown-unknown` there is no filesystem: every `std::fs` call
//! compiles cleanly and then fails at runtime. The core's answer is to take
//! its I/O as a `&dyn Vfs` parameter; this module is the host
//! end of that seam, and it can exist at all only because the core is
//! SYNCHRONOUS. wasm-bindgen can call synchronously from Rust into JS, so no
//! part of the core has to know what a Promise is.
//!
//! # The contract the host implements
//!
//! A JavaScript object with these methods, all synchronous:
//!
//! ```ts
//! readFile(path: string): Uint8Array
//! readDir(path: string): Array<{ name: string; isDir: boolean }>
//! writeFile(path: string, bytes: Uint8Array): void
//! createDirAll(path: string): void
//! exists(path: string): boolean
//! isExecutable(path: string): boolean
//! setExecutable(path: string, yes: boolean): void
//! ```
//!
//! The imports are `structural`, because the host passes a plain object literal
//! rather than an instance of a class — a non-structural import would look the
//! method up on a prototype that does not exist.
//!
//! # Two methods are optional until something calls them
//!
//! `rename` and `remove` are optional on the host object, so a host written
//! before them is still accepted:
//!
//! ```ts
//! rename?(from: string, to: string): void
//! remove?(path: string): void
//! ```
//!
//! Nothing checks for them at [`JsVfs::from_js`] — a host without them pays
//! only when something actually renames or removes, and then it gets an error
//! naming the missing method rather than the JS engine's `undefined is not a
//! function`.
//!
//! # `stat` is optional, and cheaper than its fallback
//!
//! ```ts
//! stat?(path: string): { size: number; mtimeMs: number | null; isDir: boolean; isSymlink: boolean }
//! ```
//!
//! `Vfs::metadata`, which the vault walk asks of every file it lists.
//! Symlinks are not followed: a link reports `isSymlink: true` and its own
//! size. With the link methods, `isSymlink` is asked too, and a path either
//! answer calls a link is one. `mtimeMs` is milliseconds since the Unix epoch, or `null` when the
//! host has none. A missing path throws with `code` `ENOENT`.
//!
//! A host without it still works: the bridge answers from `readFile` (a
//! file, its length, no time) or `readDir` (a directory), and from
//! `isSymlink` when the host has the link methods. That reads each file once
//! more, so a host that can stat should.
//!
//! # The link questions are optional: all three, or none
//!
//! A host that holds a real filesystem can answer where a path leads:
//!
//! ```ts
//! realpath?(path: string): string     // every symlink resolved; throws when missing
//! readLink?(path: string): string     // a link's target text; throws when not a link
//! isSymlink?(path: string): boolean   // the path itself is a link, not followed
//! ```
//!
//! Paths cross in the core's `/` form on every platform. A Windows host
//! converts at its boundary.
//!
//! The answers are trusted, so their failures carry meaning:
//!
//! - `realpath` throws with `code` `ENOENT` or `ENOTDIR` for a missing path,
//!   and only then.
//! - `readLink` throws with `code` `EINVAL` for a path that is not a link,
//!   or `ENOENT`/`ENOTDIR` for a missing one, and only then.
//! - `realpath` answers an absolute path.
//! - Any other throw, an answer that is not a string, or a `realpath` answer
//!   that is not absolute refuses the check that asked (`refused: unsafe-path`)
//!   rather than reading as "missing" or "not a link".
//! - `isSymlink` that throws or answers with anything but a boolean counts
//!   as a link, so the caller refuses the path rather than walking past it.
//!
//! They map to `Vfs::canonicalize`, `Vfs::read_link` and `Vfs::is_symlink`,
//! which `path-guard`'s containment checks and discovery's symlink refusal
//! use. [`JsVfs::from_js`] checks for all three once. With all three,
//! `Vfs::sees_links` is `true`, and the core runs the narrowed checks that
//! need an honest answer about links. With fewer, the bridge uses none of
//! them: `canonicalize` folds `.` and `..` and asks nothing, `is_symlink`
//! answers `false`, `sees_links` is `false`, and the core keeps its strict
//! checks. A host with part of the set gets the strict answer, never a mix.
//!
//! # A thrown exception is an error, never a panic
//!
//! Every call is declared `catch`, so a JS `throw` arrives as
//! `Err(JsValue)` instead of unwinding through the wasm frame. A panic here
//! is much worse than an error: `wasm32-unknown-unknown` aborts on panic, and
//! an aborted module is poisoned — every later call into it traps, so one
//! unreadable file would take down the plugin's engine for the rest of the
//! session. Instead the thrown value's message is lifted out and wrapped in a
//! `VfsError` with the path that provoked it — the shape `NativeVfs`
//! produces for the same failure, so nothing above the seam can tell which
//! host it is talking to.
//!
//! `exists` and `is_executable` return `bool` rather than `Result` in the
//! trait, so a throw from either becomes `false` — the same answer
//! `NativeVfs` gives when a `stat` fails, and the honest one: the caller asked
//! a yes/no question about a path it may not be allowed to see.

use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};

use tradecraft_core::vfs::{Metadata, VfsError, ms_f64_to_ns, normalize};
use tradecraft_core::{CoreError, DirEntry, Vfs, refusal};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The host's filesystem object, as seen from Rust.
    ///
    /// `extends = js_sys::Object` gives it the `JsCast` machinery (so a
    /// `JsValue` can be checked and cast) without asserting anything about
    /// its prototype.
    #[wasm_bindgen(extends = js_sys::Object)]
    pub type HostVfs;

    #[wasm_bindgen(method, structural, catch, js_name = readFile)]
    fn read_file(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = readDir)]
    fn read_dir(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    // `bytes: &[u8]` reaches JS as a Uint8Array VIEW onto wasm linear memory,
    // valid only for the duration of the call. The host must copy it if it
    // retains it; every implementation we ship writes synchronously, which is
    // the same reason the trait is synchronous in the first place.
    #[wasm_bindgen(method, structural, catch, js_name = writeFile)]
    fn write_file(this: &HostVfs, path: &str, bytes: &[u8]) -> Result<(), JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = createDirAll)]
    fn create_dir_all(this: &HostVfs, path: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, structural, catch)]
    fn exists(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    // Optional. Only ever invoked after `JsVfs::from_js` saw a function under
    // the name.
    #[wasm_bindgen(method, structural, catch)]
    fn stat(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = isExecutable)]
    fn is_executable(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = setExecutable)]
    fn set_executable(this: &HostVfs, path: &str, yes: bool) -> Result<(), JsValue>;

    // The optional pair. Only ever invoked after `JsVfs::require` has seen a
    // function under the name, so the structural lookup cannot land on
    // `undefined`.
    #[wasm_bindgen(method, structural, catch)]
    fn rename(this: &HostVfs, from: &str, to: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, structural, catch)]
    fn remove(this: &HostVfs, path: &str) -> Result<(), JsValue>;

    // The link questions. Only ever invoked when `JsVfs::from_js` saw all three.
    #[wasm_bindgen(method, structural, catch)]
    fn realpath(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = readLink)]
    fn read_link(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = isSymlink)]
    fn is_symlink(this: &HostVfs, path: &str) -> Result<JsValue, JsValue>;
}

/// The host methods that together let the bridge see links.
const LINK_METHODS: [&str; 3] = ["realpath", "readLink", "isSymlink"];

/// A [`Vfs`] backed by a JavaScript object.
///
/// Holds the host object for the duration of one exported call and no longer:
/// the core takes `&dyn Vfs`, does its work, and returns, so there is never a
/// JS reference parked in wasm memory between calls.
pub struct JsVfs {
    host: HostVfs,
    /// The host supplied every one of [`LINK_METHODS`].
    links: bool,
    /// The host supplied `stat`.
    stat: bool,
}

impl JsVfs {
    /// Takes the `JsValue` the caller passed as its filesystem.
    ///
    /// Checked here: that it is an object, and whether it has all three link
    /// methods (see the module doc). The seven required methods are resolved
    /// at call time by the `structural` imports, so a host missing one fails
    /// on the operation that needed it, with the path in the message, rather
    /// than failing the whole build before anything is attempted.
    pub fn from_js(value: JsValue) -> Result<Self, CoreError> {
        if !value.is_object() {
            return Err(CoreError::refused(
                refusal::INVALID_VFS,
                "the filesystem passed to the core must be an object with readFile, readDir, \
                 writeFile, createDirAll, exists, isExecutable, and setExecutable",
            ));
        }
        let host: HostVfs = value.unchecked_into();
        let links = LINK_METHODS
            .iter()
            .all(|method| has_function(&host, method));
        let stat = has_function(&host, "stat");
        Ok(Self { host, links, stat })
    }

    /// Refuses a call to an additive host method the host did not supply.
    ///
    /// The `structural` import's own failure — a `TypeError` about `undefined`
    /// not being a function — names neither the contract nor the fix, so the
    /// presence check is explicit and the error says what to add. Checked per
    /// call rather than at [`JsVfs::from_js`] on purpose: see the module doc.
    fn require(&self, method: &str, path: &Path) -> Result<(), VfsError> {
        if has_function(&self.host, method) {
            return Ok(());
        }
        Err(VfsError::io(
            path,
            io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "the filesystem object this host passed has no `{method}` method — it is \
                     an additive part of the Vfs contract, and this operation needs it"
                ),
            ),
        ))
    }
}

impl Vfs for JsVfs {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        let value = self
            .host
            .read_file(&as_str(path))
            .map_err(|e| thrown(path, &e))?;
        to_bytes(path, &value)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        let value = self
            .host
            .read_dir(&as_str(path))
            .map_err(|e| thrown(path, &e))?;
        to_entries(path, &value)
    }

    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), VfsError> {
        self.host
            .write_file(&as_str(path), bytes)
            .map_err(|e| thrown(path, &e))
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        self.host
            .create_dir_all(&as_str(path))
            .map_err(|e| thrown(path, &e))
    }

    fn metadata(&self, path: &Path) -> Result<Metadata, VfsError> {
        if self.stat {
            let value = self
                .host
                .stat(&as_str(path))
                .map_err(|e| thrown(path, &e))?;
            let meta = to_metadata(path, &value)?;
            // `isSymlink` is this bridge's word on links, and it fails
            // closed, so a walk refuses what it would refuse.
            return Ok(Metadata {
                is_symlink: meta.is_symlink || self.is_symlink(path),
                ..meta
            });
        }
        // No `stat`: a file is what `readFile` reads, a directory what
        // `readDir` lists. See the module doc.
        let is_symlink = self.is_symlink(path);
        match self.read_file(path) {
            Ok(bytes) => Ok(Metadata {
                is_symlink,
                ..Metadata::file_ns(bytes.len() as u64, None)
            }),
            Err(error) => match self.read_dir(path) {
                Ok(_) => Ok(Metadata {
                    is_symlink,
                    ..Metadata::dir_ns(None)
                }),
                Err(_) => Err(error),
            },
        }
    }

    fn exists(&self, path: &Path) -> bool {
        self.host
            .exists(&as_str(path))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn is_executable(&self, path: &Path) -> bool {
        self.host
            .is_executable(&as_str(path))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn set_executable(&self, path: &Path, yes: bool) -> Result<(), VfsError> {
        self.host
            .set_executable(&as_str(path), yes)
            .map_err(|e| thrown(path, &e))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        self.require("rename", from)?;
        self.host
            .rename(&as_str(from), &as_str(to))
            .map_err(|e| thrown(from, &e))
    }

    fn remove(&self, path: &Path) -> Result<(), VfsError> {
        self.require("remove", path)?;
        self.host
            .remove(&as_str(path))
            .map_err(|e| thrown(path, &e))
    }

    fn sees_links(&self) -> bool {
        self.links
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, VfsError> {
        if !self.links {
            return Ok(normalize(path));
        }
        let value = self
            .host
            .realpath(&as_str(path))
            .map_err(|e| thrown(path, &e))?;
        let resolved = to_path(path, &value, "realpath must return a string")?;
        // A relative answer would compare as outside every root. `has_root`,
        // since `is_absolute` is always false on wasm32-unknown-unknown.
        if !resolved.has_root() {
            return Err(protocol(path, "realpath must return an absolute path"));
        }
        Ok(resolved)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, VfsError> {
        if !self.links {
            return Err(VfsError::io(
                path,
                io::Error::new(io::ErrorKind::Unsupported, "this host has no symlinks"),
            ));
        }
        let value = self
            .host
            .read_link(&as_str(path))
            .map_err(|e| thrown(path, &e))?;
        to_path(path, &value, "readLink must return a string")
    }

    /// Fails closed: a throw, or an answer that is not a boolean, is `true`.
    fn is_symlink(&self, path: &Path) -> bool {
        self.links
            && self
                .host
                .is_symlink(&as_str(path))
                .map_or(true, |v| v.as_bool().unwrap_or(true))
    }
}

/// Is `method` a function on the host object?
fn has_function(host: &HostVfs, method: &str) -> bool {
    js_sys::Reflect::get(host, &JsValue::from_str(method))
        .map(|value| value.is_function())
        .unwrap_or(false)
}

// --- crossing the boundary -------------------------------------------------

/// Paths cross as strings.
///
/// Lossy in principle, exact in practice: every path the core sees on this host
/// originated as a JavaScript string — either from the JSON request or from a
/// `readDir` listing — so there are no non-UTF-8 bytes to lose. The alternative
/// (an error on lossy conversion) would trade a real failure mode for an
/// impossible one.
fn as_str(path: &Path) -> Cow<'_, str> {
    path.to_string_lossy()
}

/// `readFile`'s return value as owned bytes.
///
/// `ArrayBuffer` is accepted alongside `Uint8Array` because some hosts hand
/// one back, and mis-reading one as the other shows up as an empty file in
/// the output.
fn to_bytes(path: &Path, value: &JsValue) -> Result<Vec<u8>, VfsError> {
    if let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() {
        return Ok(bytes.to_vec());
    }
    if let Some(buffer) = value.dyn_ref::<js_sys::ArrayBuffer>() {
        return Ok(js_sys::Uint8Array::new(buffer).to_vec());
    }
    Err(protocol(path, "readFile must return a Uint8Array"))
}

/// `readDir`'s return value as directory entries.
///
/// `isDir` must be resolved *without* following symlinks, which is the host's
/// obligation and cannot be checked from here — the trait's contract is that a
/// symlinked directory reports `is_dir: false`, and that is what stops the
/// walker descending into it.
fn to_entries(path: &Path, value: &JsValue) -> Result<Vec<DirEntry>, VfsError> {
    if !js_sys::Array::is_array(value) {
        return Err(protocol(
            path,
            "readDir must return an array of { name, isDir }",
        ));
    }
    let array = js_sys::Array::from(value);
    let mut out = Vec::with_capacity(array.length() as usize);
    for item in array.iter() {
        let name = field(&item, "name")
            .and_then(|v| v.as_string())
            .ok_or_else(|| protocol(path, "a readDir entry has no string `name`"))?;
        let is_dir = field(&item, "isDir")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| protocol(path, "a readDir entry has no boolean `isDir`"))?;
        out.push(DirEntry { name, is_dir });
    }
    Ok(out)
}

/// A host answer that must be a path string.
fn to_path(path: &Path, value: &JsValue, what: &str) -> Result<PathBuf, VfsError> {
    value
        .as_string()
        .map(PathBuf::from)
        .ok_or_else(|| protocol(path, what))
}

/// `stat`'s return value as metadata. `mtimeMs` may be `null` or absent; a
/// fractional one (Node's) keeps its fraction, see [`ms_f64_to_ns`].
fn to_metadata(path: &Path, value: &JsValue) -> Result<Metadata, VfsError> {
    let what = "stat must return { size, mtimeMs, isDir, isSymlink }";
    let size = field(value, "size")
        .and_then(|v| v.as_f64())
        .filter(|n| *n >= 0.0)
        .ok_or_else(|| protocol(path, what))?;
    let modified_ns = field(value, "mtimeMs")
        .and_then(|v| v.as_f64())
        .and_then(ms_f64_to_ns);
    let flag = |key: &str| {
        field(value, key)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| protocol(path, what))
    };
    let is_dir = flag("isDir")?;
    let is_symlink = flag("isSymlink")?;
    Ok(Metadata {
        len: if is_dir { 0 } else { size as u64 },
        modified_ns,
        is_dir,
        is_symlink,
    })
}

fn field(value: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(value, &JsValue::from_str(key)).ok()
}

// --- thrown JS value -> VfsError -------------------------------------------

/// A host exception, as the error the core expects from a filesystem.
///
/// A [`VfsError`], the shape `NativeVfs` produces for the same failure. Above
/// the seam it becomes [`CoreError::Io`], whose `Display` — `reading {path}:
/// {message}` — carries both halves a user needs. The message is the host's,
/// verbatim, and the kind comes from its `code` ([`kind_of`]).
fn thrown(path: &Path, error: &JsValue) -> VfsError {
    VfsError::io(path, io::Error::new(kind_of(error), message_of(error)))
}

/// The error kind a Node-style `code` names, for the codes the link contract
/// gives a meaning (see the module doc). Anything else is `Other`.
fn kind_of(error: &JsValue) -> io::ErrorKind {
    match field(error, "code").and_then(|v| v.as_string()).as_deref() {
        Some("ENOENT") => io::ErrorKind::NotFound,
        Some("ENOTDIR") => io::ErrorKind::NotADirectory,
        Some("EINVAL") => io::ErrorKind::InvalidInput,
        _ => io::ErrorKind::Other,
    }
}

/// The host answered, but not with what it promised. Distinguished from
/// [`thrown`] by kind, because a wrong return type is a bridge bug and a throw
/// is a filesystem condition.
fn protocol(path: &Path, what: &str) -> VfsError {
    VfsError::io(path, io::Error::new(io::ErrorKind::InvalidData, what))
}

/// The message out of whatever JavaScript threw.
///
/// `throw` accepts any value, so this handles the three that actually occur: an
/// `Error` (the message, without the `Error:` prefix `toString` would add), a
/// bare string, and anything else (its JSON form, which is at least
/// diagnosable). A value with no JSON form (`undefined`, a symbol) gets a
/// fixed message: `JSON.stringify` returns `undefined` for those rather than
/// throwing.
fn message_of(error: &JsValue) -> String {
    if let Some(err) = error.dyn_ref::<js_sys::Error>() {
        return String::from(err.message());
    }
    if let Some(text) = error.as_string() {
        return text;
    }
    js_sys::JSON::stringify(error)
        .ok()
        .and_then(|json| json.as_string())
        .unwrap_or_else(|| "the host filesystem threw a value with no message".to_string())
}
