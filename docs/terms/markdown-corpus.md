# markdown-corpus

Status: planned.

osdlc's corpus model, which [load](load.md) reads the vault through (in osdlc, a corpus is a folder of Markdown notes and the files they link to; a vault is one): notes, links and backlinks. Headings and blocks come from `markdown-contract`, which skillwright calls itself. Defined in osdlc's [markdown-corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/markdown-corpus.md) ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Load does not use it yet; `apply`'s note move builds `markdown-relink`'s facts from it. Today load reads notes with core's `discover_components`, walking through `markdown-vault-fs` and splitting frontmatter with `frontmatter-lossless`, and resolves links with `markdown-link`'s `LinkIndex::resolve_strict` over a walk of its own.

## What it needs first

Moving load onto it would change what skillwright publishes, so these come first, as options in osdlc:

- **The walk's options.** `Corpus::load` walks with fixed rules: dot-named files left out, `node_modules` kept, a folder it cannot list skipped, symlinks listed. Load needs dot-named files kept (a bundle may ship one, a link may name one), `node_modules`, `.obsidian` and `.trash` left out, a symlink refused, a walk limited to the skills root, and a folder it cannot list to fail the load. `LoadOptions` would take a `markdown_vault_fs::walk::WalkOptions`, and a `Corpus::try_load` would return the walk's error.
- **A lenient frontmatter split.** `read_note` uses `frontmatter_lossless::split`. Load reads with `split_lenient`, which also accepts an indented opening fence and trailing spaces on the fences. `read_note` would take an option to read that way.
