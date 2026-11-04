# Emit

Status: planned.

The last stage: write a [file-tree](file-tree.md) out. `build` swaps trees, `check` diffs, `publish` writes to a destination with its manifest, `status` is a dry run of `publish` ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)).

## In code

Not built yet as a stage. Today `commit`/`diff_root` in `export.rs` and `finish` in `publish.rs`.
