# Marketplace

Status: current.

A named set of [plugin](plugin.md)s that harnesses install from. Built by `build` into `dist/<m>/`, one tree per [harness](harness.md).

## In code

A folder `marketplaces/<m>/` and its note `<m>.md`. The cascade folds the note over the root file's shared defaults into `MarketplaceConfig` in `model/config.rs`, which is internal.

## Not to be confused with

The [marketplace-model](marketplace-model.md), the in-memory value a marketplace is loaded into.
