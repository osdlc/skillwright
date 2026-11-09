---
proposition:
  subject: "[Cursor](../../harnesses/Cursor.md)"
  predicate: cli/structured-output
  object:
    output: --output-format json|stream-json (with --print)
    partial: --stream-partial-output with stream-json
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Cursor help output-format](../../study-results/Cursor/2026-11-09%20Cursor%20help%20output-format.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Installed --help.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs output default](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20output%20default.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: "Docs: json emits one result object; stream-json is NDJSON."
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Cursor](../../harnesses/Cursor.md)"
subject: "[Structured output](../../subjects/cli/Structured%20output.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
