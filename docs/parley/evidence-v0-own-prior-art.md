# Claims and evidence: our own prior art

A scan of small-model-evals, osdlc, the dev monorepo and skillwright, 2026-11-08, mapped onto
GA4GH VA-Spec's core classes. Plans: **(a)** adopt the shared model, **(b)** feed a corpus,
**(c)** stay as is. Read from `origin/main` (small-model-evals `97850cf`, osdlc `943891bf`,
skillwright `0f8b1f4`, dev `e97111bc88`).

## sksizer/small-model-evals

- **Ledger, snapshots, environments, model builds** (`scripts/ledger.ts`,
  `results/ledger/<suite>.jsonl`, `results/suites/<suite>/<fp>.json`, `results/environments/`,
  `results/models/<digest>.json`, `results/conditions/<run>.json`). 9,178 append-only lines over 19
  suites. Cases are immutable (superseded via `metadata.supersedes`, moved to `cases/retired/`).
  Snapshots freeze prompts, providers, scorer hashes and case hashes (`suite_fp`), enforced by
  `scripts/check.ts`.
  - `run`, `at` → Contribution. `machine`, `env`, runtime versions → study-result context.
    `model{label, tag, digest}` → what the result is about. `suite`, `suite_fp`, `case` → Method.
    `pass`, `score`, `latency_ms`, `tokens`, `metrics`, `reason`, `output`, `detail` → Study Result
    data. Conditions (machine load) → result-quality qualifiers.
  - No statements as records: "model X is good enough for job Y" lives only in `results/summary.md`,
    site pages and prose in `docs/plans/*.md`.
  - Plan (b) then (a): keep the ledger as study-result storage; add statements citing ledger slices.
- **Model catalogue** (`scripts/catalog.ts`): url, publisher, licence, parameters, paper. Document
  plus uncited facts. (b)
- **Corpus image provenance** (`corpus/sources.yaml`): licence, credit, derived-from. (c)

## osdlc

- **system-model Evidence** (`packages/ts/system-model/src/aspect.ts`): `{kind, tier, source,
  found}`, tier ranks directness, explicitly no confidence score. Closest field-level match.
- **Conformance suites** (`packages/conformance/<family>/suite.json`): reference implementation
  and version, cases with expected values. Method plus expected results; test runs are unpersisted
  study results. (c), optionally (b) for "implementation conforms" statements.
- **detect-harness report** (`packages/ts/detect-harness/src/report.ts`, D-05): provenance on
  every fact (`scope`, `origin`, file), `sources[]` with parse status, `install` with version. Each
  report is a study result of one machine. (b) as a run producer for harness-reference.
- **dashboard bench** (`bench/`): a method whose results (`bench/results/runtime.json`) are not
  committed, behind a rule in `docs/modules.md`. Statement with its evidence missing. (b)
- **markdown-contract Finding**: a lint diagnostic, not a claim. (c); name clash.

## dev monorepo

- **Fitness vault Evidence and Sources** (`packages/vaults/fitness/vault/Evidence/`, 2,760 notes;
  `vault/Sources/`, about 737; schema `solutions/determined/crates/determined-fitness/src/schema/evidence.rs`;
  decisions D-0020 corpus claims carry evidence, D-0030 provenance has two faces). Evidence note:
  `subject` + `subject_kind` + `field` + `field_key` (Proposition subject and predicate), `claim`
  (statement text), `quote`, `source` (Document), `pack_id`, `updated_at` (Contribution). Direction
  is always supports. Sources carry `quality` tiers and `retrieved_at`. Evidence floor: ≥ 2
  citations from ≥ 2 publishers, ≥ 1 not Wikipedia. Audit shards hold dispute verdicts with a
  corrected value. (a)
- **sdlc step-evidence ledger** (`solutions/ontological/lib/services/pr/evidence.ts`,
  `.sdlc/pr-evidence/<head_sha>.jsonl`): `{step, head_sha, result, timestamp, session}`. Method,
  study result, Contribution; the merge gate's decision is the statement. (c) storage; match names.
- **verify findings** (`lib/services/verify/reports/changes/schema.ts`): `{sourceId, file, line,
  evidence (quote), why, severity}`, plus a verdict. Subject, predicate "breaches", a quoted item,
  an evidence-line rationale. (a) for names.
- **agent-dev judge eval** (`packages/ts/agent-dev/src/eval/`): planted breaches, matched / missed
  / spurious scoring, engine and model. Method and study result, unpersisted. (b)
- **review-pr state ref** (`refs/review-pr/<pr>`, 232 refs): findings with `open | fixed |
  declined`, history as git commits with compare-and-swap. A pattern for contribution history. (c)
- **bug-finder validate** (`*.workflow.js`, `docs/planning/bug-sweeps/…/findings.jsonl`):
  several agents' `{refuted, reason, evidence: file:line, severity}` tallied into confirmed or
  refuted. Evidence lines with direction, one per agent. (a)
- **duplication-scan** arbiter verdicts and objections in rounds: same pattern. (a)
- **task claims, review reports, friction, research notes** (S-0014 requires observation-based
  findings with version and fixture), **D-3PDM evidence attachment**, delivery receipt, pr-check
  verdicts, plugin eval suites, parley answers: mostly (b) or (c).

## skillwright

- **harness-reference alpha**: 589 facts, about 633 observations, 223 open questions, 6 probes. An
  observation merges a statement (`value`, `since`, `until`), an evidence line and a study result
  (`observed`, `source`, `via`, `locator`, `checked_version`, `evidence` kind, `probe`, `model`).
  Missing: direction and strength. (a), reference adopter.
- **harness-reference beta**: already markdown-db. (a) first.
- **tradecraft-core `profile.rs`**: a consumer; its tests check hand-written rows against facts.

## Strongest patterns

1. harness-reference observation + probe: split into study result, statement and evidence line.
2. Fitness Evidence/Source: the subject address, verbatim quotes, source quality, evidence floors,
   dispute verdicts.
3. small-model-evals provenance: immutable cases, hashed snapshots, environment and build files,
   append-only ledger. Lacks statements.
4. bug-finder / duplication-scan adversarial validation: per-agent supports / disputes lines.
5. sdlc step ledger keyed by sha, and review-pr's git-native history.

## Name clashes

- **evidence**: fitness claim + quote + source; harness-reference method kind; sdlc step ledger;
  receipt signals. None is VA-Spec's Evidence Line.
- **finding**: markdown-contract diagnostic; verify rule breach; task-claims diagnostic; review-pr
  comment.
- **verdict**: one agent's evidence line; an aggregate statement; a classification.
- **observation**: harness-reference's merged record; unrelated session hook wiring.
- **claim**: fitness statement text; sdlc task prose assertions.
- **source**: consistently a Document; keep as an alias.
- Statement, Method, Study Result and Contribution are unused, so they can be introduced cleanly.
- Strength-like fields are scattered (confidence, quality, research_status); severity is impact.

Not covered: dashboard bench results (uncommitted); `apps/ontological-legacy` and
`experiences/system-graph` evidence docs were only touched.
