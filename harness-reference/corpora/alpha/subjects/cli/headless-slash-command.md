---
type: Subject
title: Slash command in a headless prompt
description: "How a slash command or skill call in a headless (one-shot) prompt is parsed: where the call must sit in the prompt and which text becomes its arguments."
order: 63
tags: [cli]
model_dependent: true
---

How a slash command or skill call in a headless (one-shot) prompt is parsed: where the call must sit
in the prompt and which text becomes its arguments.

Part of the [CLI and sessions](/categories/cli.md) category. Each harness's observations for this
subject are in `facts/<harness>/cli/headless-slash-command.md`.

When the call is not parsed, the model may still run the command itself, so an observation may
name the `model` it was made with.
