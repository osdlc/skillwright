/**
 * Read the beta corpus once and answer questions about its records: which record a link names,
 * what a record's folder-relative path is. Shared by `lint`, `import-alpha` and `compile`.
 */
import { posix } from 'node:path'
import { resolve } from 'node:path'

import { type CorpusReport, type Data, validateCorpus } from '@osdlc/corpus-schema'

export const BETA = resolve(import.meta.dirname, '../../corpora/beta')

export interface RecordRef {
  type: string
  id: string
  /** Corpus-relative path, with `/` separators. */
  file: string
}

export interface Beta {
  report: CorpusReport
  /** The record a corpus-relative path is, by path. */
  byFile: Map<string, RecordRef>
  /** Every record of a type, sorted by file path. */
  all(type: string): { ref: RecordRef; data: Data }[]
  get(type: string, id: string): Data | undefined
  /** The record `link` names when written in the record at `fromFile`, or undefined. */
  resolve(fromFile: string, link: unknown): RecordRef | undefined
}

const LINK = /^\[(?:\\.|[^\]\\])*\]\((<[^>]*>|[^)]*)\)$/u

/** The path inside a relative Markdown link string, decoded, or undefined when it is not one. */
export function linkPath(link: unknown): string | undefined {
  if (typeof link !== 'string') return undefined
  const m = LINK.exec(link)
  if (m === null) return undefined
  let target = m[1] ?? ''
  if (target.startsWith('<') && target.endsWith('>')) target = target.slice(1, -1)
  const hash = target.indexOf('#')
  if (hash >= 0) target = target.slice(0, hash)
  try {
    return decodeURIComponent(target)
  } catch {
    return undefined
  }
}

export function loadBeta(root: string = BETA): Beta {
  const report = validateCorpus(root)
  const byFile = new Map<string, RecordRef>()
  for (const [type, files] of report.files)
    for (const [id, file] of files) byFile.set(file, { type, id, file })
  return {
    report,
    byFile,
    all: (type) =>
      [...(report.files.get(type) ?? [])]
        .sort(([, a], [, b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([id, file]) => ({
          ref: { type, id, file },
          data: report.records.get(type)?.get(id) ?? {},
        })),
    get: (type, id) => report.records.get(type)?.get(id),
    resolve(fromFile, link) {
      const path = linkPath(link)
      if (path === undefined) return undefined
      return byFile.get(posix.normalize(posix.join(posix.dirname(fromFile), path)))
    },
  }
}
