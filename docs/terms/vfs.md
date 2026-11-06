# Vfs

Status: current, moving.

The file-access trait every read and write goes through, so the same code runs natively and as wasm. `PreviewVfs` captures writes, which is how dry runs and `check` work. It is osdlc's [vfs-seam](https://github.com/osdlc/osdlc/blob/main/docs/terms/vfs-seam.md), re-exported ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

`tradecraft-core/src/vfs.rs` re-exports `vfs-seam`. `JsVfs` in `tradecraft-wasm` implements it for JS consumers.
