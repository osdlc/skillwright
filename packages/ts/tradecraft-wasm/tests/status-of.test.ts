/**
 * `PublishUnitStatus` is hand-written in src/core.ts (its `files` use this
 * package's widened `PublishedFile`). These checks keep its keys equal to the
 * generated report's, which is generated from the Rust source, and to what
 * `statusOf` actually hands back.
 */
import { afterEach, beforeEach, expect, test } from 'vitest'
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import ts from 'typescript'

import type { PublishUnitStatus as WirePublishUnitStatus } from '../src/core.generated.ts'
import { statusOf, type PublishUnitStatus } from '../src/core.ts'
import { createNodeVfs } from '../src/node-vfs.ts'

const SRC = path.join(import.meta.dirname, '../src')

let tmp: string

beforeEach(() => {
  tmp = realpathSync.native(mkdtempSync(path.join(tmpdir(), 'tradecraft-status-of-')))
})

afterEach(() => {
  rmSync(tmp, { recursive: true, force: true })
})

/** The property names of the interface `name` declared in `file`, sorted. */
function interfaceKeys(file: string, name: string): string[] {
  const source = ts.createSourceFile(file, readFileSync(file, 'utf-8'), ts.ScriptTarget.Latest)
  const decl = source.statements.find(
    (s): s is ts.InterfaceDeclaration => ts.isInterfaceDeclaration(s) && s.name.text === name,
  )
  if (decl === undefined) throw new Error(`${file} declares no interface ${name}`)
  return decl.members.map((m) => (m.name as ts.Identifier).text).sort()
}

/** `tsc` refuses this when the generated report has a key the hand type lacks. */
const WIRE_KEYS_ARE_KNOWN: Array<keyof PublishUnitStatus> = [] as Array<keyof WirePublishUnitStatus>

test('PublishUnitStatus declares every key of the generated report', () => {
  expect(interfaceKeys(path.join(SRC, 'core.ts'), 'PublishUnitStatus')).toEqual(
    interfaceKeys(path.join(SRC, 'core.generated.ts'), 'PublishUnitStatus'),
  )
  expect(WIRE_KEYS_ARE_KNOWN).toEqual([])
})

test('statusOf hands back exactly the keys PublishUnitStatus declares', () => {
  const vault = path.join(tmp, 'vault')
  const skill = path.join(vault, 'marketplaces/house/plugins/tools/skills/review')
  mkdirSync(skill, { recursive: true })
  writeFileSync(
    path.join(vault, 'marketplaces/house/house.md'),
    '---\nname: house\ndescription: The house marketplace.\nsw-owner: acme\n---\n',
  )
  const note = path.join(skill, 'review.md')
  writeFileSync(note, '---\nname: review\ndescription: Reviews a diff.\n---\n\n# Review\n')

  const status = statusOf(createNodeVfs(), {
    skill_path: note,
    destination_root: path.join(tmp, 'out'),
    target: 'claude',
    skills_root: vault,
  })

  expect(Object.keys(status).sort()).toEqual(
    interfaceKeys(path.join(SRC, 'core.ts'), 'PublishUnitStatus'),
  )
})
