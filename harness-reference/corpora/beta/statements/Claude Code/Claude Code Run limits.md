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
      env: CLAUDE_CODE_MAX_TURNS
    max_budget:
      argv:
        - --max-budget-usd
        - "{usd}"
    on_limit: "budget: the run stops once estimated spend reaches the cap"
    note: Both flags work in print mode only. The docs list --max-turns and say --help does not list every flag.
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 help max budget](../../study-results/Claude%20Code/2026-11-09%20help%20max%20budget.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Help text lists --max-budget-usd, print mode only.
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
  - What exit code or result subtype does a run report when --max-turns or the budget cap ends it?
  - The 2.1.319 `claude --help` output does not list --max-turns; is that a docs/help gap or is the flag hidden on purpose?
  - Is there a wall-time limit flag? None was found in the help or the quoted docs.
---
