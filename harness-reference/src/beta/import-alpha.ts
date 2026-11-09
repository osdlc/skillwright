/**
 * `import-alpha`: convert the alpha corpus into beta records, for the categories asked for.
 *
 *   alpha sources and notes -> documents      alpha probes       -> methods
 *   alpha observations      -> study-results  alpha facts        -> statements
 *   alpha subjects          -> subjects
 *
 * Output is deterministic and idempotent: the same alpha and beta give the same files, and a
 * second run writes nothing new. A document already in beta (same URL) is reused, and a subject
 * record already in beta is never rewritten, so a hand-tuned `valueSchema` survives.
 */
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { basename, dirname, join, posix, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'

import { fold, markdownLink } from '@osdlc/corpus-schema'
import { stringify } from 'yaml'

import { type Dataset, load, parseConcept, ROOT } from '../alpha/load.ts'
import type { Observation, Probe, Subject } from '../alpha/schema.ts'
import { BETA, type Beta, loadBeta } from './corpus.ts'
import { compareVersions, type Versioning } from './versions.ts'

export const IMPORTER = 'harness-reference import'
const REPO_BLOB = 'https://github.com/osdlc/skillwright/blob/main/'
const NOTES_PREFIX = `${REPO_BLOB}harness-reference/corpora/alpha/sources/`

type Strength = 'strong' | 'moderate' | 'weak'
const STRENGTHS: Strength[] = ['weak', 'moderate', 'strong']
/** The strength rule: the best evidence kind on a supporting line sets the line's strength. */
export const KIND_STRENGTH: Record<string, Strength> = {
  run: 'strong',
  source: 'strong',
  'help-text': 'strong',
  docs: 'moderate',
  changelog: 'moderate',
  issue: 'weak',
  'third-party': 'weak',
  inferred: 'weak',
}

export interface Source {
  title: string
  description?: string
}

export interface Context {
  dataset: Dataset
  beta: Beta
  /** Alpha categories to import. */
  categories: Set<string>
  /** The research note at a repository URL, when `url` names one. */
  note: (url: string) => Source | undefined
  /** The body of `probes/<id>/probe.md`. */
  probeBody: (id: string) => string
  /** Whether `probes/<id>/fixture/` can be copied into a record folder (see `fixtureFiles`). */
  fixtureFiles: (id: string) => string[]
  /** Whether the beta record at this corpus-relative path was written by this importer. */
  imported: (file: string) => boolean
}

export interface Plan {
  /** Corpus-relative path -> file text. */
  files: Map<string, string>
  /** Corpus-relative path -> source file to copy, for method fixtures. */
  copies: Map<string, string>
  warnings: string[]
  counts: Record<string, number>
}

// ---- names and files ------------------------------------------------------------------------

/** A record id from free text: no character the spec forbids, no edge spaces or dots. */
export function idSafe(text: string): string {
  return (
    text
      .replace(/[/\\:*?"<>|[\]#^]/gu, ' ')
      // eslint-disable-next-line no-control-regex
      .replace(/[\u0000-\u001f]/gu, ' ')
      .replace(/\s+/gu, ' ')
      .replace(/^[ .]+|[ .]+$/gu, '')
  )
}

function record(data: Record<string, unknown>, body: string): string {
  return `---\n${stringify(data, { lineWidth: 0 })}---\n${body === '' ? '' : `\n${body}\n`}`
}

const link = (from: string, to: string, text?: string): string =>
  markdownLink(text ?? posix.basename(to, '.md'), posix.relative(posix.dirname(from), to))

/** Claims unique ids: the first caller of a folded name keeps it, later ones get a number. */
class Names {
  private readonly taken = new Set<string>()
  constructor(existing: Iterable<string> = []) {
    for (const id of existing) this.taken.add(fold(id))
  }
  claim(wanted: string): string {
    let id = wanted
    for (let n = 2; this.taken.has(fold(id)); n++) id = `${wanted} ${n}`
    this.taken.add(fold(id))
    return id
  }
}

// ---- values ---------------------------------------------------------------------------------

const same = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b)

type Json = null | boolean | number | string | Json[] | { [k: string]: Json }
const jsonType = (v: unknown): string =>
  v === null
    ? 'null'
    : Array.isArray(v)
      ? 'array'
      : typeof v === 'number'
        ? Number.isInteger(v)
          ? 'integer'
          : 'number'
        : typeof v

/** A permissive object schema for the values seen: each key with the types it has held. */
export function inferValueSchema(values: Record<string, unknown>[]): Record<string, unknown> {
  const keys = new Map<string, Set<string>>()
  const items = new Map<string, Set<string>>()
  for (const value of values)
    for (const [key, v] of Object.entries(value)) {
      const types = keys.get(key) ?? new Set<string>()
      types.add(jsonType(v))
      keys.set(key, types)
      if (Array.isArray(v)) {
        const it = items.get(key) ?? new Set<string>()
        for (const x of v) it.add(jsonType(x))
        items.set(key, it)
      }
    }
  const properties: Record<string, unknown> = {}
  for (const key of [...keys.keys()].sort()) {
    const types = [...(keys.get(key) ?? [])].sort()
    const prop: Record<string, unknown> = { type: types.length === 1 ? types[0] : types }
    const it = [...(items.get(key) ?? [])].sort()
    if (it.length > 0) prop['items'] = { type: it.length === 1 ? it[0] : it }
    properties[key] = prop
  }
  return { type: 'object', properties, additionalProperties: true }
}

// ---- the plan -------------------------------------------------------------------------------

const hostOf = (url: string): string => {
  try {
    return new URL(url).host.replace(/^www\./u, '')
  } catch {
    return 'web'
  }
}
const trimUrl = (url: string): string => url.replace(/\/+$/u, '')

export function plan(ctx: Context): Plan {
  const { dataset, beta } = ctx
  const out: Plan = { files: new Map(), copies: new Map(), warnings: [], counts: {} }
  const count = (k: string, n = 1): void => void (out.counts[k] = (out.counts[k] ?? 0) + n)

  const harnessBySlug = new Map<string, { id: string; file: string; versioning: Versioning }>()
  for (const { ref, data } of beta.all('harnesses'))
    harnessBySlug.set(String(data['slug']), {
      id: ref.id,
      file: ref.file,
      versioning: data['versioning'] as Versioning,
    })

  const subjects = dataset.subjects.filter((s) => ctx.categories.has(s.category))
  const facts = dataset.facts
    .filter((f) => ctx.categories.has(f.subject.split('/')[0] ?? ''))
    .sort((a, b) => (a.conceptId < b.conceptId ? -1 : a.conceptId > b.conceptId ? 1 : 0))
  const subjectById = new Map(dataset.subjects.map((s) => [s.id, s]))

  // Subjects. An existing record is kept as it is.
  const subjectNames = new Names(beta.all('subjects').map((s) => s.ref.id))
  const subjectFile = new Map<string, string>()
  for (const { ref, data } of beta.all('subjects')) subjectFile.set(String(data['slug']), ref.file)
  for (const s of [...subjects].sort((a, b) => (a.id < b.id ? -1 : 1))) {
    if (subjectFile.has(s.id)) continue
    const id = subjectNames.claim(idSafe(s.title))
    const file = `subjects/${s.category}/${id}.md`
    subjectFile.set(s.id, file)
    const seen = dataset.facts
      .filter((f) => f.subject === s.id)
      .flatMap((f) => f.observations.map((o) => o.value))
    out.files.set(
      file,
      record(
        {
          slug: s.id,
          category: s.category,
          description: s.description,
          ...(s.model_dependent === true ? { model_dependent: true } : {}),
          valueSchema: inferValueSchema(seen),
        },
        '',
      ),
    )
    count('subjects')
  }

  // Documents: reuse a beta document with the same URL, else add one.
  const docByUrl = new Map<string, string>()
  const docNames = new Names(beta.all('documents').map((d) => d.ref.id))
  for (const { ref, data } of beta.all('documents'))
    for (const u of (data['urls'] as string[]) ?? []) docByUrl.set(trimUrl(u), ref.file)
  const newDocs = new Map<string, { file: string; title: string; url: string }>()
  const documentFor = (url: string, title: string): string => {
    const known = docByUrl.get(trimUrl(url))
    if (known !== undefined) return known
    const note = url.startsWith(NOTES_PREFIX) ? ctx.note(url) : undefined
    const host = url.startsWith(NOTES_PREFIX) ? 'skillwright' : hostOf(url)
    const docTitle = note?.title ?? title
    let id = idSafe(docTitle)
    if (id === '') id = host
    if (docNames.claim(id) !== id) {
      // Name taken by another URL: qualify it with the publisher.
      id = docNames.claim(`${id} ${host}`)
    }
    const file = `documents/${host}/${id}.md`
    docByUrl.set(trimUrl(url), file)
    newDocs.set(file, { file, title: docTitle, url })
    out.files.set(
      file,
      record(
        {
          title: docTitle,
          ...(url.startsWith(NOTES_PREFIX) ? { documentType: 'research note' } : {}),
          urls: [url],
          publisher: host,
        },
        note?.description ?? '',
      ),
    )
    count('documents')
    return file
  }
  const isNote = (url: string): boolean => url.startsWith(NOTES_PREFIX)

  // Methods: the probes that imported run observations name.
  const probeIds = new Set<string>()
  for (const f of facts)
    for (const o of f.observations) if (o.probe !== undefined) probeIds.add(o.probe)
  const methodFile = new Map<string, string>()
  const methodNames = new Names(
    beta
      .all('methods')
      .filter((m) => !ctx.imported(m.ref.file))
      .map((m) => m.ref.id),
  )
  for (const id of [...probeIds].sort()) {
    const p = dataset.probes.find((x) => x.id === id)
    if (p === undefined) continue
    const name = methodNames.claim(idSafe(p.title))
    methodFile.set(id, `methods/${name}/${name}.md`)
  }
  for (const [id, file] of [...methodFile].sort()) {
    const p = dataset.probes.find((x) => x.id === id) as Probe
    const harnesses = p.harnesses
      .map((h) => harnessBySlug.get(h))
      .filter((h) => h !== undefined)
      .map((h) => link(file, h.file, h.id))
    for (const h of p.harnesses)
      if (!harnessBySlug.has(h)) out.warnings.push(`probe ${id}: harness ${h} has no beta record`)
    const subs = p.subjects
      .map((s) => subjectFile.get(s))
      .filter((s): s is string => s !== undefined)
      .map((s) => link(file, s))
    let fixture = ctx.fixtureFiles(id)
    const nested = fixture.some((f) => {
      const parts = f.split('/')
      const name = parts.at(-1) ?? ''
      const parent = parts.at(-2)
      return parent !== undefined && ['.md', '.yaml', '.json'].some((e) => name === parent + e)
    })
    if (nested) {
      out.warnings.push(`probe ${id}: fixture holds a record-folder shape, so it was not copied`)
      fixture = []
    }
    for (const f of fixture)
      out.copies.set(`${posix.dirname(file)}/fixture/${f}`, `${id}/fixture/${f}`)
    const body = `${ctx.probeBody(id).trim()}\n\nImported from \`probes/${id}/probe.md\`.${fixture.length > 0 ? ' The fixture files are in [fixture](fixture).' : ''}`
    out.files.set(
      file,
      record(
        {
          methodType: 'probe',
          harnesses,
          ...(subs.length > 0 ? { subjects: subs } : {}),
          commands: p.commands,
          record: p.record,
          safety: p.safety,
        },
        body,
      ),
    )
    count('methods')
  }

  // Study results and statements, fact by fact.
  // Records this importer wrote are regenerated, so their names are free to claim again.
  const others = (type: string): string[] =>
    beta
      .all(type)
      .filter((r) => !ctx.imported(r.ref.file))
      .map((r) => r.ref.id)
  const resultNames = new Names(others('study-results'))
  const statementNames = new Names(others('statements'))
  for (const fact of facts) {
    const h = harnessBySlug.get(fact.harness)
    const subject = subjectById.get(fact.subject) as Subject | undefined
    const sfile = subjectFile.get(fact.subject)
    if (h === undefined || subject === undefined || sfile === undefined) {
      out.warnings.push(
        `${fact.conceptId}.md: harness ${fact.harness} has no beta record; not imported`,
      )
      count('facts skipped')
      continue
    }
    const versioning = h.versioning
    const alphaPath = `${fact.conceptId}.md`
    const sourceUrl = new Map(fact.sources.map((s) => [s.id, s]))
    const docFor = (sid: string): string => {
      const s = sourceUrl.get(sid) as { resource: string; title: string }
      return documentFor(s.resource, s.title)
    }

    // One study result per observation.
    interface Made {
      obs: Observation
      file: string
      kind: string
    }
    const made: Made[] = []
    fact.observations.forEach((o, i) => {
      const s = sourceUrl.get(o.source)
      const kind = o.evidence ?? (s !== undefined && isNote(s.resource) ? 'third-party' : 'docs')
      const name = resultNames.claim(idSafe(`${o.observed} ${h.id} ${subject.title}`))
      const file = `study-results/${h.id}/${name}.md`
      const reported = [
        ...new Set([docFor(o.source), ...(o.via === undefined ? [] : [docFor(o.via)])]),
      ]
      const method = o.probe === undefined ? undefined : methodFile.get(o.probe)
      const lines = [
        `Imported from \`${alphaPath}\`, observation ${i + 1}.`,
        '',
        ...Object.entries(o.value).map(([k, v]) => `- ${k}: ${JSON.stringify(v)}`),
        ...(o.model === undefined ? [] : ['', `Model: ${o.model}.`]),
        ...(o.note === undefined ? [] : ['', o.note]),
      ]
      out.files.set(
        file,
        record(
          {
            kind,
            ...(method === undefined ? {} : { specifiedBy: link(file, method) }),
            reportedIn: reported.map((d) => link(file, d)),
            harness: link(file, h.file, h.id),
            ...(o.checked_version === undefined
              ? {}
              : { version: o.checked_version, version_basis: 'exact' }),
            observed: o.observed,
            ...(o.locator === undefined ? {} : { locator: o.locator }),
            contributions: [
              {
                contributor: { name: IMPORTER, agentType: 'software' },
                activityType: 'created',
                date: `${o.observed}T00:00:00Z`,
              },
            ],
          },
          lines.join('\n'),
        ),
      )
      made.push({ obs: o, file, kind })
      count('study-results')
    })

    // One statement per distinct scope.
    const scopeKey = (o: Observation): string => JSON.stringify([o.since, o.until, o.model ?? null])
    const groups = new Map<string, Made[]>()
    for (const m of made) groups.set(scopeKey(m.obs), [...(groups.get(scopeKey(m.obs)) ?? []), m])
    const ordered = [...groups.values()].sort((a, b) => {
      const x = a[0]!.obs
      const y = b[0]!.obs
      if (x.since !== y.since) {
        if (x.since === null) return -1
        if (y.since === null) return 1
        return versioning === 'unknown' ? 0 : compareVersions(x.since, y.since, versioning)
      }
      return (x.model ?? '') < (y.model ?? '') ? -1 : (x.model ?? '') > (y.model ?? '') ? 1 : 0
    })
    const open = (fact.open ?? []).map((q) => q.question)
    const statementFiles = new Map<string, string>()
    const names: string[] = []
    // Names first, so a superseding statement can link the older one.
    for (const g of ordered) {
      const o = g[0]!.obs
      const suffix = [
        o.since !== null ? ` from ${o.since}` : '',
        o.until !== null ? ` until ${o.until}` : '',
        o.model !== undefined ? ` on ${o.model}` : '',
      ].join('')
      const name = statementNames.claim(idSafe(`${h.id} ${subject.title}${suffix}`))
      names.push(name)
      statementFiles.set(scopeKey(o), `statements/${h.id}/${name}.md`)
    }
    ordered.forEach((g, gi) => {
      const o0 = g[0]!.obs
      const file = statementFiles.get(scopeKey(o0)) as string
      const merged: Record<string, unknown> = {}
      const conflicts = new Set<string>()
      for (const m of g)
        for (const [k, v] of Object.entries(m.obs.value)) {
          if (k in merged && !same(merged[k], v)) conflicts.add(k)
          merged[k] = v
        }
      const lines = g.map((m) => {
        const disputes = [...conflicts].some(
          (k) => k in m.obs.value && !same(m.obs.value[k], merged[k]),
        )
        return {
          hasEvidenceItems: [link(file, m.file)],
          directionOfEvidenceProvided: disputes ? 'disputes' : 'supports',
          strengthOfEvidenceProvided: KIND_STRENGTH[m.kind] ?? 'weak',
          description: `${m.kind} observed ${m.obs.observed}${disputes ? '; its value differs from the latest' : ''}.`,
        }
      })
      const supporting = lines.filter((l) => l.directionOfEvidenceProvided === 'supports')
      let best = Math.max(
        ...supporting.map((l) => STRENGTHS.indexOf(l.strengthOfEvidenceProvided as Strength)),
      )
      if (supporting.length < lines.length) best = Math.max(0, best - 1)
      const status = o0.until !== null ? 'superseded' : conflicts.size > 0 ? 'disputed' : 'current'
      const prior =
        o0.since === null
          ? undefined
          : ordered.find(
              (p) =>
                p[0]!.obs.until === o0.since && (p[0]!.obs.model ?? null) === (o0.model ?? null),
            )
      const last = gi === ordered.length - 1
      const date = g
        .map((m) => m.obs.observed)
        .sort()
        .at(-1) as string
      const sources = [...new Set(g.flatMap((m) => [m.obs.source]))].map(docFor)
      const body = [
        `Imported from \`${alphaPath}\`.`,
        ...(conflicts.size > 0
          ? [
              '',
              `The observations disagree on: ${[...conflicts].sort().join(', ')}. The latest value is kept.`,
            ]
          : []),
        ...(last && open.length > 0 ? ['', 'Open questions:', ...open.map((q) => `- ${q}`)] : []),
      ].join('\n')
      out.files.set(
        file,
        record(
          {
            proposition: {
              subject: link(file, h.file, h.id),
              predicate: fact.subject,
              object: merged,
            },
            direction: 'supports',
            strength: STRENGTHS[best],
            hasEvidenceLines: lines,
            reportedIn: sources.map((d) => link(file, d)),
            contributions: [
              {
                contributor: { name: IMPORTER, agentType: 'software' },
                activityType: 'created',
                date: `${date}T00:00:00Z`,
              },
            ],
            harness: link(file, h.file, h.id),
            subject: link(file, sfile),
            scope: { since: o0.since, until: o0.until, platform: [], model: o0.model ?? null },
            status,
            ...(prior === undefined
              ? {}
              : { supersedes: link(file, statementFiles.get(scopeKey(prior[0]!.obs)) as string) }),
            ...(last && open.length > 0 ? { open } : {}),
          },
          body,
        ),
      )
      count('statements')
      count(`statements ${status}`)
    })

    // A fact with only open questions is one open statement.
    if (fact.observations.length === 0) {
      const name = statementNames.claim(idSafe(`${h.id} ${subject.title} open`))
      const file = `statements/${h.id}/${name}.md`
      const date = (fact.open ?? [])
        .map((q) => q.observed)
        .sort()
        .at(-1) as string
      out.files.set(
        file,
        record(
          {
            proposition: {
              subject: link(file, h.file, h.id),
              predicate: fact.subject,
              object: { open },
            },
            direction: 'neutral',
            reportedIn: [...new Set((fact.open ?? []).map((q) => q.source))].map((s) =>
              link(file, docFor(s)),
            ),
            contributions: [
              {
                contributor: { name: IMPORTER, agentType: 'software' },
                activityType: 'created',
                date: `${date}T00:00:00Z`,
              },
            ],
            harness: link(file, h.file, h.id),
            subject: link(file, sfile),
            scope: { since: null, until: null, platform: [], model: null },
            status: 'open',
            open,
          },
          [
            `Imported from \`${alphaPath}\`.`,
            '',
            'Open questions:',
            ...open.map((q) => `- ${q}`),
          ].join('\n'),
        ),
      )
      count('statements')
      count('statements open')
    }
    count('facts')
  }
  return out
}

// ---- disk -----------------------------------------------------------------------------------

/** Write the plan under `root`, leaving files whose text is already right. Returns the paths changed. */
export function writePlan(root: string, p: Plan): string[] {
  const changed: string[] = []
  const put = (rel: string, write: () => void, same: boolean): void => {
    if (same) return
    mkdirSync(dirname(join(root, rel)), { recursive: true })
    write()
    changed.push(rel)
  }
  for (const [rel, text] of [...p.files].sort(([a], [b]) => (a < b ? -1 : 1))) {
    const file = join(root, rel)
    put(
      rel,
      () => writeFileSync(file, text),
      existsSync(file) && readFileSync(file, 'utf8') === text,
    )
  }
  for (const [rel, from] of [...p.copies].sort(([a], [b]) => (a < b ? -1 : 1)))
    put(
      rel,
      () => copyFileSync(from, join(root, rel)),
      existsSync(join(root, rel)) && readFileSync(join(root, rel)).equals(readFileSync(from)),
    )
  return changed
}

const walkFiles = (dir: string, base = ''): string[] =>
  existsSync(dir)
    ? readdirSync(dir)
        .sort()
        .flatMap((n) => {
          const full = join(dir, n)
          return statSync(full).isDirectory() ? walkFiles(full, `${base}${n}/`) : [`${base}${n}`]
        })
    : []

export function defaultContext(categories: Set<string>, betaRoot: string = BETA): Context {
  const { dataset, errors } = load()
  if (dataset === null || errors.length > 0)
    throw new Error(`the alpha corpus has errors:\n${errors.join('\n')}`)
  const probes = join(ROOT, 'probes')
  return {
    dataset,
    beta: loadBeta(betaRoot),
    categories,
    note: (url) => {
      const file = join(ROOT, 'corpora/alpha/sources', basename(url))
      if (!existsSync(file)) return undefined
      const fm = parseConcept(readFileSync(file, 'utf8')).frontmatter as Record<string, unknown>
      return {
        title: String(fm['title']),
        ...(typeof fm['description'] === 'string' ? { description: fm['description'] } : {}),
      }
    },
    probeBody: (id) => parseConcept(readFileSync(join(probes, id, 'probe.md'), 'utf8')).body,
    fixtureFiles: (id) => walkFiles(join(probes, id, 'fixture')),
    imported: (file) => {
      const full = join(betaRoot, file)
      return existsSync(full) && readFileSync(full, 'utf8').includes('\nImported from `')
    },
  }
}

/** Copy sources resolve against the probes folder. */
export function importAlpha(
  categories: Set<string>,
  betaRoot: string = BETA,
): { plan: Plan; changed: string[] } {
  const ctx = defaultContext(categories, betaRoot)
  const p = plan(ctx)
  for (const [rel, from] of p.copies) p.copies.set(rel, join(ROOT, 'probes', from))
  return { plan: p, changed: writePlan(betaRoot, p) }
}

function main(argv: string[]): number {
  const { values } = parseArgs({
    args: argv,
    options: {
      category: { type: 'string', multiple: true },
      all: { type: 'boolean' },
      beta: { type: 'string' },
    },
  })
  const { dataset } = load()
  const known = new Set((dataset?.categories ?? []).map((c) => c.id))
  const chosen = values.all === true ? known : new Set(values.category ?? [])
  const unknown = [...chosen].filter((c) => !known.has(c))
  if (chosen.size === 0 || unknown.length > 0) {
    console.error(
      `usage: import-alpha (--category <id>)... | --all [--beta <dir>]\ncategories: ${[...known].join(', ')}${unknown.length > 0 ? `\nunknown: ${unknown.join(', ')}` : ''}`,
    )
    return 2
  }
  const { plan: p, changed } = importAlpha(
    chosen,
    values.beta === undefined ? BETA : resolve(values.beta),
  )
  for (const w of p.warnings) console.error(`warning: ${w}`)
  console.log(
    Object.entries(p.counts)
      .map(([k, n]) => `${n} ${k}`)
      .join(', '),
  )
  console.log(
    `${changed.length} file(s) written, ${p.files.size + p.copies.size - changed.length} unchanged`,
  )
  return 0
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  process.exitCode = main(process.argv.slice(2))
