// node --test scripts/stage-npm-package.test.ts
import { strict as assert } from 'node:assert'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'

import { NPM_PUBLISHED, runWithStubNpx } from './stub-npx.ts'

const SCRIPT = join(import.meta.dirname, 'stage-npm-package.ts')
const REQUIRED = ['--repository', 'osdlc/skillwright', '--workflow', 'release.yml']

/** Runs the script on one package that npm reports as published. */
function run(flags: string[], stubBody = NPM_PUBLISHED) {
  return runWithStubNpx((root) => {
    const pkg = join(root, 'pkg')
    mkdirSync(pkg)
    writeFileSync(
      join(pkg, 'package.json'),
      JSON.stringify({ name: '@osdlc/example', version: '1.0.0', license: 'MIT' }),
    )
    return [SCRIPT, ...flags, pkg]
  }, stubBody)
}

test('--repository and --workflow set the trusted publisher', () => {
  const r = run(['--dry-run', '--repository', 'someone/else', '--workflow', 'ci.yml'])
  assert.equal(r.status, 0, r.stderr)
  assert.match(r.stdout, /npm trust github --file ci\.yml --repository someone\/else /)
})

test('--repository is required', () => {
  const r = run(['--dry-run', '--workflow', 'release.yml'])
  assert.equal(r.status, 2)
  assert.match(r.stderr, /no --repository given/)
})

test('--workflow is required', () => {
  const r = run(['--dry-run', '--repository', 'osdlc/skillwright'])
  assert.equal(r.status, 2)
  assert.match(r.stderr, /no --workflow given/)
})

test('--repository must be owner/repo', () => {
  for (const bad of ['foo', 'a/b/c', '/b', 'a/', 'a b/c']) {
    const r = run(['--dry-run', '--repository', bad, '--workflow', 'release.yml'])
    assert.equal(r.status, 2, bad)
    assert.match(r.stderr, /--repository must be <owner>\/<repo>/, bad)
  }
})

test('a failed npm trust prints the organization and repository separately', () => {
  // whoami succeeds, the package is on npm with no trusted publisher, and `trust github` fails.
  const stub =
    'case "$3" in whoami) echo me ;; view) ;; trust) case "$4" in list) echo "[]" ;; *) exit 1 ;; esac ;; esac\n'
  const r = run(REQUIRED, stub)
  assert.equal(r.status, 1)
  assert.match(r.stderr, /organization osdlc, repository skillwright, workflow release\.yml/)
})
