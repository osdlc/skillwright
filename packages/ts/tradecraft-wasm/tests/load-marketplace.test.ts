import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { expect, test } from 'vitest'

import { CoreError, loadMarketplace, parseRootFile } from '../src/core.ts'
import { createNodeVfs } from '../src/node-vfs.ts'

// The marketplace crosses as the generated `Marketplace` shape: plugins by
// name, each skill with the skills it calls.
test('loadMarketplace returns the model with calls resolved', () => {
  const root = mkdtempSync(path.join(tmpdir(), 'tradecraft-load-'))
  try {
    const vault = path.join(root, 'vault')
    const skills = path.join(vault, 'marketplaces/house/plugins/tools/skills')
    mkdirSync(path.join(skills, 'review'), { recursive: true })
    mkdirSync(path.join(skills, 'lint'), { recursive: true })
    writeFileSync(path.join(vault, 'skillwright.md'), '---\n---\n')
    writeFileSync(
      path.join(vault, 'marketplaces/house/house.md'),
      '---\ndescription: The house marketplace.\nowner: acme\n---\n',
    )
    writeFileSync(path.join(vault, 'marketplaces/house/plugins/tools/tools.md'), '---\n---\n')
    writeFileSync(
      path.join(skills, 'review/review.md'),
      '---\ndescription: Reviews a diff.\ntargets: [claude, pi]\n---\n\nRun /[[lint]] first.\n',
    )
    writeFileSync(path.join(skills, 'lint/lint.md'), '---\ndescription: Lints.\n---\n\nLint it.\n')

    const market = loadMarketplace(createNodeVfs(), { vault_root: vault })

    const tools = market.plugins['tools']
    expect(tools?.skills.map((s) => s.name)).toEqual(['lint', 'review'])
    const review = tools?.skills.find((s) => s.name === 'review')
    expect(review?.calls).toEqual([{ plugin: 'tools', name: 'lint', kind: 'skill' }])
    expect(review?.description).toBe('Reviews a diff.')
    expect(review?.targets).toEqual(['claude', 'pi'])

    // Only the narrow shape crosses: no body segments, no model folder, no
    // agent-adapt content.
    expect(Object.keys(market).sort()).toEqual([
      'checked',
      'name',
      'plugins',
      'uncarried',
      'warnings',
    ])
    expect(Object.keys(tools ?? {}).sort()).toEqual([
      'assets',
      'commands',
      'name',
      'references',
      'skills',
    ])
    expect(Object.keys(review ?? {}).sort()).toEqual([
      'blocks',
      'calls',
      'description',
      'kind',
      'name',
      'path',
      'resources',
      'targets',
    ])
    expect(JSON.stringify(market)).not.toMatch(/"(segments|folder|resolved|content)"/)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

// One reader for the CLI and a wasm host: the root file's keys and its
// unknown-key warnings come from the core.
test('parseRootFile reads a root file with the core reader', () => {
  const root = parseRootFile(
    '---\nlicense: MIT\nbuild:\n  harnesses: [claude]\nodd: 1\n---\n\nThe README.\n',
  )
  expect(root.defaults.license).toBe('MIT')
  expect(root.build.harnesses).toEqual(['claude'])
  expect(root.warnings).toHaveLength(1)
  expect(() => parseRootFile('---\nbuild: 1\n---\n')).toThrow(CoreError)
  expect(() => parseRootFile('---\nbuild:\n  out: dist\n---\n')).toThrow(/build\.out/)
  expect(() => parseRootFile('---\nowner: acme\n---\n')).toThrow(/<m>\.md/)
  expect(() => parseRootFile('---\nname: house\n---\n')).toThrow(/delete it/)
  expect(() => parseRootFile('---\ntargets: [opencode]\n---\n')).toThrow(
    /`claude`, `codex`, `cursor` and `pi`/,
  )
})
