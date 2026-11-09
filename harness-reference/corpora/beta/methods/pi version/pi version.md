---
methodType: probe
harnesses:
  - "[Pi](../../harnesses/Pi.md)"
subjects:
  - "[Version seen](../../subjects/meta/Version%20seen.md)"
commands:
  pi:
    - pi --version
record:
  - The version printed.
safety: []
---

# pi version

Prints the version of the `pi` on the path. It needs no fixture and no account.

Imported from `probes/pi-version/probe.md`.
