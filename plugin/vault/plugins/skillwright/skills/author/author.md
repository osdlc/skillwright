---
name: author
description: "Author coding-agent skills, plugins and marketplaces as markdown notes in a skillwright vault: the folder layout, the frontmatter (`description`, `name` and the `sw-*` keys), and the verbs that create and reorganise notes (`new`, `rename`, `move`, `delete`, `convert`, `bump`, `adopt`, `list`). Use when asked to write a new skill, add a plugin or marketplace, rename or move a skill, bump a plugin version, or bring existing SKILL.md files into a vault."
---

# /[[author]]

Scope: writes and reorganises vault notes. Publishing and building are `/[[publish]]`. If
`skillwright --version` fails, run `/[[setup]]` first.

## The vault

A vault is a folder of markdown notes. Where a note sits decides what it is:

| Path | Read as |
|---|---|
| `skills/<name>.md` | a skill |
| `skills/<name>/<name>.md` | a skill that carries the files beside it (scripts, references) |
| `skills/<name>/SKILL.md` | the same, named by its folder |
| `plugins/<p>/<p>.md` | a plugin; its skills go in `plugins/<p>/skills/` |
| `marketplaces/<m>/<m>.md` | a marketplace; its plugins go in `marketplaces/<m>/plugins/` |

So `marketplaces/<m>/plugins/<p>/skills/<s>.md` is the full shape. A note under no kind folder is
a resource or the author's own writing, and is never published.

## Frontmatter

- `description` opts a note in. A note without it is skipped. Write it in the third person: what
  the skill does and when to use it. The limit is 1024 characters.
- `name` is optional and defaults to the file name. Names are lowercase words joined by hyphens,
  at most 64 characters.
- `sw-kind` overrides the kind the folder implies.
- `sw-plugin` puts a skill in a plugin when the folder does not.
- `sw-targets` limits a note to some harnesses, for example `[claude-code, codex]`. Absent means
  every destination.
- `sw-version` is a plugin's semver. Set it on every plugin note; a build warns when it is missing.
- `sw-owner` and `sw-author` go on a marketplace note.

The `sw-` keys are stripped on publish. Everything else, such as `allowed-tools`, passes through
to the harness. The `sw` prefix is the project's `namespace` setting.

## Links

- `[[note]]` links to another note or a file beside the skill. The build rewrites it as a plain
  markdown link, and copies the files a skill folder holds.
- `\/[[skill]]` calls another skill. The build writes each harness's own form, such as
  `/plugin:skill` for Claude Code, `$plugin:skill` for Codex and `/skill:plugin-skill` for Pi.
  Write calls this way, not in one harness's form, so one note works in every harness.
- To show a call as literal text, put a backslash before its slash. A fenced code block does
  not escape it; the build still resolves the call.

## Steps

1. Run `skillwright list` to see what the vault holds: path, kind, name, plugin and targets.
2. Make the change with a verb. Run it with `--dry-run` first, then again with `--yes`:

   | Verb | Does |
   |---|---|
   | `new marketplace <name> --description <text> [--owner <text>] [--author <text>]` | a marketplace note |
   | `new plugin <name> --marketplace <name> --description <text>` | a plugin note inside a marketplace |
   | `new skill <name> --description <text> [--plugin <name>] [--marketplace <name>] [--targets <t>]` | a skill note |
   | `rename <entry> <new-name>` | renames a note and rewrites every link to it |
   | `move <entry> --to-plugin <p> [--to-marketplace <m>]` | moves a component to another plugin |
   | `delete <entry> [--force]` | moves a note to the vault's trash; `--force` also trashes a non-empty container's members |
   | `convert <entry> --kind <kind>` | changes a note's kind and moves it into that kind's folder |
   | `bump <plugin> [--major\|--minor\|--patch]` | raises a plugin's `sw-version`; patch is the default |
   | `adopt [source...] [--plugin <p>] [--into <path>]` | turns SKILL.md folders already on disk into notes; `--from-git owner/repo[/path][@ref]` reads a repository |

   An `<entry>` is a note's `name`. Edit a note's body directly; no verb is needed for that.
   `adopt --as` and `adopt --promote` exit `not implemented`.
3. Run `skillwright list` again and confirm the note shows with the kind, plugin and targets you
   meant. A note listed as `skipped` names the fix.
