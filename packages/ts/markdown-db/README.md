# @osdlc/markdown-db

A validator for markdown-db corpora. A corpus is a folder of typed records kept as Markdown, YAML
and JSON files, with a schema for each type, that people edit as plain notes. This package
implements [S-GFFY: markdown-db, version 0](../../../docs/standards/S-GFFY-markdown-db.md). Every
rule it checks comes from that standard or from the corpus's own files.

## Install

The package is private for now and is used inside this workspace. Add it to a workspace package's
dependencies:

```json
{ "dependencies": { "@osdlc/markdown-db": "workspace:*" } }
```

Node 24 or later runs the TypeScript source directly. There is no build step.

## Command line

```sh
markdown-db validate [--fix] <folder>...
```

`validate` checks every corpus at or below each folder. It prints one line per problem, as
`<file>: <field>: <message>` with the file relative to the corpus root, and `ok: <name>: <counts>`
for each corpus that passes. `--fix` first rewrites every wikilink it can resolve as a relative
Markdown link.

| Exit code | Meaning                                                   |
| --------- | --------------------------------------------------------- |
| `0`       | Every corpus is valid. Warnings may have been printed.    |
| `1`       | A corpus has an error, or a folder holds no corpus.       |
| `2`       | Bad usage: an unknown verb, or no folder.                 |

## Library

```ts
import { findCorpora, validateCorpus } from '@osdlc/markdown-db'

for (const root of findCorpora('Data')) {
  const report = validateCorpus(root) // { name, id, records, errors, warnings, fixed }
  for (const e of report.errors) console.error(e)
}
```

`src/index.ts` also exports `readData` and `idProblem`, and the link helpers the validator uses:
`extractLinks`, `wikilinkIndex`, `matchWikilink` and `markdownLink`.

## Tests

`pnpm run test`. `tests/cases/` holds corpora with their expected reports, written so that a
validator in another language can run the same cases.

## Known gap

`@osdlc/markdown-link` finds only some Markdown links: only `.md` and media targets, with no titles
and no parentheses in the path. S-GFFY checks every relative link, so `src/links.ts` finds Markdown
links with its own pattern. [osdlc/osdlc#635](https://github.com/osdlc/osdlc/issues/635) tracks
the fix; the local pattern goes once it lands.
