# Load

Status: current.

The first stage: read the vault into the [marketplace-model](marketplace-model.md). Reads each note once, then resolves every link and call to a [resolved-reference](resolved-reference.md) before any translator runs ([D-EPSA](../decisions/D-EPSA-resolve-at-load.md)).

## In code

`load` in `model/load.rs`. Today in two passes. `read_notes` walks the skills source with `markdown-vault-fs` (`load_source`), places its notes with `discover_in`, and builds `markdown-link`'s `LinkIndex` of the vault: from the same walk when the skills source is the whole vault, else from one walk of the vault. Then body resolution runs through `compile.rs` in recording mode, which finds links with `markdown-link`'s scanner and resolves them with that index. Load does not use [markdown-corpus](markdown-corpus.md): it needs paths, not every note's links. `load_component` and `load_plugin` load one note or one pack for `publish`.
