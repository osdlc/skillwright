---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: cli/output-format
  object:
    supported: true
    formats:
      - name: text
        argv:
          - -p
      - name: json
        argv:
          - --mode
          - json
      - name: rpc
        argv:
          - --mode
          - rpc
    note: --mode text is the default; -p prints the final answer and exits; json is a JSONL event stream
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Pi help print mode](../../study-results/Pi/2026-11-09%20Pi%20help%20print%20mode.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: --mode text|json|rpc and -p.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Pi](../../harnesses/Pi.md)"
subject: "[Output format flag](../../subjects/cli/Output%20format%20flag.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
