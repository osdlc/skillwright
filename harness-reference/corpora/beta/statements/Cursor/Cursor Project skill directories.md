---
proposition:
  subject: "[Cursor](../../harnesses/Cursor.md)"
  predicate: skills/project-dirs
  object:
    dirs:
      - .agents/skills
      - .cursor/skills
    legacy:
      - .claude/skills
      - .codex/skills
    nested: true
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs skill dirs](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20skill%20dirs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Skill directories table.
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs skill compat dirs](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20skill%20compat%20dirs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Claude and Codex directories load for compatibility.
reportedIn:
  - "[Harness comparison](../../documents/skillwright/Harness%20comparison.md)"
  - "[Cursor docs Agent Skills](../../documents/cursor.com/Cursor%20docs%20Agent%20Skills.md)"
contributions:
  - contributor:
      name: harness-reference import
      agentType: software
    activityType: created
    date: 2026-10-01T00:00:00Z
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[Cursor](../../harnesses/Cursor.md)"
subject: "[Project skill directories](../../subjects/skills/Project%20skill%20directories.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
imported_from: fact:skills/project-dirs@cursor#scope-[null,null,null]
---

Imported from `facts/cursor/skills/project-dirs.md`.

The observations disagree on: dirs. The latest value is kept.

Rechecked 2026-11-09 against the installed CLI and vendor docs. The dispute was only a flattened list, so the statement is current.
