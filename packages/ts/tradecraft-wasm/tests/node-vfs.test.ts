import { afterEach, beforeEach, describe, expect, test } from 'vitest'
import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { discoverComponents, exportSkills, publishSkill } from '../src/core.ts'
import { createNodeVfs, type Vfs } from '../src/node-vfs.ts'

let tmp: string

beforeEach(() => {
  // Canonical, so expected paths match what `realpath` returns on macOS.
  tmp = realpathSync.native(mkdtempSync(path.join(tmpdir(), 'tradecraft-node-vfs-')))
})

afterEach(() => {
  rmSync(tmp, { recursive: true, force: true })
})

describe('the link methods', () => {
  test('realpath resolves a link and throws on a missing path', () => {
    mkdirSync(path.join(tmp, 'real'))
    symlinkSync(path.join(tmp, 'real'), path.join(tmp, 'alias'))
    const vfs = createNodeVfs()

    expect(vfs.realpath?.(path.join(tmp, 'alias'))).toBe(path.join(tmp, 'real'))
    expect(() => vfs.realpath?.(path.join(tmp, 'missing'))).toThrow()
  })

  test('readLink returns the target text and throws on a non-link', () => {
    symlinkSync('nowhere/target.md', path.join(tmp, 'dangling'))
    mkdirSync(path.join(tmp, 'dir'))
    const vfs = createNodeVfs()

    expect(vfs.readLink?.(path.join(tmp, 'dangling'))).toBe('nowhere/target.md')
    expect(() => vfs.readLink?.(path.join(tmp, 'dir'))).toThrow()
  })

  test('isSymlink answers for the link itself and is false when nothing is there', () => {
    mkdirSync(path.join(tmp, 'dir'))
    symlinkSync(path.join(tmp, 'dir'), path.join(tmp, 'link'))
    symlinkSync(path.join(tmp, 'missing'), path.join(tmp, 'dangling'))
    const vfs = createNodeVfs()

    expect(vfs.isSymlink?.(path.join(tmp, 'link'))).toBe(true)
    expect(vfs.isSymlink?.(path.join(tmp, 'dangling'))).toBe(true)
    expect(vfs.isSymlink?.(path.join(tmp, 'dir'))).toBe(false)
    expect(vfs.isSymlink?.(path.join(tmp, 'missing'))).toBe(false)
  })
})

describe('stat', () => {
  test('describes a file, a directory and a link without following it, and throws on a missing path', () => {
    writeFileSync(path.join(tmp, 'a.md'), 'alpha')
    mkdirSync(path.join(tmp, 'dir'))
    symlinkSync(path.join(tmp, 'a.md'), path.join(tmp, 'link.md'))
    const vfs = createNodeVfs()

    const file = vfs.stat(path.join(tmp, 'a.md'))
    expect(file).toMatchObject({ size: 5, isDir: false, isSymlink: false })
    expect(file.mtimeMs).toBeGreaterThan(0)
    expect(vfs.stat(path.join(tmp, 'dir'))).toMatchObject({
      size: 0,
      isDir: true,
      isSymlink: false,
    })
    expect(vfs.stat(path.join(tmp, 'link.md'))).toMatchObject({ isDir: false, isSymlink: true })
    expect(() => vfs.stat(path.join(tmp, 'missing'))).toThrow()
  })
})

/** A repository holding its vault at `src/`, the out root being the repository. */
function repoWithVault(name: string): { repo: string; vault: string } {
  const repo = path.join(tmp, name)
  const vault = path.join(repo, 'src')
  const skill = path.join(vault, 'marketplaces/house/plugins/tools/skills/review')
  mkdirSync(skill, { recursive: true })
  writeFileSync(
    path.join(vault, 'marketplaces/house/house.md'),
    '---\nname: house\ndescription: The house marketplace.\nowner: acme\n---\n',
  )
  writeFileSync(
    path.join(skill, 'review.md'),
    '---\nname: review\ndescription: Reviews a diff.\n---\n\n# Review\n',
  )
  return { repo, vault }
}

function build(vfs: Vfs, repo: string, vault: string) {
  return exportSkills(vfs, {
    vault_root: vault,
    out_root: repo,
    mode: 'build',
    config: { name: 'core' },
  })
}

