# Plan and task graph

The plan and its task graph are in osdlc/markdown, in
[`docs/ecosystem-plan.md`](https://github.com/osdlc/markdown/blob/ecosystem-plan/docs/ecosystem-plan.md).
The plan is under review in [osdlc/markdown#4](https://github.com/osdlc/markdown/pull/4).
Keep the work order there, not here.

The full context for the work in this repo is in
[markdown-db-and-harness-reference.md](markdown-db-and-harness-reference.md).

## Plan tasks that touch this repo

- Task 4: the VA-Spec evidence types. This is done in this repo, in a separate change from this
  PR.
- Task 8: switch to `@osdlc/corpus-schema` from npm, and rename the root file to
  `corpus-schema.md`.
- Task 22: `capture` writes study results.
- Task 23: the types move into a claims profile.

## Owner answers, 2026-11-08

The owner answered two naming questions about the evidence design. The current design lives in
osdlc/markdown: the [ecosystem plan](https://github.com/osdlc/markdown/blob/ecosystem-plan/docs/ecosystem-plan.md)
([osdlc/markdown#4](https://github.com/osdlc/markdown/pull/4)), the claims example
([osdlc/markdown#6](https://github.com/osdlc/markdown/pull/6)) and the corpus-schema guide
([osdlc/markdown#7](https://github.com/osdlc/markdown/pull/7)). Where those differ from the answers
below (for example "claims" versus "statements"), the osdlc/markdown documents win.

- q-names: use VA-Spec's model, type names and field names. GA4GH's Variant Annotation spec is a
  JSON Schema model for claims backed by evidence. `references` becomes `documents`, and `evidence`
  becomes `study-results`. Other words come later as name overrides through a profile.
- q-focus: drop the `harness` field from evidence. Study results have no `focus` field. What one is
  about comes from the statements that cite it.
