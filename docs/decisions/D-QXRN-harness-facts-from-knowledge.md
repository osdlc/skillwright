# D-QXRN: Harness facts come from the knowledge file

Accepted 2026-11-09. [Plan](https://github.com/osdlc/skillwright/pull/96).
Amends [D-FDLQ](D-FDLQ-harness-profile-and-translator.md).

## Decision

A harness profile's *fact* values are looked up in the compiled
`harness-knowledge.json` at first use, not written in the row. The file is
compiled from the `harness-reference` claims corpus and embedded in
`tradecraft-core` (`data/harness-knowledge.json`).

- **Facts move:** the user skills folder, the pack manifest path, the
  marketplace manifest path, the plugin-root variable and the naming rule.
- **Policy stays in code:** the packs folder, the dialect, Pi's prefixed
  naming, the override lists, runtime mappings, and the config overrides of
  [D-WNTQ](D-WNTQ-harness-profile-values-from-config.md). The translator trait
  and the profile's public shape are unchanged.
- **A fact stays a constant** with a comment saying why, when the file has no
  value in a usable shape, only a weak or disputed one, or a value that
  disagrees with the row. Install CLI shapes and the project skills folder are
  such constants today.
- **The embedded copy is reproducible.** It is compiled with the placeholder
  commit `main` (`pnpm --filter harness-reference run compile:embed`), and a
  `harness-reference` test fails when it is stale.
- A test holds every lookup a profile makes to a moderate or stronger
  statement and to the value the row held before, so a corpus change that moves
  a fact fails a test instead of changing output.

## Why

D-FDLQ rejected generating the profile because a research note's wording would
become a build input. The knowledge file is not prose: each value has a scope, a
strength and a link to its evidence, and a consumer opts in to weak ones. A
hand-written row can drift from the research without a failing test; a lookup
cannot.

## Options rejected

- Generate the whole row. Policy is not a fact.
- Read the corpus at build time. The file is the only interface.
- A separate Rust crate for the file. No second consumer yet.

## Reopen if

A second Rust consumer needs the file, or the facts kept as constants outnumber
the ones read.
