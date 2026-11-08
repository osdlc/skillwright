# Handoff: markdown-db and harness-reference

The full context for the next session. The order of the remaining work is drawn in
[release-graph.md](release-graph.md). It is meant to be split later into issues, standard edits
and package docs; until then it is the one place that holds it.

## The goal

harness-reference is a cited record of how coding-agent harnesses differ. It must back code: code
reads its facts as typed values, and every value cites the evidence behind it.

markdown-db is the data format it is written in: a folder of Markdown records with JSON Schema
frontmatter. It is a pure, language-neutral data spec with implementations built around it. It
lives in skillwright until it is promoted to osdlc, and it is meant to be used in other projects
too (for example inherited policies and shared information distributed as corpora).

## Where everything is

| Piece | Path |
|---|---|
| The standard, S-GFFY (v0) | `docs/standards/S-GFFY-markdown-db.md` |
| Its design review, with the reviewer's answers | `docs/parley/markdown-db-v0.json`, `.answers.json` |
| The `corpus` term | `docs/terms/corpus.md` |
| The validator package, `@osdlc/markdown-db` | `packages/ts/markdown-db/` |
| Language-neutral example corpora with expected results | `packages/ts/markdown-db/tests/cases/` |
| Corpus alpha, the frozen prototype (OKF v0.2) | `harness-reference/corpora/alpha/` |
| Corpus beta, on the standard | `harness-reference/corpora/beta/` |
| The evidence design review and its prior-art survey | `docs/parley/evidence-v0.json`, `docs/parley/evidence-v0-survey.md` |
| The evidence writer | `harness-reference/src/beta/capture.ts` |

Run `pnpm run validate:beta` from `harness-reference/`, or `pnpm exec markdown-db validate <folder>`.

## Corpus beta today

| Type | Records | Holds |
|---|---|---|
| `harnesses` | 14 | One per harness, named by product (`Claude Code.md`). Vendor, URLs, license, CLI command, surfaces, status, `successor` / `fork_of` links, `sources` links to references |
| `references` | 52 | One per web page, in publisher folders (`references/Anthropic/…`) |
| `evidence` | 5 | One observation of a page: kind, reference, harness, observed date, optional version, locator and excerpt |

Beta has no subjects or facts yet. Alpha holds 16 harnesses, 65 subjects, 589 facts and 633
observations; code still reads alpha.

## Code that reads the data

- `packages/rust/tradecraft-core/src/harness/profile.rs`: the harness profiles are written by hand.
  Its tests check them against alpha facts read from
  `harness-reference/corpora/alpha/facts/<harness>/<subject>.md`: `skills/project-dirs`,
  `plugins/manifest` and `plugins/marketplace`. Its doc comments also name `skills/user-dirs` and
  `plugins/manifest-fields`; `field_types.rs` comments name `plugins/manifest-fields` and
  `plugins/marketplace-fields`. Checking rather than generating is deliberate: it keeps a research
  note's wording out of the build.
- `packages/rust/skillwright/src/commands/harness/link.rs` and `pi.rs` cite harness-reference in
  comments and errors (which harnesses follow symlinked skill folders).
- osdlc `packages/ts/detect-harness/src/locations.ts` has an uncited `LOCATIONS` table per harness.
  It is the obvious second consumer.

## Next for harness-reference, in order

1. [ ] **Subjects and claims types in beta, with typed values.** A claim is one record per harness
   and subject, for example `skills/project-dirs` as a list of paths, not prose. It cites evidence
   through a top-level `evidence` array of links (`x-ref: evidence`), which S-GFFY v0 can validate.
   Supports / contradicts per evidence needs `x-ref` in nested objects (markdown-db item 3).
2. [ ] **Move the facts code reads first** from alpha to beta (the subjects listed above), capture
   evidence for them, and point the `profile.rs` tests at beta.
