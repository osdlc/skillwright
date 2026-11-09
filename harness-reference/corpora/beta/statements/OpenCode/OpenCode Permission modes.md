---
proposition:
  subject: "[OpenCode](../../harnesses/OpenCode.md)"
  predicate: permissions/modes
  object:
    supported: partial
    modes:
      - name: auto
        intent: full-auto
        argv:
          - --auto
        note: approves requests that would ask; explicit deny rules still apply
      - name: permission config
        intent: edits
        env:
          OPENCODE_PERMISSION: '{"edit":"allow","bash":"deny"}'
        config_key: permission
        note: each action is allow, ask or deny; there are no named modes
    default: ask per the permission config
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Auto flag source v2.2.6](../../study-results/OpenCode/2026-11-09%20OpenCode%20Auto%20flag%20source%20v2.2.6.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source at v2.2.6.
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Auto mode docs](../../study-results/OpenCode/2026-11-09%20OpenCode%20Auto%20mode%20docs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: permissions docs.
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Permission env docs](../../study-results/OpenCode/2026-11-09%20OpenCode%20Permission%20env%20docs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: OPENCODE_PERMISSION in the env var table.
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode OPENCODE_PERMISSION source v1.19.5](../../study-results/OpenCode/2026-11-09%20OpenCode%20OPENCODE_PERMISSION%20source%20v1.19.5.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: OPENCODE_PERMISSION read at v1.19.5.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[OpenCode](../../harnesses/OpenCode.md)"
subject: "[Permission modes](../../subjects/permissions/Permission%20modes.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
open:
  - --auto is absent from the 1.1.36 run help; the release that added it was not found.
---
