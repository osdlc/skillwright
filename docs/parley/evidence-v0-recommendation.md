# Claims and evidence: recommendation

Built from three sources: our own prior art (`evidence-v0-own-prior-art.md`), the outside survey
(`evidence-v0-survey.md`), and GA4GH VA-Spec's core information model.

## Recommendation

Adopt VA-Spec's core model as a reusable **claims profile** of markdown-db: a set of record types
any corpus can use, with its own domain types beside them. Use VA-Spec's names. Keep markdown-db's
one-file-per-record layout and relative links, and depart from VA-Spec only where noted.

### The framework

| Type (folder) | VA-Spec | Holds | Our own prior art it follows |
|---|---|---|---|
| `statements` | Statement + Proposition | the claim: `subject` (link to a domain record), `predicate` (link to a domain question), `object` (the value), qualifiers (`since`, `until`, version range), `direction`, `evidence_lines` | harness-reference facts; fitness `claim` + subject address |
| evidence lines, inside a statement | Evidence Line | `items` (links), `direction` (supports, disputes, neutral), optional `strength`, `warrant` | bug-finder and duplication-scan verdicts; fitness audit verdicts (disputes) |
| `study-results` | Study Result | one observation: `kind`, `document` or `method` link, `locator`, `excerpt` or data, `context` (version, environment, machine), `contributions`. Asserts nothing | harness-reference observations; small-model-evals ledger lines; detect-harness reports |
| `methods` | Method | a procedure: stages, fixtures, positive controls, safety notes, a fingerprint of its files, `maturity` | harness-reference probes; small-model-evals suites and snapshots (`suite_fp`); agent-dev judge eval |
| `documents` | Document | a thing to point at: `url` or `path` (+ `sha256`), `title`, `publisher`, `quality`, `retrieved`, `archive` | fitness `Sources` (quality tiers); #74 references |
| `contributions`, inline on every record | Contribution + Agent | `{agent, role: created, verified or disputed, at}` | sdlc step-evidence ledger (`session`, `timestamp`); review-pr state ref history |

Departures from VA-Spec:

- **No `focus` on a study result.** What evidence is about is decided by the statement that cites
  it, through an evidence line. One observation can back statements about several things.
- **Strength is derived, not stored.** It comes from `kind` and the statement's claim type
  (behaviour or policy), with an optional override on an evidence line that must give a `warrant`.
  This follows osdlc system-model, OKF and the fitness evidence floor.
- **Links, not nested objects or IRIs.** Each VA-Spec object reference is a relative Markdown link
  to a record. A link may also point into a data file that is not a record (see gap G-2).

### The domain

Each corpus brings its own subject and predicate types. They are not part of the framework.

| Corpus | Subject type | Predicate type |
|---|---|---|
| harness-reference | `harnesses` | `subjects` (`skills/project-dirs`) |
| small-model-evals | `models` (by digest) | `jobs` or suites ("good enough for redaction") |
| fitness vault | exercises, muscles, nutrients | `field` + `field_key` |

The fitness vault's subject address `(subject_kind, subject, field, field_key)` is the best
predicate scheme we have and should be the pattern.

### Names

VA-Spec's names are unused in our repos, so they can replace words that mean four different
things today: **evidence** (fitness claim + quote, harness-reference's method kind, sdlc step
ledger, receipt signals), **finding** (lint diagnostic, rule breach, resolver diagnostic, review
comment), **verdict** (one agent's evidence line, an aggregate statement, a classification).
`source` stays as an alias for Document. Severity is impact, not strength, and stays separate.

### Adoption order

1. **harness-reference** (reference adopter). Split each alpha observation into a study result and
   a statement with one evidence line. Probes become methods. Rename #74's types: `references` →
   `documents`, `evidence` → `study-results`; drop the `harness` field; group study results by
   date or document, not harness.
2. **small-model-evals.** Keep the JSONL ledger as study-result storage (9,178 lines is too many
   for one file each). Add `statements` ("model X passes suite Y on machine class Z") whose
   evidence lines cite ledger slices by `suite_fp`, `model.digest` and `env`. Suites become
   methods.
3. **fitness vault.** Add `direction` and `strength` to its Evidence notes, turn audit verdicts
   into dispute lines, and make Sources documents.
4. **sdlc producers** (bug-finder, duplication-scan, verify, review reports): match the field
   names; persist only the statements worth keeping.

## Does markdown-db hold it?

Yes, as a container. Every type is a record type with a JSON Schema, and every VA-Spec reference
is an `x-ref` link. Today's S-GFFY expresses documents, study results and statements with evidence
lines as separate records. It needs these to express the whole profile well:

| Gap | Needed for | In the handoff roadmap |
|---|---|---|
| `x-ref` inside nested objects | evidence lines inside a statement; contributions | item 3 |
| Folder records | methods with stages and fixtures; study results with output files | item 4 |
| `x-warn` | "a statement should have an evidence line"; "excerpt missing" | item 5 (decided) |
| G-1 **Shared schemas across corpora** | the claims profile is defined once and used by every corpus (`allOf` / `$ref` to a published schema, as VA-Spec profiles do) | new |
| G-2 **Links into non-record data** | an evidence line citing ledger lines in a JSONL file (`results/ledger/<suite>.jsonl` with a key), as VA-Spec's `iriReference` does | new |
| G-3 **Rules across records** | a statement's `object` must match its predicate record's value schema; evidence floors such as "≥ 2 lines from ≥ 2 publishers" | new; may belong in a validator plugin, not the spec |
| G-4 **Append-only records** | study results must not change once written (small-model-evals enforces this with hashes) | new; can start as a convention plus a check script |

G-1 and G-3 matter most. Without G-1 each corpus copies the profile; without G-3 a statement's
value is not checked against its question.

## Where it lives

As a second standard next to S-GFFY in skillwright: a markdown-db **claims profile**, with its JSON
Schemas, promoted to osdlc together with markdown-db.
