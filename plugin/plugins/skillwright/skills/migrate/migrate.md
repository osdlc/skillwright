---
name: migrate
description: "Move a skillwright vault to the current structure: `skillwright.yaml` to `skillwright.md`, prefixed frontmatter keys to bare, `sw-kind` and `sw-plugin` to folders, `sw-overrides` to harness blocks, then refresh adopt hashes and verify with list and check. Use when a repository has a `skillwright.yaml` or `tradecraft.yaml`, when notes carry `sw-*` or `tc-*` keys, when skillwright refuses a `kind` or `plugin` key, or when asked to upgrade a vault."
---

# /[[migrate]]

Scope: moves one vault. If `skillwright --version` fails, run `/[[setup]]` first.

skillwright has no `migrate` verb and reads none of the old shape. A leftover `skillwright.yaml` or
`tradecraft.yaml` gets a note saying it is no longer read. With no namespace set, a note with a `kind`
or `plugin` key, or a `sw-<key>` spelling of a key skillwright owns, is refused `removed-key`, and
`build` and `check` refuse to run until it is gone. A `sw-` key that matches nothing is only an
unknown key. Do the whole move in one pass, on a branch.

The tables below use the `sw` stem. A `tc-` vault and a `tradecraft.yaml` move the same way, but only
`sw-` keys are refused when left behind: a leftover `tc-` key is an unknown key, so search for them.

A note another tool generates or seals, such as one with a hash or a generated marker, must change at
its generator. Do not hand-edit it: the next run undoes the edit, or the seal breaks. Change the
generator and regenerate.

## Steps

1. List what is there before changing anything. Search the repository for `skillwright.yaml`,
   `tradecraft.yaml`, `sw-`, `sw:`, `tc-` and `tc:` in project files, notes, scripts and CI.

2. Decide on the keys. Bare keys are the default. If the notes share their frontmatter with another
   tool and `version` or `targets` means something else there, keep the prefix: set
   `build.namespace: sw` in step 3 and skip the key renames in step 5. Under a namespace every key
   skillwright owns takes the prefix, including the blocks in steps 6 and 7: write `sw-claude:` and
   `sw-adopted:`, never `claude:` and `adopted:`. A bare `claude:` would miss the harness, and a bare
   `adopted:` would ship its source path under `metadata:`. The warning for each names the right
   spelling.

   Keeping the prefix still changes two things. Rename `sw-display-name` to `sw-display_name`. On a
   skill, agent or command, delete `sw-version`, `sw-author`, `sw-owner`, `sw-license`,
   `sw-keywords`, `sw-category` and `sw-display-name` if they ship nothing, as step 5 explains:
   left in place, each is a warning on every build.

3. Write `skillwright.md` in the vault folder, the folder that holds the notes, and delete the old
   file. It is the only configuration file. Use `skillwright init --out <path>` to start one, or
   write it by hand:

   | `skillwright.yaml` | `skillwright.md` |
   |---|---|
   | `vault: <path>` | none: the file sits in that folder |
   | `namespace: <stem>` | `build.namespace`, or remove it for bare keys |
   | `marketplace.name`, `.display_name`, `.owner`, `.author`, `.license` | the same key at the top level |
   | `marketplace.out`, `.harnesses`, `.require_descriptions` | the same key under `build:` |
   | `bump:` | `build.bump:`, with the same `patch` value. A top-level `bump:` is refused |
   | `marketplace.shim_target` | `build.launcher_path`, with the same value. If the key was absent and the vault relied on the old default, set `build.launcher_path: cli/sdlc`, because there is no default now |
   | `marketplace.default_plugin` | none: see step 4 |
   | `marketplace.plugins.<p>` | the keys of `plugins/<p>/<p>.md` |
   | `marketplace.skills_root` | none: put `skillwright.md` where the kind folders are, or pass `--skills-root` |
   | `destinations`, `scan_locations`, `assets`, `hooks` | the same key under `build:` |
   | `paths`, `declared_folders` | none: use `build.patterns` for a layout that differs |

   `build`, `check` and `build --watch` need `build.out` or `--out`, because there is no default
   output folder. A vault that only publishes can leave it out. If the old file set no
   `marketplace.out` and the vault builds, ask where builds should land.

   A leftover `build.shim_target` is refused. Without `build.launcher_path`, `${CLAUDE_PLUGIN_ROOT}`
   calls ship unrewritten to Codex, Cursor and Pi.

   Every relative path in the file resolves against the file's own folder. When the new file sits
   in a different folder from the old one, rewrite each path so it names the same place. With
   `vault: notes` and `out: .` at the repository root, the new file is `notes/skillwright.md` and
   needs `out: ..` to keep the output at the root. A wrong `out` builds without any error, in the
   wrong folder.

   An `assets` or `hooks` entry with no `plugin` used `default_plugin`. Name the plugin.

   Delete the machine file `<config-home>/skillwright/config.yaml` after moving any key the project
   needs. Personal defaults now come from the environment. Remove `SKILLWRIGHT_MACHINE_CONFIG` from
   scripts and CI.

