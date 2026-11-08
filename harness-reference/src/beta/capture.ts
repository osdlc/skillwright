/**
 * `capture`: fetch a page, check that a quote occurs on it, and write an evidence record to the
 * beta corpus, adding the page's reference record when it has none. The schemas in
 * corpora/beta/references/ and corpora/beta/evidence/ hold the rules; this checks the result with
 * the markdown-db validator and removes what it wrote if the corpus no longer validates.
 */
import { existsSync, mkdirSync, readdirSync, rmdirSync, rmSync, writeFileSync } from 'node:fs'
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'

import { type CorpusReport, type Data, validateCorpus } from '@osdlc/markdown-db'
import { stringify } from 'yaml'

/** The kinds that observe a page, so carry a `reference`. */
export const PAGE_KINDS = ['docs', 'changelog', 'source', 'issue', 'third-party'] as const
export type PageKind = (typeof PAGE_KINDS)[number]
const VERSION_BASES = ['exact', 'inferred', 'unknown'] as const
type VersionBasis = (typeof VERSION_BASES)[number]

export interface CaptureOptions {
  corpus: string
  /** The harness record's id, such as `Claude Code`. */
  harness: string
  /** An https URL, or the path of a reference record. */
  reference: string
  kind: PageKind
  excerpt: string
  observedBy: string
  locator?: string
  version?: string
  /** Defaults to `inferred` when `version` is given. */
  versionBasis?: VersionBasis
  /** The evidence record's name after its date. Defaults to the reference's id. */
  name?: string
  /** For a new reference: its id, title and publisher. Default: the page title, the page title, the harness's vendor. */
  referenceName?: string
  title?: string
  publisher?: string
}

export interface CaptureDeps {
  fetch: (url: string, init?: RequestInit) => Promise<Response>
  /** Today as YYYY-MM-DD. */
  today: () => string
}

export interface CaptureResult {
  /** Paths relative to the corpus root. */
  evidence: string
  reference: string
  createdReference: boolean
}

/** Characters around the quote kept in `prefix` and `suffix`, as Hypothes.is does. */
const CONTEXT = 32

export const normalize = (s: string): string => s.normalize('NFC').replace(/\s+/gu, ' ').trim()

const ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' ',
  mdash: '—',
  ndash: '–',
  hellip: '…',
  lsquo: '‘',
  rsquo: '’',
  ldquo: '“',
  rdquo: '”',
  copy: '©',
  middot: '·',
}

const decodeEntities = (s: string): string =>
  s.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/giu, (whole, code: string) => {
    if (code[0] === '#') {
      const n =
        code[1] === 'x' || code[1] === 'X' ? parseInt(code.slice(2), 16) : Number(code.slice(1))
      return Number.isFinite(n) && n <= 0x10ffff ? String.fromCodePoint(n) : whole
    }
    return ENTITIES[code.toLowerCase()] ?? whole
  })

const BLOCK_TAGS =
  'address|article|aside|blockquote|br|dd|details|div|dl|dt|figcaption|figure|footer|h[1-6]|header|hr|li|main|nav|ol|option|p|pre|section|summary|table|td|th|tr|ul'

/** The visible text of an HTML page: no markup, scripts or styles; block tags become spaces. */
export function htmlToText(html: string): string {
  const text = html
    .replace(/<!--[\s\S]*?-->/gu, '')
    .replace(/<(head|script|style|noscript|template|svg)\b[\s\S]*?<\/\1\s*>/giu, ' ')
    .replace(new RegExp(`</?(?:${BLOCK_TAGS})\\b[^>]*>`, 'giu'), ' ')
    .replace(/<[^>]*>/gu, '')
  return normalize(decodeEntities(text))
}

const pageTitle = (html: string): string | undefined => {
  const m = /<title\b[^>]*>([\s\S]*?)<\/title\s*>/iu.exec(html)
  const title = m?.[1] === undefined ? '' : normalize(decodeEntities(m[1]))
  return title === '' ? undefined : title
}

/** The quote found in `text`, whitespace-normalized, with the text either side of it. */
export function findExcerpt(
  text: string,
  excerpt: string,
): { exact: string; prefix: string; suffix: string } | undefined {
  const exact = normalize(excerpt)
  const at = exact === '' ? -1 : text.indexOf(exact)
  if (at < 0) return undefined
  return {
    exact,
    prefix: text.slice(Math.max(0, at - CONTEXT), at),
    suffix: text.slice(at + exact.length, at + exact.length + CONTEXT),
  }
}

