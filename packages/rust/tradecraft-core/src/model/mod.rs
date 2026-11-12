//! The marketplace model — a vault loaded once, harness-neutral, for every
//! verb to translate (D-COMG).
//!
//! The vault is read into one object graph, marketplace → plugins → skills,
//! commands, the hooks file and assets, and the translator writes that graph
//! out for each harness. What a note means is decided once; how a harness
//! spells it is the translator's.
//!
//! # Two passes, then validation
//!
//! [`load()`] reads each note once and resolves before translating (D-EPSA),
//! today in two passes:
//!
//! 1. **Read** every note into a node — [`crate::discover::discover_in`], over the
//!    layout [`crate::layout::read_layout`] walked once.
//! 2. **Resolve** every body: embeds expanded, every `[[link]]` and
//!    `/[[call]]` checked and pointed at the node it names, and every
//!    `${PLUGIN_ROOT}/<path>` recorded. A body becomes [`Segment`]s, so a
//!    translator writes each without re-reading the note it is in.
//!
//! Validation runs between and after the passes and carries every refusal the
//! two pipelines raised, token and message unchanged.
//!
//! # What stays harness-dependent
//!
//! A call is checked once — that it names one skill or command and has no
//! fragment — but whether a pack *ships* what it names is a
//! question about a harness, so [`Call`] carries what the translator needs
//! to ask it. A node's harness-only fields ([`Extensions`], D-ISOU) are
//! folded on load and checked for the harnesses a load is for.
//!
//! A load resolves only what some requested harness admits. A command when
//! no harness built carries commands is not compiled, and a fault in it must
//! not stop a build that does not include it.
//!
//! # Embeds are expanded on load
//!
//! An embed's bytes become the body's text, the same for every harness, so
//! the model keeps the expanded text, not a segment.

pub mod cascade;
pub(crate) mod config;
pub mod load;
pub mod vault;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::compile::{MARK_CLOSE, MARK_OPEN, Recorded};
use crate::content::SkillContent;
use crate::discover::{DiscoveredComponent, DiscoveryWarning};
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::harness::Harness;
use crate::layout::Position;
use crate::refusal;
use config::MarketplaceConfig;

pub use load::{LoadRequest, load, resolve_body};

/// A vault, loaded as the one marketplace a build renders.
#[derive(Debug, Clone)]
pub struct Marketplace {
    /// The name the marketplace publishes under.
    pub name: String,
    /// The vault's marketplace folder this was loaded from.
    pub folder: String,
    /// The marketplace's identity and every plugin's manifest fields, the
    /// vault's notes folded over the root file's shared defaults.
    pub(crate) config: MarketplaceConfig,
    /// Every plugin, by name.
    pub plugins: BTreeMap<String, Plugin>,
    /// Components no pack has a place for, such as agents — named, never
    /// dropped. In path order.
    pub uncarried: Vec<Component>,
    /// What the scan and the resolve pass filled in or noticed, in path order.
    pub warnings: Vec<DiscoveryWarning>,
    /// How many skills the marketplace holds.
    pub checked: usize,
    /// The vault this was loaded from.
    pub vault_root: PathBuf,
}

/// One plugin and everything it carries.
#[derive(Debug, Clone, Default)]
pub struct Plugin {
    /// The plugin's name.
    pub name: String,
    /// Its skills, in discovery's name order.
    pub skills: Vec<Component>,
    /// Its commands.
    pub commands: Vec<Component>,
    /// Files copied verbatim into the built pack.
    pub assets: Vec<Asset>,
    /// The Markdown in its shared `references/`, compiled, each at its path
    /// from the pack root.
    pub references: Vec<Resource>,
    /// The plugin's `hooks/hooks.json`, parsed.
    pub hooks_file: Option<HooksFile>,
    /// The harnesses that name this plugin's skills bare against their
    /// profile's default: `<harness>: {naming: bare}` where the default
    /// prefixes.
    pub bare_on: Vec<Harness>,
}

impl Plugin {
    /// Every component of every kind, skills first.
    pub fn components(&self) -> impl Iterator<Item = &Component> {
        self.skills.iter().chain(&self.commands)
    }

    /// Whether a harness, admitting what `admits` admits, carries this
    /// plugin: it admits one of its components, or the plugin has none (an
    /// agent is not one), which every harness built carries.
    pub fn carried_by(&self, admits: impl FnMut(&Component) -> bool) -> bool {
        let mut components = self.components().peekable();
        components.peek().is_none() || components.any(admits)
    }
}

