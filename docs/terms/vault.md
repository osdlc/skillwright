# Vault

Status: current.

The folder of markdown notes that is the source of everything skillwright publishes. The markdown is the system of record and git is its history.

osdlc's shared crates call it a [corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/corpus.md); the two words mean the same thing there. In harness-reference and [S-GFFY](../standards/S-GFFY-markdown-db.md), [corpus](corpus.md) means a markdown-db corpus.

## In code

`--vault`, `vault:` in `skillwright.yaml`; read by `tradecraft-core/src/vault.rs`.

## Not to be confused with

A [destination](destination.md), which skillwright writes to. skillwright never writes into the vault except through authoring verbs and [adopt](adopt.md).
