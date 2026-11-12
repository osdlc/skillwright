# Drift

Status: current.

A difference between what `build` would write and what is in `dist/<m>/`. A missing `dist/<m>/` is drift too. `check` reports it and exits 1.

## In code

`UnitStatus::Drift` in `export.rs`.
