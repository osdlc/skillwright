import assert from 'node:assert/strict'
import { describe, test } from 'vitest'

import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

import { validateCorpus } from '../src/validate.ts'

const CASES = join(import.meta.dirname, 'cases')

describe('cases', () => {
  for (const name of readdirSync(CASES, { withFileTypes: true })) {
    if (!name.isDirectory()) continue
    test(name.name, () => {
      const dir = join(CASES, name.name)
      const expected = JSON.parse(readFileSync(join(dir, 'expected.json'), 'utf8'))
      const { errors, warnings } = validateCorpus(join(dir, 'corpus'))
      assert.deepEqual({ errors, warnings }, expected)
    })
  }
})
