//! The frontmatter namespace — which prefix, if any, skillwright's keys
//! carry in a note.
//!
//! Keys are bare by default: `version`, `targets`, `codex:`. Which bare keys
//! are skillwright's is a closed list per kind ([`crate::keys`]), so the
//! publish strip and the unknown-key warning read one table. A vault where
//! those words mean something else sets `build.namespace: sw`, and every
//! skillwright key moves under that stem: flat `sw-version`, or a nested
//! `sw:` block. Spec keys are never prefixed.
//!
//! # One stem, two derived forms
//!
//! A prefixed namespace is configured as a **stem** — `sw` — and both spellings the
//! reader uses are derived from it at construction:
//!
//! | Form | From `sw` | Used by |
//! |---|---|---|
//! | flat prefix | `sw-` | every flat key: `sw-version`, `sw-targets`, … |
//! | nested block key | `sw` | the nested shape, `sw:` |
//!
//! They are derived rather than configured separately on purpose. Two
//! independently-set strings are two things that can disagree, and a build
//! whose prefix was `sw-` while its block key was `other` would strip one
//! shape and publish the other — the exact failure the strip exists to prevent.
//! There is no constructor that takes both.
//!
//! # A stem is not free-form
//!
//! [`Namespace::new`] validates, because a stem is not a label: it decides
//! which of the author's keys this crate *deletes* at publish. The rules, and
//! why each one is a rejection rather than a caveat:
//!
//! * **Empty** is not refused: it is the bare namespace, the default.
//! * **Lowercase ASCII letters and digits only, starting with a letter.** The
//!   reader does not case-fold — that is a deliberate property, pinned by its
//!   own test — so a stem carrying an uppercase letter would make `X-kind` ours
//!   and `x-kind` foreign, and the foreign one publishes silently to a
//!   destination. Underscores, dots, spaces and non-ASCII are refused for the
//!   same reason a key that has to survive Obsidian's Properties panel, YAML,
//!   and JSON is kept boring. Starting with a letter is about the *block*
//!   shape: a leading digit makes `1:` a key a YAML reader types as a number,
//!   and [`crate::vault`] refuses frontmatter whose keys are not strings — the
//!   whole note becomes unreadable, not just our block.
//! * **Not a word YAML types as something other than a string** — `y`, `no`,
//!   `on`, `true`, `null` and the rest. Same failure as the leading digit,
//!   arrived at by a different route: under YAML 1.1, which several readers in
//!   this ecosystem still use, `no:` is the boolean key `false`.
//! * **Not a stem that claims one of the Agent Skills spec's six keys.** Our
//!   keys are dropped at publish, so a stem that claims a spec key deletes a
//!   field the standard requires, from every published skill, silently.
//! * **Not a stem that claims a key the reader models and a renderer
//!   publishes** ([`Namespace::PUBLISHED_KEYS`]: `user-invocable`, `model`,
//!   …). Same failure: the key drains as ours, the modeled field falls back
//!   to its default, and `user-invocable: false` publishes as invocable.
//!
//! # Two boundary rules
//!
//! **A stem may not contain a hyphen.** The hyphen is the separator between the
//! namespace and the field, so a stem carrying one moves the boundary somewhere
//! only the configuration knows: `my-vault-kind` is field `vault-kind` under
//! stem `my` and field `kind` under stem `my-vault`, and nothing in the note
//! says which. Worse, the block key of the longer stem (`my-vault:`) is itself
//! a well-formed *flat key* of the shorter one, so two configurations a user
//! might plausibly try in sequence disagree about whether one key is a block or
//! a field.
//!
//! **A stem may be a textual prefix of a spec key.** `desc` is allowed even
//! though `description` starts with it, because the namespace never claims
//! `description` — it claims `desc-…` and `desc:`, neither of which is a spec
//! key. What is rejected is a stem that claims one, and the only way to do that
//! is for the boundary to fall on a hyphen: `allowed` claims `allowed-tools`,
//! and `metadata` claims `metadata` itself as its block. Rejecting every
//! textual prefix instead would ban `n`, `na`, `nam`, `l`, `li`, … for nothing.

use std::fmt;

