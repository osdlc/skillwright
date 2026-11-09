---
proposition:
  subject: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
  predicate: cli/headless
  object:
    command: dsh --profile headless [--json] [--session-id <id>] <task>
    stdin: omit the task or pass -
    one_task_per_run: true
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-10-08 DeepSeek Harness Headless run](../../study-results/DeepSeek%20Harness/2026-10-08%20DeepSeek%20Harness%20Headless%20run.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-10-08.
  - hasEvidenceItems:
      - "[2026-11-09 headless help usage](../../study-results/DeepSeek%20Harness/2026-11-09%20headless%20help%20usage.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 headless help task stdin](../../study-results/DeepSeek%20Harness/2026-11-09%20headless%20help%20task%20stdin.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 reference headless persisted](../../study-results/DeepSeek%20Harness/2026-11-09%20reference%20headless%20persisted.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 reference headless exit code](../../study-results/DeepSeek%20Harness/2026-11-09%20reference%20headless%20exit%20code.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
reportedIn:
  - "[DeepSeek Harness (dsh) harness notes for a dispatcher](../../documents/skillwright/DeepSeek%20Harness%20%28dsh%29%20harness%20notes%20for%20a%20dispatcher.md)"
  - "[deepseek-ai deepseek-harness cli-help.md at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20cli-help.md%20at%20fcc0ad4.md)"
  - "[deepseek-ai deepseek-harness apps cli reference README at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20apps%20cli%20reference%20README%20at%20fcc0ad4.md)"
contributions:
  - contributor:
      name: harness-reference import
      agentType: software
    activityType: created
    date: 2026-10-08T00:00:00Z
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
subject: "[Headless run](../../subjects/cli/Headless%20run.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
imported_from: fact:cli/headless@dsh#scope-[null,null,null]
---

Imported from `facts/dsh/cli/headless.md`.
