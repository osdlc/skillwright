---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: permissions/tool-rules
  object:
    supported: partial
    allow_argv:
      - --tools
      - "{rules}"
    deny_argv:
      - --exclude-tools
      - "{rules}"
    syntax: tool names or * patterns, such as read,grep or mcp__server__*
    note: allowlist/denylist of whole tools; no per-argument patterns
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Pi help tools](../../study-results/Pi/2026-11-09%20Pi%20help%20tools.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: --help.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Pi](../../harnesses/Pi.md)"
subject: "[Tool allow and deny rules](../../subjects/permissions/Tool%20allow%20and%20deny%20rules.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
