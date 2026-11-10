---
type: Harness Probe
title: Slash command arguments in a headless prompt
description: "Run a project slash command from a headless prompt in three ways, to see where the call must sit, which text becomes its arguments, and whether an appended system prompt keeps the parsing."
tags: [cli]
subjects: [cli/headless-slash-command, cli/append-system-prompt]
harnesses: [claude-code]
commands:
  claude-code:
    - "claude --version"
    - 'cd fixture && claude -p --model haiku --output-format json "$(printf "/echoargs alpha beta\nHow you were called\n- line two\n- line three")"'
    - 'cd fixture && claude -p --model haiku --output-format json "$(printf "Some context first.\n\n/echoargs alpha beta")"'
    - 'cd fixture && claude -p --model haiku --output-format json "/echoargs alpha beta" --append-system-prompt "How you were called: headless test run."'
    - 'cd fixture && claude -p --model haiku --output-format json "/runid" --append-system-prompt "How you were called: headless test run; the run id is ZEBRA-42."'
record:
  - The harness version.
  - "For each run, the reply text and `num_turns`."
  - "Whether the reply holds ARGS-START[...]ARGS-END, and exactly what is between the brackets (newlines included)."
  - "Whether the model called the command itself through a tool when the call was not at the prompt start."
  - "Whether the `/runid` reply is RUNID=ZEBRA-42."
safety:
  - Uses the signed-in account and counts against its usage.
  - "Runs only the fixture's two commands, which only reply with text."
---

# Slash command arguments in a headless prompt

The fixture holds two project commands in `fixture/.claude/commands/`. `echoargs` replies with
its `$ARGUMENTS` between markers. `runid` replies with a run id taken from the model's context.

The runs answer:

- whether a call at the start of the prompt is parsed, and whether later lines become arguments;
- whether a call after other text is parsed, or the model runs it (model-dependent);
- whether `--append-system-prompt` keeps the parsing and reaches the model.
