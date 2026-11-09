---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: skills/project-dirs
  object:
    dirs:
      - <cwd>/.pi/skills
      - .agents/skills
    scope: .agents/skills in each ancestor up to the git root
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-08-08 Pi Project skill directories](../../study-results/Pi/2026-08-08%20Pi%20Project%20skill%20directories.md)"
    directionOfEvidenceProvided: neutral
    description: "third-party observed 2026-08-08; its value differs from the latest. Rechecked 2026-11-09: the 2026-08-08 third-party list gave .pi/skills and .agents/skills without the ancestor walk; the source and docs agree on the fuller form."
  - hasEvidenceItems:
      - "[2026-10-01 Pi Project skill directories](../../study-results/Pi/2026-10-01%20Pi%20Project%20skill%20directories.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: source observed 2026-10-01.
  - hasEvidenceItems:
      - "[2026-11-09 Pi skills agents dirs](../../study-results/Pi/2026-11-09%20Pi%20skills%20agents%20dirs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: "Skills page: .agents/skills walk."
  - hasEvidenceItems:
      - "[2026-11-09 Pi skills project dirs](../../study-results/Pi/2026-11-09%20Pi%20skills%20project%20dirs.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: "Source: project skills dir under .pi."
  - hasEvidenceItems:
      - "[2026-11-09 Pi security trust resources](../../study-results/Pi/2026-11-09%20Pi%20security%20trust%20resources.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Security page lists .pi/skills and project .agents/skills.
reportedIn:
  - "[Harness comparison](../../documents/skillwright/Harness%20comparison.md)"
  - "[Pi README](../../documents/Earendil%20Inc/Pi%20README.md)"
contributions:
  - contributor:
      name: harness-reference import
      agentType: software
    activityType: created
    date: 2026-10-01T00:00:00Z
  - contributor:
      name: harness-reference research
      agentType: software
    activityType: verified
    date: 2026-11-09T12:00:00Z
harness: "[Pi](../../harnesses/Pi.md)"
subject: "[Project skill directories](../../subjects/skills/Project%20skill%20directories.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
---

Imported from `facts/pi/skills/project-dirs.md`.

The observations disagree on: dirs. The latest value is kept.

Rechecked 2026-11-09 against the installed pi 1.5.0 and the v1.5.0 source tree. The dispute was an incomplete old list.