4. Move notes so the folder says what the keys said. Use `git mv`, because a move keeps the history.

   | Old | New |
   |---|---|
   | `sw-kind: <kind>` | move the note into that kind's folder, and remove the key |
   | `sw-plugin: <p>` | move the note under `plugins/<p>/`, and remove the key |
   | no `sw-plugin`, joined `default_plugin` | move the note under `plugins/<default_plugin>/`, or leave it in a top-level `skills/` to join a plugin named after the marketplace |
   | `marketplaces/<m>/<m>.md`, with the plugins inside `marketplaces/<m>/` | `marketplaces/<m>/skillwright.md`, with `workspace: true` in an outer `skillwright.md`; each of these files needs its own `build.out`, which resolves against that file |
   | `marketplaces/<m>/<m>.md`, with the plugins beside `marketplaces/` at the vault root | fold only the keys the old build read into the vault's `skillwright.md`, and delete the note and its folder. Where the old config set a key too, keep the config's value. If the old build never read this note, fold nothing, because its keys would change what ships. Moving it instead leaves a marketplace with nothing in it, and `build` fails `empty-marketplace` |

   A vault that cannot move its notes maps them with `build.patterns`. Patterns are experimental.

5. Rename the keys. The migration must not change what ships, and the rule depends on the kind of
   note.

   On a plugin note or the marketplace:

   | Old | New |
   |---|---|
   | `sw-version`, `sw-author`, `sw-owner`, `sw-license`, `sw-keywords`, `sw-category`, `sw-targets` | the same word, bare |
   | `sw-display-name` | `display_name` |
   | `sw: {version: …}` | the same keys at the top level |

   On a skill, agent or command:

   | Old | New |
   |---|---|
   | `sw-targets`, `sw-arguments` | `targets`, `arguments` |
   | `sw-version`, `sw-author`, `sw-owner`, `sw-keywords`, `sw-category`, `sw-display-name` | delete the key |
   | `sw-license` | delete the key, and leave any bare `license` as it is |

   The deleted keys shipped nothing, because they were stripped at publish. Do not rename them.
   `version` on a skill is an unknown key and would ship under `metadata:`, and `license` on a skill
   is the spec field and would ship as written. If the value matters, tell the user: moving it to
   the plugin note changes that plugin's manifest, so it is a separate decision.

   An unknown key ships under `metadata:` as YAML, so an unquoted `true` now publishes as `true`,
   where it used to be `"true"`. Quote a value that must stay a string.

6. Move each `sw-overrides` entry to a top-level block named for the harness, and remove
   `sw-overrides`:

   ```yaml
   # before
   sw-overrides:
     claude:
       allowed-tools: Bash(cargo:*)
   # after
   claude:
     allowed-tools: Bash(cargo:*)
   ```

   Under `build.namespace: sw` the block is `sw-claude:`.

   Delete `policy.allow_implicit_invocation` rather than moving it. No harness fact shows Codex reads
   it, and a block holding it stops the build with `unknown-override-key`.

   Only `claude`, `codex`, `cursor` and `pi` have blocks. Move an override for any other target
   into the note, or drop it. A block a skill inherits cannot set its `name` or `description`, and a
   block's `metadata` merges with the note's own. Blocks and `author`, `license`, `repository`,
   `homepage` and `targets` inherit from the plugin note and `skillwright.md`, so a value repeated on
   every note can move up once. On one member, its own block wins over its frontmatter, which wins
   over an inherited block. A key the member's kind does not accept stays with the container.

