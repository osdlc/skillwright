# Load

Status: current.

The first stage: read the vault into the [marketplace-model](marketplace-model.md). Reads each note once, then resolves every link and call to a [resolved-reference](resolved-reference.md) before any translator runs ([D-EPSA](../decisions/D-EPSA-resolve-at-load.md)).

## In code

`load` in `model/load.rs`. Today in two passes: `read_notes`, which reads the skills source through [markdown-corpus](markdown-corpus.md) (`load_source`) and places its notes with `discover_in`, then body resolution through `compile.rs` in recording mode, which finds links with `markdown-link`'s scanner and resolves them with its `LinkIndex`. When the skills source is the whole vault, that index is the corpus's. `load_component` and `load_plugin` load one note or one pack for `publish`.
