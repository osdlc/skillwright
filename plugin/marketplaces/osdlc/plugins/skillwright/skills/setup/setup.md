---
description: "Install the skillwright CLI and set up a project for it: check `skillwright --version`, install with brew, npm, cargo or install.sh (or from source when the release is not out), and write a `skillwright.md` root file with `skillwright init`. Use when `skillwright` is not on PATH, when a repository has no `skillwright.md`, or when asked to install skillwright, configure it, or start a skill vault. The other skillwright skills send you here when the binary is missing."
---

# /[[setup]]

Scope: installs the binary and writes the root file. It does not author notes (see
/[[author]]) or publish them (see /[[publish]]).

## Steps

1. Run `skillwright --version`. If it prints a version, go to step 4.

2. Install the binary with one of the release channels, then run `skillwright --version` again:

   ```sh
   brew install osdlc/tap/skillwright
   npm install --global @osdlc/skillwright   # or run once: npx @osdlc/skillwright --help
   cargo binstall skillwright
   cargo install skillwright
   curl -fsSL https://github.com/osdlc/skillwright/releases/download/skillwright/v<version>/install.sh | sh
   ```

   `install.sh` checks the binary against the release's `SHA256SUMS` and installs to `~/.local/bin`,
   or to the directory given with `--to`. Prebuilt binaries cover macOS and Linux, arm64 and x86_64.
   Windows is not supported.

   On macOS, a binary downloaded with a web browser is quarantined. The user must allow it in System
   Settings, Privacy & Security. The installers above are not affected.

3. If no channel has a `skillwright` package for this platform, build from source
   instead (Rust 1.89 or later):

   ```sh
   cargo install --locked --git https://github.com/osdlc/skillwright skillwright
   ```

   Run `skillwright --version` again. Stop and report the error if it still fails.

4. Look for a root file. skillwright reads the nearest `skillwright.md` at or above the working
   directory, or the file named with `--config`. If one exists, read it and stop.

5. Run `skillwright init --out <path>` in the folder that will hold the notes. It writes a
   `skillwright.md` with the marketplace's name and `build.out`, the folder `build` writes to. The
   name defaults to the folder's name; `--name` sets another. There is no default output folder:
   `--out` is asked for on a terminal and required otherwise. `--out .` builds beside
   `skillwright.md`, which makes the repository installable as a marketplace.

   The result is the smallest working file. Add to it as needed:

   ```markdown
   ---
   name: house                  # the marketplace's fields are top-level keys
   owner: me
   license: MIT                 # plugins and skills inherit this
   build:                       # how skillwright runs
     out: dist
     destinations:              # where `publish` writes; absent means each harness's user folder
       - root: ../my-project    # must sit outside the vault, or `publish` refuses `unsafe-root`
         target: claude-code
   ---
   ```

   The body is ignored. Relative paths resolve against the file's own folder. A flag overrides
   this file. There is no machine file: personal defaults come from the environment.

6. Verify: run `skillwright list`. Exit code 4 means no vault was found; check the file name and
   the working directory.