describe('export at a repository root that holds its vault', () => {
  test('builds when the host can see links', () => {
    const { repo, vault } = repoWithVault('repo')
    expect(build(createNodeVfs(), repo, vault).changed).toBe(true)
  })

  test('is refused when a tree it replaces is a link into the vault', () => {
    const { repo, vault } = repoWithVault('linked')
    mkdirSync(path.join(vault, 'skills'))
    symlinkSync(path.join(vault, 'skills'), path.join(repo, '.claude-plugin'))

    // The narrowed rail's own refusal, which names the write: the strict rail
    // would refuse this out root as an ancestor of the vault instead.
    expect(() => build(createNodeVfs(), repo, vault)).toThrow(/would write .* inside the vault/)
  })

  test('is refused when the host has no link methods', () => {
    const { repo, vault } = repoWithVault('blind')
    const { realpath: _r, readLink: _l, isSymlink: _s, ...blind } = createNodeVfs()

    expect(() => build(blind, repo, vault)).toThrow(/unsafe-root/)
  })
})

describe('a host without stat', () => {
  test('discovers the same components as one with it', () => {
    const { vault } = repoWithVault('no-stat')
    const { stat: _stat, ...withoutStat } = createNodeVfs()
    const names = (vfs: Vfs) =>
      discoverComponents(vfs, { vault_root: vault }).components.map((c) => c.name)
    expect(names(withoutStat)).toEqual(names(createNodeVfs()))
    expect(names(withoutStat)).toContain('review')
  })

  test('the vault walk asks stat, and a malformed answer fails it', () => {
    const { vault } = repoWithVault('bad-stat')
    const malformed: Vfs = {
      ...createNodeVfs(),
      stat: () => ({ size: -1, mtimeMs: null, isDir: false, isSymlink: false }),
    }
    expect(() => discoverComponents(malformed, { vault_root: vault })).toThrow(/stat must return/)
  })
})

describe('a host whose link answers fail', () => {
  /** An error with a Node-style code. */
  const failure = (code: string) => Object.assign(new Error(`host failed: ${code}`), { code })

  test('a realpath error other than a missing path refuses the check', () => {
    const { repo, vault } = repoWithVault('denied')
    const base = createNodeVfs()
    const denied: Vfs = {
      ...base,
      realpath: (p: string) => {
        if (p.includes('locked')) throw failure('EACCES')
        return base.realpath(p)
      },
    }
    const result = publishSkill(denied, {
      skill_path: path.join(vault, 'marketplaces/house/plugins/tools/skills/review/review.md'),
      destination_root: path.join(repo, 'locked'),
      target: 'claude',
      skills_root: vault,
    })
    expect(result.files.map((f) => f.status)).toEqual(['refused'])
    expect(result.files[0]?.reason?.detail).toMatch(/cannot tell where this path leads/)
  })

  test('a realpath answer that is not absolute refuses the check', () => {
    const { repo, vault } = repoWithVault('relative')
    mkdirSync(path.join(repo, 'elsewhere'))
    const base = createNodeVfs()
    const relative: Vfs = {
      ...base,
      realpath: (p: string) => {
        const answer = base.realpath(p)
        return p.includes('elsewhere') ? path.relative('/', answer) : answer
      },
    }
    const result = publishSkill(relative, {
      skill_path: path.join(vault, 'marketplaces/house/plugins/tools/skills/review/review.md'),
      destination_root: path.join(repo, 'elsewhere'),
      target: 'claude',
      skills_root: vault,
    })
    expect(result.files.map((f) => f.status)).toEqual(['refused'])
    expect(result.files[0]?.reason?.detail).toMatch(/realpath must return an absolute path/)
  })

  test('isSymlink that throws counts as a link, so discovery refuses the note', () => {
    const { vault } = repoWithVault('unsure')
    const base = createNodeVfs()
    const unsure: Vfs = {
      ...base,
      isSymlink: (p: string) => {
        if (p.endsWith('review.md')) throw failure('EIO')
        return base.isSymlink(p)
      },
    }
    expect(discoverComponents(base, { vault_root: vault }).components.length).toBeGreaterThan(0)
    expect(() => discoverComponents(unsure, { vault_root: vault })).toThrow(
      /a symlink in a skills source/,
    )
  })
})
