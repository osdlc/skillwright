import assert from 'node:assert/strict'
import { afterEach, beforeEach, describe, test } from 'node:test'

import { cpSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { parse } from 'yaml'

import type { Dataset } from '../alpha/load.ts'
import { BETA, loadBeta } from './corpus.ts'
import {
  type Context,
  idSafe,
  importAlpha,
  inferValueSchema,
  plan,
  writePlan,
} from './import-alpha.ts'
import { lint } from './lint.ts'

let dir: string
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'harness-reference-import-'))
  cpSync(BETA, dir, { recursive: true })
})
afterEach(() => rmSync(dir, { recursive: true, force: true }))

const front = (rel: string): Record<string, any> =>
  parse(/^---\n([\s\S]*?)\n---/u.exec(readFileSync(join(dir, rel), 'utf8'))?.[1] ?? '') as Record<
    string,
    any
  >

const NOTE =
  'https://github.com/osdlc/skillwright/blob/main/harness-reference/corpora/alpha/sources/n.md'
const DOCS = 'https://example.com/docs'

const obs = (over: Record<string, unknown>) => ({
  value: { a: 1 },
  since: null,
  until: null,
  observed: '2026-01-01',
  source: 'doc',
  ...over,
})
const fact = (subject: string, over: Record<string, unknown>) => ({
  conceptId: `facts/pi/${subject}`,
  harness: 'pi',
  subject,
  observations: [],
  sources: [
    { id: 'doc', resource: DOCS, title: 'Upstream docs' },
    { id: 'note', resource: NOTE, title: 'A note' },
  ],
  ...over,
})
const dataset = (facts: unknown[]): Dataset =>
  ({
    categories: [],
    harnesses: [],
    probes: [],
    subjects: [
      { id: 'zz/thing', category: 'zz', title: 'Thing', description: 'A thing.', order: 1 },
      { id: 'zz/other', category: 'zz', title: 'Other', description: 'Another.', order: 2 },
    ],
    facts,
  }) as unknown as Dataset
const context = (facts: unknown[]): Context => ({
  dataset: dataset(facts),
  beta: loadBeta(dir),
  categories: new Set(['zz']),
  note: () => ({ title: 'A note', description: 'Notes.' }),
  probeBody: () => '',
  fixtureFiles: () => [],
  imported: () => false,
})

describe('mapping', () => {
  test('merges, supersedes, disputes, and sets strength by the rule', () => {
    const facts = [
      fact('zz/thing', {
        observations: [
          obs({ value: { a: 1, b: 'x' }, until: '1.0.0', evidence: 'docs' }),
          obs({
            value: { a: 2, b: 'x' },
            since: '1.0.0',
            observed: '2026-02-01',
            evidence: 'source',
          }),
          obs({
            value: { b: 'y' },
            since: '1.0.0',
            observed: '2026-03-01',
            source: 'note',
            evidence: 'help-text',
          }),
        ],
      }),
      fact('zz/other', {
        observations: [],
        open: [{ question: 'Does it?', observed: '2026-04-01', source: 'doc' }],
      }),
    ]
    const p = plan(context(facts))
    writePlan(dir, p)
    const beta = loadBeta(dir)
    assert.deepEqual([...beta.report.errors], [])
    assert.deepEqual(lint(beta), [])

    const old = front('statements/Pi/Pi Thing until 1.0.0.md')
    assert.equal(old.status, 'superseded')
    assert.equal(old.strength, 'moderate')
    assert.equal(old.supersedes, undefined)
    assert.deepEqual(old.scope, { since: null, until: '1.0.0', platform: [], model: null })

    const now = front('statements/Pi/Pi Thing from 1.0.0.md')
    assert.equal(now.status, 'disputed')
    assert.deepEqual(now.proposition.object, { a: 2, b: 'y' })
    assert.equal(now.supersedes, '[Pi Thing until 1.0.0](Pi%20Thing%20until%201.0.0.md)')
    assert.deepEqual(
      now.hasEvidenceLines.map((l: any) => [
        l.directionOfEvidenceProvided,
        l.strengthOfEvidenceProvided,
      ]),
      [
        ['disputes', 'strong'],
        ['supports', 'strong'],
      ],
    )
    // Strong, one step lower for the disputing line.
    assert.equal(now.strength, 'moderate')

    const open = front('statements/Pi/Pi Other open.md')
    assert.equal(open.status, 'open')
    assert.equal(open.direction, 'neutral')
    assert.equal(open.strength, undefined)
    assert.deepEqual(open.proposition.object, { open: ['Does it?'] })
    assert.deepEqual(open.open, ['Does it?'])
  })

  test('maps a missing evidence kind by where the source lives, and cites notes as documents', () => {
    const p = plan(
      context([
        fact('zz/thing', {
          observations: [
            obs({ source: 'doc' }),
            obs({ source: 'note', observed: '2026-01-02', value: { c: true } }),
          ],
        }),
      ]),
    )
    writePlan(dir, p)
    assert.equal(front('study-results/Pi/2026-01-01 Pi Thing.md').kind, 'docs')
    assert.equal(front('study-results/Pi/2026-01-02 Pi Thing.md').kind, 'third-party')
    const st = front('statements/Pi/Pi Thing.md')
    assert.equal(st.strength, 'moderate')
    assert.deepEqual(st.proposition.object, { a: 1, c: true })
    const note = front('documents/skillwright/A note.md')
    assert.equal(note.documentType, 'research note')
    assert.deepEqual(note.urls, [NOTE])
    assert.deepEqual(front('documents/example.com/Upstream docs.md').urls, [DOCS])
    assert.deepEqual(lint(loadBeta(dir)), [])
  })

  test('an existing document with the same URL is reused', () => {
    const url = front('documents/Anthropic/Claude Code overview.md').urls[0]
    const p = plan(
      context([
        fact('zz/thing', {
          observations: [obs({})],
          sources: [{ id: 'doc', resource: url, title: 'Whatever' }],
        }),
      ]),
    )
    assert.equal(p.counts['documents'], undefined)
    const [file] = [...p.files].find(([f]) => f.startsWith('study-results/'))!
    assert.ok(readFileSync(join(dir, 'documents/Anthropic/Claude Code overview.md'), 'utf8') !== '')
    assert.ok(p.files.get(file)!.includes('Claude%20Code%20overview.md'))
  })

  test('skips a fact whose harness has no beta record, with a warning', () => {
    const p = plan(
      context([fact('zz/thing', { harness: 'agent-skills', observations: [obs({})] })]),
    )
    assert.equal(p.counts['facts skipped'], 1)
    assert.equal(p.counts['statements'], undefined)
    assert.match(p.warnings[0] ?? '', /agent-skills has no beta record/)
  })
})

