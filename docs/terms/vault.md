# Vault

Status: current.

The folder of markdown notes that is the source of everything skillwright publishes. The markdown is the system of record and git is its history.

## In code

`--vault`, `vault:` in `skillwright.yaml`; read by `tradecraft-core/src/vault.rs`.

## Not to be confused with

A [destination](destination.md), which skillwright writes to. skillwright never writes into the vault except through authoring verbs and [adopt](adopt.md).
