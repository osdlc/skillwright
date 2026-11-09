---
proposition:
  subject: "[Codex CLI](../../harnesses/Codex%20CLI.md)"
  predicate: cli/output-format
  object:
    supported: true
    formats:
      - name: jsonl
        argv:
          - --json
      - name: text
        argv: []
    note: --json prints events to stdout as JSON Lines; without it Codex prints human-readable output (--color always|never|auto, default auto).
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Codex CLI Output format flag help](../../study-results/Codex%20CLI/2026-11-09%20Codex%20CLI%20Output%20format%20flag%20help.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: help-text observed 2026-11-09 at 0.162.0.
  - hasEvidenceItems:
      - "[2026-11-09 Codex CLI Output format color](../../study-results/Codex%20CLI/2026-11-09%20Codex%20CLI%20Output%20format%20color.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: help-text observed 2026-11-09 at 0.162.0.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Codex CLI](../../harnesses/Codex%20CLI.md)"
subject: "[Output format flag](../../subjects/cli/Output%20format%20flag.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