describe('helpers', () => {
  test('idSafe removes what the spec forbids', () => {
    assert.equal(
      idSafe(' Claude Code: harness notes / v[2] #1. '),
      'Claude Code harness notes v 2 1',
    )
  })

  test('inferValueSchema names every key with the types seen', () => {
    assert.deepEqual(inferValueSchema([{ b: true, a: ['x'] }, { b: 'partial' }]), {
      type: 'object',
      properties: {
        a: { type: 'array', items: { type: 'string' } },
        b: { type: ['boolean', 'string'] },
      },
      additionalProperties: true,
    })
  })
})

describe('cli import', () => {
  test('regenerating the committed cli import changes nothing', () => {
    const first = importAlpha(new Set(['cli']), dir)
    assert.deepEqual(first.changed, [])
    assert.deepEqual(first.plan.warnings, [])
    assert.equal(first.plan.counts['statements'], 30)
    assert.equal(first.plan.counts['study-results'], 30)
    assert.equal(first.plan.counts['facts'], 30)
    const second = importAlpha(new Set(['cli']), dir)
    assert.deepEqual(second.changed, [])
  })

  test('is byte-identical from an empty corpus, and the result validates and lints', () => {
    const fresh = mkdtempSync(join(tmpdir(), 'harness-reference-fresh-'))
    try {
      for (const t of [
        'harnesses',
        'documents',
        'study-results',
        'methods',
        'subjects',
        'statements',
      ])
        cpSync(join(BETA, t, `${t}.schema.md`), join(fresh, t, `${t}.schema.md`))
      for (const f of ['corpus-schema.md', 'README.md']) cpSync(join(BETA, f), join(fresh, f))
      cpSync(join(BETA, 'harnesses'), join(fresh, 'harnesses'), { recursive: true })
      const r = importAlpha(new Set(['cli']), fresh)
      assert.ok(r.changed.length > 60)
      const beta = loadBeta(fresh)
      // Documents the corpus cites that only the full corpus holds are missing here, so only the
      // import's own files are compared.
      assert.deepEqual(
        beta.report.errors.filter((e) => !e.startsWith('harnesses/')),
        [],
      )
      // Subjects are the exception: the committed ones were tidied by hand.
      for (const rel of r.changed.filter((f) => !f.startsWith('subjects/'))) {
        const committed = join(BETA, rel)
        assert.equal(readFileSync(join(fresh, rel), 'utf8'), readFileSync(committed, 'utf8'), rel)
      }
      assert.deepEqual(importAlpha(new Set(['cli']), fresh).changed, [])
    } finally {
      rmSync(fresh, { recursive: true, force: true })
    }
  })

  test('the session-env run is a strong statement that cites its method', () => {
    const st = front('statements/Claude Code/Claude Code Session environment.md')
    assert.equal(st.strength, 'strong')
    assert.equal(st.status, 'current')
    assert.deepEqual(st.proposition.object, { vars: ['CLAUDE_CODE_SESSION_ID', 'CLAUDE_PID'] })
    const run = front('study-results/Claude Code/2026-10-10 Claude Code Session environment.md')
    assert.equal(run.kind, 'run')
    assert.match(run.specifiedBy, /Session%20environment\.md\)$/)
    assert.equal(run.version, '2.1.294')
  })
})
