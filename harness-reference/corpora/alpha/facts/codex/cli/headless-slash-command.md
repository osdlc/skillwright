---
type: Harness Fact
title: "Codex CLI: Slash command in a headless prompt"
description: "Slash command in a headless prompt in Codex CLI, as observed over time."
tags: [cli]
harness: codex
subject: cli/headless-slash-command
observations: []
open:
  - question: "Does a slash command or skill mention (a $skill) at the start of an exec prompt run deterministically, or does the model choose? Which text becomes its arguments, and does text on later lines reach it?"
    observed: "2026-11-10"
    source: codex-noninteractive
sources:
  - id: codex-noninteractive
    resource: "https://developers.openai.com/codex/noninteractive"
    title: "Codex docs: non-interactive mode"
---

What the sources say about [Slash command in a headless prompt](/subjects/cli/headless-slash-command.md) in [Codex CLI](/harnesses/codex.md).

## Open questions

- Does a slash command or skill mention (a $skill) at the start of an exec prompt run deterministically, or does the model choose? Which text becomes its arguments, and does text on later lines reach it?[^codex-noninteractive]

[^codex-noninteractive]: Codex docs: non-interactive mode
