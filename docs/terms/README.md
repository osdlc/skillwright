# Terms

The words used for skillwright's parts. One file per term. A term marked *planned* comes from an accepted decision and is not built yet.

| Term | Status | Means |
|---|---|---|
| [Vault](vault.md) | current | The folder that holds both `skillwright.md` and `marketplaces/`: the source of everything skillwright builds |
| [Note](note.md) | current | One markdown file in the vault |
| [Component](component.md) | current | A [skill](skill.md), [command](command.md) or agent inside a [plugin](plugin.md) |
| [Kind](kind.md) | current | What a note is: marketplace, plugin, skill, command or agent |
| [Marketplace](marketplace.md) | current | A named set of [plugin](plugin.md)s that harnesses install from |
| [Plugin](plugin.md) | current | A group of skills, commands and hooks that installs as one unit |
| [Pack](pack.md) | current | One plugin as written for one harness: its directory, its plugin manifest and its skills, under that harness's tree in `dist/<m>/` (for example `.claude-plugin/packs/skillwright/`) |
| [Skill](skill.md) | current | A component an agent loads as instructions |
| [Command](command.md) | current | A component a user invokes by name |
| [Hook](hook.md) | current | A command a harness runs on an event |
| [Harness](harness.md) | current | A coding-agent tool skillwright builds for: Claude Code, Codex, Cursor and Pi |
| [Target](target.md) | current | A harness's name as written in `build.harnesses` or `--harness` |
| [Drift](drift.md) | current | A difference between what `build` would write and what is in `dist/<m>/` |
| [Marketplace model](marketplace-model.md) | current | The in-memory object graph a vault loads into: marketplace → plugins → skills, commands, hooks, assets |
| [Load](load.md) | current | The first stage: read the vault into the [marketplace-model](marketplace-model.md) |
| [Resolved reference](resolved-reference.md) | current | A link or call in a body, checked during [load](load.md) and pointing at the node it names |
| [Extensions](extensions.md) | current | Fields of a model node that only one harness reads, held as one validated map per harness |
| [Annotations](annotations.md) | current | Frontmatter keys that are neither skillwright's nor modeled, carried to every harness as written |
| [Translate](translate.md) | current | The third stage: turn the validated [marketplace-model](marketplace-model.md) into one [file-tree](file-tree.md) per harness, with no I/O ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)) |
| [Harness profile](harness-profile.md) | current | One row of data per [harness](harness.md): paths, manifest locations, naming rule, which kinds a pack carries, accepted harness-block keys and the plugin-root variable |
| [Harness translator](harness-translator.md) | current | The code half of a harness: one implementation of a shared trait |
| [File tree](file-tree.md) | current | What a translator returns: paths mapped to bytes, the execute bit, and the notes each file came from |
| [Emit](emit.md) | current | The last stage: write a [file-tree](file-tree.md) out to `dist/<m>/`, or diff it for `check` |
| [Vfs](vfs.md) | current, moving | The file-access trait every read and write goes through; osdlc's `vfs-seam` |
| [Corpus](corpus.md) | current | A folder holding a `corpus-schema.md` root file: typed records, their schemas and narrative ([corpus-schema](https://github.com/osdlc/osdlc/blob/main/packages/conformance/corpus-schema/SPEC.md)) |

Terms for the shared vault crates are in [osdlc's terms](https://github.com/osdlc/osdlc/blob/main/docs/terms/README.md).
