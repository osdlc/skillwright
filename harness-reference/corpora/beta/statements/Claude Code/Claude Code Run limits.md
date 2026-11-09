---
proposition:
  subject: "[Claude Code](../../harnesses/Claude%20Code.md)"
  predicate: cli/limits
  object:
    supported: true
    max_turns:
      argv:
        - --max-turns
        - "{n}"
      config_key: CLAUDE_CODE_MAX_TURNS (environment variable)
    max_budget:
      argv:
        - --max-budget-usd
        - "{usd}"
    on_limit: the run stops; a spawn after the budget cap fails with Budget limit reached
    note: Both flags work in print mode only. --max-turns does not appear in `claude --help` of 2.1.319, but the docs list it and say help does not list every flag. There is no wall-time flag.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 help max budget](../../study-results/Claude%20Code/2026-11-09%20help%20max%20budget.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Help text lists --max-budget-usd, print mode only.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli max-turns](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20max-turns.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The CLI reference documents --max-turns, print mode only.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli max-budget](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20max-budget.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The CLI reference describes the budget cap.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli help not exhaustive](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20help%20not%20exhaustive.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The docs say a flag missing from --help may still be available.
  - hasEvidenceItems:
      - "[2026-11-09 docs env MAX_TURNS](../../study-results/Claude%20Code/2026-11-09%20docs%20env%20MAX_TURNS.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: CLAUDE_CODE_MAX_TURNS is equivalent to --max-turns.
  - hasEvidenceItems:
      - "[2026-11-09 changelog max-turns](../../study-results/Claude%20Code/2026-11-09%20changelog%20max-turns.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: A 2.1.281 fix names --max-turns as a working limit.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
subject: "[Run limits](../../subjects/cli/Run%20limits.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
open:
  - What exit code or result subtype does a run report when --max-turns ends it? The excerpts do not say.
---
