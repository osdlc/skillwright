---
proposition:
  subject: "[Pi](../../harnesses/Pi.md)"
  predicate: plugins/install
  object:
    commands:
      - pi install ./local-package
      - pi install git:github.com/example/pi-tools@v1
      - pi install npm:@example/pi-tools@1.0.0
      - pi install https://github.com/user/repo
    sources: npm, git (also a bare URL or ssh), or a local path
    recorded_in: packages in ~/.pi/agent/settings.json, or .pi/settings.json with -l
direction: supports
strength: strong
hasEvidenceLines:
  - hasEvidenceItems:
      - "[2026-08-08 Pi Plugin install](../../study-results/Pi/2026-08-08%20Pi%20Plugin%20install.md)"
    directionOfEvidenceProvided: neutral
    description: "third-party observed 2026-08-08; its value differs from the latest. Rechecked 2026-11-09: the 2026-08-08 third-party entry listed only `pi install` with no sources."
  - hasEvidenceItems:
      - "[2026-10-01 Pi Plugin install](../../study-results/Pi/2026-10-01%20Pi%20Plugin%20install.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: run observed 2026-10-01.
  - hasEvidenceItems:
      - "[2026-11-09 Pi help install](../../study-results/Pi/2026-11-09%20Pi%20help%20install.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: "`pi install --help` examples."
  - hasEvidenceItems:
      - "[2026-11-09 Pi help install local](../../study-results/Pi/2026-11-09%20Pi%20help%20install%20local.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: strong
    description: -l writes project settings.
  - hasEvidenceItems:
      - "[2026-11-09 Pi packages install](../../study-results/Pi/2026-11-09%20Pi%20packages%20install.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Packages page.
  - hasEvidenceItems:
      - "[2026-11-09 Pi packages settings files](../../study-results/Pi/2026-11-09%20Pi%20packages%20settings%20files.md)"
    directionOfEvidenceProvided: supports
    strengthOfEvidenceProvided: moderate
    description: Settings files.
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
subject: "[Plugin install](../../subjects/plugins/Plugin%20install.md)"
scope:
  since: null
  until: null
  platform: []
  model: null
status: current
open:
  - Is a plain string entry in `packages` accepted besides the object form with `source`? Only the object form is shown in the docs.
imported_from: fact:plugins/install@pi#scope-[null,null,null]
---

Imported from `facts/pi/plugins/install.md`.

The observations disagree on: commands. The latest value is kept.

Open questions:
- What is the shape of a `packages` entry for a local install (a string, or an object with `source`), and is a relative entry read against the settings file's folder or the folder `pi install` ran in?

Rechecked 2026-11-09 against the installed pi 1.5.0 and the v1.5.0 source tree.
