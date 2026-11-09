---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: cli/session-env
  object:
    vars:
      - PI_SESSION_ID
      - PI_SESSION_FILE
      - PI_PROVIDER
      - PI_MODEL
      - PI_REASONING_LEVEL
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Pi env session variables](../../study-results/Pi/2026-11-09%20Pi%20env%20session%20variables.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Variables injected into the bash and powershell tools.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Pi](../../harnesses/Pi.md)"
subject: "[Session environment](../../subjects/cli/Session%20environment.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---

Only the LLM-callable shell tools get them, not user-typed ! commands.
