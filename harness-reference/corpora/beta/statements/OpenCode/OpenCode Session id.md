---
proposition:
  subject: "[OpenCode](../../harnesses/OpenCode.md)"
  predicate: cli/session-id
  object:
    supported: partial
    assign_argv:
      - --session
      - "{id}"
    reported_in:
      - event
    note: 2.x creates the session when the id does not exist; 1.x --session only continues one
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Session flag source v2.2.6](../../study-results/OpenCode/2026-11-09%20OpenCode%20Session%20flag%20source%20v2.2.6.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source at v2.2.6.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[OpenCode](../../harnesses/OpenCode.md)"
subject: "[Session id](../../subjects/cli/Session%20id.md)"
scope:
  since: 2.0.0
  until: null
  platform: []
  model: null
status: current
open:
  - The accepted id format was not checked.
---
