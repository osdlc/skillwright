# Extensions

Status: current.

Fields of a model node that only one harness reads, held as one validated map per harness. Sourced from a note's harness blocks, such as `claude:` ([D-HJCZ](../decisions/D-HJCZ-harness-blocks.md)).

## In code

`Extensions` in `model/mod.rs`, read from the harness blocks and validated per harness by `fold_harness_blocks` (`model/cascade.rs`) during [load](load.md). Applied by `projection_of` in `harness/translator.rs`.

## Not to be confused with

[annotations](annotations.md), which every harness receives.
