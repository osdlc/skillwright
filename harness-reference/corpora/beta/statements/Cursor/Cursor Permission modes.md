---
proposition:
  subject: "[Cursor](../../harnesses/Cursor.md)"
  predicate: permissions/modes
  object:
    supported: true
    modes:
      - name: ask
        intent: read-only
        argv:
          - --mode
          - ask
        notes: Q&A mode; read-only per --help
      - name: plan
        intent: read-only
        argv:
          - --mode
          - plan
      - name: sandbox enabled
        intent: full-auto
        argv:
          - --sandbox
          - enabled
        notes: intent is the closest generic match; help does not describe auto-running
      - name: force
        intent: bypass
        argv:
          - --force
        config_key: approvalMode
        notes: --yolo is an alias; approvalMode in cli-config.json is allowlist, auto-review or unrestricted
    default: "agent mode (docs: agent is the default when no mode is given)"
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Cursor help mode](../../study-results/Cursor/2026-11-09%20Cursor%20help%20mode.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: --mode plan and ask are read-only.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor help force sandbox](../../study-results/Cursor/2026-11-09%20Cursor%20help%20force%20sandbox.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: --force, --auto-review and --sandbox flags.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs config approvalMode](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20config%20approvalMode.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: approvalMode setting.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Cursor](../../harnesses/Cursor.md)"
subject: "[Permission modes](../../subjects/permissions/Permission%20modes.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---

Intents are the closest generic match, not the vendor’s words.
