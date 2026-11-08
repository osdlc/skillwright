import assert from 'node:assert/strict'
import { afterEach, beforeEach, describe, test } from 'node:test'

import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { validateCorpus } from '@osdlc/markdown-db'

import { capture, type CaptureDeps, type CaptureOptions, htmlToText } from './capture.ts'

const beta = resolve(import.meta.dirname, '../../corpora/beta')

const PAGE = `<!doctype html><html><head><title>Fixture &amp; page</title>
<style>p { color: red }</style></head><body>
<nav>Docs\u200b home</nav>
<script>var hidden = "Claude Code starts as claude"</script>
<p>Install it, then run   <code>claude</code>
in your project.</p><p>Claude Code starts as <strong>claude</strong> in a terminal&nbsp;and more text follows here.</p>
</body></html>`

let dir: string
let fetched: string[]
const deps = (body = PAGE, status = 200): CaptureDeps => ({
  fetch: async (url) => {
    fetched.push(url)
    return new Response(body, { status, headers: { 'content-type': 'text/html; charset=utf-8' } })
  },
  today: () => '2026-11-08',
})
const opts = (over: Partial<CaptureOptions> = {}): CaptureOptions => ({
  corpus: dir,
  harness: 'Claude Code',
  reference: 'references/Anthropic/Claude Code overview.md',
  kind: 'docs',
  excerpt: 'Claude Code starts as claude\n  in a terminal',
  observedBy: 'person: test',
  ...over,
})

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'harness-reference-capture-'))
  cpSync(beta, dir, { recursive: true })
  rmSync(join(dir, 'evidence'), { recursive: true })
  cpSync(join(beta, 'evidence/evidence.schema.md'), join(dir, 'evidence/evidence.schema.md'))
  fetched = []
})
afterEach(() => rmSync(dir, { recursive: true, force: true }))

describe('htmlToText', () => {
  test('keeps visible text, joins inline tags, spaces block tags and decodes entities', () => {
    const text = htmlToText(PAGE)
    assert.ok(!text.includes('hidden') && !text.includes('color') && !text.includes('Fixture'))
    assert.ok(text.includes('Docs home Install it, then run claude in your project. Claude Code'))
    assert.ok(text.includes('starts as claude in a terminal and more'))
  })
})

describe('capture', () => {
  test('writes an evidence record for an existing reference, with the quote and its context', async () => {
    const result = await capture(opts({ locator: 'Get started', version: '2.1.318' }), deps())
    assert.deepEqual(result, {
      evidence: 'evidence/Claude Code/2026-11-08 Claude Code overview.md',
      reference: 'references/Anthropic/Claude Code overview.md',
      createdReference: false,
    })
    assert.deepEqual(fetched, ['https://code.claude.com/docs/en/overview'])
    const text = readFileSync(join(dir, result.evidence), 'utf8')
    assert.equal(
      text,
      `---
kind: docs
reference: "[Claude Code overview](../../references/Anthropic/Claude%20Code%20overview.md)"
harness: "[Claude Code](../../harnesses/Claude%20Code.md)"
version: 2.1.318
version_basis: inferred
observed: 2026-11-08
locator: Get started
excerpt:
  exact: Claude Code starts as claude in a terminal
  prefix: "hen run claude in your project. "
  suffix: " and more text follows here."
observed_by: "person: test"
---

> Claude Code starts as claude in a terminal
`,
    )
    const report = validateCorpus(dir)
    assert.deepEqual(report.errors, [])
    assert.equal(report.records.get('evidence')?.size, 1)
  })

  test('adds a reference record for a URL that has none', async () => {
    const url = 'https://code.claude.com/docs/en/new-page'
    const result = await capture(opts({ reference: url }), deps())
    assert.equal(result.createdReference, true)
    assert.equal(result.reference, 'references/Anthropic/Fixture & page.md')
    assert.equal(
      readFileSync(join(dir, result.reference), 'utf8'),
      `---\nurl: ${url}\ntitle: Fixture & page\npublisher: Anthropic\nurl_status: live\n---\n`,
    )
    assert.equal(result.evidence, 'evidence/Claude Code/2026-11-08 Fixture & page.md')
    assert.deepEqual(validateCorpus(dir).errors, [])
  })

  test('reuses the reference record for a URL it already has', async () => {
    const result = await capture(
      opts({ reference: 'https://code.claude.com/docs/en/overview' }),
      deps(),
    )
    assert.equal(result.createdReference, false)
    assert.equal(result.reference, 'references/Anthropic/Claude Code overview.md')
  })

  test('refuses an excerpt the page does not hold, and writes nothing', async () => {
    await assert.rejects(
      capture(opts({ reference: 'https://example.com/x', excerpt: 'hidden' }), deps()),
      /excerpt not found/,
    )
    assert.ok(!existsSync(join(dir, 'references/Anthropic/Fixture & page.md')))
    assert.ok(!existsSync(join(dir, 'evidence/Claude Code')))
  })

  test('refuses a failed fetch, an unknown harness and a kind that is not a page', async () => {
    await assert.rejects(capture(opts(), deps('gone', 404)), /HTTP 404/)
    await assert.rejects(capture(opts({ harness: 'Nope' }), deps()), /no harness record/)
    await assert.rejects(capture(opts({ kind: 'run' as 'docs' }), deps()), /pages only/)
    await assert.rejects(capture(opts({ name: 'a/b' }), deps()), /holds a slash/)
    assert.deepEqual(fetched, [
      'https://code.claude.com/docs/en/overview',
      'https://code.claude.com/docs/en/overview',
    ])
    assert.ok(!existsSync(join(dir, 'evidence/Claude Code')))
  })

  test('never overwrites a record: a second look the same day needs its own name', async () => {
    await capture(opts(), deps())
    await assert.rejects(capture(opts(), deps()), /append-only/)
    const second = await capture(opts({ name: 'overview again' }), deps())
    assert.equal(second.evidence, 'evidence/Claude Code/2026-11-08 overview again.md')
  })
})
