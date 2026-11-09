---
kind: changelog
reportedIn:
  - "[Claude Code CHANGELOG github.com](../../documents/github.com/Claude%20Code%20CHANGELOG%20github.com.md)"
version: 2.1.281
version_basis: exact
locator: CHANGELOG.md, 2.1.281
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
observed: 2026-11-09
excerpt:
  exact: Fixed a turn that could retry indefinitely, ignoring `--max-turns`, when the model alternated unparseable tool calls and output-limit truncation
  prefix: "API request was being retried - "
  suffix: " - Fixed resumed sessions re-sen"
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
---

> Fixed a turn that could retry indefinitely, ignoring `--max-turns`, when the model alternated unparseable tool calls and output-limit truncation
