---
proposition:
  subject: "[OpenCode](../../harnesses/OpenCode.md)"
  predicate: config/credentials-env
  object:
    supported: true
    login_command: opencode auth login
    vars:
      - name: OPENCODE_SERVER_PASSWORD
        kind: other
        provider: opencode serve/web basic auth
    note: provider keys come from provider environment variables or stored login in ~/.local/share/opencode/auth.json; the config file wins over environment variables
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Credentials docs](../../study-results/OpenCode/2026-11-09%20OpenCode%20Credentials%20docs.md)"
      - "[2026-11-09 OpenCode Auth login docs](../../study-results/OpenCode/2026-11-09%20OpenCode%20Auth%20login%20docs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: providers and CLI docs.
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Provider env var docs](../../study-results/OpenCode/2026-11-09%20OpenCode%20Provider%20env%20var%20docs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: "providers docs: provider environment variables."
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[OpenCode](../../harnesses/OpenCode.md)"
subject: "[Credential variables](../../subjects/config/Credential%20variables.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
