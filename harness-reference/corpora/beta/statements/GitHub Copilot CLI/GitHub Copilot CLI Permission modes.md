---
proposition:
  subject: "[GitHub Copilot CLI](../../harnesses/GitHub%20Copilot%20CLI.md)"
  predicate: permissions/modes
  object:
    supported: true
    modes:
      - name: plan
        intent: read-only
        argv:
          - --mode
          - plan
      - name: allow-tool write
        intent: edits
        argv:
          - --allow-tool=write
      - name: allow-all-tools
        intent: full-auto
        argv:
          - --allow-all-tools
        notes: tools only; path and URL checks remain
      - name: allow-all
        intent: bypass
        argv:
          - --allow-all
        notes: --yolo is the same; env COPILOT_ALLOW_ALL
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 GitHub Copilot CLI Mode option docs](../../study-results/GitHub%20Copilot%20CLI/2026-11-09%20GitHub%20Copilot%20CLI%20Mode%20option%20docs.md)"
      - "[2026-11-09 GitHub Copilot CLI Allow all option docs](../../study-results/GitHub%20Copilot%20CLI/2026-11-09%20GitHub%20Copilot%20CLI%20Allow%20all%20option%20docs.md)"
      - "[2026-11-09 GitHub Copilot CLI Yolo option docs](../../study-results/GitHub%20Copilot%20CLI/2026-11-09%20GitHub%20Copilot%20CLI%20Yolo%20option%20docs.md)"
      - "[2026-11-09 GitHub Copilot CLI Allow tool option docs](../../study-results/GitHub%20Copilot%20CLI/2026-11-09%20GitHub%20Copilot%20CLI%20Allow%20tool%20option%20docs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: command reference.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[GitHub Copilot CLI](../../harnesses/GitHub%20Copilot%20CLI.md)"
subject: "[Permission modes](../../subjects/permissions/Permission%20modes.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
