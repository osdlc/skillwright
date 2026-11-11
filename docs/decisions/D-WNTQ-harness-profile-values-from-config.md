# D-WNTQ: A harness profile value can come from config

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).
Amends [D-FDLQ](D-FDLQ-harness-profile-and-translator.md).

## Decision

A harness profile stays one row of data per harness, but a vault can set one
of its values:

- `<harness>: {naming: bare | prefixed}` on a harness whose profile allows it
  (Cursor, Pi) changes how that harness names a plugin's skills. It may be set
  in the root file, on a marketplace or on a plugin, and inherits key by key,
  so the nearest value wins. On a component it is refused.

The profile value is the default: `prefixed`, so Cursor and Pi name a skill
`<p>-<s>`. Every other profile value is fixed. `packs` is not settable: each
harness's packs folder is fixed under `dist/<m>/`, and a `packs` key in a
harness block is refused as a removed key.

## Why

Marketplaces need skill names to differ on Cursor and Pi, and naming is a
choice about how a marketplace presents its skills, not about what the harness
can read. D-FDLQ made every profile value fixed.

## Options rejected

- A new profile per variation.
- A general override of any profile value. Most values are facts a harness
  decides, and setting them would write trees the harness cannot read.
- A settable packs folder. A fixed output layout is simpler to check and to
  copy, and nothing needs another place.

## Reopen if

A second value needs the same treatment. Then decide which profile values are
settable as a rule, not one at a time.
