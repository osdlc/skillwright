// node --test scripts/stage-npm-package.test.ts
import { strict as assert } from 'node:assert'
import { spawnSync } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { test } from 'node:test'

const SCRIPT = join(import.meta.dirname, 'stage-npm-package.ts')

/**
 * Dry-runs the script on one package that npm reports as published, with a
 * stub `npx` on PATH: `whoami` fails and `view` succeeds. The script then
 * prints the `npm trust github` command without running it.
 */
function dryRun(flags: string[]): string {
  const root = mkdtempSync(join(tmpdir(), 'stage-npm-package-test-'))
  try {
    const bin = join(root, 'bin')
    mkdirSync(bin)
    const npx = join(bin, 'npx')
    writeFileSync(npx, '#!/bin/sh\ncase "$3" in view) exit 0 ;; *) exit 1 ;; esac\n')
    chmodSync(npx, 0o755)
    const pkg = join(root, 'pkg')
    mkdirSync(pkg)
    writeFileSync(
      join(pkg, 'package.json'),
      JSON.stringify({ name: '@osdlc/example', version: '1.0.0', license: 'MIT' }),
    )
    const r = spawnSync(process.execPath, [SCRIPT, '--dry-run', ...flags, pkg], {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env['PATH'] ?? ''}` },
    })
    assert.equal(r.status, 0, r.stderr)
    return r.stdout
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

test('the trusted publisher names osdlc/skillwright by default', () => {
  assert.match(dryRun([]), /npm trust github .*--repository osdlc\/skillwright /)
})

test('--repository sets the trusted publisher repository', () => {
  assert.match(
    dryRun(['--repository', 'someone/else']),
    /npm trust github .*--repository someone\/else /,
  )
})
