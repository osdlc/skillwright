---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: skills/agent-loading
  object:
    mechanism: model reads SKILL.md with the read tool (bash when there is no read tool)
    base_dir: told to resolve relative paths against the skill directory
    trigger: task matches the description
    explicit: /skill:name
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-10-01 Pi How the agent loads a skill](../../study-results/Pi/2026-10-01%20Pi%20How%20the%20agent%20loads%20a%20skill.md)"
    directionOfEvidenceProvided: neutral
    description: "source observed 2026-10-01; its value differs from the latest. Rechecked 2026-11-09: the source line and the later docs line say the same thing in different words: the model loads SKILL.md itself."
  - hasEvidenceItems:
      - "[2026-11-04 Pi How the agent loads a skill](../../study-results/Pi/2026-11-04%20Pi%20How%20the%20agent%20loads%20a%20skill.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: docs observed 2026-11-04.
  - hasEvidenceItems:
      - "[2026-11-09 Pi skills loading](../../study-results/Pi/2026-11-09%20Pi%20skills%20loading.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Skills page.
  - hasEvidenceItems:
      - "[2026-11-09 Pi skill prompt text](../../study-results/Pi/2026-11-09%20Pi%20skill%20prompt%20text.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: The skills prompt tells the model to use the read tool.
  - hasEvidenceItems:
      - "[2026-11-09 Pi skill relative paths](../../study-results/Pi/2026-11-09%20Pi%20skill%20relative%20paths.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: The prompt tells the model to resolve relative paths against the skill directory.
reportedIn:
  - "[Pi README](../../documents/Earendil%20Inc/Pi%20README.md)"
  - "[Pi docs Skills (main at d57c2d2)](../../documents/github.com/Pi%20docs%20Skills%20%28main%20at%20d57c2d2%29.md)"
contributions:
  - contributor:
      name: harness-reference import
      agentType: software
    activityType: created
    date: 2026-11-04T00:00:00Z
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[Pi](../../harnesses/Pi.md)"
subject: "[How the agent loads a skill](../../subjects/skills/How%20the%20agent%20loads%20a%20skill.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
imported_from: fact:skills/agent-loading@pi#scope-[null,null,null]
---

Imported from `facts/pi/skills/agent-loading.md`.

The observations disagree on: mechanism. The latest value is kept.

Rechecked 2026-11-09 against the installed pi 1.5.0 and the v1.5.0 source tree. The dispute was wording only.
