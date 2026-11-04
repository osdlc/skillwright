# Emit

Status: current.

The last stage: write a [file-tree](file-tree.md) out. `build` swaps trees, `check` diffs, `publish` writes to a destination with its manifest, `status` is a dry run of `publish` ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)).

## In code

`build` and `check` emit a translator's `FileTree` through `diff_root` and `commit` in `export.rs`; `publish` and `status` emit `FileTree::projected()` through `finish` in `publish.rs`.
