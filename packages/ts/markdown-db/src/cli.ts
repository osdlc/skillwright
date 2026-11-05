#!/usr/bin/env node
/**
 * markdown-db command line (docs/standards/S-GFFY-markdown-db.md). Version 0 defines `validate`.
 *
 *   markdown-db validate [--fix] <folder>...
 *
 * Validates every corpus at or below each folder. Exits 1 on any error, 2 on bad usage.
 */
import { existsSync, statSync } from 'node:fs'
import { relative, resolve } from 'node:path'
import { findCorpora, validateCorpus } from './validate.ts'

const args = process.argv.slice(2)
const fix = args.includes('--fix')
const [verb, ...dirs] = args.filter((a) => a !== '--fix')
if (verb !== 'validate' || dirs.length === 0) {
  process.stderr.write('usage: markdown-db validate [--fix] <folder>...\n')
  process.exit(2)
}

let failed = false
const roots = new Set<string>()
for (const dir of dirs) {
  const base = resolve(dir)
  const found = existsSync(base) && statSync(base).isDirectory() ? findCorpora(base) : []
  if (found.length === 0) {
    process.stderr.write(`${dir}: no markdown-db root file at or below it\n`)
    failed = true
  }
  for (const root of found) roots.add(root)
}

const ids = new Map<string, string>()
for (const root of roots) {
  const where = relative(process.cwd(), root) || '.'
  const report = validateCorpus(root, { fix })
  if (roots.size > 1) process.stderr.write(`${where}:\n`)
  for (const f of report.fixed) process.stdout.write(`fixed: ${f}\n`)
  for (const w of report.warnings) process.stderr.write(`warning: ${w}\n`)
  for (const e of report.errors) process.stderr.write(`${e}\n`)
  if (report.id !== undefined) {
    const other = ids.get(report.id)
    if (other !== undefined)
      process.stderr.write(`warning: ${where}: id: "${report.id}" is also ${other}'s id\n`)
    ids.set(report.id, where)
  }
  if (report.errors.length > 0) {
    failed = true
    process.stderr.write(`${report.name}: ${report.errors.length} error(s)\n`)
    continue
  }
  const counts = [...report.records].map(([type, m]) => `${m.size} ${type}`).join(', ')
  process.stdout.write(`ok: ${report.name}: ${counts === '' ? 'no types' : counts}\n`)
}
process.exit(failed ? 1 : 0)
