//! The compiled harness knowledge file, embedded, and the lookup over it.
//!
//! `harness-reference` compiles its claims corpus into `harness-knowledge.json`: one value per
//! harness, subject and scope, each with a strength and a permalink to the statement that sets
//! it. A copy lives at `data/harness-knowledge.json`, is read once on first use, and is the only
//! thing this crate reads of that corpus (D-QXRN). [`lookup`] mirrors
//! `harness-reference/src/compile/lookup.ts`; the two are held to the same cases.
//!
//! The copy holds only the harnesses and subjects in `data/embedded-slice.json`, written compact
//! and without `valueSchema`. A new lookup adds its subject to that list and reruns
//! `compile:embed`; a test fails until it does.
//!
//! The copy is compiled with the placeholder commit `main`, so it is the same on every checkout;
//! a `harness-reference` test fails when it is stale.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

const EMBEDDED: &str = include_str!("../../data/harness-knowledge.json");

/// The knowledge file.
#[derive(Debug, Clone, Deserialize)]
pub struct Knowledge {
    /// The file's schema version.
    pub schema: u32,
    /// The corpus commit the file was compiled from.
    pub corpus: Corpus,
    /// Harnesses by slug.
    pub harnesses: BTreeMap<String, HarnessInfo>,
    /// Subjects by slug.
    pub subjects: BTreeMap<String, SubjectInfo>,
    /// Every value, ordered by harness, subject and scope.
    pub values: Vec<KnowledgeValue>,
}

/// Where the corpus lives.
#[derive(Debug, Clone, Deserialize)]
pub struct Corpus {
    /// The repository, `owner/name`.
    pub repo: String,
    /// The corpus folder in the repository.
    pub path: String,
    /// The commit the permalinks name.
    pub commit: String,
}

/// How a harness numbers its releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Versioning {
    /// Semantic Versioning 2.0.0.
    Semver,
    /// `YYYY-MM-DD`.
    Date,
    /// No usable version order; a version in a query is ignored.
    Unknown,
}

/// One harness.
#[derive(Debug, Clone, Deserialize)]
pub struct HarnessInfo {
    /// The harness's name.
    pub title: String,
    /// How it numbers releases.
    pub versioning: Versioning,
    /// Its command, when it has one.
    #[serde(default)]
    pub cli: Option<String>,
}

/// One subject.
#[derive(Debug, Clone, Deserialize)]
pub struct SubjectInfo {
    /// The question's name.
    pub title: String,
    /// The category it sits in.
    pub category: String,
}

/// How well the evidence supports a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    /// Issues, third parties, inference.
    Weak,
    /// Documentation and changelogs.
    Moderate,
    /// Source, help text and runs.
    Strong,
}

/// The versions, platforms and model a value holds for.
#[derive(Debug, Clone, Deserialize)]
pub struct Scope {
    /// First version it holds for, inclusive.
    pub since: Option<String>,
    /// First version it no longer holds for, exclusive.
    pub until: Option<String>,
    /// Platforms it holds on; empty is all.
    pub platform: Vec<String>,
    /// The model it holds for; `None` is all.
    pub model: Option<String>,
}

/// One value for one harness and subject.
#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeValue {
    /// The harness slug.
    pub harness: String,
    /// The subject slug.
    pub subject: String,
    /// The value, in the shape the subject's schema describes.
    pub value: Value,
    /// Where it holds.
    pub scope: Scope,
    /// How well it is supported.
    pub strength: Strength,
    /// A permalink to the statement that sets it.
    pub basis: String,
    /// True when study results disagree.
    #[serde(default)]
    pub disputed: bool,
}

/// What a lookup asks for. The default asks for the open-ended value, on any platform and model,
/// and leaves out weak and disputed values.
#[derive(Debug, Clone, Copy, Default)]
pub struct Query<'a> {
    /// The harness version, in its own spelling. Without it only values with no `until` count.
    pub version: Option<&'a str>,
    /// The platform.
    pub platform: Option<&'a str>,
    /// The model.
    pub model: Option<&'a str>,
    /// Count weak values.
    pub accept_weak: bool,
    /// Count disputed values.
    pub accept_disputed: bool,
}

