/**
 * `statusOf` hands back exactly the keys of the generated `PublishUnitStatus`,
 * which is generated from the Rust source.
 */
import { afterEach, beforeEach, expect, test } from 'vitest'
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import type { PublishUnitStatus as WirePublishUnitStatus } from '../src/core.generated.ts'
import { statusOf } from '../src/core.ts'
import { createNodeVfs } from '../src/node-vfs.ts'

/** `tsc` refuses this when a key is missing or extra. */
const WIRE_KEYS = {
  state: true,
  reason: true,
  published_at: true,
  files: true,
  contributing_notes: true,
  missing_notes: true,
  not_published: true,
  version_bump_suggestion: true,
} satisfies Record<keyof WirePublishUnitStatus, true>

let tmp: string

beforeEach(() => {
  tmp = realpathSync.native(mkdtempSync(path.join(tmpdir(), 'tradecraft-status-of-')))
})

afterEach(() => {
  rmSync(tmp, { recursive: true, force: true })
})

test('statusOf hands back exactly the keys of the generated report', () => {
  const vault = path.join(tmp, 'vault')
  const skill = path.join(vault, 'marketplaces/house/plugins/tools/skills/review')
  mkdirSync(skill, { recursive: true })
  writeFileSync(
    path.join(vault, 'marketplaces/house/house.md'),
    '---\nname: house\ndescription: The house marketplace.\nowner: acme\n---\n',
  )
  const note = path.join(skill, 'review.md')
  writeFileSync(note, '---\nname: review\ndescription: Reviews a diff.\n---\n\n# Review\n')

  const status = statusOf(createNodeVfs(), {
    skill_path: note,
    destination_root: path.join(tmp, 'out'),
    target: 'claude',
    skills_root: vault,
  })

  expect(Object.keys(status).sort()).toEqual(Object.keys(WIRE_KEYS).sort())
})
