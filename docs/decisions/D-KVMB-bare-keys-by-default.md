# D-KVMB: Frontmatter keys are bare by default

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).
Replaces the earlier rule that skillwright's keys are always prefixed.

## Decision

skillwright's keys are bare: `version`, `targets`, `codex:`. Setting
`build.namespace: sw` in `skillwright.md` restores the prefixed forms, flat
(`sw-version`) or nested (`sw:`). Spec keys are never prefixed.

One closed table lists the keys of each kind. It drives the publish strip,
validation and the generated field reference. A key outside it gets a warning
and is never refused.

## Why

Every note carried a prefix that authors with nothing to mix it with did not
need. The earlier rule had three reasons, and each has an answer:

- *Bare words collide with keys users already have.* The worst two, `kind` and
  `plugin`, are gone, because the folder decides structure. The rest are read
  only on notes already classed as components. A vault where they mean something
  else sets a namespace.
- *The publish strip is one prefix test.* It becomes a closed list per kind. That
  list is also what lets `check` report an unknown key.
- *Spec names collide with spec meaning.* `license` is the one real case. The kind
  tells a pack's licence from the spec field.

## Options rejected

- Keep the prefix and shorten it.
- Bare keys with no namespace option. Vaults that share notes with other tools
  need one.

## Reopen if

Vaults hit collisions often enough that most of them set a namespace.
