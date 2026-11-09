---
proposition:
  subject: "[Claude Code](../../harnesses/Claude%20Code.md)"
  predicate: cli/output-format
  object:
    supported: true
    formats:
      - name: text
        argv:
          - --output-format
          - text
      - name: json
        argv:
          - --output-format
          - json
      - name: stream-json
        argv:
          - --output-format
          - stream-json
          - --verbose
    note: Print mode only. The flag defaults to text. The docs example for stream-json passes --verbose; the help text does not say it is required.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 help output format](../../study-results/Claude%20Code/2026-11-09%20help%20output%20format.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: Help text lists text (default), json and stream-json, print mode only.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli output-format](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20output-format.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The CLI reference lists the same three formats.
  - hasEvidenceItems:
      - "[2026-11-09 docs cli stream flags](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20stream%20flags.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The docs example for stream-json passes --verbose with --output-format stream-json.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
subject: "[Output format flag](../../subjects/cli/Output%20format%20flag.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
open:
  - Does stream-json fail without --verbose in print mode? The help text and docs excerpts do not state a requirement; the osdlc agent-call adapter always adds --verbose.
---
