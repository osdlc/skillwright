# Load

Status: planned.

The first stage: read the vault into the [marketplace-model](marketplace-model.md). Two passes: read every note into a node, then resolve every link and call to a [resolved-reference](resolved-reference.md) ([D-EPSA](../decisions/D-EPSA-two-phase-load.md)).

## In code

Not built yet. Today split across `discover.rs`, `vault.rs` and `compile.rs`.
