# markdown-vault

Status: planned.

osdlc's vault model, which [load](load.md) reads the vault through: notes, links and backlinks. Headings and blocks come from `markdown-contract`, which skillwright calls itself. Defined in osdlc's [markdown-vault](https://github.com/osdlc/osdlc/blob/main/docs/terms/markdown-vault.md) ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Not used yet. Today load reads notes with core's `discover_components`, walking through `markdown-vault-fs`, and resolves links with its own scanner in `compile.rs`.
