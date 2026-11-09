/** Harness version spellings and how they order. */

export type Versioning = 'semver' | 'date' | 'unknown'

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u
const DATE = /^\d{4}-\d{2}-\d{2}$/u

/** Whether `v` is spelled the way `versioning` says. `unknown` allows no version at all. */
export function validVersion(v: string, versioning: Versioning): boolean {
  if (versioning === 'semver') return SEMVER.test(v)
  if (versioning === 'date') return DATE.test(v)
  return false
}

/** Semver 2.0.0 precedence (build metadata ignored): -1, 0 or 1. */
export function compareSemver(a: string, b: string): number {
  const split = (v: string): [number[], string[]] => {
    const [core = '', pre] = (v.split('+')[0] ?? '').split(/-(.*)/su, 2)
    return [core.split('.').map(Number), pre === undefined ? [] : pre.split('.')]
  }
  const [ca, pa] = split(a)
  const [cb, pb] = split(b)
  for (let i = 0; i < 3; i++) {
    const d = (ca[i] ?? 0) - (cb[i] ?? 0)
    if (d !== 0) return Math.sign(d)
  }
  if (pa.length === 0 || pb.length === 0) return Math.sign(pb.length - pa.length)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i]
    const y = pb[i]
    if (x === undefined) return -1
    if (y === undefined) return 1
    if (x === y) continue
    const nx = /^\d+$/u.test(x)
    const ny = /^\d+$/u.test(y)
    if (nx && ny) return Math.sign(Number(x) - Number(y))
    if (nx) return -1
    if (ny) return 1
    return x < y ? -1 : 1
  }
  return 0
}

/** Order two versions of one harness. Dates compare as strings; `unknown` has no order. */
export function compareVersions(a: string, b: string, versioning: Versioning): number {
  if (versioning === 'semver') return compareSemver(a, b)
  return a < b ? -1 : a > b ? 1 : 0
}

/** A scope's version range: `since` inclusive, `until` exclusive, null for no limit. */
export interface VersionRange {
  since: string | null
  until: string | null
}

/** Whether `version` lies in `[since, until)`. */
export function inRange(version: string, r: VersionRange, versioning: Versioning): boolean {
  if (r.since !== null && compareVersions(version, r.since, versioning) < 0) return false
  if (r.until !== null && compareVersions(version, r.until, versioning) >= 0) return false
  return true
}

/** Whether two ranges share a version. */
export function overlaps(a: VersionRange, b: VersionRange, versioning: Versioning): boolean {
  const before = (x: string | null, y: string | null): boolean =>
    x === null || y === null || compareVersions(x, y, versioning) < 0
  return before(a.since, b.until) && before(b.since, a.until)
}
