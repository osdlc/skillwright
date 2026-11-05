# Cases

Each folder holds one case: `corpus/`, a markdown-db corpus, and `expected.json`, what validating it
reports. A case names no code, so a validator in any language can run the same cases.
`tests/cases.test.ts` runs them here.

`expected.json` holds `errors` and `warnings`: every line the validator reports, in order, with
files relative to the corpus root.
