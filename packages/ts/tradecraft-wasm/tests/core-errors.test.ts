/**
 * How a throw from the binding reaches a caller: which typed error, and what
 * its message names.
 */
import { afterEach, beforeEach, describe, expect, test } from 'vitest'
import { mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import {
  adoptSkills,
  CoreError,
  discoverComponents,
  exportSkills,
  InvalidNamespaceError,
  publishSkill,
  statusOf,
} from '../src/core.ts'
import { createNodeVfs, type Vfs } from '../src/node-vfs.ts'

let tmp: string

beforeEach(() => {
  tmp = realpathSync.native(mkdtempSync(path.join(tmpdir(), 'tradecraft-core-errors-')))
})

afterEach(() => {
  rmSync(tmp, { recursive: true, force: true })
})

/** The value `call` threw. Fails the test when it returned instead. */
function thrownBy(call: () => unknown): unknown {
  try {
    call()
  } catch (raw) {
    return raw
  }
  throw new Error('expected the call to throw')
}

describe('a host filesystem that throws a value with no message', () => {
  for (const [label, value] of [
    ['undefined', undefined],
    ['a Symbol', Symbol('host')],
  ] as const) {
    test(`a throw of ${label} is an Io CoreError naming the path`, () => {
      const vfs: Vfs = {
        ...createNodeVfs(),
        readDir: () => {
          throw value
        },
      }

      const raw = thrownBy(() => discoverComponents(vfs, { vault_root: tmp }))

      expect(raw).toBeInstanceOf(CoreError)
      expect((raw as CoreError).coreName).toBe('Io')
      expect((raw as CoreError).message).toContain(tmp)
    })
  }
})

describe('a bad namespace on a JSON export', () => {
  const namespace = 'Bad Stem!'
  const calls: Array<[string, (vfs: Vfs) => unknown]> = [
    [
      'publishSkill',
      (vfs) =>
        publishSkill(vfs, {
          skill_path: path.join(tmp, 'note.md'),
          destination_root: path.join(tmp, 'out'),
          target: 'claude',
          namespace,
        }),
    ],
    ['discoverComponents', (vfs) => discoverComponents(vfs, { vault_root: tmp, namespace })],
    [
      'exportSkills',
      (vfs) =>
        exportSkills(vfs, {
          vault_root: tmp,
          out_root: path.join(tmp, 'out'),
          mode: 'check',
          config: { default_plugin: 'core' },
          namespace,
        }),
    ],
    [
      'adoptSkills',
      (vfs) =>
        adoptSkills(vfs, {
          source: path.join(tmp, 'in'),
          vault_root: tmp,
          adopted_at: '2026-01-01T00:00:00Z',
          namespace,
        }),
    ],
    [
      'statusOf',
      (vfs) =>
        statusOf(vfs, {
          skill_path: path.join(tmp, 'note.md'),
          destination_root: path.join(tmp, 'out'),
          target: 'claude',
          namespace,
        }),
    ],
  ]

  for (const [name, call] of calls) {
    test(`${name} throws InvalidNamespaceError`, () => {
      const raw = thrownBy(() => call(createNodeVfs()))

      expect(raw).toBeInstanceOf(InvalidNamespaceError)
      expect((raw as Error).message).toContain(namespace)
    })
  }
})