const percent = (c: string): string =>
  [...new TextEncoder().encode(c)]
    .map((b) => `%${b.toString(16).toUpperCase().padStart(2, '0')}`)
    .join('')

/** A relative Markdown link from the file `from` to the file `to`, both corpus-relative. */
function link(from: string, to: string): string {
  const path = relative(dirname(from), to).split(sep).join('/')
  return `[${basename(to, extname(to))}](${path.replace(/[%\s()<>?#]/gu, percent)})`
}

/** Corpus-relative paths of every record file under `typeDir`, by id. */
function recordFiles(corpus: string, typeDir: string): Map<string, string> {
  const files = new Map<string, string>()
  const visit = (rel: string): void => {
    const abs = join(corpus, rel)
    if (!existsSync(abs)) return
    for (const d of readdirSync(abs, { withFileTypes: true })) {
      if (d.name.startsWith('.')) continue
      const path = `${rel}/${d.name}`
      if (d.isDirectory()) visit(path)
      else if (['.md', '.yaml', '.json'].includes(extname(d.name)) && !d.name.includes('.schema.'))
        files.set(basename(d.name, extname(d.name)), path)
    }
  }
  visit(typeDir)
  return files
}

const fold = (s: string): string => s.normalize('NFC').toLowerCase()

function record(data: Data, body: string): string {
  return `---\n${stringify(data, { lineWidth: 0 })}---\n${body === '' ? '' : `\n${body}\n`}`
}

/** Find or create the reference, fetch the page, check the quote and write the evidence record. */
export async function capture(opts: CaptureOptions, deps: CaptureDeps): Promise<CaptureResult> {
  const corpus = resolve(opts.corpus)
  const before = validateCorpus(corpus)
  if (before.errors.length > 0)
    throw new Error(`the corpus has errors; fix them first:\n${before.errors.join('\n')}`)
  if (!PAGE_KINDS.includes(opts.kind))
    throw new Error(`kind must be one of ${PAGE_KINDS.join(', ')}; capture records pages only`)
  if (opts.versionBasis !== undefined && !VERSION_BASES.includes(opts.versionBasis))
    throw new Error(`version basis must be one of ${VERSION_BASES.join(', ')}`)

  const harness = before.records.get('harnesses')?.get(opts.harness)
  const harnessFile = recordFiles(corpus, 'harnesses').get(opts.harness)
  if (harness === undefined || harnessFile === undefined)
    throw new Error(`no harness record named "${opts.harness}"`)

  const references = before.records.get('references') ?? new Map<string, Data>()
  const referenceFiles = recordFiles(corpus, 'references')
  let referenceId: string | undefined
  let url: string
  if (/^[a-z][a-z0-9+.-]*:/iu.test(opts.reference)) {
    url = opts.reference
    if (!url.startsWith('https://')) throw new Error(`${url}: a reference URL must use https`)
    referenceId = [...references].find(([, data]) => data['url'] === url)?.[0]
  } else {
    const abs = [resolve(opts.reference), resolve(corpus, opts.reference)].find(existsSync)
    const rel = abs === undefined ? undefined : relative(corpus, abs).split(sep).join('/')
    referenceId = [...referenceFiles].find(([, file]) => file === rel)?.[0]
    if (referenceId === undefined) throw new Error(`${opts.reference}: not a reference record`)
    url = String(references.get(referenceId)?.['url'])
  }

  const response = await deps.fetch(url, {
    redirect: 'follow',
    headers: { accept: 'text/html, text/markdown, text/plain;q=0.9, */*;q=0.1' },
  })
  if (!response.ok) throw new Error(`${url}: fetch failed with HTTP ${response.status}`)
  const raw = await response.text()
  const html =
    /html/iu.test(response.headers.get('content-type') ?? '') || /^\s*<(!doctype|html)/iu.test(raw)
  const found = findExcerpt(html ? htmlToText(raw) : normalize(raw), opts.excerpt)
  if (found === undefined)
    throw new Error(`${url}: excerpt not found on the page (compared with whitespace collapsed)`)

  const written: string[] = []
  const write = (rel: string, text: string): void => {
    mkdirSync(dirname(join(corpus, rel)), { recursive: true })
    writeFileSync(join(corpus, rel), text, { flag: 'wx' })
    written.push(rel)
  }
  try {
    let referenceFile = referenceId === undefined ? undefined : referenceFiles.get(referenceId)
    const createdReference = referenceFile === undefined
    if (referenceFile === undefined) {
      const title = opts.title ?? (html ? pageTitle(raw) : undefined)
      if (title === undefined) throw new Error(`${url}: the page has no title; pass --title`)
      const name = normalize(opts.referenceName ?? title)
      const publisher = normalize(opts.publisher ?? String(harness['vendor']))
      if (/[/\\]/u.test(name) || /[/\\]/u.test(publisher))
        throw new Error(
          `"${name}" or "${publisher}" holds a slash; pass --reference-name or --publisher`,
        )
      if ([...references.keys()].some((id) => fold(id) === fold(name)))
        throw new Error(
          `a reference named "${name}" already exists for another URL; pass --reference-name`,
        )
      referenceId = name
      referenceFile = `references/${publisher.replace(/[ .]+$/u, '')}/${name}.md`
      write(referenceFile, record({ url, title, publisher, url_status: 'live' }, ''))
    }

    const evidenceFile = `evidence/${opts.harness}/${deps.today()} ${opts.name ?? referenceId}.md`
    if (existsSync(join(corpus, evidenceFile)))
      throw new Error(`${evidenceFile} already exists; evidence is append-only, so pass --name`)
    const version =
      opts.version === undefined
        ? opts.versionBasis === undefined
          ? {}
          : { version_basis: opts.versionBasis }
        : { version: opts.version, version_basis: opts.versionBasis ?? 'inferred' }
    const data: Data = {
      kind: opts.kind,
      reference: link(evidenceFile, referenceFile),
      harness: link(evidenceFile, harnessFile),
      ...version,
      observed: deps.today(),
      ...(opts.locator === undefined ? {} : { locator: opts.locator }),
      excerpt: found,
      observed_by: opts.observedBy,
    }
    write(evidenceFile, record(data, `> ${found.exact}`))

    const after: CorpusReport = validateCorpus(corpus)
    if (after.errors.length > 0)
      throw new Error(`the new records do not validate:\n${after.errors.join('\n')}`)
    return { evidence: evidenceFile, reference: referenceFile, createdReference }
  } catch (e) {
    for (const rel of written.reverse()) {
      rmSync(join(corpus, rel))
      for (
        let d = dirname(rel);
        d !== '.' && readdirSync(join(corpus, d)).length === 0;
        d = dirname(d)
      )
        rmdirSync(join(corpus, d))
    }
    throw e
  }
}

