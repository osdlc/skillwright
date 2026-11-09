---
proposition:
  subject: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
  predicate: skills/agent-loading
  object:
    mechanism: skill tool
    loads: "on demand: the model calls skill with the exact skill name"
    user_activation: a /name token in direct user input that names a user-invocable skill injects the body
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-10-08 DeepSeek Harness How the agent loads a skill](../../study-results/DeepSeek%20Harness/2026-10-08%20DeepSeek%20Harness%20How%20the%20agent%20loads%20a%20skill.md)"
    directionOfEvidenceProvided: neutral
    description: source observed 2026-10-08; its locator (skill-filesystem/README.md at 5badb150) does not state the mechanism, so it is neutral.
  - hasEvidenceItems:
      - "[2026-11-04 DeepSeek Harness How the agent loads a skill](../../study-results/DeepSeek%20Harness/2026-11-04%20DeepSeek%20Harness%20How%20the%20agent%20loads%20a%20skill.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-04.
  - hasEvidenceItems:
      - "[2026-11-09 tool-skill loader tool](../../study-results/DeepSeek%20Harness/2026-11-09%20tool-skill%20loader%20tool.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 tool-skill user invocation](../../study-results/DeepSeek%20Harness/2026-11-09%20tool-skill%20user%20invocation.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 tool-skill summary](../../study-results/DeepSeek%20Harness/2026-11-09%20tool-skill%20summary.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
  - hasEvidenceItems:
      - "[2026-11-09 skill-filesystem on demand](../../study-results/DeepSeek%20Harness/2026-11-09%20skill-filesystem%20on%20demand.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-11-09.
reportedIn:
  - "[DeepSeek Harness (dsh) harness notes for a dispatcher](../../documents/skillwright/DeepSeek%20Harness%20%28dsh%29%20harness%20notes%20for%20a%20dispatcher.md)"
  - "[dsh tool-skill package README](../../documents/github.com/dsh%20tool-skill%20package%20README.md)"
  - "[dsh skill-filesystem package README](../../documents/github.com/dsh%20skill-filesystem%20package%20README.md)"
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
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[DeepSeek Harness](../../harnesses/DeepSeek%20Harness.md)"
subject: "[How the agent loads a skill](../../subjects/skills/How%20the%20agent%20loads%20a%20skill.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
imported_from: fact:skills/agent-loading@dsh#scope-[null,null,null]
---
