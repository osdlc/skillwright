// node scripts/harness-knowledge-slice.ts          (pnpm run harness-knowledge:slice)
//
// Writes packages/rust/tradecraft-core/data/harness-knowledge.json, the copy of the compiled
// harness knowledge that tradecraft-core embeds.
//
// The source is the `@osdlc/harness-knowledge` npm package at the version the root package.json
// pins. Its data file is compiled in osdlc/osdlc from the harness reference
// (packages/corpora/harness-reference). The copy keeps the harnesses and subjects that
// data/embedded-slice.json lists, drops each subject's `valueSchema`, and is written compact.
//
// To take newer data: raise the version in package.json, run `pnpm install`, then this script.
// To embed another subject: add it to embedded-slice.json, then run this script.
import { readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'

const DATA = join(import.meta.dirname, '../packages/rust/tradecraft-core/data')
export const EMBEDDED = join(DATA, 'harness-knowledge.json')
export const SLICE = join(DATA, 'embedded-slice.json')

interface Knowledge {
  harnesses: Record<string, unknown>
  subjects: Record<string, Record<string, unknown>>
  values: { harness: string; subject: string }[]
}

/** The harnesses and subjects the crate reads. */
export interface Slice {
  harnesses: string[]
  subjects: string[]
}

/** The compiled file the pinned `@osdlc/harness-knowledge` ships. */
export function published(): Knowledge {
  const file = createRequire(import.meta.url).resolve(
    '@osdlc/harness-knowledge/data/harness-knowledge.json',
  )
  return JSON.parse(readFileSync(file, 'utf8')) as Knowledge
}

/** `knowledge` cut to `slice`, without `valueSchema`. Every other field is kept as it is. */
export function sliceKnowledge(knowledge: Knowledge, slice: Slice): Knowledge {
  const missing = [
    ...slice.harnesses.filter((h) => !(h in knowledge.harnesses)),
    ...slice.subjects.filter((s) => !(s in knowledge.subjects)),
  ]
  if (missing.length > 0) throw new Error(`not in the knowledge file: ${missing.join(', ')}`)
  const pick = <T>(o: Record<string, T>, keep: string[]): [string, T][] =>
    Object.entries(o).filter(([k]) => keep.includes(k))
  return {
    ...knowledge,
    harnesses: Object.fromEntries(pick(knowledge.harnesses, slice.harnesses)),
    subjects: Object.fromEntries(
      pick(knowledge.subjects, slice.subjects).map(([k, { valueSchema: _, ...rest }]) => [k, rest]),
    ),
    values: knowledge.values.filter(
      (v) => slice.harnesses.includes(v.harness) && slice.subjects.includes(v.subject),
    ),
  }
}

/** The text of the embedded copy, as the pinned package and embedded-slice.json give it. */
export function embedded(): string {
  const slice = JSON.parse(readFileSync(SLICE, 'utf8')) as Slice
  return `${JSON.stringify(sliceKnowledge(published(), slice))}\n`
}

if (import.meta.main) {
  writeFileSync(EMBEDDED, embedded())
  console.log(`wrote ${EMBEDDED}`)
}
