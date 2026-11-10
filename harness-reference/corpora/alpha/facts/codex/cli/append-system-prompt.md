---
type: Harness Fact
title: "Codex CLI: Appended system prompt"
description: "Appended system prompt in Codex CLI, as observed over time."
tags: [cli]
harness: codex
subject: cli/append-system-prompt
observations: []
open:
  - question: "Is there a flag or config key that adds system or developer instructions to a headless run, such as a -c instructions key or an instructions file?"
    observed: "2026-11-10"
    source: codex-noninteractive
sources:
  - id: codex-noninteractive
    resource: "https://learn.chatgpt.com/docs/non-interactive-mode"
    title: "Codex docs: non-interactive mode"
---

What the sources say about [Appended system prompt](/subjects/cli/append-system-prompt.md) in [Codex CLI](/harnesses/codex.md).

## Open questions

- Is there a flag or config key that adds system or developer instructions to a headless run, such as a -c instructions key or an instructions file?[^codex-noninteractive]

[^codex-noninteractive]: Codex docs: non-interactive mode
