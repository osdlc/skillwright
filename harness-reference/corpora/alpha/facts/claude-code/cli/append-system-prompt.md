---
type: Harness Fact
title: "Claude Code: Appended system prompt"
description: "Appended system prompt in Claude Code, as observed over time."
tags: [cli]
harness: claude-code
subject: cli/append-system-prompt
observations:
  - value: { supported: true, flag: "--append-system-prompt", keeps_slash_command_parsing: true }
    since: null
    until: null
    observed: "2026-11-10"
    source: probe-headless-slash-args
    checked_version: "2.1.319"
    evidence: run
    probe: claude-headless-slash-args
    note: "A prompt '/echoargs alpha beta' with the flag still parsed as the command with exactly 'alpha beta', 1 turn. The model saw the appended text: /runid returned RUNID=ZEBRA-42."
sources:
  - id: probe-headless-slash-args
    resource: "https://github.com/osdlc/skillwright/blob/main/harness-reference/probes/claude-headless-slash-args/probe.md"
    title: "Probe: slash command arguments in a headless prompt"
---

What the sources say about [Appended system prompt](/subjects/cli/append-system-prompt.md) in
[Claude Code](/harnesses/claude-code.md). The flag that adds system instructions to a headless run.

## Observations

- 2026-11-10: supported: `true`; flag: --append-system-prompt; keeps slash command parsing:
  `true`. The appended text reached the model.[^probe-headless-slash-args]

[^probe-headless-slash-args]: Probe: slash command arguments in a headless prompt
