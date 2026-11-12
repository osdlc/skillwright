// node --test scripts/plugin-notes.test.ts
//
// The plugin's source notes teach only the current CLI. A removed command, flag,
// key or diagnostic may appear only in a clause that says it is gone.
// The Rust test `skillwright/tests/repo_plugin.rs` checks that the plugin
// builds with no warning and matches `plugin/dist/`.
import { strict as assert } from 'node:assert'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { test } from 'node:test'

const repo = join(import.meta.dirname, '..')
const plugin = join(repo, 'plugin')

function notes(): string[] {
  const out = [join(plugin, 'skillwright.md')]
  const walk = (dir: string): void => {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name)
      if (statSync(path).isDirectory()) walk(path)
      else if (name.endsWith('.md')) out.push(path)
    }
  }
  walk(join(plugin, 'marketplaces/osdlc'))
  return out
}

const commands = [
  'publish',
  'unpublish',
  'harness',
  'adopt',
  'new',
  'rename',
  'move',
  'delete',
  'convert',
  'list',
  'status',
  'author',
  'link',
  'unlink',
].join('|')

// Words in command position, so ordinary English is not flagged.
const removed: RegExp[] = [
  new RegExp(`\\bskillwright (${commands})\\b`),
  // In code with an argument after it, as a command is written.
  /`(publish|unpublish|status|list|adopt|harness|rename|move|delete|convert) [^`]+`/,
  /--(config|out|namespace|skills-root|watch|name|from|from-git|target|all|into)\b/,
  /\bbuild\.(out|destinations|namespace|hooks|assets|exclude|patterns|launcher_path|scan_locations)\b/,
  /\b(destinations|scan_locations|launcher_path|workspace):/,
  /\b(unsafe-root|stale-output|invalid-name|link-ambiguous|skill-call-ambiguous|reserved-name)\b/,
  /\bclaude-code\b/,
]

const gone = /\b(gone|removed|no longer)\b/i

/** Each removed word in `line`, less one in a clause that says it is gone. */
function removedIn(line: string): string[] {
  const hits: string[] = []
  for (const clause of line.split(/(?<=[.;!?])(?:\s|$)/)) {
    if (gone.test(clause)) continue
    for (const re of removed) {
      const m = re.exec(clause)
      if (m) hits.push(m[0])
    }
  }
  return hits
}

test('the rules flag a command, not a word, and exempt only the clause that says gone', () => {
  assert.deepEqual(removedIn('the `status` of each unit'), [])
  assert.deepEqual(removedIn('Run skillwright publish; the old flag is removed.'), [
    'skillwright publish',
  ])
  assert.deepEqual(removedIn('`skillwright publish` is gone.'), [])
  assert.deepEqual(removedIn('Run `publish tools` first.'), ['`publish tools`'])
  assert.deepEqual(removedIn('Set destinations: x.'), ['destinations:'])
})

test('no note teaches a removed command, flag, key or diagnostic', () => {
  const hits: string[] = []
  for (const path of notes()) {
    readFileSync(path, 'utf8')
      .split('\n')
      .forEach((line, i) => {
        for (const m of removedIn(line)) hits.push(`${relative(repo, path)}:${i + 1}: ${m}`)
      })
  }
  assert.deepEqual(hits, [])
})

test('no note calls the removed publish skill', () => {
  const hits = notes().filter((path) => readFileSync(path, 'utf8').includes('/[[publish]]'))
  assert.deepEqual(
    hits.map((path) => relative(repo, path)),
    [],
  )
})
