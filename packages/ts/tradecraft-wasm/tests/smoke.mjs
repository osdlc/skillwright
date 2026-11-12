// Runs the built package under Node. The unit tests run the TypeScript source
// (vitest.config.ts aliases the package name and the wasm glue to it), so a
// broken emit, or a dist/ without the wasm, could pass them and fail on the
// consumer's machine.
//
// It imports the package by name, so Node resolves it through `exports` as a
// consumer would. `pnpm run smoke` runs it here, where the name resolves to
// this package's own dist/. It is not a test-runner suite and is not in
// `files`.

import { strict as assert } from 'node:assert'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { buildVault, CoreError, createNodeVfs, knownTargets } from '@osdlc/tradecraft-wasm'

assert.ok(knownTargets().includes('claude'))

const root = mkdtempSync(join(tmpdir(), 'tradecraft-smoke-'))
try {
  const vault = join(root, 'vault')
  const folder = join(vault, 'marketplaces/house/plugins/tools/skills/review')
  mkdirSync(folder, { recursive: true })
  writeFileSync(join(vault, 'skillwright.md'), '---\n---\n')
  writeFileSync(
    join(vault, 'marketplaces/house/house.md'),
    '---\ndescription: The house marketplace.\nowner: acme\n---\n',
  )
  writeFileSync(join(vault, 'marketplaces/house/plugins/tools/tools.md'), '---\n---\n')
  writeFileSync(join(folder, 'review.md'), '---\ndescription: Reviews a diff.\n---\n\n# Review\n')
  const out = join(vault, 'dist/house')

  const report = buildVault(createNodeVfs(), {
    vault_root: vault,
    mode: 'build',
  })
  assert.equal(report.marketplaces[0].report.changed, true)
  assert.match(readFileSync(join(out, '.claude-plugin/marketplace.json'), 'utf-8'), /house/)

  assert.throws(
    () =>
      buildVault(createNodeVfs(), {
        vault_root: join(root, 'missing'),
        mode: 'build',
      }),
    (error) => error instanceof CoreError,
  )
} finally {
  rmSync(root, { recursive: true, force: true })
}

console.log('tradecraft-wasm smoke: OK')