3. [ ] **A loader** so code reads validated records without parsing Markdown (markdown-db item 2).
   Then cite beta from `detect-harness`.
4. [ ] Move the remaining alpha facts, then retire alpha.
5. [ ] Experiments: `harness-reference/probes/` become level-2 experiments whose runs write `kind: run`
   evidence (needs markdown-db item 4).

## Next for markdown-db, in order

Items 1 to 3 are the alpha: usable from another project and able to back code.

1. [ ] **Installable from another project.** The package ships `.ts` files and relies on Node 24
   type stripping, which Node does not do inside `node_modules`, so as a dependency it fails.
   - [ ] Build to `dist/` JavaScript with `.d.ts` files, like `packages/ts/tradecraft-wasm`.
   - [ ] Version 0.1.0, drop `private`, add a CHANGELOG, `npm publish --dry-run` in CI.
   - [ ] The owner publishes. Until then a consumer can install a packed `.tgz` or a git dependency.
2. [ ] **A loader API.** `validateCorpus` already holds every record; expose a stable read API
   (`loadCorpus(path) → { types, records by type and id, links }`) and a JSON build target (one file
   per type) so non-TS code can read a corpus.
3. [ ] **`x-ref` inside nested objects.** S-GFFY v0 allows `x-ref` only on a top-level property or
   the items of a top-level array, and a validator must report it anywhere else. Claims need
   `lines: [{ evidence: "[…](…)", stance: supports }]`, so each entry holds its link instead of an
   index into another list (OKF warns against positional keys: agents rewrite lists).
4. [ ] **Folder records.** A folder inside a type that holds `<folder name>.md` is one record, and
   every other file in that folder belongs to it. This follows the `<type>/<type>.schema.md` naming
   rule. Experiments need it: today a `result.json` in a markdown-only corpus is an error.
5. [ ] **`x-warn` (decided).** A schema keyword holding a second schema: the validator checks each
   record against it too, and each failure is a warning, not an error. Example:
   `x-warn: { required: [excerpt] }`. JSON Schema alone can only pass or fail, so today a type cannot
   say "allowed, but tell me"; the validator's only warnings are its built-in ones (wikilinks, links
   that leave the corpus, unknown root keys).
6. [ ] **Checks between fields and paths**: that a folder name matches a field (`publisher`,
   `harness`), and that the date in an id matches `observed`. Today nothing can express these.
