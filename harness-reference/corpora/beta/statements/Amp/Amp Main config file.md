---
proposition:
  subject: "[Amp](../../harnesses/Amp.md)"
  predicate: config/main-file
  object:
    file: .amp/settings.json or .amp/settings.jsonc (workspace, nearest searched upward); ~/.config/amp/settings.json or settings.jsonc (user)
    format: JSON or JSONC
    merge: workspace settings override user settings
    note: All settings use the amp. prefix. --settings-file <path> replaces the user file.
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-08-08 Amp Main config file](../../study-results/Amp/2026-08-08%20Amp%20Main%20config%20file.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: weak
    description: third-party observed 2026-08-08.
  - hasEvidenceItems:
      - "[2026-11-09 settings locations](../../study-results/Amp/2026-11-09%20settings%20locations.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: docs observed 2026-11-09.
reportedIn:
  - "[Harness comparison](../../documents/skillwright/Harness%20comparison.md)"
  - "[Amp docs Configuration](../../documents/ampcode.com/Amp%20docs%20Configuration.md)"
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
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[Amp](../../harnesses/Amp.md)"
subject: "[Main config file](../../subjects/config/Main%20config%20file.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
imported_from: fact:config/main-file@amp#scope-[null,null,null]
---

Imported from `facts/amp/config/main-file.md`.
