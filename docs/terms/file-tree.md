# File tree

Status: current.

What a translator returns: paths mapped to bytes and the execute bit. Pure data, read by [emit](emit.md).

## In code

`FileTree` in `harness/translator.rs`; each file is a `vfs::PreviewWrite`.
