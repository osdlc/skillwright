# Vfs

Status: current, moving.

The file-access trait every read and write goes through, so the same code runs natively and as wasm. `PreviewVfs` captures writes, which is how dry runs and `check` work. It moves out of core into the osdlc crate `vfs-seam`, used by every vault crate ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

`vfs-seam`: `Vfs`, `NativeVfs`, `MemVfs`, `PreviewVfs`, `Clock`. `tradecraft-core/src/vfs.rs` re-exports it. `JsVfs` in `tradecraft-wasm`.

## Not to be confused with

[vfs-walk](vfs-walk.md), which walks a tree through this trait. The `vfs` crate on crates.io, which this is not built on.
