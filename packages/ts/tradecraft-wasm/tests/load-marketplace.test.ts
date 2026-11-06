import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { expect, test } from 'vitest'

import { CoreError, loadMarketplace, parseRootFile, rootFileConfig } from '../src/core.ts'
import { createNodeVfs } from '../src/node-vfs.ts'

// The model crosses the boundary as the generated `Marketplace` shape: plugins
// by name, each body as segments, a call resolved to the skill it names.
test('loadMarketplace returns the model with calls resolved', () => {
  const root = mkdtempSync(path.join(tmpdir(), 'tradecraft-load-'))
  try {
    const vault = path.join(root, 'vault')
    const skills = path.join(vault, 'marketplaces/house/plugins/tools/skills')
    mkdirSync(path.join(skills, 'review'), { recursive: true })
    mkdirSync(path.join(skills, 'lint'), { recursive: true })
    writeFileSync(
      path.join(vault, 'marketplaces/house/house.md'),
      '---\nname: house\ndescription: The house marketplace.\nsw-owner: acme\n---\n',
    )
    writeFileSync(
      path.join(skills, 'review/review.md'),
      '---\nname: review\ndescription: Reviews a diff.\n---\n\nRun [[lint]] first.\n',
    )
    writeFileSync(
      path.join(skills, 'lint/lint.md'),
      '---\nname: lint\ndescription: Lints.\n---\n\nLint it.\n',
    )

    const market = loadMarketplace(createNodeVfs(), {
      vault_root: vault,
      config: { name: 'house' },
    })

    const tools = market.plugins['tools']
    expect(tools?.skills.map((s) => s.name)).toEqual(['lint', 'review'])
    const review = tools?.skills.find((s) => s.name === 'review')
    const kinds = review?.resolved?.body.segments.map((s) => s.type)
    expect(kinds).toContain('call')
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

// One reader for the CLI and a wasm host: the root file's keys, its unknown-key
// warnings, and the folder-name default all come from the core.
test('parseRootFile reads a root file with the core reader', () => {
  const root = parseRootFile(
    '---\nname: house\nlicense: MIT\nbuild:\n  out: dist\nodd: 1\n---\n\nThe README.\n',
  )
  expect(root.marketplace.name).toBe('house')
  expect(root.build.out).toBe('dist')
  expect(root.warnings).toHaveLength(1)
  expect(() => parseRootFile('---\nbuild: 1\n---\n')).toThrow(CoreError)
})

test('rootFileConfig gives the config, named after the folder by default', () => {
  const config = rootFileConfig('---\nlicense: MIT\n---\n', 'My House')
  expect(config.name).toBe('my-house')
  expect(config.license).toBe('MIT')
  expect(() => rootFileConfig('---\nbuild: 1\n---\n', 'x')).toThrow(CoreError)
})
