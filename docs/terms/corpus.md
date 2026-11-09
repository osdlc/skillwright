# Corpus

Status: current.

In harness-reference and [S-GFFY](../standards/S-GFFY-markdown-db.md): a folder holding a
`corpus-schema.md` root file (or `.yaml` / `.json`), and everything under it: types of records, their
schemas and free-form narrative. Its name is its folder name. Elsewhere in this repository, corpus
has osdlc's meaning (see below).

## In code

`validateCorpus` in the [`@osdlc/corpus-schema`](https://www.npmjs.com/package/@osdlc/corpus-schema) package. `harness-reference/corpora/beta/` is one.

## Not to be confused with

osdlc's [corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/corpus.md), which is any folder of linked markdown notes, such as a [vault](vault.md). A corpus-schema corpus is one of those with a root file.
