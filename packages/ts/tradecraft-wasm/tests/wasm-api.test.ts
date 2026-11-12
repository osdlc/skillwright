import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { afterEach, beforeEach, expect, expectTypeOf, test } from 'vitest'

import {
  buildVault,
  CoreError,
  knownTargets,
  loadMarketplace,
  type Component,
  type DiscoveredComponent,
  type LoadRequest,
  type Plugin,
  type Harness,
  type VaultRequest,
} from '../src/core.ts'
import { toCorePath } from '../src/host-path.ts'
import type { BumpSettings, PatchScheme, StaleKind } from '../src/index.ts'
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

test("buildVault writes the vault's dist/<m>/ through the host's vfs", () => {
  marketplace('acme')
  const writes: string[] = []
  const report = buildVault(recordingVfs(writes), { vault_root: vault })
  expect(report.marketplaces[0]?.report.changed).toBe(true)
  expect(existsSync(path.join(vault, 'dist/acme/.claude-plugin/marketplace.json'))).toBe(true)
  expect(writes.length).toBeGreaterThan(0)
  // The vfs sees the core's spelling of each path, not the host's.
  const dist = `${toCorePath(vault)}/dist/`
  for (const file of writes) {
    expect(file.startsWith(dist)).toBe(true)
  }
})

test('the package has no exportSkills', async () => {
  const pkg: Record<string, unknown> = await import('../src/index.ts')
  expect(pkg.exportSkills).toBeUndefined()
})

test('buildVault builds the one marketplace it names', () => {
  marketplace('acme')
  marketplace('labs')
  const report = buildVault(createNodeVfs(), { vault_root: vault, marketplace: 'labs' })
  expect(report.marketplaces.map((m) => m.marketplace)).toEqual(['labs'])
  expect(existsSync(path.join(vault, 'dist/acme'))).toBe(false)
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

test('a buildVault check compares with the folder compare_with names', () => {
  marketplace('acme')
  buildVault(createNodeVfs(), { vault_root: vault })
  const baseline = path.join(root, 'baseline')
  renameSync(path.join(vault, 'dist/acme'), baseline)
  const check = buildVault(createNodeVfs(), {
    vault_root: vault,
    mode: 'check',
    compare_with: toCorePath(baseline),
  })
  expect(check.marketplaces[0]?.report.changed).toBe(false)
  expect(existsSync(path.join(vault, 'dist/acme'))).toBe(false)
})

test('a buildVault check reports every marketplace when one refuses', () => {
  marketplace('acme')
  marketplace('labs')
  writeFile(
    'marketplaces/acme/plugins/tools/skills/review/review.md',
    '---\n---\n\nNo description.\n',
  )
  const check = buildVault(createNodeVfs(), { vault_root: vault, mode: 'check' })
  expect(check.marketplaces.map((m) => [m.marketplace, m.report.changed])).toEqual([['labs', true]])
  expect(check.refused?.map((r) => r.marketplace)).toEqual(['acme'])
  expect(check.refused?.[0]?.token).toBe('missing-description')
  expect(check.refused?.[0]?.detail).toContain('review.md')
  expect(existsSync(path.join(vault, 'dist'))).toBe(false)
})

test("the core reads the root file's defaults and harnesses", () => {
  writeFile('skillwright.md', '---\nlicense: MIT\nbuild:\n  harnesses: [claude]\n---\n')
  marketplace('acme')
  buildVault(createNodeVfs(), { vault_root: vault })
  expect(existsSync(path.join(vault, 'dist/acme/.agents'))).toBe(false)
  const manifest = path.join(
    vault,
    'dist/acme/.claude-plugin/packs/tools/.claude-plugin/plugin.json',
  )
  expect(JSON.parse(readFileSync(manifest, 'utf8')).license).toBe('MIT')

  buildVault(createNodeVfs(), { vault_root: vault, harnesses: ['codex'] })
  expect(existsSync(path.join(vault, 'dist/acme/.agents'))).toBe(true)
})

test('a request that carries defaults is refused', () => {
  marketplace('acme')
  const request = { vault_root: vault, defaults: { license: 'MIT' } } as unknown as VaultRequest
  expect(() => buildVault(createNodeVfs(), request)).toThrow(/defaults/)
})

test('loadMarketplace loads the marketplace it names', () => {
  marketplace('acme')
  marketplace('labs')
  const market = loadMarketplace(createNodeVfs(), { vault_root: vault, marketplace: 'acme' })
  expect(market.name).toBe('acme')
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
  const { message, coreName, token } = error as CoreError
  expect(coreName).toBe('Refused')
  expect(token).toBe('ambiguous-marketplace')
  expect(message).toContain('`acme`')
  expect(message).toContain('`labs`')
  expect(message).toContain('marketplace')
})

test('Harness is the four names knownTargets() returns', () => {
  const four = ['claude', 'codex', 'cursor', 'pi'] as const
  expectTypeOf<Harness>().toEqualTypeOf<(typeof four)[number]>()
  expect(knownTargets()).toEqual([...four])
  expectTypeOf(knownTargets()).toEqualTypeOf<Harness[]>()
  expectTypeOf<NonNullable<LoadRequest['harnesses']>>().toEqualTypeOf<Harness[]>()
  expectTypeOf<NonNullable<VaultRequest['harnesses']>>().toEqualTypeOf<Harness[]>()
  expectTypeOf<NonNullable<Plugin['bare_on']>>().toEqualTypeOf<Harness[]>()
  expectTypeOf<DiscoveredComponent>().not.toHaveProperty('targets')
  expectTypeOf<Component>().not.toHaveProperty('targets')
})

test('the package names the types its exported types use', () => {
  expectTypeOf<StaleKind>().toEqualTypeOf<'plugin' | 'file' | 'empty-dir'>()
  expectTypeOf<PatchScheme>().toEqualTypeOf<'increment' | 'timestamp'>()
  expectTypeOf<BumpSettings['patch']>().toEqualTypeOf<PatchScheme>()
})