7. Fold adoption provenance into one `adopted:` block, or `sw-adopted:` under `build.namespace: sw`,
   leaving every value as it is:

   | Old | New |
   |---|---|
   | `sw-source` | `adopted.source` |
   | `sw-format` | `adopted.format` |
   | `sw-source-hash` | `adopted.source_hash` |
   | `sw-adopted-hash` | `adopted.hash` |
   | `sw-adopted-at` | `adopted.at` |
   | `sw-also-at` | `adopted.also_at` |

8. Refresh the adopt hashes. A key that is skillwright's now, such as `arguments`, was covered by
   the old hash, and a `license` renamed from `sw-license` was not, so the rename alone can read as
   an edit:

   ```sh
   skillwright check --fix-adopt-hashes --dry-run
   skillwright check --fix-adopt-hashes --yes
   ```

   A note listed as `edited, left alone` changed in more than its keys, and the command exits 1. Do
   not change its hash by hand. Tell the user: the note keeps its edit, and the next `adopt` of it
   treats it as edited.

9. Replace old names in scripts and CI. Add `--out` to any `build` or `check` that relied on a
   default, and read `version`, not `sw-version`, in anything that parses a note. Anything that read
   `kind` or `plugin` from a note must read the folder instead. Anything that reads notes from old
   git refs must accept both spellings.

   skillwright finds `skillwright.md` by walking up, so it must be at or above the folder a command
   runs from. If the file moved down into `notes/`, a command run from the repository root exits 4.
   Run it from `notes/`, or pass `--config notes/skillwright.md`.

10. Fix links to files that never ship. A link or embed that reaches a never-ship or excluded file,
    such as `docs/x.md`, is now refused `excluded-link`, in every link form, and stops the build.
    Move the file into the skill's folder or a plugin's `assets/`, or remove the link.

11. Replace hand-written skill calls. A body that spells a call to a known skill, such as `/name`,
    `/plugin:name` or `$plugin:name`, is right for one harness only, and `build` refuses it
    `literal-skill-call`. Write `\/[[name]]`, and each harness gets its own spelling. Put `\` before a
    call meant literally. Publishing one skill does not check a bare `/name`, so run `build`.

12. Verify. If `skillwright.md` moved and you run from another folder, add `--config <path>` to each
    command:

    ```sh
    skillwright list            # every note listed, with the kind and plugin you meant
    skillwright build --yes     # rebuild; read the warnings
    skillwright check           # exits 0
    skillwright status --check  # destinations match the vault
    ```

    `list` passing does not prove `build` will. `build` is the real check, because it follows links
    and renders every harness. A vault with no `build.out` still needs this: pass
    `--out <scratch folder>` to `build` and `check`. A `missing-version` warning for the plugin
    of a root `skills/` folder, named after the marketplace (the folder's name by default), means
    `skillwright.md` has no `version`: set it there. A `plugins/<marketplace>/` plugin takes its
    own note's `version` instead. The build makes that plugin to hold top-level skills, and `publish`
    never writes it.

    Each unknown key is a warning, and it ships under `metadata:`, so clear them all and search for
    `sw-` again. A note that still has a `kind` or `plugin` key, or a `sw-<key>` spelling of a key
    skillwright owns, is listed as `skipped · removed-key`, and `build` and `check` refuse to run
    until the key is gone.

    Compare the build with the old output. These differences are expected:

    - The `Generated by` line.
    - Inherited `license`: a `license` on the plugin note or `skillwright.md` now reaches every
      `SKILL.md`. A skill cannot clear it, only set its own.
    - `category` and `tags`, which now reach the manifests that accept them.
    - Fields a harness does not document, which are no longer written: `version` in Claude's
      marketplace entry, and `author.url` and `owner.url` for Cursor.
    - A space-separated `allowed-tools` string is now read as a real list.
    - The per-skill `scripts/<name>` shim is gone, so each of those files disappears from the output.
    - Calls to `${CLAUDE_PLUGIN_ROOT}/<launcher>` are rewritten to the launcher's path relative to the
      skill's folder on Codex, Cursor and Pi. In a vault with a launcher this can touch most built
      files.
    - Unknown keys under `metadata:` keep their YAML type, as step 5 says.
    - A `\/[[name]]` call renders in backticks, in a heading too.
    - Plugin overrides, which now reach that plugin's manifest.

    Anything else is a change to explain to the user before merging.

    A build replaces each harness's packs folder and the manifests it writes. Files the old build
    left elsewhere in a harness folder stay. Delete any folder the old `out` or `packs` left behind.
    A moved `out` reports nothing, and a non-default `packs` folder inside the vault stops the build
    until it is gone.
