// Test helper: runs a command with a stub `npx` first on PATH, so tests of
// stage-npm-package.ts never reach npm.
import { spawnSync, type SpawnSyncReturns } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'

/**
 * The stub's `/bin/sh` body. The script calls `npx -y npm@<version> <command> ...`,
 * so `$3` is the npm command and `$4` its subcommand. This default reports every
 * package as published and fails every other command (`whoami` included).
 */
export const NPM_PUBLISHED = 'case "$3" in view) exit 0 ;; *) exit 1 ;; esac\n'

/**
 * Runs `node <args>` with a stub `npx` running `stubBody` on PATH. `setup` gets
 * a fresh scratch directory to create any files `args` needs; it returns the args.
 */
export function runWithStubNpx(
  setup: (root: string) => string[],
  stubBody: string = NPM_PUBLISHED,
): SpawnSyncReturns<string> {
  const root = mkdtempSync(join(tmpdir(), 'stub-npx-'))
  try {
    const bin = join(root, 'bin')
    mkdirSync(bin)
    const npx = join(bin, 'npx')
    writeFileSync(npx, `#!/bin/sh\n${stubBody}`)
    chmodSync(npx, 0o755)
    return spawnSync(process.execPath, setup(root), {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env['PATH'] ?? ''}` },
    })
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}
