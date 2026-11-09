---
proposition:
  subject: "[Claude Code](../../harnesses/Claude%20Code.md)"
  predicate: config/credentials-env
  object:
    supported: true
    vars:
      - name: ANTHROPIC_API_KEY
        note: API key; wins over a logged-in subscription, always used with -p
      - name: CLAUDE_CODE_OAUTH_TOKEN
        note: OAuth token from `claude setup-token`; wins over keychain credentials
      - name: ANTHROPIC_AUTH_TOKEN
        note: sent as a Bearer Authorization header
    login_command: claude auth login
    note: Bedrock, Vertex and Foundry use their own provider credentials.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 docs env API_KEY](../../study-results/Claude%20Code/2026-11-09%20docs%20env%20API_KEY.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: ANTHROPIC_API_KEY is sent as X-Api-Key and overrides a subscription.
  - hasEvidenceItems:
      - "[2026-11-09 docs env OAUTH_TOKEN](../../study-results/Claude%20Code/2026-11-09%20docs%20env%20OAUTH_TOKEN.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: CLAUDE_CODE_OAUTH_TOKEN is the SDK and automation alternative to /login.
  - hasEvidenceItems:
      - "[2026-11-09 docs env AUTH_TOKEN](../../study-results/Claude%20Code/2026-11-09%20docs%20env%20AUTH_TOKEN.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: ANTHROPIC_AUTH_TOKEN sets the Authorization header.
  - hasEvidenceItems:
      - "[2026-11-09 help auth login](../../study-results/Claude%20Code/2026-11-09%20help%20auth%20login.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: "`claude auth login` signs in."
  - hasEvidenceItems:
      - "[2026-11-09 help setup token](../../study-results/Claude%20Code/2026-11-09%20help%20setup%20token.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: "`claude setup-token` makes a long-lived token."
  - hasEvidenceItems:
      - "[2026-11-09 docs cli auth login](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20auth%20login.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The CLI reference lists `claude auth login`.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
subject: "[Credential variables](../../subjects/config/Credential%20variables.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
