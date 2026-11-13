// node --test scripts/harness-knowledge-slice.test.ts
//
// The copy of the harness knowledge that tradecraft-core embeds must be what
// scripts/harness-knowledge-slice.ts writes from the pinned `@osdlc/harness-knowledge`.
import { strict as assert } from 'node:assert'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { EMBEDDED, embedded, sliceKnowledge } from './harness-knowledge-slice.ts'

test('the copy tradecraft-core embeds is cut from the pinned @osdlc/harness-knowledge', () => {
  assert.equal(
    readFileSync(EMBEDDED, 'utf8'),
    embedded(),
    'stale: run `pnpm run harness-knowledge:slice` and commit the result',
  )
})

test('a slice keeps the listed harnesses and subjects, their values, and no valueSchema', () => {
  const knowledge = {
    schema: 1,
    harnesses: { a: { title: 'A' }, b: { title: 'B' } },
    subjects: { 'x/one': { title: 'One', valueSchema: {} }, 'x/two': { title: 'Two' } },
    values: [
      { harness: 'a', subject: 'x/one', value: 1 },
      { harness: 'a', subject: 'x/two', value: 2 },
      { harness: 'b', subject: 'x/one', value: 3 },
    ],
  }
  assert.deepEqual(sliceKnowledge(knowledge, { harnesses: ['a'], subjects: ['x/one'] }), {
    schema: 1,
    harnesses: { a: { title: 'A' } },
    subjects: { 'x/one': { title: 'One' } },
    values: [{ harness: 'a', subject: 'x/one', value: 1 }],
  })
})

test('a slice that names an unknown harness or subject is refused', () => {
  const knowledge = { harnesses: {}, subjects: {}, values: [] }
  assert.throws(
    () => sliceKnowledge(knowledge, { harnesses: ['a'], subjects: ['x/one'] }),
    /not in the knowledge file: a, x\/one/u,
  )
})
