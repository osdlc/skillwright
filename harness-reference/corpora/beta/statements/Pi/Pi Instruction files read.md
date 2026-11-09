---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: instructions/files
  object:
    files:
      - AGENTS.md
      - CLAUDE.md
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-08-08 Pi Instruction files read](../../study-results/Pi/2026-08-08%20Pi%20Instruction%20files%20read.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: weak
    description: third-party observed 2026-08-08.
  - hasEvidenceItems:
      - "[2026-11-09 Pi context file names](../../study-results/Pi/2026-11-09%20Pi%20context%20file%20names.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Source candidate names.
  - hasEvidenceItems:
      - "[2026-11-09 Pi security context files](../../study-results/Pi/2026-11-09%20Pi%20security%20context%20files.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Context files load regardless of project trust.
  - hasEvidenceItems:
      - "[2026-11-09 Pi help no-context-files](../../study-results/Pi/2026-11-09%20Pi%20help%20no-context-files.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: --no-context-files.
reportedIn:
  - "[Harness comparison](../../documents/skillwright/Harness%20comparison.md)"
contributions:
  - contributor:
      name: harness-reference import
      agentType: software
    activityType: created
    date: 2026-08-08T00:00:00Z
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[Pi](../../harnesses/Pi.md)"
subject: "[Instruction files read](../../subjects/instructions/Instruction%20files%20read.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---

Imported from `facts/pi/instructions/files.md`.

Rechecked 2026-11-09 against the installed pi 1.5.0 and the v1.5.0 source tree.
