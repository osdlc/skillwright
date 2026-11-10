# Harness translator

Status: current.

The code half of a harness: one implementation of a shared trait. Default methods do the common work from the [harness-profile](harness-profile.md); a harness overrides what is its own code, mostly manifests and hook rules ([D-FDLQ](../decisions/D-FDLQ-harness-profile-and-translator.md)).

## In code

The `HarnessTranslator` trait and its impls in `harness/translator.rs`; `translator_for` picks one by name.
