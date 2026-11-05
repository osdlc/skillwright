# markdown-corpus

Status: planned.

osdlc's corpus model, which [load](load.md) reads the vault through (in osdlc, a corpus is a folder of Markdown notes and the files they link to; a vault is one): notes, links and backlinks. Headings and blocks come from `markdown-contract`, which skillwright calls itself. Defined in osdlc's [markdown-corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/markdown-corpus.md) ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Load does not use it yet; `apply`'s note move builds `markdown-relink`'s facts from it. Today load reads notes with core's `discover_components`, walking through `markdown-vault-fs`, and resolves links with its own scanner in `compile.rs`.
