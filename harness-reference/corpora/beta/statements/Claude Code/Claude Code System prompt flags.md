---
proposition:
  subject: "[Claude Code](../../harnesses/Claude%20Code.md)"
  predicate: instructions/system-prompt-flag
  object:
    supported: true
    replace_argv:
      - --system-prompt
      - "{text}"
    replace_file_argv:
      - --system-prompt-file
      - "{file}"
    append_argv:
      - --append-system-prompt
      - "{text}"
    append_file_argv:
      - --append-system-prompt-file
      - "{file}"
    replace_drops_defaults: true
    note: The file variants are in the docs but not in the 2.1.319 help text. A recorded system prompt is reused on resume unless --system-prompt-snapshot off is passed.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 help system prompt](../../study-results/Claude%20Code/2026-11-09%20help%20system%20prompt.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Help text lists --system-prompt.
  - hasEvidenceItems:
      - "[2026-11-09 help append system prompt](../../study-results/Claude%20Code/2026-11-09%20help%20append%20system%20prompt.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Help text lists --append-system-prompt.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli system prompt](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20system%20prompt.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The CLI reference says --system-prompt replaces the entire default prompt.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli system prompt file](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20system%20prompt%20file.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The reference lists --system-prompt-file.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli append system prompt file](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20append%20system%20prompt%20file.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The reference lists --append-system-prompt-file.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
subject: "[System prompt flags](../../subjects/instructions/System%20prompt%20flags.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
