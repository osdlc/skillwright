---
proposition:
  subject: "[Cursor](../../harnesses/Cursor.md)"
  predicate: hooks/events
  object:
    events:
      - sessionStart
      - sessionEnd
      - preToolUse
      - postToolUse
      - postToolUseFailure
      - subagentStart
      - subagentStop
      - beforeShellExecution
      - afterShellExecution
      - beforeMCPExecution
      - afterMCPExecution
      - beforeReadFile
      - afterFileEdit
      - beforeSubmitPrompt
      - preCompact
      - stop
      - afterAgentResponse
      - afterAgentThought
      - beforeTabFileRead
      - afterTabFileEdit
      - workspaceOpen
    via: hooks.json
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 Cursor docs hook events](../../study-results/Cursor/2026-11-09%20Cursor%20docs%20hook%20events.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Hook categories list.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T12:00:00Z
harness: "[Cursor](../../harnesses/Cursor.md)"
subject: "[Hook events](../../subjects/hooks/Hook%20events.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---

Whether the CLI fires every event the IDE does is not stated.
