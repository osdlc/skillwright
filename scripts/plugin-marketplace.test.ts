// node --test scripts/plugin-marketplace.test.ts
import { strict as assert } from 'node:assert'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'

import { HARNESSES, pluginVersion, syncPlugin, upsertEntry } from './plugin-marketplace.ts'

function write(root: string, rel: string, text: string): void {
  mkdirSync(join(root, rel, '..'), { recursive: true })
  writeFileSync(join(root, rel), text)
}

function fixture(): { plugin: string; target: string } {
  const base = mkdtempSync(join(tmpdir(), 'plugin-marketplace-'))
  const plugin = join(base, 'plugin')
  const target = join(base, 'marketplace')
  write(
    plugin,
    'vault/plugins/skillwright/skillwright.md',
    '---\nname: skillwright\nsw-version: 1.2.3\n---\nbody\n',
  )
  for (const { root, manifest } of HARNESSES) {
    write(plugin, `${root}/packs/skillwright/new.txt`, 'new')
    write(target, `${root}/packs/skillwright/old.txt`, 'old')
    write(target, `${root}/packs/other/keep.txt`, 'keep')
    if (manifest === null) continue
    write(
      plugin,
      manifest,
      JSON.stringify({ name: 'm', plugins: [{ name: 'skillwright', version: '1.2.3' }] }),
    )
    write(
      target,
      manifest,
      JSON.stringify({
        name: 'm',
        plugins: [{ name: 'sdlc' }, { name: 'skillwright', version: '0.0.1' }],
      }),
    )
  }
  return { plugin, target }
}

test('pluginVersion reads sw-version from the plugin note', () => {
  assert.equal(pluginVersion(fixture().plugin), '1.2.3')
})

test('upsertEntry replaces the entry and keeps the others, sorted', () => {
  const out = upsertEntry(
    { plugins: [{ name: 'sdlc' }, { name: 'craft' }] },
    { name: 'skillwright' },
  )
  assert.deepEqual(
    out.plugins.map((p) => p.name),
    ['craft', 'sdlc', 'skillwright'],
  )
})

test('syncPlugin replaces only the skillwright pack and entry', () => {
  const { plugin, target } = fixture()
  syncPlugin(plugin, target)
  for (const { root, manifest } of HARNESSES) {
    assert.equal(readFileSync(join(target, root, 'packs/skillwright/new.txt'), 'utf8'), 'new')
    assert.throws(() => readFileSync(join(target, root, 'packs/skillwright/old.txt')))
    assert.equal(readFileSync(join(target, root, 'packs/other/keep.txt'), 'utf8'), 'keep')
    if (manifest === null) continue
    const plugins = JSON.parse(readFileSync(join(target, manifest), 'utf8')).plugins
    assert.deepEqual(plugins, [{ name: 'sdlc' }, { name: 'skillwright', version: '1.2.3' }])
  }
})

test('syncPlugin refuses a target without a manifest', () => {
  const { plugin, target } = fixture()
  const manifest = HARNESSES[0]!.manifest!
  write(target, manifest, '{"plugins":[]}')
  rmSync(join(target, HARNESSES[1]!.manifest!))
  assert.throws(() => syncPlugin(plugin, target), /has no/)
})
