# tests/fixtures/vault — a frozen test fixture

A small Skillwright vault that the `skillwright` golden build reads
(`tests/build_golden.rs`). It is a fixture, not a product: its content is
frozen to what those tests expect, and it is not kept in sync with any other
vault.

Change it only together with the tests that read it.
