# Vault

Status: current.

The folder that holds both `skillwright.md` and `marketplaces/`: the source of everything skillwright builds. The markdown is the system of record and git is its history.

osdlc's shared crates call it a [corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/corpus.md); the two words mean the same thing there. In harness-reference and [S-GFFY](../standards/S-GFFY-markdown-db.md), [corpus](corpus.md) means a markdown-db corpus.

## In code

`--vault`, or the first folder at or above the working directory that holds both; found by `find_vault_root` in `tradecraft-core/src/layout.rs` and loaded by `load_vault` in `model/vault.rs`.

## Not to be confused with

`dist/<m>/`, the build output. It sits in the vault root but is never read as source. Apart from it, only `init` and `bump` write into the vault.
