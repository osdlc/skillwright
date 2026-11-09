---
proposition:
  subject: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
  predicate: instructions/system-prompt-flag
  object:
    supported: false
    note: No system-prompt flag. The persona is the personaPrefix and personaSuffix settings of the system-prompt row, set through a patch. The system-prompt README says to remove the 'Your working directory is {{cwd}}.' clause from personaSuffix settings because the loop registers no {{cwd}} variable and an unresolved reference fails prompt assembly.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 persona cwd unresolved](../../study-results/DeepSeek%20Harness/2026-11-09%20persona%20cwd%20unresolved.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
reportedIn:
  - "[deepseek-ai deepseek-harness system-prompt README at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20system-prompt%20README%20at%20fcc0ad4.md)"
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
subject: "[System prompt flags](../../subjects/instructions/System%20prompt%20flags.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---


