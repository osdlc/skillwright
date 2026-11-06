---
name: skillwright
description: "Skills for the skillwright CLI: install it, author skills, plugins and marketplaces as markdown notes in a vault, publish and build them for Claude Code, Codex, Cursor and Pi, check for drift, and move a vault over from tradecraft."
sw-version: 0.1.0
---

These skills teach an agent to drive the `skillwright` command-line tool. They need the
`skillwright` binary on `PATH`; `/[[setup]]` checks for it and installs it. The plugin does not
ship the binary.

This plugin is itself a skillwright vault, built with `skillwright build`.
