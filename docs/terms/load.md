# Load

Status: current.

The first stage: read the vault into the [marketplace-model](marketplace-model.md). Two passes: read every note into a node, then resolve every link and call to a [resolved-reference](resolved-reference.md) ([D-EPSA](../decisions/D-EPSA-two-phase-load.md)).

## In code

`load` in `model/load.rs`. Pass 1 is `read_notes` (a wrapper over `discover_components`); pass 2 resolves bodies through `compile.rs` in recording mode. `load_component` and `load_plugin` load one note or one pack for `publish`.
