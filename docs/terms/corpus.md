# Corpus

Status: current.

A folder holding a `markdown-db.md` root file (or `.yaml` / `.json`), and everything under it: types
of records, their schemas and free-form narrative. Its name is its folder name. Defined in
[S-GFFY](../standards/S-GFFY-markdown-db.md).

## In code

`validateCorpus` in `harness-reference/src/markdown-db/validate.ts`. `harness-reference/corpora/beta/` is one.

## Not to be confused with

osdlc's [corpus](https://github.com/osdlc/osdlc/blob/main/docs/terms/corpus.md), which is any folder of linked markdown notes, such as a [vault](vault.md). A markdown-db corpus is one of those with a root file.
