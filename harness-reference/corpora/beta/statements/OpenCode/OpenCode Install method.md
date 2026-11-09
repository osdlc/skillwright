---
proposition:
  subject: "[OpenCode](../../harnesses/OpenCode.md)"
  predicate: cli/install
  object:
    supported: true
    methods:
      - kind: script
        argv:
          - bash
          - -c
          - curl -fsSL https://opencode.ai/v2/install | bash
      - kind: npm
        argv:
          - npm
          - i
          - -g
          - "@opencode/cli@latest"
        package: "@opencode/cli"
      - kind: brew
        argv:
          - brew
          - install
          - anomalyco/tap/opencode-v2
        package: anomalyco/tap/opencode-v2
    update: opencode upgrade [target] with --method curl|npm|pnpm|bun|brew|choco|scoop
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode README v2 install](../../study-results/OpenCode/2026-11-09%20OpenCode%20README%20v2%20install.md)"
      - "[2026-11-09 OpenCode README v2 npm install](../../study-results/OpenCode/2026-11-09%20OpenCode%20README%20v2%20npm%20install.md)"
      - "[2026-11-09 OpenCode README v2 brew install](../../study-results/OpenCode/2026-11-09%20OpenCode%20README%20v2%20brew%20install.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: README at v2.2.6.
  - hasEvidenceItems:
      - "[2026-11-09 OpenCode Upgrade methods help](../../study-results/OpenCode/2026-11-09%20OpenCode%20Upgrade%20methods%20help.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: help-text for 1.1.36 lists the upgrade methods.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[OpenCode](../../harnesses/OpenCode.md)"
subject: "[Install method](../../subjects/cli/Install%20method.md)"
scope:
  since: 2.0.0
  until: null
  platform: []
  model: null
status: current
open:
  - The install commands of the 1.x line were not checked.
---
