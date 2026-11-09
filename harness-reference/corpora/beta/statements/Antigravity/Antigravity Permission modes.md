---
proposition:
  subject: "[Antigravity](../../harnesses/Antigravity.md)"
  predicate: permissions/modes
  object:
    supported: true
    modes:
      - name: accept-edits
        intent: edits
        argv:
          - --mode
          - accept-edits
        notes: auto-approves file edits in the workspace; still asks before terminal commands
      - name: auto
        intent: full-auto
        argv:
          - --mode
          - auto
        notes: needs the terminal sandbox and a personal account; v1.3.5+ on macOS and Linux, v1.4.4+ on Windows
      - name: dangerously-skip-permissions
        intent: bypass
        argv:
          - --dangerously-skip-permissions
    default: default
    note: No read-only mode. --sandbox adds terminal containment; plan mode only prepends the /plan instruction.
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 reference execution mode flag](../../study-results/Antigravity/2026-11-09%20reference%20execution%20mode%20flag.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: docs observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 modes accept-edits](../../study-results/Antigravity/2026-11-09%20modes%20accept-edits.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: docs observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 headless permission flags](../../study-results/Antigravity/2026-11-09%20headless%20permission%20flags.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: docs observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 reference sandbox flag](../../study-results/Antigravity/2026-11-09%20reference%20sandbox%20flag.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: docs observed 2026-11-09.
reportedIn:
  - "[Antigravity docs CLI commands](../../documents/antigravity.google/Antigravity%20docs%20CLI%20commands.md)"
  - "[Antigravity docs CLI execution modes](../../documents/antigravity.google/Antigravity%20docs%20CLI%20execution%20modes.md)"
  - "[Antigravity docs CLI headless mode](../../documents/antigravity.google/Antigravity%20docs%20CLI%20headless%20mode.md)"
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Antigravity](../../harnesses/Antigravity.md)"
subject: "[Permission modes](../../subjects/permissions/Permission%20modes.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---


