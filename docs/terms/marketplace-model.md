# Marketplace model

Status: current.

The in-memory object graph a vault loads into: marketplace → plugins → skills, commands, hooks, assets. Harness-neutral, validated, and exported to wasm and `--json`. Every verb works from it ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)).

## In code

`Marketplace`, `Plugin` and `Component` in `model/mod.rs`. `skillwright list --model` prints it, and the wasm export `loadMarketplace` returns it.

## Not to be confused with

The [marketplace](marketplace.md) as a concept, or the built marketplace trees on disk.