/// The value a lookup found.
#[derive(Debug, Clone, Copy)]
pub struct Found<'k> {
    /// The value.
    pub value: &'k Value,
    /// Its strength.
    pub strength: Strength,
    /// The permalink to its statement.
    pub basis: &'k str,
    /// Whether it is disputed.
    pub disputed: bool,
}

/// Why a lookup could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    /// Two values fit the query equally well, so the file cannot say which to use.
    #[error("{harness} {subject}: {} values fit equally well: {}", .candidates.len(), .candidates.join(", "))]
    Ambiguous {
        /// The harness slug.
        harness: String,
        /// The subject slug.
        subject: String,
        /// The `basis` of each candidate.
        candidates: Vec<String>,
    },
    /// The query's version is not spelled the way the harness's versioning says.
    #[error("{harness}: \"{version}\" is not a {versioning} version")]
    BadVersion {
        /// The harness slug.
        harness: String,
        /// The version asked for.
        version: String,
        /// The spelling it should have had.
        versioning: &'static str,
    },
}

/// The embedded file, parsed on first use.
///
/// # Panics
///
/// If the embedded copy does not parse, which a test rules out.
pub fn embedded() -> &'static Knowledge {
    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();
    KNOWLEDGE.get_or_init(|| {
        serde_json::from_str(EMBEDDED).expect("data/harness-knowledge.json is the compiled file")
    })
}

/// [`lookup_in`] over the embedded file.
///
/// # Errors
///
/// As [`lookup_in`].
pub fn lookup(
    harness: &str,
    subject: &str,
    query: Query<'_>,
) -> Result<Option<Found<'static>>, LookupError> {
    lookup_in(embedded(), harness, subject, query)
}

/// How narrow a scope is: a platform list and a model each count one.
fn specificity(v: &KnowledgeValue) -> usize {
    usize::from(!v.scope.platform.is_empty()) + usize::from(v.scope.model.is_some())
}

/// The value for `harness` and `subject` that fits `query`, or `None`.
///
/// - Version: `since` is inclusive and `until` exclusive. Semver compares by precedence, dates
///   as strings. With no version only values with `until: null` count. A value with no version
///   limit fits every version. A version is checked against the harness's versioning; for
///   `unknown` versioning it is ignored.
/// - Platform and model: an empty platform list or no model fits any query. A listed platform
///   or a model fits only when the query names it. Candidates are ranked by how many omitted
///   dimensions they narrow first, then by how narrow the supplied ones are.
/// - Two equally narrow candidates are [`LookupError::Ambiguous`]; a disputed one loses to an
///   undisputed one.
/// - Weak and disputed values are skipped unless the query accepts them.
///
/// # Errors
///
/// [`LookupError::BadVersion`] for a malformed version and [`LookupError::Ambiguous`] for a tie.
pub fn lookup_in<'k>(
    knowledge: &'k Knowledge,
    harness: &str,
    subject: &str,
    query: Query<'_>,
) -> Result<Option<Found<'k>>, LookupError> {
    let versioning = knowledge
        .harnesses
        .get(harness)
        .map_or(Versioning::Unknown, |h| h.versioning);
    if let Some(version) = query.version
        && !valid_version(version, versioning)
        && versioning != Versioning::Unknown
    {
        return Err(LookupError::BadVersion {
            harness: harness.to_owned(),
            version: version.to_owned(),
            versioning: match versioning {
                Versioning::Semver => "semver",
                _ => "date",
            },
        });
    }
    let mut candidates: Vec<&KnowledgeValue> = knowledge
        .values
        .iter()
        .filter(|v| v.harness == harness && v.subject == subject)
        .filter(|v| v.strength != Strength::Weak || query.accept_weak)
        .filter(|v| !v.disputed || query.accept_disputed)
        .filter(|v| match query.version {
            None => v.scope.until.is_none(),
            Some(_) if versioning == Versioning::Unknown => true,
            Some(version) => in_range(version, &v.scope, versioning),
        })
        .filter(|v| {
            (v.scope.platform.is_empty()
                || query
                    .platform
                    .is_none_or(|p| v.scope.platform.iter().any(|x| x == p)))
                && (v.scope.model.is_none()
                    || query
                        .model
                        .is_none_or(|m| v.scope.model.as_deref() == Some(m)))
        })
        .collect();
    let narrow_on_omitted = |v: &KnowledgeValue| {
        usize::from(query.platform.is_none() && !v.scope.platform.is_empty())
            + usize::from(query.model.is_none() && v.scope.model.is_some())
    };
    let Some(fewest) = candidates.iter().map(|v| narrow_on_omitted(v)).min() else {
        return Ok(None);
    };
    candidates.retain(|v| narrow_on_omitted(v) == fewest);
    let best = candidates.iter().map(|v| specificity(v)).max().unwrap_or(0);
    candidates.retain(|v| specificity(v) == best);
    if candidates.iter().any(|v| !v.disputed) {
        candidates.retain(|v| !v.disputed);
    }
    if candidates.len() > 1 {
        return Err(LookupError::Ambiguous {
            harness: harness.to_owned(),
            subject: subject.to_owned(),
            candidates: candidates.iter().map(|v| v.basis.clone()).collect(),
        });
    }
    Ok(candidates.first().map(|v| Found {
        value: &v.value,
        strength: v.strength,
        basis: &v.basis,
        disputed: v.disputed,
    }))
}

