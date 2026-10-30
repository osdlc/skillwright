// Publishes the built skillwright plugin into a checkout of osdlc/marketplace.
//
//   node scripts/plugin-marketplace.ts version [--plugin plugin]
//   node scripts/plugin-marketplace.ts sync <marketplace-checkout> [--plugin plugin]
//
// `version` prints the plugin's `sw-version`. `sync` changes only what belongs
// to this plugin: for each harness, the `packs/skillwright` folder is replaced,
// and the `skillwright` entry in the harness's marketplace manifest is added or
// replaced. Every other file and every other plugin's entry stays as it is.
// The same `sync` runs in dry runs against a scratch folder.
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { parseArgs } from 'node:util'

export const PLUGIN_NAME = 'skillwright'

/** Each harness root, and the marketplace manifest in it (Pi has none). */
export const HARNESSES: readonly { root: string; manifest: string | null }[] = [
  { root: '.claude-plugin', manifest: '.claude-plugin/marketplace.json' },
  { root: '.agents', manifest: '.agents/plugins/marketplace.json' },
  { root: '.cursor-plugin', manifest: '.cursor-plugin/marketplace.json' },
  { root: '.pi', manifest: null },
]

type Manifest = { plugins: { name: string }[] } & Record<string, unknown>

/** The `sw-version` of the plugin note in the vault. */
export function pluginVersion(pluginDir: string): string {
  const note = readFileSync(
    join(pluginDir, `vault/plugins/${PLUGIN_NAME}/${PLUGIN_NAME}.md`),
    'utf8',
  )
  const front = /^---\n([\s\S]*?)\n---/.exec(note)?.[1] ?? ''
  const version = /^sw-version:\s*(\S+)\s*$/m.exec(front)?.[1]
  if (version === undefined) throw new Error('the plugin note has no sw-version')
  return version
}

/** Adds or replaces the plugin's entry, keeping the other entries in order. */
export function upsertEntry(target: Manifest, entry: { name: string }): Manifest {
  const plugins = target.plugins.filter((p) => p.name !== entry.name)
  plugins.push(entry)
  plugins.sort((a, b) => a.name.localeCompare(b.name))
  return { ...target, plugins }
}

function readJson(path: string): Manifest {
  return JSON.parse(readFileSync(path, 'utf8')) as Manifest
}

/** Copies the plugin's built packs and manifest entries into `target`. */
export function syncPlugin(pluginDir: string, target: string): string[] {
  const touched: string[] = []
  for (const { root, manifest } of HARNESSES) {
    const pack = `${root}/packs/${PLUGIN_NAME}`
    if (!existsSync(join(pluginDir, pack))) throw new Error(`the build has no ${pack}`)
    rmSync(join(target, pack), { recursive: true, force: true })
    mkdirSync(join(target, root, 'packs'), { recursive: true })
    cpSync(join(pluginDir, pack), join(target, pack), { recursive: true })
    touched.push(pack)
    if (manifest === null) continue
    const entry = readJson(join(pluginDir, manifest)).plugins.find((p) => p.name === PLUGIN_NAME)
    if (entry === undefined) throw new Error(`${manifest} lists no ${PLUGIN_NAME} entry`)
    const dest = join(target, manifest)
    if (!existsSync(dest)) throw new Error(`the target has no ${manifest}`)
    writeFileSync(dest, `${JSON.stringify(upsertEntry(readJson(dest), entry), null, 2)}\n`)
    touched.push(manifest)
  }
  return touched
}

function main(): void {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { plugin: { type: 'string', default: 'plugin' } },
  })
  const [command, target] = positionals
  if (command === 'version') {
    console.log(pluginVersion(values.plugin))
  } else if (command === 'sync' && target !== undefined) {
    for (const path of syncPlugin(values.plugin, target)) console.log(path)
  } else {
    console.error(
      'usage: plugin-marketplace.ts version | sync <marketplace-checkout> [--plugin dir]',
    )
    process.exit(2)
  }
}

if (process.argv[1] !== undefined && import.meta.filename === process.argv[1]) main()
