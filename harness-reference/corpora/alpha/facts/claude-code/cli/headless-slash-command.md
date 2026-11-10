---
type: Harness Fact
title: "Claude Code: Slash command in a headless prompt"
description: "Slash command in a headless prompt in Claude Code, as observed over time."
tags: [cli]
harness: claude-code
subject: cli/headless-slash-command
observations:
  - value: { supported: true, position: "prompt start only", arguments: "all text after the command name, newlines included", later_text: "becomes arguments, not context" }
    since: null
    until: null
    observed: "2026-11-10"
    source: probe-headless-slash-args
    checked_version: "2.1.319"
    evidence: run
    probe: claude-headless-slash-args
    model: haiku
    note: "Prompt '/echoargs alpha beta' plus three more lines: $ARGUMENTS held the whole rest, newlines included. 1 turn."
  - value: { not_at_start: "not parsed as a command; the model called the command itself through a tool, with arguments alpha beta, in 3 turns" }
    since: null
    until: null
    observed: "2026-11-10"
    source: probe-headless-slash-args
    checked_version: "2.1.319"
    evidence: run
    probe: claude-headless-slash-args
    model: haiku
    note: "Prompt was text, a blank line, then '/echoargs alpha beta'. Model-dependent behaviour."
sources:
  - id: probe-headless-slash-args
    resource: "https://github.com/osdlc/skillwright/blob/main/harness-reference/probes/claude-headless-slash-args/probe.md"
    title: "Probe: slash command arguments in a headless prompt"
---

What the sources say about [Slash command in a headless prompt](/subjects/cli/headless-slash-command.md)
in [Claude Code](/harnesses/claude-code.md). How a slash command or skill call in a headless
prompt is parsed.

## Observations

- 2026-11-10: supported: `true`; position: prompt start only; arguments: all text after the command
  name, newlines included; later text: becomes arguments, not context.[^probe-headless-slash-args]
- 2026-11-10: not at start: not parsed as a command; the model called the command itself through a
  tool, with arguments alpha beta, in 3 turns (model: haiku).[^probe-headless-slash-args]

[^probe-headless-slash-args]: Probe: slash command arguments in a headless prompt
