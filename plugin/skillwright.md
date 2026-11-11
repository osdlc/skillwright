---
author: osdlc
license: MIT OR Apache-2.0
---

# osdlc

The root file for the skillwright plugin's vault. From the repository root:

    skillwright build --yes --vault plugin
    skillwright check --vault plugin

The built trees are in `dist/osdlc/` beside this file and are committed. The
`plugin` workflow rebuilds them and fails on any difference. See README.md.