/// The frontmatter namespace skillwright claims in a note: one validated stem,
/// and both spellings the reader derives from it.
///
/// Construct with [`Namespace::new`], or take [`Namespace::default`] for the
/// bare namespace. This is a value, passed to the
/// functions that need it — there is deliberately no global, no static, and no
/// thread-local holding "the" namespace, for the same reason
/// [`crate::vfs::Vfs`] is a parameter: a core that reads its configuration from
/// an ambient cannot be given two of them, and cannot be tested with either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    /// The configured stem, and the nested block key. Empty when bare.
    stem: String,
    /// `stem` + `-`, computed once in [`Namespace::new`]; empty when bare. Stored rather than
    /// formatted per call because [`Namespace::is_ours`] runs on every key of
    /// every note; derived rather than configured so it cannot drift from the
    /// stem.
    prefix: String,
}

impl Namespace {
    /// The stem `build.namespace: sw` names: the conventional prefixed form.
    pub const SW: &'static str = "sw";

    /// The Agent Skills spec's six allowed top-level keys, sorted.
    ///
    /// A stem may not claim one — see the module documentation. Sorted so a
    /// refusal that lists them is stable.
    pub const SPEC_KEYS: &'static [&'static str] = &[
        "allowed-tools",
        "compatibility",
        "description",
        "license",
        "metadata",
        "name",
    ];

    /// The top-level keys outside the spec that a renderer reads by name,
    /// sorted: the ones [`crate::vault`] lifts into modeled fields, and the
    /// agent's `tools` and `temperature`, which [`crate::projection`] reads
    /// from the carried keys. A stem may not claim one either: the namespace
    /// drain runs first, so a claimed key never reaches its field.
    pub const PUBLISHED_KEYS: &'static [&'static str] = &[
        "argument-hint",
        "arguments",
        "context",
        "disable-model-invocation",
        "model",
        "temperature",
        "tools",
        "user-invocable",
    ];

    /// Words a YAML reader may type as something other than a string. A stem
    /// from this list would make the nested block a non-string key, which
    /// [`crate::vault`] refuses — taking the whole note with it.
    ///
    /// The YAML 1.1 set rather than 1.2 core: this crate's own parser is the
    /// narrower one, but the note is edited by Obsidian and read by whatever
    /// else the author points at it, and the stem has to survive all of them.
    const YAML_SCALARS: &'static [&'static str] =
        &["false", "n", "no", "null", "off", "on", "true", "y", "yes"];

    /// Validates `stem` and derives the namespace from it.
    ///
    /// # Errors
    ///
    /// [`InvalidStem`], naming the stem and the reason. See the module
    /// documentation for what each rule is protecting.
    pub fn new(stem: &str) -> Result<Self, InvalidStem> {
        if stem.is_empty() {
            return Ok(Self::bare());
        }
        let reject = |reason| {
            Err(InvalidStem {
                stem: stem.to_owned(),
                reason,
            })
        };

        // Before the character rule, because the advice is different: a
        // hyphenated stem is well-formed and still refused.
        if stem.contains('-') {
            return reject(StemProblem::Hyphenated);
        }
        if !stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return reject(StemProblem::NotLowercaseAlphanumeric);
        }
        if !stem.starts_with(|c: char| c.is_ascii_lowercase()) {
            return reject(StemProblem::DoesNotStartWithLetter);
        }
        if Self::YAML_SCALARS.contains(&stem) {
            return reject(StemProblem::ReadsAsAYamlScalar);
        }
        // Claiming, not resembling: the stem *is* a spec key (so its block
        // would be one), or a spec key is one of its flat keys.
        let claims = |key: &&&str| {
            **key == stem || key.strip_prefix(stem).is_some_and(|r| r.starts_with('-'))
        };
        if let Some(key) = Self::SPEC_KEYS.iter().find(claims) {
            return reject(StemProblem::ClaimsSpecKey { key });
        }
        if let Some(key) = Self::PUBLISHED_KEYS.iter().find(claims) {
            return reject(StemProblem::ClaimsPublishedKey { key });
        }

        Ok(Self {
            prefix: format!("{stem}-"),
            stem: stem.to_owned(),
        })
    }

    /// The bare namespace: skillwright's keys carry no prefix.
    pub fn bare() -> Self {
        Self {
            stem: String::new(),
            prefix: String::new(),
        }
    }

    /// The conventional prefixed namespace, `sw`.
    pub fn sw() -> Self {
        Self::new(Self::SW).expect("`sw` is a valid stem")
    }

    /// True when skillwright's keys carry no prefix.
    pub fn is_bare(&self) -> bool {
        self.stem.is_empty()
    }

    /// The configured stem, as given. Empty when bare.
    pub fn stem(&self) -> &str {
        &self.stem
    }

    /// The prefix on every flat key we own — `sw-` for the `sw` stem, empty
    /// when bare.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The key holding the nested form of the same fields — `sw` for the
    /// default stem.
    ///
    /// The stem itself, which is why it never collides with the flat scan: no
    /// stem ends in the separator, so the block key never starts with the
    /// prefix.
    pub fn block_key(&self) -> &str {
        &self.stem
    }

    /// The flat spelling of one of our fields: `key("version")` is
    /// `sw-version` under `sw`, and `version` when bare.
    ///
    /// The only way anything in this crate names one of our keys. A literal
    /// `"sw-kind"` written anywhere would be a key that stops being ours the
    /// moment a vault configures a stem, and [`Namespace::is_ours`] would
    /// disagree with the code that reads it — so every field name goes through
    /// here, and every key it returns satisfies `is_ours` by construction.
    pub fn key(&self, field: &str) -> String {
        format!("{}{field}", self.prefix)
    }

    /// The field a flat spelling names: [`Self::key`] undone. `sw-version` is
    /// `version` under `sw`, and any other key is itself.
    pub fn field<'k>(&self, key: &'k str) -> &'k str {
        key.strip_prefix(self.prefix.as_str()).unwrap_or(key)
    }

    /// True for any key a prefixed namespace claims, in either shape.
    ///
    /// The bare namespace claims no key by its spelling: which bare keys are
    /// skillwright's is the key table's answer ([`crate::keys`]), so this is
    /// false for every key when bare.
    ///
    /// Not case-folded. `SW-KIND` is a key we did not claim, and treating it as
    /// ours would delete a key the author typed on the strength of a guess.
    pub fn is_ours(&self, key: &str) -> bool {
        !self.is_bare() && (key == self.block_key() || key.starts_with(&self.prefix))
    }
}

