# Emit

Status: current.

The last stage: write a [file-tree](file-tree.md) out. `build` swaps what each harness owns under `dist/<m>/`, and `check` diffs the same trees and writes nothing ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)).

## In code

`diff_root` in `export/dest.rs` and `commit` in `export/swap.rs`.