7. [ ] **Promotion to osdlc.**
   - [ ] Reconcile `corpus`: osdlc uses it for any folder of linked notes; S-GFFY uses it for a
     markdown-db corpus. Both terms are scoped today (`docs/terms/corpus.md`, `docs/terms/vault.md`).
   - [ ] Add `@osdlc/markdown-db` to the dev repo's `docs/LIBRARIES.md`, which today points
     markdown-as-data validation at `markdown-contract` and records an archived `markdown-db` repo
     folded into it.
   - [ ] State how it relates to `markdown-contract`: markdown-contract checks one document's
     structure; markdown-db checks a corpus (schemas, ids, links, references). See
     [#42](https://github.com/osdlc/skillwright/issues/42), which proposes using markdown-contract
     inside markdown-db for record bodies.

Smaller known issues:

- `@osdlc/markdown-link` misses some relative links (titles, parentheses in the path, non-`.md`
  targets), so the validator keeps a local pattern:
  [osdlc/osdlc#635](https://github.com/osdlc/osdlc/issues/635).
- A `dependentRequired` failure is reported at the record (`(data): …`) rather than at the field.
- The validator checks every relative Markdown link in frontmatter strings, including links inside
  quoted text such as `excerpt.exact`. A quote from a README that contains `[Apache-2.0
  License](LICENSE)` fails, because `LICENSE` is not in the corpus. The `Codex README on the
  license` record works around it by ending the quote before `(LICENSE)`. Fix: check links only in
  `x-ref` properties and the body, or let a schema mark a property as quoted text (`x-quote`), and
  then capture that record again with the whole sentence.
- S-GFFY's Deferred list is unordered. Items 3 to 6 above come from it or from building beta; the
  rest stay deferred: links between corpora and mount / distribution, schema versions and
  migrations, concurrent edits, Obsidian's vault-wide property types, reserved names, stable error
  codes, `move` / `rename` verbs, and other build targets (Astro, Nuxt, Obsidian Bases).

## The evidence model

Evidence is the primitive the other levels build on.

| Level | What | In beta |
|---|---|---|
| 0 Reference | the page: a locator | `references` |
| 1 Evidence | one look at a page: when, which version, the excerpt | `evidence` |
| 2 Experiment | a folder with a procedure, fixtures and safety notes; each run writes level-1 evidence | not yet (`harness-reference/probes/`) |
| 3 Synthesis | a narrative citing many records | not yet (alpha `sources/`) |

Rules taken from the survey, where both passes agreed:

- No stored confidence score. Strength comes from `kind`, ranked by how direct it is, and the
  ranking depends on what the claim is about: for behaviour, run > source at a pinned commit >
  help text > changelog > docs; for policy (deprecated, recommended), docs and changelog first.
- Supports / contradicts lives on the claim's link to the evidence, not on the evidence, because one
  observation can support one claim and contradict another.
- A negative claim ("does not read AGENTS.md") needs a run with a positive control.
- The date we looked (`observed`) is kept apart from the source's own date.
- Excerpts are `{exact, prefix, suffix}` (W3C text-quote selector), so a re-check can report
  present / moved / gone. Source code is pinned to a commit, never `blob/main`; all three license
  references in beta (Claude Code, GitHub Copilot CLI, Grok Build) are pinned.
- Evidence records are append-only; a correction or a later contradicting run is a new record.
- A type ships with a writer and a reader (the dev repo's D-ORMG rule). `capture` is the first writer.

Kinds and their writers:

| Kind | Writer |
|---|---|
| `docs`, `changelog`, `source`, `issue`, `third-party` | `capture`: fetch the URL (a GitHub `blob/` URL as the raw file, a repository URL as its README source), check the excerpt is there, store it with the date |
| `help-text` | `capture` with a command instead of a URL (not built) |
| `run` | an experiment (not built) |
| `inferred` | a person or agent by hand, citing what it infers from |

The review questions on `docs/parley/evidence-v0.json` were not answered. Beta uses each question's
recommended option; all are open:

| Question | Default used |
|---|---|
| q-split | reference and evidence are two types |
| q-stance | stance on the claim's link (not exercised yet) |
| q-score | no stored strength |
| q-kind | alpha's kinds plus `issue` and `third-party` |
| q-excerpt | optional; the warning needs `x-warn` |
| q-archive | optional `archive` and `url_status`; `capture` does not request snapshots yet |
| q-immutable | append-only, by convention |
| q-folder-records | add to S-GFFY now (not done; markdown-db item 4) |
| q-claims | claims as records with a top-level `evidence` array (not built) |
| q-layout | references in publisher folders |
| q-first-writer | `capture` first, experiment runs second |

## Lessons for whoever continues

- **Stacked PRs merged into dead branches.** Twice a PR stacked on another merged into the base
  PR's branch after that branch had already merged, so its work never reached `main` and needed a
  carry PR ([#37](https://github.com/osdlc/skillwright/pull/37),
  [#38](https://github.com/osdlc/skillwright/pull/38)). The repo setting "Automatically delete head
  branches" is off; turning it on makes GitHub retarget stacked PRs. Until then, retarget a stacked
  PR to `main` before merging it.
- **Uncommitted work in a worktree can be removed by a cleanup.** Commit and push early, even WIP.
- **The release PR's wasm conflicts resolve themselves**: the release workflow rebuilds its branch
  from `main` and commits a fresh wasm on each push to `main`.
