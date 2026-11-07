---
name: osdlc
owner: osdlc
author: osdlc
license: MIT OR Apache-2.0
build:
  out: .
---

# osdlc

The root file for the skillwright plugin's vault. From the repository root:

    skillwright build --yes --config plugin/skillwright.md
    skillwright check --config plugin/skillwright.md

The built trees (`.claude-plugin/`, `.agents/`, `.cursor-plugin/`, `.pi/`)
beside this file are committed. The `plugin` workflow rebuilds them and fails
on any difference. See README.md.
