# tests/fixtures/identity-vault — a frozen test fixture

A vault that sets every marketplace and plugin identity field, so the golden
build in `build_golden.rs` pins how each one reaches a manifest.

- `skillwright.md` at the vault root sets the shared defaults: `author`,
  `license`, `repository` and `homepage`.
- `marketplaces/guild/guild.md` sets every marketplace field, overrides the
  root file's `author`, and carries a block for each harness.
- `marketplaces/guild/plugins/full/full.md` sets every plugin key.
- `marketplaces/guild/plugins/bare/bare.md` sets none.

Change it only together with the tests that read it.
