# Vfs

Status: current, moving.

The file-access trait every read and write goes through, so the same code runs natively and as wasm. `PreviewVfs` captures writes, which is how dry runs and `check` work. Planned: moves out of core into its own osdlc crate, used by every vault crate ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

`tradecraft-core/src/vfs.rs`: `Vfs`, `NativeVfs`, `MemVfs`, `PreviewVfs`. `JsVfs` in `tradecraft-wasm`.
