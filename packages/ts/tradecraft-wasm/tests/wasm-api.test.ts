import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { afterEach, beforeEach, expect, expectTypeOf, test } from 'vitest'

import {
  buildVault,
  CoreError,
  exportSkills,
  knownTargets,
  loadMarketplace,
  type ExportRequest,
  type LoadRequest,
  type Target,
  type VaultRequest,
} from '../src/core.ts'
import { createNodeVfs, type Vfs } from '../src/node-vfs.ts'

let root: string
let vault: string

beforeEach(() => {
  root = mkdtempSync(path.join(tmpdir(), 'tradecraft-api-'))
  vault = path.join(root, 'vault')
  writeFile('skillwright.md', '---\n---\n')
})

afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

function writeFile(rel: string, text: string) {
  const file = path.join(vault, rel)
  mkdirSync(path.dirname(file), { recursive: true })
  writeFileSync(file, text)
}

/** Marketplace `name` with plugin `tools` and skill `review`. */
function marketplace(name: string) {
  const base = `marketplaces/${name}`
  writeFile(
    `${base}/${name}.md`,
    `---\ndescription: The ${name} marketplace.\nowner: ${name}\n---\n`,
  )
  writeFile(`${base}/plugins/tools/tools.md`, '---\nversion: 1.0.0\n---\n')
  writeFile(
    `${base}/plugins/tools/skills/review/review.md`,
    '---\ndescription: Reviews a diff.\n---\n\nReview it.\n',
  )
}

/** The Node vfs, recording every path written through it. */
function recordingVfs(writes: string[]): Vfs {
  const vfs = createNodeVfs()
  return {
    ...vfs,
    writeFile(file: string, bytes: Uint8Array) {
      writes.push(file)
      vfs.writeFile(file, bytes)
    },
  }
}

test("exportSkills writes the vault's dist/<m>/ through the host's vfs", () => {
  marketplace('acme')
  const writes: string[] = []
  const report = exportSkills(recordingVfs(writes), { vault_root: vault })
  expect(report.changed).toBe(true)
  expect(existsSync(path.join(vault, 'dist/acme/.claude-plugin/marketplace.json'))).toBe(true)
  expect(writes.length).toBeGreaterThan(0)
  for (const file of writes) {
    expect(file.startsWith(`${vault}/dist/`)).toBe(true)
  }
})

test('buildVault builds every marketplace', () => {
  marketplace('acme')
  marketplace('labs')
  const report = buildVault(createNodeVfs(), { vault_root: vault })
  expect(report.marketplaces.map((m) => m.marketplace)).toEqual(['acme', 'labs'])
  for (const name of ['acme', 'labs']) {
    expect(existsSync(path.join(vault, `dist/${name}/.claude-plugin/marketplace.json`))).toBe(true)
  }
  const check = buildVault(createNodeVfs(), { vault_root: vault, mode: 'check' })
  expect(check.marketplaces.every((m) => !m.report.changed)).toBe(true)
})

test("the core reads the root file's defaults and harnesses", () => {
  writeFile('skillwright.md', '---\nlicense: MIT\nbuild:\n  harnesses: [claude]\n---\n')
  marketplace('acme')
  exportSkills(createNodeVfs(), { vault_root: vault })
  expect(existsSync(path.join(vault, 'dist/acme/.agents'))).toBe(false)
  const manifest = path.join(
    vault,
    'dist/acme/.claude-plugin/packs/tools/.claude-plugin/plugin.json',
  )
  expect(JSON.parse(readFileSync(manifest, 'utf8')).license).toBe('MIT')

  let error: unknown
  try {
    exportSkills(createNodeVfs(), { vault_root: vault, harnesses: ['codex'] })
  } catch (raw) {
    error = raw
  }
  expect((error as CoreError).message).toContain('harness-not-built')
})

test('a request that carries defaults is refused', () => {
  marketplace('acme')
  const request = { vault_root: vault, defaults: { license: 'MIT' } } as unknown as ExportRequest
  expect(() => exportSkills(createNodeVfs(), request)).toThrow(/defaults/)
})

test('loadMarketplace loads the marketplace it names', () => {
  marketplace('acme')
  marketplace('labs')
  const market = loadMarketplace(createNodeVfs(), { vault_root: vault, marketplace: 'acme' })
  expect(market.name).toBe('acme')
  expect(market.folder).toBe('acme')
  expect(Object.keys(market.plugins)).toEqual(['tools'])
})

test('loadMarketplace on a vault of two marketplaces needs a name', () => {
  marketplace('acme')
  marketplace('labs')
  let error: unknown
  try {
    loadMarketplace(createNodeVfs(), { vault_root: vault })
  } catch (raw) {
    error = raw
  }
  expect(error).toBeInstanceOf(CoreError)
  const { message, coreName } = error as CoreError
  expect(coreName).toBe('Refused')
  expect(message).toContain('`acme`')
  expect(message).toContain('`labs`')
  expect(message).toContain('marketplace')
})

test('Target is the four names knownTargets() returns', () => {
  const four = ['claude', 'codex', 'cursor', 'pi'] as const
  expectTypeOf<Target>().toEqualTypeOf<(typeof four)[number]>()
  expect(knownTargets()).toEqual([...four])
  expectTypeOf(knownTargets()).toEqualTypeOf<Target[]>()
  expectTypeOf<NonNullable<ExportRequest['harnesses']>>().toEqualTypeOf<Target[]>()
  expectTypeOf<NonNullable<LoadRequest['harnesses']>>().toEqualTypeOf<Target[]>()
  expectTypeOf<NonNullable<VaultRequest['harnesses']>>().toEqualTypeOf<Target[]>()
})
