---
proposition:
  subject: "[Gemini CLI](../../harnesses/Gemini%20CLI.md)"
  predicate: cli/effort
  object:
    supported: false
    config_key: modelConfigs thinkingBudget / thinkingLevel in settings.json
    note: no CLI flag for effort in 0.65.0 help
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Model flag help 0.65](../../study-results/Gemini%20CLI/2026-11-09%20Model%20flag%20help%200.65.md)"
    directionOfEvidenceProvided: neutral
    description: the 0.65.0 help lists no effort flag.
  - hasEvidenceItems:
      - "[2026-11-09 Thinking config docs](../../study-results/Gemini%20CLI/2026-11-09%20Thinking%20config%20docs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: thinking budget is a settings value (modelConfigs), not a flag.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Gemini CLI](../../harnesses/Gemini%20CLI.md)"
subject: "[Effort level flag](../../subjects/cli/Effort%20level%20flag.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
