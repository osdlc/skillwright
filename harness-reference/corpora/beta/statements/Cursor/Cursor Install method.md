---
proposition:
  subject: "[Cursor](../../harnesses/Cursor.md)"
  predicate: cli/install
  object:
    supported: true
    methods:
      - kind: script
        package: https://cursor.com/install
        platforms:
          - macos
          - linux
        note: curl https://cursor.com/install -fsS | bash (Windows via WSL)
      - kind: script
        package: https://cursor.com/install?win32=true
        platforms:
          - windows
        note: irm 'https://cursor.com/install?win32=true' | iex
    update: agent update
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs install](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20install.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Install page.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs update](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20update.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Update command.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor help update](../../study-results/Cursor/2026-11-09%20Cursor%20help%20update.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: The CLI lists an update command.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Cursor](../../harnesses/Cursor.md)"
subject: "[Install method](../../subjects/cli/Install%20method.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---

Uninstall is not documented.
