# data

Files `tradecraft-core` reads at build time. Two are vendored copies. The source of both is
[osdlc/osdlc](https://github.com/osdlc/osdlc): the harness reference, a corpus of cited
statements about coding-agent harnesses, in `packages/corpora/harness-reference`, and the file
compiled from it, which the npm package
[`@osdlc/harness-knowledge`](https://www.npmjs.com/package/@osdlc/harness-knowledge) ships.
A fact is corrected there, not here.

| File                     | What it is                                                                                                                                                                                                                                                     |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `harness-knowledge.json` | A vendored copy of part of `@osdlc/harness-knowledge`'s `data/harness-knowledge.json`, at the version the root `package.json` pins. Its `corpus` block names the repository, folder and commit of the corpus it was compiled from. Written by a script, not by hand. |
| `embedded-slice.json`    | Ours. The harnesses and subjects the copy keeps.                                                                                                                                                                                                              |
| `version-cases.json`     | A vendored copy of `packages/corpora/harness-reference/src/corpus/version-cases.json`. The crate's tests read it, so its version rules match the corpus's. Copied by hand.                                                                                    |

## Refresh the copy

From the repository root:

```sh
pnpm install
pnpm run harness-knowledge:slice
```

`scripts/harness-knowledge-slice.ts` reads the installed package, keeps the harnesses and
subjects in `embedded-slice.json`, drops each subject's `valueSchema` and writes the file compact.

- To take newer data, raise `@osdlc/harness-knowledge` in the root `package.json` first.
- To embed another subject, add it to `embedded-slice.json` first. Both lists stay sorted.

`scripts/harness-knowledge-slice.test.ts` (`pnpm run test:scripts`, run by CI) fails when the
copy is not what the script writes from the pinned version. The crate's tests fail when
`profile.rs` looks up a subject the slice lacks, or when a looked-up fact changes.
