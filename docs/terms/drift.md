# Drift

Status: current.

A difference between what `build` would write and what is in `dist/<m>/`. A missing `dist/<m>/` is drift too. `check` reports it and exits 1.

## In code

`UnitStatus::Differs` in a check report (`export/report.rs`); the CLI prints it as `drift` (`status_word` in `skillwright/src/report.rs`).
