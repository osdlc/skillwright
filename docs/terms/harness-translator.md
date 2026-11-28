# Harness translator

Status: current.

The code half of a harness. One translator does the common work from the [harness-profile](harness-profile.md); what is a harness's own code, mostly its manifests, is a `match` on the harness ([D-FDLQ](../decisions/D-FDLQ-harness-profile-and-translator.md)).

## In code

`Translator` in `harness/translator.rs`, keyed by a `Harness`.
