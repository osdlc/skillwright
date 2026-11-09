---
proposition:
  subject: "[Claude Code](../../harnesses/Claude%20Code.md)"
  predicate: plugins/session-load
  object:
    supported: true
    load_argv:
      - --plugin-dir
      - "{dir}"
    repeatable: true
    disable_via: enabledPlugins in settings; the osdlc adapter passes --settings {"enabledPlugins":{"<id>":false}}
    persists: false
    note: The flag also takes a .zip or a folder of plugins; --plugin-url fetches a .zip for one session.
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-11-09 help plugin dir](../../study-results/Claude%20Code/2026-11-09%20help%20plugin%20dir.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: "Help text: loads a plugin for this session only; repeatable."
  - hasEvidenceItems:
      - "[2026-11-09 docs cli plugin-dir](../../study-results/Claude%20Code/2026-11-09%20docs%20cli%20plugin-dir.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The CLI reference repeats it.
  - hasEvidenceItems:
      - "[2026-11-09 docs pinstall plugin-dir](../../study-results/Claude%20Code/2026-11-09%20docs%20pinstall%20plugin-dir.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: The plugin install page says --plugin-dir loads a plugin for one session instead of installing it.
  - hasEvidenceItems:
      - "[2026-11-09 docs pinstall user scope](../../study-results/Claude%20Code/2026-11-09%20docs%20pinstall%20user%20scope.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: enabledPlugins in settings.json holds enable state.
contributions:
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: created
    date: 2026-11-09T07:53:54Z
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
subject: "[Session-only plugins](../../subjects/plugins/Session-only%20plugins.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
open:
  - Is {"enabledPlugins":{"<id>":false}} passed through --settings documented as a way to disable an installed plugin for one session? The settings reference was not quoted.
---
