# markdown-corpus

Status: current.

osdlc's corpus model, which [load](load.md) reads the vault through (a corpus is a folder of Markdown notes and the files they link to; a vault is one): notes, links and backlinks. Headings and blocks come from `markdown-contract`, which skillwright calls itself. Defined in osdlc's [markdown-corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/markdown-corpus.md) ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

`discover::load_source` reads the skills source with `Corpus::try_load`, and discovery and load's pass 1 place and read notes from its walk. When the skills source is the whole vault, compile resolves links with the corpus's link index instead of walking the vault again. `apply`'s note move builds `markdown-relink`'s facts from a corpus of the whole vault.

## The options load sets

What load needed from markdown-corpus 0.1.0 is in 0.2.0 (osdlc/osdlc#621), so these gaps are closed:

- **The walk.** `LoadOptions` takes a `WalkOptions` for each walk, and `Corpus::try_load` returns the walk's error. Load's walk keeps dot-named files (a bundle may ship one, a link may name one), leaves out `node_modules`, `.obsidian` and `.trash`, refuses a symlink, walks only the skills root, and fails when it cannot list a folder.
- **A lenient frontmatter split.** `SplitMode::Lenient`, which also accepts blank lines before the opening fence and spaces after a fence, as load's own `split_frontmatter` does. `apply` uses it too, so a rename reads a note the way a build does.

The vault's excluded files are still applied by discovery after the walk: a walk that sees dot-named files ignores them.
