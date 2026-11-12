//! Every refusal token: the reason a [`crate::CoreError::Refused`] carries,
//! beside its message.
//!
//! A token is stable and machine-readable. A host matches on it; the message
//! is prose for a person. [`Token`] can be built only here, so this list is
//! every token the crate and its hosts refuse with.

use std::fmt;

/// A refusal's reason token, such as `unsafe-path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token(&'static str);

impl Token {
    /// The token as it is written in a message and on the wire.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// A report's reason, such as [`crate::SkippedNote::reason`], against a token.
impl PartialEq<Token> for String {
    fn eq(&self, token: &Token) -> bool {
        self == token.0
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A load of a vault with several marketplaces names none.
pub const AMBIGUOUS_MARKETPLACE: Token = Token("ambiguous-marketplace");
/// A note or a hooks file writes `${CLAUDE_PLUGIN_ROOT}`, which is Claude Code's own.
pub const CLAUDE_PLUGIN_ROOT: Token = Token("claude-plugin-root");
/// `compare_with` on a build, or on a request for several marketplaces.
pub const COMPARE_WITH: Token = Token("compare-with");
/// Embeds that embed each other.
pub const CYCLE: Token = Token("cycle");
/// An embed of a file that is not text.
pub const EMBED_BINARY: Token = Token("embed-binary");
/// An embed with a fragment where none is allowed.
pub const EMBED_FRAGMENT: Token = Token("embed-fragment");
/// An embed of a note whose frontmatter cannot be dropped.
pub const EMBED_FRONTMATTER: Token = Token("embed-frontmatter");
/// A marketplace with no plugin to build.
pub const EMPTY_MARKETPLACE: Token = Token("empty-marketplace");
/// A link into a folder that never ships.
pub const EXCLUDED_LINK: Token = Token("excluded-link");
/// An owned path in `dist/<m>/` holds work this vault did not build.
pub const FOREIGN_TREE: Token = Token("foreign-tree");
/// A link's heading or block fragment names nothing in its target.
pub const FRAGMENT_UNRESOLVED: Token = Token("fragment-unresolved");
/// A note's frontmatter cannot take the `version` edit.
pub const FRONTMATTER_EDIT: Token = Token("frontmatter-edit");
/// An edited frontmatter is not UTF-8.
pub const FRONTMATTER_ENCODING: Token = Token("frontmatter-encoding");
/// A note's frontmatter cannot be read for its `version`.
pub const FRONTMATTER_READ: Token = Token("frontmatter-read");
/// A command's `arguments` list breaks a rule of its shape.
pub const INVALID_ARGUMENTS: Token = Token("invalid-arguments");
/// The filesystem a wasm host passed is not an object.
pub const INVALID_VFS: Token = Token("invalid-vfs");
/// The root is not a vault root, or the vault breaks the layout.
pub const LAYOUT: Token = Token("layout");
/// A link whose target matches more than one file.
pub const LINK_AMBIGUOUS: Token = Token("link-ambiguous");
/// A link to a component the pack does not ship.
pub const LINK_NOT_SHIPPED: Token = Token("link-not-shipped");
/// A link to a file outside its scope: another plugin, or another marketplace.
pub const LINK_OUT_OF_SCOPE: Token = Token("link-out-of-scope");
/// A link or embed whose target is not found.
pub const LINK_UNRESOLVED: Token = Token("link-unresolved");
/// A hand-written skill call, which is right for one harness only.
pub const LITERAL_SKILL_CALL: Token = Token("literal-skill-call");
/// A note that parsed holds a value of the wrong kind.
pub const MALFORMED: Token = Token("malformed");
/// A plugin's `hooks/hooks.json` cannot be read.
pub const MALFORMED_HOOKS_FILE: Token = Token("malformed-hooks-file");
/// The root file is not UTF-8 or cannot be parsed.
pub const MALFORMED_ROOT_FILE: Token = Token("malformed-root-file");
/// A skill, command or agent with no `description`. A scan lists it as a warning.
pub const MISSING_DESCRIPTION: Token = Token("missing-description");
/// A runtime renders no files of the kind asked for.
pub const NO_RENDERER: Token = Token("no-renderer");
/// A note holds a Unicode noncharacter the build uses internally.
pub const NONCHARACTER: Token = Token("noncharacter");
/// Two notes or files resolve onto one path.
pub const PATH_COLLISION: Token = Token("path-collision");
/// A plugin-root path that names no file.
pub const PLUGIN_ROOT_NO_PATH: Token = Token("plugin-root-no-path");
/// A plugin-root path naming a file the pack does not ship.
pub const PLUGIN_ROOT_NOT_SHIPPED: Token = Token("plugin-root-not-shipped");
/// A prefixed skill name that breaks the name rule.
pub const PUBLISHED_NAME_INVALID: Token = Token("published-name-invalid");
/// A call to two or more skills of one name.
pub const SKILL_CALL_AMBIGUOUS: Token = Token("skill-call-ambiguous");
/// A call with a fragment: a call cannot target a section.
pub const SKILL_CALL_FRAGMENT: Token = Token("skill-call-fragment");
/// A skill call the harness cannot spell.
pub const SKILL_CALL_INVALID: Token = Token("skill-call-invalid");
/// A call to a skill the pack does not ship.
pub const SKILL_CALL_NOT_SHIPPED: Token = Token("skill-call-not-shipped");
/// A call that names no skill or command in its marketplace.
pub const SKILL_CALL_UNRESOLVED: Token = Token("skill-call-unresolved");
/// A harness name that no harness answers to.
pub const UNKNOWN_HARNESS: Token = Token("unknown-harness");
/// A hooks file names an event the harness has no name for.
pub const UNKNOWN_HOOK_EVENT: Token = Token("unknown-hook-event");
/// A hook command holds a placeholder other than `{{harness}}` and `{{event}}`.
pub const UNKNOWN_HOOK_PLACEHOLDER: Token = Token("unknown-hook-placeholder");
/// A marketplace the vault does not hold.
pub const UNKNOWN_MARKETPLACE: Token = Token("unknown-marketplace");
/// A plugin's `naming` is not `bare` or `prefixed`.
pub const UNKNOWN_NAMING: Token = Token("unknown-naming");
/// A target name the wasm binding does not know.
pub const UNKNOWN_TARGET: Token = Token("unknown-target");
/// A note that cannot be read: not UTF-8, frontmatter that will not parse, or
/// a value the Agent Skills spec does not admit.
pub const UNREADABLE_NOTE: Token = Token("unreadable-note");
/// Link syntax left in a compiled file, or a link in frontmatter.
pub const UNRESOLVED_SYNTAX: Token = Token("unresolved-syntax");
/// A path that is absolute, climbs out of its root, or cannot be written safely.
pub const UNSAFE_PATH: Token = Token("unsafe-path");

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Appends every `.rs` file under `dir` but this one to `text`.
    fn sources(dir: &Path, text: &mut String) {
        for path in std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()) {
            if path.is_dir() {
                sources(&path, text);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("refusal.rs") {
                text.push_str(&std::fs::read_to_string(&path).unwrap());
            }
        }
    }

    #[test]
    fn every_token_is_documented_used_and_distinct() {
        // This crate and the wasm binding are the crates that refuse.
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut used = String::new();
        sources(&crates.join("tradecraft-core/src"), &mut used);
        sources(&crates.join("tradecraft-wasm/src"), &mut used);
        let lines: Vec<&str> = include_str!("refusal.rs").lines().collect();
        let mut tokens = std::collections::BTreeSet::new();
        for (at, line) in lines.iter().enumerate() {
            let Some((name, token)) = line
                .strip_prefix("pub const ")
                .and_then(|rest| rest.split_once(": Token = Token("))
            else {
                continue;
            };
            assert!(
                lines[at - 1].starts_with("///"),
                "`{name}` has no doc comment"
            );
            assert!(
                used.contains(&format!("refusal::{name}")),
                "`{name}` is never used"
            );
            assert!(tokens.insert(token), "`{name}` repeats {token}");
        }
        assert!(tokens.len() > 40, "{tokens:?}");
    }
}
