# Translate

Status: current.

The third stage: turn the validated [marketplace-model](marketplace-model.md) into one [file-tree](file-tree.md) per harness, with no I/O ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)).

## In code

`HarnessTranslator::translate` (whole marketplace) and `translate_component` / `translate_pack` (publish) in `harness/translator.rs`.
