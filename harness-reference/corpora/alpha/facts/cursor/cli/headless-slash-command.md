---
type: Harness Fact
title: "Cursor: Slash command in a headless prompt"
description: "Slash command in a headless prompt in Cursor, as observed over time."
tags: [cli]
harness: cursor
subject: cli/headless-slash-command
observations: []
open:
  - question: "Does a /command at the start of an agent -p prompt run deterministically, or does the model choose? Which text becomes its arguments, and does text on later lines reach it?"
    observed: "2026-11-10"
    source: cursor-cli-headless
sources:
  - id: cursor-cli-headless
    resource: "https://cursor.com/docs/cli/headless"
    title: "Cursor docs: CLI headless"
---

What the sources say about [Slash command in a headless prompt](/subjects/cli/headless-slash-command.md) in [Cursor](/harnesses/cursor.md).

## Open questions

- Does a /command at the start of an agent -p prompt run deterministically, or does the model choose? Which text becomes its arguments, and does text on later lines reach it?[^cursor-cli-headless]

[^cursor-cli-headless]: Cursor docs: CLI headless
