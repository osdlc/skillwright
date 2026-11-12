# Pack

Status: current.

One plugin as written for one harness: its directory, its plugin manifest and its skills, under that harness's tree in `dist/<m>/` (for example `.claude-plugin/packs/skillwright/`).

## In code

`PluginPolicy::packs` in `harness/profile.rs`; written by `Translator::write_pack`.

## Not to be confused with

A [plugin](plugin.md), which is the harness-neutral thing in the vault.
