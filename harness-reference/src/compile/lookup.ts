/**
 * `lookup`: the value a harness has for a subject, from a compiled `harness-knowledge.json`.
 */
import { inRange } from '../beta/versions.ts'
import type { Knowledge, KnowledgeValue, Strength } from './knowledge.ts'

export interface LookupQuery {
  /** The harness version, in the harness's own spelling. Without it, the open-ended value wins. */
  version?: string
  platform?: string
  model?: string
  /** Weak and disputed values are left out unless accepted. */
  accept?: { weak?: boolean; disputed?: boolean }
}

export interface Found {
  value: unknown
  strength: Strength
  basis: string
  disputed: boolean
}

/** Two values fit the query equally well, so the file cannot say which one to use. */
export class AmbiguousValue extends Error {
  readonly candidates: readonly KnowledgeValue[]
  constructor(harness: string, subject: string, candidates: readonly KnowledgeValue[]) {
    super(
      `${harness} ${subject}: ${candidates.length} values fit equally well: ${candidates.map((c) => c.basis).join(', ')}`,
    )
    this.name = 'AmbiguousValue'
    this.candidates = candidates
  }
}

/** How narrow a scope is: a platform list and a model each count one. */
const specificity = (v: KnowledgeValue): number =>
  (v.scope.platform.length > 0 ? 1 : 0) + (v.scope.model !== null ? 1 : 0)

/**
 * The value for `harness` and `subject` that fits `query`, or undefined.
 *
 * - Version: `since` is inclusive and `until` exclusive. Semver compares by precedence, dates as
 *   strings. With no version, only values with `until: null` count. A value with no version limit
 *   fits every version.
 * - Platform and model: an empty platform list or a null model fits any query. A listed platform
 *   or a model fits only when the query names it. When the query names none, such a value is
 *   used only if nothing broader remains.
 * - The narrowest scope wins. Two equally narrow candidates throw {@link AmbiguousValue}; a
 *   disputed one loses to an undisputed one.
 * - Weak and disputed values are skipped unless `query.accept` allows them.
 */
export function lookup(
  knowledge: Knowledge,
  harness: string,
  subject: string,
  query: LookupQuery = {},
): Found | undefined {
  const versioning = knowledge.harnesses[harness]?.versioning ?? 'unknown'
  let candidates = knowledge.values.filter((v) => {
    if (v.harness !== harness || v.subject !== subject) return false
    if (v.strength === 'weak' && query.accept?.weak !== true) return false
    if (v.disputed === true && query.accept?.disputed !== true) return false
    if (query.version === undefined) return v.scope.until === null
    if (versioning === 'unknown') return true
    return inRange(query.version, v.scope, versioning)
  })

  const fits = (v: KnowledgeValue, strict: boolean): boolean => {
    const platformOk =
      v.scope.platform.length === 0 ||
      (query.platform === undefined ? !strict : v.scope.platform.includes(query.platform))
    const modelOk =
      v.scope.model === null ||
      (query.model === undefined ? !strict : v.scope.model === query.model)
    return platformOk && modelOk
  }
  let fitting = candidates.filter((v) => fits(v, true))
  if (fitting.length === 0) fitting = candidates.filter((v) => fits(v, false))
  candidates = fitting
  if (candidates.length === 0) return undefined

  const best = Math.max(...candidates.map(specificity))
  let top = candidates.filter((v) => specificity(v) === best)
  if (top.some((v) => v.disputed !== true)) top = top.filter((v) => v.disputed !== true)
  if (top.length > 1) throw new AmbiguousValue(harness, subject, top)
  const v = top[0] as KnowledgeValue
  return { value: v.value, strength: v.strength, basis: v.basis, disputed: v.disputed === true }
}
