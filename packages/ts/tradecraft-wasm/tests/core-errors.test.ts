/**
 * How a throw from the binding reaches a caller: which typed error, and what
 * its message names.
 */
import { afterEach, beforeEach, describe, expect, test } from 'vitest'
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { CoreError, discoverComponents } from '../src/core.ts'
import { createNodeVfs, type Vfs } from '../src/node-vfs.ts'

let tmp: string

beforeEach(() => {
  tmp = realpathSync.native(mkdtempSync(path.join(tmpdir(), 'tradecraft-core-errors-')))
  // A vault root, so the scan gets past the root check to the folder reads.
  writeFileSync(path.join(tmp, 'skillwright.md'), '---\n---\n')
  mkdirSync(path.join(tmp, 'marketplaces'))
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