impl Default for Namespace {
    /// The bare namespace.
    fn default() -> Self {
        Self::bare()
    }
}

impl fmt::Display for Namespace {
    /// The prefix, which is the form a message about a key wants.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.prefix)
    }
}

/// A configured stem that cannot be a frontmatter namespace.
///
/// A configuration error rather than a note refusal — no note is involved, and
/// nothing has been read yet — so it is a type of its own rather than a
/// [`crate::CoreError`] variant. A caller wiring this to a config file decides
/// how loudly its own layer fails; what it gets from here is the offending stem
/// and the reason, both addressable rather than only printable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStem {
    /// The stem as it was configured.
    pub stem: String,
    /// Why it was refused.
    pub reason: StemProblem,
}

impl fmt::Display for InvalidStem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` cannot be a frontmatter namespace: {}",
            self.stem, self.reason
        )
    }
}

impl std::error::Error for InvalidStem {}

/// Why a stem was refused. One variant per rule, so a caller can act on the
/// reason rather than match on prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StemProblem {
    /// The stem contains the separator between the namespace and the field.
    Hyphenated,
    /// Something outside lowercase ASCII letters and digits.
    NotLowercaseAlphanumeric,
    /// Starts with a digit, so the nested block key is a number to a YAML
    /// reader.
    DoesNotStartWithLetter,
    /// A word YAML types as a boolean or null, with the same consequence.
    ReadsAsAYamlScalar,
    /// The namespace would own one of the Agent Skills spec's six keys — and
    /// therefore delete it at publish.
    ClaimsSpecKey {
        /// The Agent Skills spec key it would claim.
        key: &'static str,
    },
    /// The namespace would own a key outside the spec that the reader models
    /// and a renderer publishes ([`Namespace::PUBLISHED_KEYS`]) — and so drop
    /// it at publish.
    ClaimsPublishedKey {
        /// The published key it would claim.
        key: &'static str,
    },
}

impl fmt::Display for StemProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hyphenated => f.write_str(
                "`-` separates the namespace from the field, so the stem may not contain one — \
                 `my-vault-kind` would be field `vault-kind` under stem `my` and field `kind` \
                 under stem `my-vault`, and the note does not say which",
            ),
            Self::NotLowercaseAlphanumeric => f.write_str(
                "only lowercase ASCII letters and digits are allowed. The reader does not \
                 case-fold, so an uppercase letter would leave the lowercase spelling foreign — \
                 and a foreign key of ours publishes to a destination",
            ),
            Self::DoesNotStartWithLetter => f.write_str(
                "a stem must start with a lowercase letter: one starting with a digit makes the \
                 nested block a key a YAML reader types as a number, and frontmatter whose keys \
                 are not strings is refused whole",
            ),
            Self::ReadsAsAYamlScalar => f.write_str(
                "YAML types this word as a boolean or null rather than a string, so the nested \
                 block key would not survive being read back",
            ),
            Self::ClaimsSpecKey { key } => write!(
                f,
                "it would claim `{key}`, one of the Agent Skills spec's six keys ({}) — and our \
                 keys are dropped at publish, so every published skill would lose it",
                Namespace::SPEC_KEYS.join(", ")
            ),
            Self::ClaimsPublishedKey { key } => write!(
                f,
                "it would claim `{key}`, a key skillwright reads and publishes ({}) — and our \
                 keys are dropped at publish, so the author's value would be lost",
                Namespace::PUBLISHED_KEYS.join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(stem: &str) -> InvalidStem {
        Namespace::new(stem).unwrap_err()
    }

    #[test]
    fn the_default_is_bare_and_claims_nothing_by_spelling() {
        let ns = Namespace::default();
        assert!(ns.is_bare());
        assert_eq!(ns.prefix(), "");
        assert_eq!(ns.key("version"), "version");
        assert!(!ns.is_ours("version"));
        assert!(!ns.is_ours("sw-kind"));
        assert_eq!(Namespace::new("").unwrap(), ns);
    }

    #[test]
    fn sw_behaves_as_the_constants_did() {
        let ns = Namespace::sw();
        assert_eq!(ns.stem(), "sw");
        assert_eq!(ns.prefix(), "sw-");
        assert_eq!(ns.block_key(), "sw");
        assert_eq!(ns.key("kind"), "sw-kind");
        assert_eq!(ns.to_string(), "sw-");

        assert!(ns.is_ours("sw-kind"));
        assert!(ns.is_ours("sw-anything-at-all"));
        assert!(ns.is_ours("sw"));
        assert!(!ns.is_ours("tcx"));
        assert!(!ns.is_ours("tcs-kind"));
        assert!(!ns.is_ours("kind"));
        assert!(!ns.is_ours("TC-KIND"), "the prefix is not case-folded");
    }

    #[test]
    fn a_custom_stem_derives_its_own_two_forms() {
        let ns = Namespace::new("x").unwrap();
        assert_eq!(ns.stem(), "x");
        assert_eq!(ns.prefix(), "x-");
        assert_eq!(ns.block_key(), "x");
        assert_eq!(ns.key("kind"), "x-kind");

        assert!(ns.is_ours("x-kind"));
        assert!(ns.is_ours("x"));
        // And the default is nobody's business in this vault.
        assert!(!ns.is_ours("sw-kind"));
        assert!(!ns.is_ours("sw"));
    }

    #[test]
    fn the_two_forms_cannot_drift_from_the_stem() {
        // The property the single-stem constructor exists to guarantee: both
        // spellings are the stem, and every key we ever name is inside the
        // predicate.
        for stem in ["sw", "x", "skillwright", "sw2", "vault"] {
            let ns = Namespace::new(stem).unwrap();
            assert_eq!(ns.prefix(), format!("{stem}-"));
            assert_eq!(ns.block_key(), stem);
            assert!(ns.is_ours(ns.block_key()));
            for field in ["kind", "targets", "source-hash", "invented-last-week"] {
                let key = ns.key(field);
                assert!(key.starts_with(ns.prefix()), "{key}");
                assert!(ns.is_ours(&key), "{key}");
            }
            // No stem ends in the separator, so the block key is never itself
            // a flat key — the one collision the two shapes could have had.
            assert!(!ns.block_key().starts_with(ns.prefix()));
        }
    }

    #[test]
    fn a_hyphenated_stem_is_refused() {
        // Including the hyphen at either end, which is the same ambiguity.
        for stem in ["my-vault", "-sw", "sw-", "a-b-c"] {
            assert_eq!(rejected(stem).reason, StemProblem::Hyphenated, "{stem}");
        }
    }

    #[test]
    fn a_stem_outside_lowercase_alphanumerics_is_refused() {
        for stem in [
            "SW", "Sw", "sW", "sw_kind", "sw kind", "sw.kind", "sw:", "æp", "sw/x",
        ] {
            assert_eq!(
                rejected(stem).reason,
                StemProblem::NotLowercaseAlphanumeric,
                "{stem}"
            );
        }
    }

    #[test]
    fn a_stem_that_does_not_start_with_a_letter_is_refused() {
        // A digit is a valid character *inside* a stem and not at the front:
        // `1:` is a number to a YAML reader, and a note whose frontmatter has a
        // non-string key is refused whole.
        for stem in ["1", "2x", "0ap"] {
            assert_eq!(
                rejected(stem).reason,
                StemProblem::DoesNotStartWithLetter,
                "{stem}"
            );
        }
        assert!(Namespace::new("sw2").is_ok(), "a digit inside is fine");
    }

    #[test]
    fn a_stem_yaml_would_type_as_a_boolean_or_null_is_refused() {
        for stem in ["y", "n", "yes", "no", "on", "off", "true", "false", "null"] {
            assert_eq!(
                rejected(stem).reason,
                StemProblem::ReadsAsAYamlScalar,
                "{stem}"
            );
        }
    }

    #[test]
    fn a_stem_that_claims_a_spec_key_is_refused_naming_it() {
        // Every one of the six, as its own block key.
        for key in Namespace::SPEC_KEYS {
            if key.contains('-') {
                // `allowed-tools` is refused one rule earlier, as a hyphenated
                // stem. It is covered below by the stem that *does* claim it.
                assert_eq!(rejected(key).reason, StemProblem::Hyphenated, "{key}");
                continue;
            }
            assert_eq!(
                rejected(key).reason,
                StemProblem::ClaimsSpecKey { key },
                "{key}"
            );
        }

        // And the interesting one: a stem whose flat prefix swallows a spec
        // key. `allowed-` claims `allowed-tools`.
        let err = rejected("allowed");
        assert_eq!(
            err.reason,
            StemProblem::ClaimsSpecKey {
                key: "allowed-tools"
            }
        );
        assert!(err.to_string().contains("allowed-tools"), "{err}");
    }

    /// The read lifts these keys into modeled fields and a renderer emits
    /// them, so a stem that claims one deletes a field the author wrote for
    /// publication — `user` would drain `user-invocable: false` and publish
    /// the skill as invocable.
    #[test]
    fn a_stem_that_claims_a_published_key_is_refused_naming_it() {
        for (stem, key) in [
            ("user", "user-invocable"),
            ("disable", "disable-model-invocation"),
            ("argument", "argument-hint"),
            ("arguments", "arguments"),
            ("model", "model"),
            ("context", "context"),
            ("tools", "tools"),
            ("temperature", "temperature"),
        ] {
            let err = rejected(stem);
            assert_eq!(
                err.reason,
                StemProblem::ClaimsPublishedKey { key },
                "{stem}"
            );
            assert!(err.to_string().contains(key), "{err}");
        }
    }

    #[test]
    fn a_stem_that_is_merely_a_textual_prefix_of_a_spec_key_is_allowed() {
        // The boundary has to fall on a hyphen for a claim to exist. `desc-…`
        // is not `description`, so nothing of the standard's is at risk.
        for stem in ["desc", "nam", "meta", "lic", "compat"] {
            let ns = Namespace::new(stem).unwrap_or_else(|e| panic!("{stem}: {e}"));
            for key in Namespace::SPEC_KEYS {
                assert!(!ns.is_ours(key), "{stem} claimed {key}");
            }
        }
    }

    #[test]
    fn every_refusal_names_the_stem_and_says_why() {
        // The message is what a user configuring this actually sees, so it has
        // to carry both halves for every rule.
        for stem in ["my-vault", "SW", "1", "no", "metadata"] {
            let err = rejected(stem);
            let message = err.to_string();
            assert_eq!(err.stem, stem);
            assert!(message.contains(&format!("`{stem}`")), "{message}");
            assert!(
                message.contains(&err.reason.to_string()),
                "the reason is missing from the message: {message}"
            );
            assert!(
                message.len() > stem.len() + 40,
                "the reason is not prose a user can act on: {message}"
            );
        }
    }
}
