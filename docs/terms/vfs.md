# Vfs

Status: current.

The filesystem interface every I/O call in core goes through, so core also runs as wasm. `PreviewVfs` captures writes, which is how dry runs and `check` work.

## In code

`vfs.rs`.
