# D-WNTQ: A harness profile value can come from config

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).
Amends [D-FDLQ](D-FDLQ-harness-profile-and-translator.md).

## Decision

A harness profile stays one row of data per harness, but a marketplace can set
two of its values for one build:

- `<harness>: {packs: <path>}` moves that harness's packs folder inside the
  output.
- `<harness>: {naming: bare | prefixed}` on a harness whose profile allows it
  (Cursor, Pi) changes how that harness names a plugin's skills, on a
  marketplace or a plugin. The plugin's value wins.

The profile value is the default. Root catalogs stay fixed, because a harness
finds them by their fixed path. `packs` is validated once, on the value that
takes effect, and is refused if it overlaps a manifest path, a harness-owned
folder or a source folder.

## Why

Marketplaces need these two values to differ, and both are facts about where a
marketplace puts its files, not about what the harness can read. D-FDLQ made
every profile value fixed.

## Options rejected

- A new profile per variation.
- A general override of any profile value. Most values are facts a harness
  decides, and setting them would write trees the harness cannot read.

## Reopen if

A third value needs the same treatment. Then decide which profile values are
settable as a rule, not one at a time.
