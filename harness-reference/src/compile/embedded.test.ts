/**
 * The copy of `harness-knowledge.json` that tradecraft-core embeds must be what `compile` writes
 * for the current corpus. Permalinks carry the commit, so the copy is compiled with the fixed
 * placeholder commit `main` (see `pnpm run compile:embed`) and is the same on every checkout.
 */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import { BETA, loadBeta } from '../beta/corpus.ts'
import { compile } from './compile.ts'

const EMBEDDED = resolve(
  import.meta.dirname,
  '../../../packages/rust/tradecraft-core/data/harness-knowledge.json',
)

test('the copy tradecraft-core embeds is current', () => {
  const expected = `${JSON.stringify(compile(loadBeta(BETA), 'main'), null, 2)}\n`
  assert.equal(
    readFileSync(EMBEDDED, 'utf8'),
    expected,
    'stale: run `pnpm --filter harness-reference run compile:embed` and commit the result',
  )
})
