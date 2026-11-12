/**
 * The core over a Windows-shaped host, with `node:fs` replaced by a stand-in
 * that accepts only Windows spellings (`tests/fake-windows-fs.ts`). This does
 * not run on Windows; it checks the path boundary on any machine.
 */
import { afterEach, beforeEach, describe, expect, test } from 'vitest'
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { fakeWindowsFs, mount, onDisk } from './fake-windows-fs.ts'
import { buildVault } from '../src/core.ts'
import { toCorePath, UNSUPPORTED_PATH } from '../src/host-path.ts'
import { createNodeVfs, type Vfs } from '../src/node-vfs.ts'

let root: string
let vfs: Vfs

beforeEach(() => {
  root = realpathSync.native(mkdtempSync(path.join(tmpdir(), 'tradecraft-windows-')))
  vfs = createNodeVfs({ platform: 'win32', fs: fakeWindowsFs(root) })
  mount(root, 'C:\\', 'D:\\', 'Z:\\', '\\\\server\\share')
})

afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

const core = (windowsPath: string) => toCorePath(windowsPath, 'win32')

/** A vault at `vault` (a Windows path) with one marketplace and one skill. */
function vaultAt(vault: string, disk: (p: string) => string = (p) => onDisk(root, p)): string {
  const skill = path.join(disk(vault), 'marketplaces/house/plugins/tools/skills/review')
  mkdirSync(skill, { recursive: true })
  writeFileSync(path.join(disk(vault), 'skillwright.md'), '---\n---\n')
  writeFileSync(
    path.join(disk(vault), 'marketplaces/house/house.md'),
    '---\ndescription: The house marketplace.\nowner: acme\n---\n',
  )
  writeFileSync(path.join(disk(vault), 'marketplaces/house/plugins/tools/tools.md'), '---\n---\n')
  writeFileSync(
    path.join(skill, 'review.md'),
    '---\ndescription: Reviews a diff.\n---\n\n# Review\n',
  )
  return `${vault}\\marketplaces\\house\\plugins\\tools\\skills\\review\\review.md`
}

/** The one marketplace's report from a build of `vault`. */
function build(vault: string, host: Vfs = vfs) {
  return buildVault(host, { vault_root: core(vault), mode: 'build' }).marketplaces[0]?.report
}

describe('a Windows host', () => {
  test('builds a vault into its dist folder', () => {
    vaultAt('C:\\repo\\src')
    expect(build('C:\\repo\\src')?.changed).toBe(true)
    expect(
      existsSync(onDisk(root, 'C:\\repo\\src\\dist\\house\\.claude-plugin\\marketplace.json')),
    ).toBe(true)
  })

  test('works on a UNC share, and on a mapped drive that resolves to one', () => {
    const shareDisk = (p: string) => path.join(root, 'UNC', ...p.slice(2).split('\\'))
    vaultAt('\\\\server\\share\\vault', shareDisk)
    expect(build('\\\\server\\share\\vault')?.changed).toBe(true)

    // Drive Z: is mapped to \\nas\team; realpath answers with the share.
    vaultAt('Z:\\vault')
    build('Z:\\vault')
    expect(
      existsSync(onDisk(root, 'Z:\\vault\\dist\\house\\.claude-plugin\\marketplace.json')),
    ).toBe(true)
  })

  test('refuses a server name as a root: it is not a directory', () => {
    for (const server of ['\\\\server', '\\\\server\\']) {
      expect(() => build(server)).toThrow(expect.objectContaining({ code: UNSUPPORTED_PATH }))
    }
    expect(existsSync(path.join(root, 'UNC', 'server', 'dist'))).toBe(false)
  })

  test('refuses `..` as a share name before anything is written', () => {
    // Folded as text, this would name `/UNC/share`, a server and not a share.
    expect(() => build('\\\\server\\..\\share')).toThrow(
      expect.objectContaining({ code: UNSUPPORTED_PATH }),
    )
    expect(existsSync(path.join(root, 'UNC', 'share'))).toBe(false)
  })

  test('a `..` at a share root stays on that share, as on Windows', () => {
    vaultAt('\\\\server\\share\\share2\\v')
    expect(build('\\\\server\\share\\..\\share2\\v')?.changed).toBe(true)
    expect(
      existsSync(
        onDisk(root, '\\\\server\\share\\share2\\v\\dist\\house\\.claude-plugin\\marketplace.json'),
      ),
    ).toBe(true)
  })

  test.each([
    // A volume with no drive letter: as libuv answers it, and as Windows does
    // before libuv strips the \\?\ prefix.
    ['\\\\?\\Volume{0b6a5c1e}\\dest', /is not a drive or share path/],
    ['Volume{0b6a5c1e}\\dest', /depends on the current directory/],
  ])('refuses the realpath answer %s, which is not a drive or share path', (answer, why) => {
    const fake = fakeWindowsFs(root)
    const volume = (p: string) =>
      p.startsWith('D:\\vault') && p.endsWith('review.md') ? answer : fake.realpathSync.native(p)
    const odd = createNodeVfs({
      platform: 'win32',
      fs: { ...fake, realpathSync: Object.assign(volume, { native: volume }) } as typeof fake,
    })
    vaultAt('D:\\vault')
    expect(() => build('D:\\vault', odd)).toThrow(/cannot tell where this path leads/)
    expect(() => build('D:\\vault', odd)).toThrow(why)
  })
})