fn in_range(version: &str, scope: &Scope, versioning: Versioning) -> bool {
    scope
        .since
        .as_deref()
        .is_none_or(|s| compare(version, s, versioning) != Ordering::Less)
        && scope
            .until
            .as_deref()
            .is_none_or(|u| compare(version, u, versioning) == Ordering::Less)
}

fn compare(a: &str, b: &str, versioning: Versioning) -> Ordering {
    match versioning {
        Versioning::Semver => compare_semver(a, b),
        _ => a.cmp(b),
    }
}

fn valid_version(v: &str, versioning: Versioning) -> bool {
    match versioning {
        Versioning::Semver => valid_semver(v),
        Versioning::Date => valid_date(v),
        Versioning::Unknown => false,
    }
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn valid_number(s: &str) -> bool {
    is_digits(s) && (s == "0" || !s.starts_with('0'))
}

/// Semantic Versioning 2.0.0: no leading zeros in numbers, no empty identifiers.
fn valid_semver(v: &str) -> bool {
    let (rest, build) = v.split_once('+').map_or((v, None), |(r, b)| (r, Some(b)));
    let (core, pre) = rest
        .split_once('-')
        .map_or((rest, None), |(c, p)| (c, Some(p)));
    let ident = |s: &str| {
        !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && (!is_digits(s) || valid_number(s))
    };
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| valid_number(p))
        && pre.is_none_or(|p| p.split('.').all(ident))
        && build.is_none_or(|b| {
            b.split('.')
                .all(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        })
}

/// A real calendar date, spelled `YYYY-MM-DD`.
fn valid_date(v: &str) -> bool {
    let parts: Vec<&str> = v.split('-').collect();
    let [y, m, d] = parts[..] else {
        return false;
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 || ![y, m, d].iter().all(|p| is_digits(p)) {
        return false;
    }
    let (y, m, d): (u32, u32, u32) = (
        y.parse().unwrap_or(0),
        m.parse().unwrap_or(0),
        d.parse().unwrap_or(0),
    );
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// Compare two digit strings by value without losing precision.
fn compare_digits(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Semver 2.0.0 precedence; build metadata is ignored.
fn compare_semver(a: &str, b: &str) -> Ordering {
    let split = |v: &str| -> (Vec<String>, Vec<String>) {
        let v = v.split('+').next().unwrap_or("");
        let (core, pre) = v.split_once('-').map_or((v, ""), |(c, p)| (c, p));
        (
            core.split('.').map(str::to_owned).collect(),
            if pre.is_empty() {
                Vec::new()
            } else {
                pre.split('.').map(str::to_owned).collect()
            },
        )
    };
    let ((ca, pa), (cb, pb)) = (split(a), split(b));
    for i in 0..3 {
        let x = ca.get(i).map_or("0", String::as_str);
        let y = cb.get(i).map_or("0", String::as_str);
        let d = compare_digits(x, y);
        if d != Ordering::Equal {
            return d;
        }
    }
    if pa.is_empty() || pb.is_empty() {
        return pb.len().min(1).cmp(&pa.len().min(1));
    }
    for i in 0..pa.len().max(pb.len()) {
        let (Some(x), Some(y)) = (pa.get(i), pb.get(i)) else {
            return pa.len().cmp(&pb.len());
        };
        if x == y {
            continue;
        }
        let d = match (is_digits(x), is_digits(y)) {
            (true, true) => compare_digits(x, y),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.cmp(y),
        };
        return d;
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope(
        since: Option<&str>,
        until: Option<&str>,
        platform: &[&str],
        model: Option<&str>,
    ) -> Scope {
        Scope {
            since: since.map(str::to_owned),
            until: until.map(str::to_owned),
            platform: platform.iter().map(|p| (*p).to_owned()).collect(),
            model: model.map(str::to_owned),
        }
    }

    /// A value named `name`, broad on everything; adjust the scope with the builders below.
    fn val(name: &str) -> KnowledgeValue {
        KnowledgeValue {
            harness: "h".into(),
            subject: "c/s".into(),
            value: json!(name),
            scope: scope(None, None, &[], None),
            strength: Strength::Strong,
            basis: format!("https://github.com/x/y/blob/sha/{name}"),
            disputed: false,
        }
    }
    fn since(mut v: KnowledgeValue, s: &str) -> KnowledgeValue {
        v.scope.since = Some(s.into());
        v
    }
    fn until(mut v: KnowledgeValue, s: &str) -> KnowledgeValue {
        v.scope.until = Some(s.into());
        v
    }
    fn on(mut v: KnowledgeValue, platform: &str) -> KnowledgeValue {
        v.scope.platform = vec![platform.into()];
        v
    }
    fn model(mut v: KnowledgeValue, m: &str) -> KnowledgeValue {
        v.scope.model = Some(m.into());
        v
    }

    fn k(values: Vec<KnowledgeValue>, versioning: Versioning) -> Knowledge {
        Knowledge {
            schema: 1,
            corpus: Corpus {
                repo: "x/y".into(),
                path: "p".into(),
                commit: "sha".into(),
            },
            harnesses: BTreeMap::from([(
                "h".to_owned(),
                HarnessInfo {
                    title: "H".into(),
                    versioning,
                    cli: None,
                },
            )]),
            subjects: BTreeMap::new(),
            values,
        }
    }
    fn semver(values: Vec<KnowledgeValue>) -> Knowledge {
        k(values, Versioning::Semver)
    }
    fn pick(kn: &Knowledge, q: Query<'_>) -> Option<String> {
        lookup_in(kn, "h", "c/s", q)
            .unwrap()
            .map(|f| f.value.as_str().unwrap().to_owned())
    }
    fn at(version: &str) -> Query<'_> {
        Query {
            version: Some(version),
            ..Query::default()
        }
    }

    #[test]
    fn since_is_inclusive_and_until_is_exclusive() {
        let kn = semver(vec![until(val("old"), "2.0.0"), since(val("new"), "2.0.0")]);
        assert_eq!(pick(&kn, at("1.9.9")).as_deref(), Some("old"));
        assert_eq!(pick(&kn, at("2.0.0")).as_deref(), Some("new"));
        assert_eq!(pick(&kn, at("10.0.0")).as_deref(), Some("new"));
    }

    #[test]
    fn semver_compares_numbers_and_prereleases_not_strings() {
        let kn = semver(vec![until(val("old"), "2.0.0"), since(val("new"), "2.0.0")]);
        assert_eq!(pick(&kn, at("2.0.0-rc.1")).as_deref(), Some("old"));
        let a = semver(vec![since(val("a"), "1.9.0")]);
        assert_eq!(pick(&a, at("1.10.0")).as_deref(), Some("a"));
        let b = semver(vec![until(val("a"), "1.10.0")]);
        assert_eq!(pick(&b, at("1.9.0")).as_deref(), Some("a"));
    }

    #[test]
    fn semver_precedence_follows_the_spec() {
        let order = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for pair in order.windows(2) {
            assert_eq!(compare_semver(pair[0], pair[1]), Ordering::Less, "{pair:?}");
            assert_eq!(compare_semver(pair[1], pair[0]), Ordering::Greater);
        }
        assert_eq!(compare_semver("1.0.0+a", "1.0.0+b"), Ordering::Equal);
    }

    #[test]
    fn with_no_version_only_open_ended_values_count() {
        let kn = semver(vec![until(val("old"), "2.0.0"), since(val("new"), "2.0.0")]);
        assert_eq!(pick(&kn, Query::default()).as_deref(), Some("new"));
        let only_old = semver(vec![until(val("old"), "2.0.0")]);
        assert_eq!(pick(&only_old, Query::default()), None);
    }

    #[test]
    fn dates_compare_as_strings() {
        let d = k(
            vec![until(val("a"), "2026-03-01"), since(val("b"), "2026-03-01")],
            Versioning::Date,
        );
        assert_eq!(pick(&d, at("2026-02-28")).as_deref(), Some("a"));
        assert_eq!(pick(&d, at("2026-03-01")).as_deref(), Some("b"));
    }

    #[test]
    fn no_limits_fits_every_version_and_unknown_versioning_ignores_the_version() {
        assert_eq!(
            pick(&semver(vec![val("any")]), at("3.1.4")).as_deref(),
            Some("any")
        );
        let unknown = k(vec![val("any")], Versioning::Unknown);
        assert_eq!(pick(&unknown, at("whatever")).as_deref(), Some("any"));
    }

    #[test]
    fn nothing_for_another_harness_or_subject() {
        let kn = semver(vec![val("a")]);
        assert!(
            lookup_in(&kn, "other", "c/s", Query::default())
                .unwrap()
                .is_none()
        );
        assert!(
            lookup_in(&kn, "h", "c/other", Query::default())
                .unwrap()
                .is_none()
        );
    }

    fn scoped() -> Knowledge {
        semver(vec![
            val("any"),
            on(val("mac"), "macos"),
            model(val("opus"), "opus"),
            model(on(val("mac opus"), "macos"), "opus"),
        ])
    }

    #[test]
    fn the_narrowest_matching_scope_wins() {
        let kn = scoped();
        let q = |platform, model| Query {
            platform,
            model,
            ..Query::default()
        };
        assert_eq!(
            pick(&kn, q(Some("macos"), Some("opus"))).as_deref(),
            Some("mac opus")
        );
        assert_eq!(pick(&kn, q(Some("macos"), None)).as_deref(), Some("mac"));
        assert_eq!(pick(&kn, q(None, Some("opus"))).as_deref(), Some("opus"));
        assert_eq!(
            pick(&kn, q(Some("linux"), Some("haiku"))).as_deref(),
            Some("any")
        );
    }

    #[test]
    fn with_no_platform_or_model_asked_the_broad_value_wins() {
        assert_eq!(pick(&scoped(), Query::default()).as_deref(), Some("any"));
    }

    #[test]
    fn a_specific_value_is_used_when_it_is_the_only_one() {
        let kn = semver(vec![on(val("mac"), "macos")]);
        assert_eq!(pick(&kn, Query::default()).as_deref(), Some("mac"));
        let linux = Query {
            platform: Some("linux"),
            ..Query::default()
        };
        assert_eq!(pick(&kn, linux), None);
    }

    #[test]
    fn a_value_broad_on_an_omitted_dimension_beats_a_narrower_one() {
        let kn = semver(vec![
            model(val("opus"), "opus"),
            model(on(val("mac opus"), "macos"), "opus"),
        ]);
        assert_eq!(pick(&kn, Query::default()).as_deref(), Some("opus"));
        let opus = Query {
            model: Some("opus"),
            ..Query::default()
        };
        assert_eq!(pick(&kn, opus).as_deref(), Some("opus"));
        let both = Query {
            model: Some("opus"),
            platform: Some("macos"),
            ..Query::default()
        };
        assert_eq!(pick(&kn, both).as_deref(), Some("mac opus"));
    }

    #[test]
    fn omitted_dimensions_are_ranked_apart_from_supplied_ones() {
        let kn = semver(vec![
            on(val("mac"), "macos"),
            model(val("opus"), "opus"),
            model(on(val("mac opus"), "macos"), "opus"),
        ]);
        let mac = Query {
            platform: Some("macos"),
            ..Query::default()
        };
        assert_eq!(pick(&kn, mac).as_deref(), Some("mac"));
        let opus = Query {
            model: Some("opus"),
            ..Query::default()
        };
        assert_eq!(pick(&kn, opus).as_deref(), Some("opus"));
        let both = semver(vec![
            on(val("mac"), "macos"),
            model(on(val("mac opus"), "macos"), "opus"),
        ]);
        assert_eq!(pick(&both, Query::default()).as_deref(), Some("mac"));
    }

    #[test]
    fn a_malformed_version_is_an_error_not_a_match_on_everything() {
        let kn = semver(vec![until(val("a"), "2.0.0")]);
        for bad in ["garbage", "1.0.0-01"] {
            let err = lookup_in(&kn, "h", "c/s", at(bad)).unwrap_err();
            assert!(err.to_string().contains("not a semver version"), "{err}");
        }
        let dates = k(vec![val("a")], Versioning::Date);
        let err = lookup_in(&dates, "h", "c/s", at("2026-02-30")).unwrap_err();
        assert!(err.to_string().contains("not a date version"), "{err}");
    }

    #[test]
    fn weak_and_disputed_values_need_accepting() {
        let mut weak = val("w");
        weak.strength = Strength::Weak;
        let kn = semver(vec![weak]);
        assert!(
            lookup_in(&kn, "h", "c/s", Query::default())
                .unwrap()
                .is_none()
        );
        let accept = Query {
            accept_weak: true,
            ..Query::default()
        };
        let found = lookup_in(&kn, "h", "c/s", accept).unwrap().unwrap();
        assert_eq!(found.strength, Strength::Weak);

        let mut disputed = val("d");
        disputed.disputed = true;
        let kn = semver(vec![disputed]);
        assert!(
            lookup_in(&kn, "h", "c/s", Query::default())
                .unwrap()
                .is_none()
        );
        let accept = Query {
            accept_disputed: true,
            ..Query::default()
        };
        assert!(
            lookup_in(&kn, "h", "c/s", accept)
                .unwrap()
                .unwrap()
                .disputed
        );
    }

    #[test]
    fn the_found_value_carries_strength_basis_and_disputed() {
        let mut v = val("x");
        v.value = json!({"a": 1});
        let kn = semver(vec![v]);
        let found = lookup_in(&kn, "h", "c/s", Query::default())
            .unwrap()
            .unwrap();
        assert_eq!(found.value, &json!({"a": 1}));
        assert_eq!(found.strength, Strength::Strong);
        assert_eq!(found.basis, "https://github.com/x/y/blob/sha/x");
        assert!(!found.disputed);
    }

    #[test]
    fn an_undisputed_value_beats_a_disputed_one_of_the_same_scope() {
        let mut d = val("d");
        d.disputed = true;
        let kn = semver(vec![d, val("ok")]);
        let accept = Query {
            accept_disputed: true,
            ..Query::default()
        };
        assert_eq!(pick(&kn, accept).as_deref(), Some("ok"));
    }

    #[test]
    fn two_equally_narrow_values_are_ambiguous() {
        let kn = semver(vec![val("a"), val("b")]);
        match lookup_in(&kn, "h", "c/s", Query::default()) {
            Err(LookupError::Ambiguous { candidates, .. }) => assert_eq!(candidates.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    /// `harness-reference/src/beta/version-cases.json`, which the TypeScript tests read too.
    fn shared_cases() -> Value {
        serde_json::from_str(include_str!(
            "../../../../../harness-reference/src/beta/version-cases.json"
        ))
        .unwrap()
    }

    #[test]
    fn version_validity_matches_the_shared_cases() {
        for case in shared_cases()["valid"].as_array().unwrap() {
            let [versioning, version, ok] = &case.as_array().unwrap()[..] else {
                panic!("{case}");
            };
            let versioning = match versioning.as_str().unwrap() {
                "semver" => Versioning::Semver,
                "date" => Versioning::Date,
                _ => Versioning::Unknown,
            };
            let version = version.as_str().unwrap();
            assert_eq!(
                valid_version(version, versioning),
                ok.as_bool().unwrap(),
                "{version}"
            );
        }
    }

    #[test]
    fn semver_order_matches_the_shared_cases() {
        for case in shared_cases()["semverOrder"].as_array().unwrap() {
            let [a, b, order] = &case.as_array().unwrap()[..] else {
                panic!("{case}");
            };
            let expected = order.as_i64().unwrap().cmp(&0);
            assert_eq!(
                compare_semver(a.as_str().unwrap(), b.as_str().unwrap()),
                expected,
                "{a} {b}"
            );
        }
    }

    #[derive(Deserialize)]
    struct Slice {
        harnesses: Vec<String>,
        subjects: Vec<String>,
    }

    fn slice() -> Slice {
        serde_json::from_str(include_str!("../../data/embedded-slice.json")).unwrap()
    }

    #[test]
    fn the_embedded_file_holds_exactly_the_slice() {
        let (kn, slice) = (embedded(), slice());
        let mut subjects: Vec<&String> = kn.subjects.keys().collect();
        subjects.sort();
        assert_eq!(subjects, slice.subjects.iter().collect::<Vec<_>>());
        let mut harnesses: Vec<&String> = kn.harnesses.keys().collect();
        harnesses.sort();
        assert_eq!(harnesses, slice.harnesses.iter().collect::<Vec<_>>());
    }

    #[test]
    fn every_subject_the_profiles_look_up_is_in_the_slice() {
        // Subjects are the `area/name` string literals in profile.rs.
        let source = include_str!("profile.rs");
        let mut found = std::collections::BTreeSet::new();
        for literal in source.split('"').skip(1).step_by(2) {
            let is_subject = literal.split_once('/').is_some_and(|(a, b)| {
                let word = |w: &str| {
                    !w.is_empty() && w.bytes().all(|c| c.is_ascii_lowercase() || c == b'-')
                };
                word(a) && word(b)
            });
            if is_subject {
                found.insert(literal.to_owned());
            }
        }
        assert!(found.contains("skills/user-dirs"), "the scan found nothing");
        let slice = slice();
        for subject in &found {
            assert!(
                slice.subjects.contains(subject),
                "{subject} is looked up in profile.rs but not in data/embedded-slice.json"
            );
        }
    }

    #[test]
    fn every_profile_harness_with_a_lookup_is_in_the_slice() {
        let slice = slice();
        for profile in crate::harness::profile::with_plugin_format() {
            let slug = crate::harness::profile::knowledge_slug(profile.name);
            assert!(slice.harnesses.iter().any(|h| h == slug), "{slug}");
        }
    }

    #[test]
    fn the_embedded_file_parses_and_names_its_corpus() {
        let kn = embedded();
        assert_eq!(kn.schema, 1);
        assert_eq!(kn.corpus.repo, "osdlc/skillwright");
        assert!(!kn.values.is_empty());
        for v in &kn.values {
            assert!(kn.harnesses.contains_key(&v.harness), "{}", v.harness);
            assert!(kn.subjects.contains_key(&v.subject), "{}", v.subject);
        }
    }
}
