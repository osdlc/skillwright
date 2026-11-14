# Harness profile

Status: current.

One row of data per [harness](harness.md): paths, manifest locations, naming rule, which kinds a pack carries, accepted harness-block keys and the plugin-root variable. Tied to [`harness-reference`](https://github.com/osdlc/osdlc/tree/main/packages/corpora/harness-reference) (now in osdlc/osdlc) by tests ([D-FDLQ](../decisions/D-FDLQ-harness-profile-and-translator.md)).

## In code

`HarnessProfile`, `PluginFormat` and `PROFILES` in `harness/profile.rs`; looked up by name with `profile_named`.

## Not to be confused with

A spec. The repo uses "spec" for its rule sets, such as the vault structure spec, never for this.
