# tests/fixtures/identity-vault — a frozen test fixture

A vault that sets every marketplace and plugin identity field, so the golden
build in `build_golden.rs` pins how each one reaches a manifest.

- `skillwright.md` at the vault root sets every marketplace key.
- `marketplaces/guild/skillwright.md` overrides some of them, leaves the rest,
  and carries a block for each harness.
- `plugins/full/full.md` sets every plugin key.
- `plugins/bare/bare.md` sets none.

Change it only together with the tests that read it.