/// A skill, a command, an agent or a container note, read and — when
/// a requested harness admits it — resolved.
#[derive(Debug, Clone)]
pub struct Component {
    /// Its harness-only fields, by harness.
    pub extensions: Extensions,
    /// The note read in full, its body and files resolved. `None` when no
    /// harness this load is for admits the component.
    pub resolved: Option<Resolved>,
    /// What the scan found.
    pub discovered: DiscoveredComponent,
}

impl Component {
    /// What it is.
    pub fn kind(&self) -> EntityKind {
        self.discovered.position.kind()
    }

    /// Its source name, never prefixed.
    pub fn name(&self) -> &str {
        self.discovered.position.name()
    }

    /// The note, vault-relative.
    pub fn path(&self) -> &str {
        &self.discovered.vault_relative_path
    }
}

/// A read note: its content, and its body and the files it reaches with
/// every link and call pointed at its node.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Name, description, the modeled fields and the annotations every
    /// harness receives, with `arguments` applied for a skill or a command.
    /// The body is the note's own; the resolved one is [`Self::body`].
    pub content: SkillContent,
    /// The entry's body.
    pub body: Body,
    /// Every file that travels with it, by output path.
    pub resources: Vec<Resource>,
}

/// One file a component carries.
#[derive(Debug, Clone)]
pub struct Resource {
    /// Where it lands, relative to the component's directory.
    pub path: String,
    /// Its content.
    pub content: ResourceContent,
    /// Whether it is executable.
    pub executable: bool,
}

/// A resource's content: a compiled markdown file, or bytes as found.
#[derive(Debug, Clone)]
pub enum ResourceContent {
    /// A markdown file, whose links and calls a translator writes.
    Body {
        /// The file, frontmatter and all.
        body: Body,
    },
    /// Anything else, copied byte for byte.
    Bytes {
        /// The bytes.
        bytes: Vec<u8>,
    },
}

/// A body as a sequence of segments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Body {
    /// The segments, in order.
    pub segments: Vec<Segment>,
}

/// One piece of a resolved body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Text, written as is.
    Text {
        /// The text.
        text: String,
    },
    /// A link to a file that travels with the component.
    Link {
        /// Where it points.
        target: ResolvedRef,
    },
    /// A call to a skill or command. The surrounding text already holds the
    /// backticks the call is written in.
    Call {
        /// The call.
        call: Call,
    },
    /// An embedded file as a fenced block, written as is: nothing in it is
    /// interpreted (spec section 5 rule 9).
    Literal {
        /// The block, fences included.
        text: String,
    },
    /// A `${PLUGIN_ROOT}/<path>`, which each harness writes its own way
    /// (spec section 4 rules 15 to 20).
    PluginRoot {
        /// The path.
        path: PluginRootPath,
    },
}

/// A `${PLUGIN_ROOT}/<path>` in text. Both paths are paths in a pack laid
/// out as Claude Code's, as a [`ResolvedRef`]'s are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRootPath {
    /// The file it names.
    pub path: String,
    /// Where the text holding it lands.
    pub from: String,
    /// The reference as written, for an error.
    pub written: String,
    /// The file it is in, vault-relative, for an error.
    pub caller: String,
}

/// A resolved link: where it is written, the file it reaches, and how it is
/// written. Both paths are paths in a pack laid out as Claude Code's
/// (`skills/<s>/…`, `commands/<c>.md`, shared files at the root); a translator
/// maps them to its own folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRef {
    /// The linked file, vault-relative.
    pub path: String,
    /// The link text, escaped. Empty for a Markdown link's destination.
    pub label: String,
    /// Where the text holding the link lands.
    pub from: String,
    /// Where the linked file lands.
    pub to: String,
    /// What follows the path: a fragment, encoded, with its `#`, or a
    /// Markdown link's own `#…` or `?…` as written. Empty for none.
    pub suffix: String,
    /// How the link is written.
    pub form: LinkForm,
    /// The component whose folder holds the linked file. A pack that does
    /// not contain it cannot hold the link (spec section 4 rule 6).
    pub component: Option<LinkedComponent>,
    /// The link as written, for an error.
    pub written: String,
    /// The file the link is in, vault-relative, for an error.
    pub caller: String,
}

impl ResolvedRef {
    /// The markdown the link is written as, from `from` to `to`.
    pub fn markdown_at(&self, from: &str, to: &str) -> String {
        let relative = crate::harness::translator::relative_path(from, to);
        let encoded: Vec<String> = relative
            .split('/')
            .map(|segment| match segment {
                ".." | "." => segment.to_owned(),
                _ => markdown_link::encode_uri_component(segment),
            })
            .collect();
        let href = format!("{}{}", encoded.join("/"), self.suffix);
        match self.form {
            LinkForm::Link => format!("[{}]({href})", self.label),
            LinkForm::Image => format!("![{}]({href})", self.label),
            LinkForm::Href => href,
        }
    }

