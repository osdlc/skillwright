# Terms

The words used for skillwright's parts. One file per term. A term marked *planned* comes from an accepted decision and is not built yet.

| Term | Status | Means |
|---|---|---|
| [Vault](vault.md) | current | The folder of markdown notes that is the source of everything skillwright publishes |
| [Note](note.md) | current | One markdown file in the vault |
| [Component](component.md) | current | Anything a note defines: a [marketplace](marketplace.md), [plugin](plugin.md), [skill](skill.md), [command](command.md), [hook](hook.md) or agent |
| [Kind](kind.md) | current | What a component is: marketplace, plugin, skill, command, hook or agent |
| [Marketplace](marketplace.md) | current | A named set of [plugin](plugin.md)s that harnesses install from |
| [Plugin](plugin.md) | current | A group of skills, commands and hooks that installs as one unit |
| [Pack](pack.md) | current | One plugin as written for one harness: its directory, its plugin manifest and its skills, under that harness's tree (for example `.claude-plugin/packs/skillwright/`) |
| [Skill](skill.md) | current | A component an agent loads as instructions |
| [Command](command.md) | current | A component a user invokes by name |
| [Hook](hook.md) | current | A command a harness runs on an event |
| [Harness](harness.md) | current | A coding-agent tool skillwright writes for: Claude Code, Codex, Cursor, Pi, and others planned |
| [Target](target.md) | current | A harness's name as written in `sw-targets` or `--target` |
| [Destination](destination.md) | current | A directory `publish` writes a note into for one [target](target.md), such as a project root |
| [Destination manifest](destination-manifest.md) | current | `.skillwright/manifest.json` at a destination: which files skillwright wrote there, from which notes, with two hashes (source and emitted) |
| [Namespace](namespace.md) | current | The frontmatter prefix skillwright owns, `sw-` by default |
| [Drift](drift.md) | current | A difference between what `build` would write and what is on disk |
| [Rail](rail.md) | current | A guard that refuses a write before any byte is written, most importantly any write into the vault |
| [Adopt](adopt.md) | current, changing | Importing published skills from disk or git into the vault as notes; the reverse of publishing |
| [Marketplace model](marketplace-model.md) | planned | The in-memory object graph a vault loads into: marketplace → plugins → skills, commands, hooks, assets |
| [Load](load.md) | planned | The first stage: read the vault into the [marketplace-model](marketplace-model.md) |
| [Resolved reference](resolved-reference.md) | planned | A link or call in a body, checked during [load](load.md) and pointing at the node it names |
| [Extensions](extensions.md) | planned | Fields of a model node that only one harness reads, held as one validated map per harness |
| [Annotations](annotations.md) | current | Frontmatter keys that are neither skillwright's nor modeled, carried to every harness as written |
| [Translate](translate.md) | planned | The third stage: turn the validated [marketplace-model](marketplace-model.md) into one [file-tree](file-tree.md) per harness, with no I/O ([D-COMG](../decisions/D-COMG-marketplace-model-and-translators.md)) |
| [Harness profile](harness-profile.md) | planned | One row of data per [harness](harness.md): paths, manifest locations, naming rule, which kinds a plugin carries, accepted override keys, alias spellings, install CLI |
| [Harness translator](harness-translator.md) | planned | The code half of a harness: one implementation of a shared trait |
| [File tree](file-tree.md) | planned | What a translator returns: paths mapped to bytes, the execute bit, and the notes each file came from |
| [Emit](emit.md) | planned | The last stage: write a [file-tree](file-tree.md) out |
| [Vfs](vfs.md) | current, moving | The file-access trait every read and write goes through |
| [markdown-vault](markdown-vault.md) | planned | The osdlc crate that models a vault |
