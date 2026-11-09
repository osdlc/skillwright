---
proposition:
  subject: "[Claude Code](../../harnesses/Claude%20Code.md)"
  predicate: cli/current-env
  object:
    supported: true
    vars:
      - name: CLAUDECODE
        value: "1"
      - name: CLAUDE_CODE_CHILD_SESSION
        value: "1"
    reliable: true
    note: CLAUDECODE is set in subprocesses Claude Code spawns (Bash and PowerShell tools, tmux sessions, hook and status line commands, stdio MCP servers); IDE extensions also set it in integrated terminals, so it does not tell a nested session from a top-level claude in an IDE terminal. CLAUDE_CODE_CHILD_SESSION is set only by Claude Code itself, not for stdio MCP servers, and requires 2.1.172 or later. CLAUDE_CODE_SESSION_ID and CLAUDE_PID are also exported but are session details, not markers.
direction: supports
strength: moderate
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 docs env CLAUDECODE](../../study-results/Claude%20Code/2026-11-09%20docs%20env%20CLAUDECODE.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The environment variable reference documents CLAUDECODE as the way to detect a script running in a subprocess spawned by Claude Code.
  - hasEvidenceItems:
      - "[2026-11-09 docs env CHILD_SESSION](../../study-results/Claude%20Code/2026-11-09%20docs%20env%20CHILD_SESSION.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The reference documents CLAUDE_CODE_CHILD_SESSION for direct tool and hook launches.
  - hasEvidenceItems:
      - "[2026-11-09 changelog session id mcp](../../study-results/Claude%20Code/2026-11-09%20changelog%20session%20id%20mcp.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The 2.1.154 changelog says stdio MCP subprocesses receive CLAUDECODE=1.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
subject: "[Running-inside variables](../../subjects/cli/Running-inside%20variables.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---
