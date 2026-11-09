---
proposition:
  subject: "[OpenCode](../../harnesses/OpenCode.md)"
  predicate: cli/effort
  object:
    supported: partial
    argv:
      - --model
      - "{provider/model}#{level}"
    note: the v2.2.6 run command defines no --variant flag; the model string carries the variant. The v2.2.6 CLI docs table still lists --variant.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Model flag source v2.2.6](../../study-results/OpenCode/2026-11-09%20OpenCode%20Model%20flag%20source%20v2.2.6.md)"
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
subject: "[Effort level flag](../../subjects/cli/Effort%20level%20flag.md)"
scope:
  since: 2.0.0
  until: null
  platform: []
  model: null
status: current
open:
  - "Whether --variant is still accepted by the 2.x run command: the docs table lists it, the command definition does not."
---
