# Load

Status: current.

The first stage: read the vault into the [marketplace-model](marketplace-model.md). Reads each note once, then resolves every link and call to a [resolved-reference](resolved-reference.md) before any translator runs ([D-EPSA](../decisions/D-EPSA-resolve-at-load.md)).

## In code

`load` in `model/load.rs`, in two passes. `read_notes` (pass 1) places every note of the layout with `discover_in`. `resolve_body` (pass 2) runs each body through `compile.rs` in recording mode, which finds links with `markdown-link`'s scanner and resolves them within the plugin.