    /// The markdown the link is written as in a Claude Code pack.
    pub fn markdown(&self) -> String {
        self.markdown_at(&self.from, &self.to)
    }
}

/// How a link is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkForm {
    /// `[label](href)`.
    Link,
    /// `![label](href)`, for an embedded image.
    Image,
    /// The href alone: a Markdown link's destination, rewritten in place.
    Href,
}

/// A component a link reaches into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedComponent {
    /// Its kind.
    pub kind: EntityKind,
    /// Its source name.
    pub name: String,
}

/// A resolved call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The skill or command it names.
    pub skill: SkillId,
    /// The harnesses that name the skill bare against their profile's
    /// default, as its plugin's [`Plugin::bare_on`].
    pub bare_on: Vec<Harness>,
    /// The call's target as written, without `/[[` and `]]`, for a refusal.
    pub destination: String,
    /// The note the call is in, for a refusal.
    pub caller: String,
}

impl Call {
    /// Refuses the call in a pack that does not ship what it names,
    /// which would send the reader to a command that is not there.
    ///
    /// # Errors
    ///
    /// `refused: skill-call-not-shipped`, naming the file, the call and the
    /// rule (spec section 4 rule 8).
    pub fn check_shipped(&self, shipped: bool, target: &str) -> Result<(), CoreError> {
        if shipped {
            return Ok(());
        }
        Err(CoreError::refused(
            refusal::SKILL_CALL_NOT_SHIPPED,
            format!(
                "{}: `/[[{}]]` calls {} `{}`, which the {target} pack does not contain \
                 (section 4 rule 8)",
                self.caller,
                self.destination,
                self.skill.kind.as_str(),
                self.skill.call_id()
            ),
        ))
    }
}

/// A skill or command as a call names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillId {
    /// The plugin it ships in.
    pub plugin: String,
    /// Its source name.
    pub name: String,
    /// A skill or a command. The call index also holds agents, and a call
    /// to one is refused: no pack contains agents.
    pub kind: EntityKind,
    /// Its entry note.
    #[serde(skip)]
    pub entry_path: PathBuf,
}

impl SkillId {
    /// The component a call to `component` names, `None` for a container.
    pub fn of(component: &DiscoveredComponent) -> Option<Self> {
        match &component.position {
            Position::Component {
                plugin, kind, name, ..
            } => Some(Self {
                plugin: plugin.clone(),
                name: name.clone(),
                kind: *kind,
                entry_path: component.entry_path.clone(),
            }),
            _ => None,
        }
    }

    /// `plugin:name`.
    pub fn call_id(&self) -> String {
        format!("{}:{}", self.plugin, self.name)
    }
}

/// A node's harness-only fields: harness blocks, folded onto canonical
/// harness names (D-ISOU).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extensions {
    /// Each harness's block, as written.
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

/// A file copied verbatim into a pack.
#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    /// Where it lands, relative to the pack's own directory.
    pub dest: String,
    /// Its bytes.
    #[serde(skip)]
    pub bytes: Vec<u8>,
    /// Whether it is executable.
    pub executable: bool,
}

/// A plugin's `hooks/hooks.json`, parsed. See [`crate::hooks_file`].
#[derive(Debug, Clone)]
pub struct HooksFile {
    /// The file, vault-relative, for an error.
    pub path: String,
    /// The hooks it declares.
    pub hooks: Vec<agent_adapt::Hook>,
}

/// Splits a compile's output into segments.
///
/// A mark that does not parse, or names a site the record does not hold, is
/// left as text: the record is this module's own and a stray mark is a bug,
/// but writing it out beats losing the bytes around it.
pub(crate) fn segments(text: &str, record: &Recorded) -> Body {
    let mut out: Vec<Segment> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find(MARK_OPEN) {
        let after = &rest[open + MARK_OPEN.len_utf8()..];
        let parsed = after.find(MARK_CLOSE).and_then(|close| {
            let inner = &after[..close];
            let mut chars = inner.chars();
            let kind = chars.next()?;
            let index: usize = chars.as_str().parse().ok()?;
            let segment = match kind {
                'c' => Segment::Call {
                    call: record.calls.get(index).map(call_of)?,
                },
                'l' => Segment::Link {
                    target: record.links.get(index).cloned()?,
                },
                'v' => Segment::Literal {
                    text: record.literals.get(index).cloned()?,
                },
                'p' => Segment::PluginRoot {
                    path: record.roots.get(index).cloned()?,
                },
                _ => return None,
            };
            Some((segment, close + MARK_CLOSE.len_utf8()))
        });
        match parsed {
            Some((segment, consumed)) => {
                append_text(&mut out, &rest[..open]);
                out.push(segment);
                rest = &after[consumed..];
            }
            None => {
                let end = open + MARK_OPEN.len_utf8();
                append_text(&mut out, &rest[..end]);
                rest = &rest[end..];
            }
        }
    }
    append_text(&mut out, rest);
    Body { segments: out }
}

