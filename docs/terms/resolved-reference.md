# Resolved reference

Status: current.

A link or call in a body, checked during [load](load.md) and pointing at the node it names. A translator only decides how to write it ([D-EPSA](../decisions/D-EPSA-resolve-at-load.md)).

## In code

`Segment::Link` and `Segment::Call` in `model/mod.rs` (`ResolvedRef`, `Call`). `compile.rs` records them as marks when a `CallContext` carries a recorder.
