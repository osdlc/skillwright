# Harness profile

Status: planned.

One row of data per [harness](harness.md): paths, manifest locations, naming rule, which kinds a plugin carries, accepted override keys, alias spellings, install CLI. Tied to `harness-reference/` by tests ([D-FDLQ](../decisions/D-FDLQ-harness-profile-and-translator.md)).

## In code

Not built yet. Today spread over `RootSpec`, `PluginLayout` and target functions.

## Not to be confused with

A spec. The repo uses "spec" for its rule sets (publish spec, vault spec), never for this.