fn call_of(site: &crate::compile::CallSite) -> Call {
    Call {
        skill: site.skill.clone(),
        bare_on: Vec::new(),
        destination: site.destination.clone(),
        caller: site.caller.clone(),
    }
}

/// Appends `text`, merged into a preceding text segment.
fn append_text(out: &mut Vec<Segment>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Segment::Text { text: last }) = out.last_mut() {
        last.push_str(text);
    } else {
        out.push(Segment::Text {
            text: text.to_owned(),
        });
    }
}

/// Is this component's kind one a marketplace pack has a place for at all?
pub(crate) fn pack_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Skill | EntityKind::Command | EntityKind::Plugin | EntityKind::Marketplace
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::CallSite;

    fn record() -> Recorded {
        Recorded {
            calls: vec![CallSite {
                skill: SkillId {
                    entry_path: PathBuf::from("/v/a.md"),
                    plugin: "p".into(),
                    name: "a".into(),
                    kind: EntityKind::Skill,
                },
                destination: "a".into(),
                caller: "b.md".into(),
            }],
            links: vec![ResolvedRef {
                path: "x.md".into(),
                label: "x".into(),
                from: "skills/b/SKILL.md".into(),
                to: "skills/b/references/x.md".into(),
                suffix: String::new(),
                form: LinkForm::Link,
                component: None,
                written: "[[x]]".into(),
                caller: "b.md".into(),
            }],
            literals: vec!["```\n[[y]]\n```".into()],
            roots: Vec::new(),
        }
    }

    #[test]
    fn a_literal_mark_is_a_literal_segment() {
        let text = format!("a{MARK_OPEN}v0{MARK_CLOSE}b");
        let body = segments(&text, &record());
        assert!(matches!(&body.segments[1], Segment::Literal { text } if text.contains("[[y]]")));
    }

    #[test]
    fn marks_split_into_calls_and_links_and_text_merges() {
        let text = format!("run `{MARK_OPEN}c0{MARK_CLOSE}` then see {MARK_OPEN}l0{MARK_CLOSE}.");
        let body = segments(&text, &record());
        assert_eq!(body.segments.len(), 5);
        assert!(
            matches!(&body.segments[1], Segment::Call { call } if call.skill.call_id() == "p:a")
        );
        assert!(
            matches!(&body.segments[3], Segment::Link { target } if target.markdown() == "[x](references/x.md)")
        );
    }

    #[test]
    fn a_self_link_is_a_link_to_skill_md() {
        use crate::vfs::{MemVfs, Vfs};
        use std::path::Path;
        let vfs = MemVfs::new();
        let path = Path::new("/vault/marketplaces/house/plugins/tools/skills/a/a.md");
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        for note in [
            "/vault/skillwright.md",
            "/vault/marketplaces/house/house.md",
            "/vault/marketplaces/house/plugins/tools/tools.md",
        ] {
            vfs.write_file(Path::new(note), b"---\n---\n").unwrap();
        }
        vfs.write_file(path, b"---\ndescription: A.\n---\n\nPart of [[a]].\n")
            .unwrap();
        let request = LoadRequest::new("/vault");
        let marketplace = load(&vfs, &request).unwrap();
        let skill = &marketplace.plugins["tools"].skills[0];
        let body = &skill.resolved.as_ref().unwrap().body;
        let links: Vec<String> = body
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Link { target } => Some(target.markdown()),
                _ => None,
            })
            .collect();
        assert_eq!(links, vec!["[a](SKILL.md)".to_owned()]);
    }

    #[test]
    fn a_mark_naming_nothing_stays_text() {
        let text = format!("a{MARK_OPEN}c9{MARK_CLOSE}b");
        let body = segments(&text, &record());
        assert_eq!(body.segments, vec![Segment::Text { text: text.clone() }]);
    }
}
