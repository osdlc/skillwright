---
proposition:
  subject: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
  predicate: permissions/modes
  object:
    supported: true
    modes:
      - name: workspace-write
        intent: edits
        argv:
          - --permission-mode
          - workspace-write
        env:
          DSH_PERMISSION_MODE: workspace-write
        notes: preset workspace-write + approval ask
      - name: danger-full-access
        intent: bypass
        argv:
          - --permission-mode
          - danger-full-access
        env:
          DSH_PERMISSION_MODE: danger-full-access
        notes: preset danger-full-access + approval never
    default: workspace-write
    note: The default configured preset table ships workspace-write and danger-full-access. --approval-policy ask|never adjusts approval after the preset is chosen. DSH_PERMISSION_MODE is documented as the process fallback.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 headless help permission-mode](../../study-results/DeepSeek%20Harness/2026-11-09%20headless%20help%20permission-mode.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 headless help approval-policy](../../study-results/DeepSeek%20Harness/2026-11-09%20headless%20help%20approval-policy.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 reference permission env](../../study-results/DeepSeek%20Harness/2026-11-09%20reference%20permission%20env.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 presets table](../../study-results/DeepSeek%20Harness/2026-11-09%20presets%20table.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 guide permission env value](../../study-results/DeepSeek%20Harness/2026-11-09%20guide%20permission%20env%20value.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 reference default preset](../../study-results/DeepSeek%20Harness/2026-11-09%20reference%20default%20preset.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
reportedIn:
  - "[deepseek-ai deepseek-harness cli-help.md at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20cli-help.md%20at%20fcc0ad4.md)"
  - "[deepseek-ai deepseek-harness apps cli reference README at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20apps%20cli%20reference%20README%20at%20fcc0ad4.md)"
  - "[deepseek-ai deepseek-harness permission-presets.md at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20permission-presets.md%20at%20fcc0ad4.md)"
  - "[deepseek-ai deepseek-harness headless-policy-composition guide at fcc0ad4](../../documents/github.com/deepseek-ai%20deepseek-harness%20headless-policy-composition%20guide%20at%20fcc0ad4.md)"
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
subject: "[Permission modes](../../subjects/permissions/Permission%20modes.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---


