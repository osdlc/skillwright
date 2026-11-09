---
proposition:
  subject: "[Cursor](../../harnesses/Cursor.md)"
  predicate: cli/continue
  object:
    supported: true
    argv:
      - --continue
    chosen_by: the previous session
    accepts_prompt: true
    note: "`agent resume` does the same without print mode"
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Cursor help resume continue](../../study-results/Cursor/2026-11-09%20Cursor%20help%20resume%20continue.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: --continue in --help.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs parameters continue](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20parameters%20continue.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: "Docs: alias for --resume=-1."
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs resume](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20resume.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: "`agent resume` resumes the latest chat."
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Cursor](../../harnesses/Cursor.md)"
subject: "[Continue the last session](../../subjects/cli/Continue%20the%20last%20session.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
