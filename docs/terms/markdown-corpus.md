# markdown-corpus

Status: current.

osdlc's corpus model (a corpus is a folder of Markdown notes and the files they link to; a vault is one): notes, links and backlinks. Defined in osdlc's [markdown-corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/markdown-corpus.md) ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Only renames use it. `apply`'s note move loads a `Corpus` of the whole vault and builds `markdown-relink`'s facts from it, because the planner needs every note's links. It loads with the walk a build indexes the vault with (dot-named files kept; `.git`, `.obsidian`, `.trash` and `node_modules` left out), except that a symlink is listed rather than refused, so one anywhere in the vault does not block a rename. It splits frontmatter leniently (`SplitMode::Lenient`), as a build does. So a rename resolves a link to the file a build does.

[Load](load.md) and discovery do not use it. They need only the walk's paths and a link index, which `markdown-vault-fs` and `markdown-link` give without reading every note. A corpus would read and link-scan every note to give the same paths.

## What it needed

markdown-corpus 0.2.0 (osdlc/osdlc#621) added what skillwright asked for, so these gaps are closed: `LoadOptions` takes a `WalkOptions` for each walk, `Corpus::try_load` returns the walk's error, and `SplitMode::Lenient` reads a block with blank lines before it or spaces after a fence.
