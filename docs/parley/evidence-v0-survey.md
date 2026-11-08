# Evidence primitive: survey of prior art

Two research passes, 2026-11-05. Part 2 was written from memory with no pages fetched; items marked
[unverified] need checking before they go in a durable doc.

## Our own work (look here first)

| Where | Model | Take |
|---|---|---|
| osdlc `packages/ts/system-model/src/aspect.ts` | `Evidence {kind, tier, source, found}`; `source` = what was inspected, findable; `found` = raw value, not a verdict; `tier` ranks directness 1-4; claim tier = min over its evidence. Comment: "Deliberately not a confidence score." | Closest pattern. Align with it. |
| dev sdlc `reference` entity | `RF-NNNN`, title, url, related (cited-by), body Summary/Material/Notes. Its definition says a Reference is a curated catalog entry, not Evidence. | Level 0 sibling. No retrieved date, version, excerpt. |
| dev `D-ORMG` | Reserves an `EV` Evidence entity, deferred. Rule: a type ships with one op that writes it and one projection that reads it. | Evidence needs a writer from day one (experiment runs). |
| `sksizer/ontological` evidence | `relates_to_id/type`, free-string `source`, `confidence` high/medium/low. Only instance was broken. | Avoid: polymorphic back-pointer, free-string kind, confidence enum. |
| skillwright alpha | `sources[] {id, resource, title}`; observations inside each fact carry `source`, `locator`, `evidence` enum (docs, source, help-text, run, changelog, inferred), `checked_version`, `probe`; `probes/<id>/probe.md` + `fixture/`. | Evidence is mixed into the claim; runs leave no record; no excerpt field; locators use `blob/main`. Enum and probe shape carry over. |
| S-GFFY | `x-ref` only on top-level properties and top-level array items. | Claims must be records with a top-level `evidence: [links]`, or nested x-ref enters the standard. |

## Provenance and claim models

- **W3C PROV-O**: entity / activity / agent; plan vs activity. Check every record answers: what was
  observed, by what act, by whom. Experiment = plan, run = activity.
- **Wikidata references**: stated in (P248), reference URL (P854), retrieved (P813), quote (P1683).
  Time qualifiers on the statement, retrieval date on the reference. Ranks + deprecation reason keep
  superseded facts.
- **Nanopublications**: assertion / provenance / pubinfo. Separate provenance of the fact from
  provenance of the record. Records immutable; corrections are new records. Content hashes.
- **Micropublications** (Clark et al. 2014): minimal form (claim + attribution) up to full; supports /
  challenges on the edge; methods and materials as first-class. Validates the 0→3 cascade.
- **SEPIO**: the evidence line. Direction (supports / disputes / inconclusive) and strength belong
  to the claim→evidence link, not the item, because one item can support one claim and dispute
  another.
- **ECO / GO evidence codes**: author statement (traceable vs not) vs experimental vs curator
  inference; manual vs automatic assertion. [exact ids unverified]
- **schema.org ClaimReview**: reviewing someone else's claim. Only relevant if we fact-check lore.

## Grading

- **GRADE / CEBM**: certainty belongs to the conclusion across all evidence. The ladder depends on
  the question type. "Indirectness": vendor docs are indirect evidence of what a binary does.
- **Admiralty code**: source reliability (A-F) and information credibility (1-6) are separate.
- **CVSS Report Confidence, STIX confidence and Sighting** (first_seen, last_seen, count).
- **OKF v0.2** refuses a credibility score ("subjective, unportable, goes stale"); separates
  `generated` from `verified`; attestation for runs.
- Ranking for behaviour claims: reproduced run > run > source at pinned commit > help output >
  changelog > docs > vendor staff in issues > third party. For policy claims, docs and changelog
  rank first. Negative claims need a positive control.

## Locators, excerpts, link rot

- **W3C Web Annotation**: `TextQuoteSelector {exact, prefix, suffix}`; `TimeState {sourceDate,
  cached}`. Avoid DOM selectors.
- **Hypothes.is**: quote + 32-char prefix/suffix; fuzzy re-anchoring gives present / moved / gone.
- **Wikipedia cite web / CSL-JSON**: source's own date (`issued`, `date`) vs when we read it
  (`accessed`, `access-date`); `archive-url`, `url-status`; `version`; typed locator (`line`,
  `section`, `timestamp`).
- **Memento (RFC 7089)**: store the archive as a URI-M (a Wayback URL with a timestamp).
- **SWHID / git permalinks**: repo + commit + path + lines. Never `blob/main`.

## Experiments

- **Popper convention**: experiment folder with fixed stages setup / run / validate / teardown;
  `validate` turns output into a verdict per claim.
- **ACM artifact badges**: available → functional → reusable → reproduced / replicated.
- **RO-Crate run profile**: `CreateAction {instrument, object, result, agent, startTime, endTime,
  actionStatus}`.
- **MLflow / DVC**: run record with run id, procedure commit, params, environment, status; hash the
  fixtures.
- **in-toto / SLSA**: subject (harness@version, digest) + predicate type + predicate; test-result
  predicate ≈ run outcome. [field list unverified]
- **ReScience**: a replication is its own record and can be negative.

## Argument structure

- **Toulmin**: a short `warrant` on the link says why this evidence bears on the claim.
- **AIF / Pollock**: rebutting (evidence for not-X) vs undercutting (the support link is broken,
  e.g. wrong version). `contradicts` plus a note is enough to start.

## Where the two passes agree

1. No stored confidence score on evidence (OKF, system-model, ontological's failure, GRADE).
2. Stance on the claim→evidence link, not on the evidence (SEPIO, Micropublications).
3. Small flat `kind` enum ranked by directness (system-model tier, ECO, alpha's enum).
4. Separate observed date from the source's own date, and from record authoring.
5. Excerpt as exact + prefix + suffix; archive as a URI-M; source code pinned to a commit.
6. Records immutable; a correction or later contradicting run is a new record.
7. Experiment = alpha probe + Popper stages + a run record per run.
8. Ship evidence with a writer (D-ORMG): experiment runs and a capture verb.

## Where they differ

- Part 1: a separate reference record (the page) that many evidence records (observations of it)
  x-ref. Part 2: the locator lives inside each evidence record.