const USAGE = `usage: capture --harness <id> --reference <url or path> --kind <${PAGE_KINDS.join('|')}>
               --excerpt <text> --observed-by <who> [--locator <where>]
               [--version <v> [--version-basis exact|inferred|unknown]] [--name <name>]
               [--reference-name <name>] [--title <title>] [--publisher <name>] [--corpus <dir>]`

const today = (): string => {
  const d = new Date()
  const pad = (n: number): string => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

async function main(argv: string[]): Promise<number> {
  let values: Record<string, string | undefined>
  try {
    const str = { type: 'string' } as const
    values = parseArgs({
      args: argv,
      options: {
        corpus: str,
        harness: str,
        reference: str,
        kind: str,
        excerpt: str,
        locator: str,
        version: str,
        'version-basis': str,
        'observed-by': str,
        name: str,
        'reference-name': str,
        title: str,
        publisher: str,
      },
    }).values
  } catch (e) {
    console.error(`${(e as Error).message}\n${USAGE}`)
    return 2
  }
  const { harness, reference, kind, excerpt } = values
  const observedBy = values['observed-by']
  if (!harness || !reference || !kind || !excerpt || !observedBy) {
    console.error(USAGE)
    return 2
  }
  try {
    const result = await capture(
      {
        corpus: values['corpus'] ?? resolve(import.meta.dirname, '../../corpora/beta'),
        harness,
        reference,
        kind: kind as PageKind,
        excerpt,
        observedBy,
        locator: values['locator'],
        version: values['version'],
        versionBasis: values['version-basis'] as VersionBasis | undefined,
        name: values['name'],
        referenceName: values['reference-name'],
        title: values['title'],
        publisher: values['publisher'],
      },
      { fetch: (url, init) => fetch(url, init), today },
    )
    if (result.createdReference) console.log(`wrote ${result.reference}`)
    console.log(`wrote ${result.evidence}`)
    return 0
  } catch (e) {
    console.error(`capture: ${(e as Error).message}`)
    return 1
  }
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  process.exitCode = await main(process.argv.slice(2))
