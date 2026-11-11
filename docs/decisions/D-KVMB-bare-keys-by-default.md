# D-KVMB: Frontmatter keys are bare

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).
Replaces the earlier rule that skillwright's keys are always prefixed.

## Decision

skillwright's keys are bare: `version`, `targets`, `codex:`. There is no
namespace option and no key prefix. A prefixed key, such as `sw-version` or an
`sw:` block, is an unknown key. Spec keys are never prefixed.

One closed table lists the keys of each kind. It drives the build's strip,
validation and the generated field reference.

A key outside the table warns and is not refused. On a component it ships
under `metadata:`; on a container or in the root file it is ignored. There are
two exceptions: the removed keys `workspace` (in the root file) and `packs` (in
a harness block) are refused.

A key the table lists is refused where it does not belong:

- in the root file, a key that is not inherited;
- `name`, on any note: the folder is the name;
- `build:`, outside the root file;
- a removed `build:` key, such as `out`, `namespace` or `hooks`.

## Why

Every note carried a prefix that authors with nothing to mix it with did not
need. The earlier rule had three reasons, and each has an answer:

- *Bare words collide with keys users already have.* The worst two, `kind` and
  `plugin`, are gone, because the folder decides structure. The rest are read
  only on notes already placed as components, and an unknown key only warns.
- *The build strip is one prefix test.* It becomes a closed list per kind. That
  list is also what lets `check` report an unknown key.
- *Spec names collide with spec meaning.* `license` is the one real case. The kind
  tells a pack's licence from the spec field.

A key the table lists in the wrong place is a mistake the author can fix, so it
is refused rather than warned about.

## Options rejected

- Keep the prefix and shorten it.
- A namespace option (`build.namespace`) that restores the prefix. It doubled
  every key's spelling for no vault that needed it.

## Reopen if

Vaults hit key collisions often enough to need a prefix.
