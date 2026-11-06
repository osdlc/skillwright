//! The compile pass — a note's body flattened for publication.
//!
//! Steps 1–3 of the vault spec's five:
//!
//! 1. `![[embed]]` of a markdown note expands to that note's *body* in place,
//!    recursively, with a `#heading` or `#^block` fragment selecting the
//!    addressed slice. A cycle is refused naming the cycle path.
//! 2. Every surviving `[[link]]` becomes a relative markdown link to where the
//!    linked file will land in the published skill directory, and the file is
//!    collected as a resource. Collection is transitive: a collected markdown
//!    file's own body is compiled the same way, and *its* references collected.
//!    A `/[[call]]` instead becomes the target harness's invocation of the
//!    named skill, and copies nothing — see [`CallContext`]. A plain link to
//!    another skill in a skill or command body is read as a call too.
//! 3. The output is validated: any Obsidian syntax remaining in an emitted
//!    markdown file is a refusal, because a destination has no resolver to
//!    give it meaning.
//!
//! [`crate::vault::read_vault_skill`] still reads the body verbatim — the
//! vault's bytes are canonical and a read must not rewrite them. Compilation
//! is a separate pass between the read and the projection:
//! [`compile_projection`] takes the projection the read produced and replaces
//! its body and resources with the compiled forms, so what
//! [`crate::projection::project_skill`] renders is the flattened skill.
//!
//! # Where resources land
//!
//! Three placements, in precedence order, all forward-slashed and relative to
//! the published skill directory:
//!
//! | Source | Output |
//! |---|---|
//! | The entry note itself | `SKILL.md` |
//! | A file under the bundle root | its bundle-relative path, unchanged |
//! | Any other vault file | `references/vault/<vault-relative>` (document extensions) or `assets/vault/<vault-relative>` (everything else) |
//!
//! Keying the vault copies by their full vault-relative path is what makes two
//! linked notes sharing a basename land at two paths instead of refusing —
//! the disambiguation the compile spec promises. A genuine collision (two
//! sources claiming one output path) is still possible across the placements,
//! and it is refused `path-collision` naming the skill, because any winner
//! would silently drop the loser.
//!
//! # The bundle root is a parameter, deliberately
//!
//! [`CompileSource::bundle_root`] takes the value instead of computing it:
//! `Some` seeds every file under it as a resource at its bundle-relative path,
//! `None` collects only what the body reaches through links. A caller whose
//! peers live in the vault-canonical sibling folder passes that folder here —
//! the resource set is *recomputed* from this source, not merged with one
//! computed elsewhere, so a peer folder not named here does not travel.
//!
//! # What comes from the shared crates, and what stays here
//!
//! The links come from `markdown-link`: its scanner finds each `[[link]]` and
//! `![[embed]]` (code by [`CodeRules::SkillExport`], so this pass keeps
//! matching `skill-export`'s renderer; links in HTML comments skipped;
//! `[[#Heading]]` a link to the note it is in), its `LinkIndex::resolve_strict`
//! resolves them, and its `relative_link` and `escape_label` write them back
//! out. `markdown-contract`'s `select` cuts a `#heading` or `#^block` out of an
//! embedded note.
//!
//! What stays here is skillwright's own: `/[[call]]` calls and `\/[[x]]`
//! escapes, read as a layer over `markdown-link`'s links and found in code
//! too; hand-written calls; where resources land; and the validation that no
//! Obsidian syntax survives.

use std::borrow::Cow;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use markdown_link::code::{CodeRules, FenceLine, Fences, Spans};
use markdown_link::{
    AbandonReason, BodyLinkKind, LinkIndex, ResolveError, ScanOptions, parse_inner_self,
    scan_body_with,
};
use sha2::{Digest, Sha256};

use crate::content::SkillProjection;
use crate::discover::DiscoveryWarning;
use crate::error::CoreError;
use crate::guard::within;
use crate::skill::{forward_slashed, read_utf8, split_frontmatter};
use crate::vfs::{Vfs, normalize};

/// Where a compile reads from: the vault that resolves its links, the note
/// being compiled, and (for a bundle-shaped source) the folder whose files
/// ship with the skill. Paths are absolute; `entry_path` and `bundle_root`
/// must lie under `vault_root`.
#[derive(Debug, Clone)]
pub struct CompileSource<'a> {
    /// The root every link resolves against and every `…/vault/<relative>`
    /// output path is derived from.
    pub vault_root: &'a Path,
    /// The note whose body is being compiled. Its own output is `SKILL.md`.
    pub entry_path: &'a Path,
    /// The folder seeded wholesale as bundle-relative resources, when the
    /// source has one. See the module doc for why this is a parameter.
    pub bundle_root: Option<&'a Path>,
    /// The skill's name, used only to say *which* skill a refusal is about.
    pub skill_name: &'a str,
    /// How `/[[x]]` calls render. Without one, every call renders its target
    /// as written, in backticks, and nothing is refused. A target that
    /// resolves by path is recorded in [`RelatedSplit::calls`], skill or not;
    /// one that does not is recorded unresolved, since there is no index for
    /// the by-name fallback. A plain `[[x]]` stays a link.
    pub calls: Option<&'a CallContext<'a>>,
    /// The vault's files, indexed for link resolution, when the caller has
    /// them: a load passes the index of the walk it read the vault with.
    /// `None` walks `vault_root` for them, leaving out tooling folders
    /// (`.obsidian`, `.trash`, `node_modules`).
    pub links: Option<&'a LinkIndex>,
}

/// One skill a call can name: its entry note and who publishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedSkill {
    /// Absolute path of the skill's entry note.
    pub entry_path: PathBuf,
    /// The plugin publishing it, or `None` for a skill published on its own.
    pub plugin: Option<String>,
    /// The source `name`, never prefixed.
    pub name: String,
    /// Whether the destination being compiled for carries this skill. A call
    /// to one it does not carry is refused.
    pub shipped: bool,
}

impl IndexedSkill {
    /// The closure's spelling of this skill: `plugin:name`, or `name`.
    fn call_id(&self) -> String {
        match &self.plugin {
            Some(plugin) => format!("{plugin}:{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Every skill a compile's calls may name, keyed by entry path.
#[derive(Debug, Clone, Default)]
pub struct SkillIndex {
    by_path: BTreeMap<PathBuf, IndexedSkill>,
}

impl SkillIndex {
    /// Builds an index from the given skills.
    pub fn new(skills: impl IntoIterator<Item = IndexedSkill>) -> Self {
        SkillIndex {
            by_path: skills
                .into_iter()
                .map(|mut skill| {
                    skill.entry_path = normalize(&skill.entry_path);
                    (skill.entry_path.clone(), skill)
                })
                .collect(),
        }
    }

    fn by_path(&self, path: &Path) -> Option<&IndexedSkill> {
        self.by_path.get(path)
    }

    /// Every skill whose `name` is exactly `name`.
    fn by_name(&self, name: &str) -> NameMatch<'_> {
        let mut hits: Vec<&IndexedSkill> = self
            .by_path
            .values()
            .filter(|skill| skill.name == name)
            .collect();
        match hits.len() {
            0 => NameMatch::None,
            1 => NameMatch::One(hits.remove(0)),
            _ => NameMatch::Many(hits),
        }
    }

    /// The skill `plugin` publishes as `name`.
    fn qualified(&self, plugin: &str, name: &str) -> Option<&IndexedSkill> {
        self.by_path
            .values()
            .find(|skill| skill.plugin.as_deref() == Some(plugin) && skill.name == name)
    }

    /// Whether any skill names its plugin.
    fn has_plugins(&self) -> bool {
        self.by_path.values().any(|skill| skill.plugin.is_some())
    }

    /// The skill Pi publishes as `prefixed`, `<plugin>-<name>`.
    fn prefixed(&self, prefixed: &str) -> Option<&IndexedSkill> {
        self.by_path.values().find(|skill| {
            skill.plugin.as_deref().is_some_and(|plugin| {
                prefixed
                    .strip_prefix(plugin)
                    .and_then(|rest| rest.strip_prefix('-'))
                    == Some(skill.name.as_str())
            })
        })
    }
}

/// The skills a name names: none, one, or several to choose between.
enum NameMatch<'a> {
    None,
    One(&'a IndexedSkill),
    Many(Vec<&'a IndexedSkill>),
}

/// Where a compile's [`SkillIndex`] comes from: built by the caller, or built
/// on first use, so a body with no call or link never pays for a vault scan.
pub struct SkillSource<'a> {
    ready: Option<&'a SkillIndex>,
    build: Option<&'a dyn Fn() -> Result<SkillIndex, CoreError>>,
    built: OnceCell<SkillIndex>,
    /// A build failed, so [`Self::get_or_none`] does not scan again.
    failed: Cell<bool>,
}

impl<'a> SkillSource<'a> {
    /// A source over an index the caller already built.
    pub fn ready(index: &'a SkillIndex) -> Self {
        SkillSource {
            ready: Some(index),
            build: None,
            built: OnceCell::new(),
            failed: Cell::new(false),
        }
    }

    /// A source that builds its index on first use.
    pub fn lazy(build: &'a dyn Fn() -> Result<SkillIndex, CoreError>) -> Self {
        SkillSource {
            ready: None,
            build: Some(build),
            built: OnceCell::new(),
            failed: Cell::new(false),
        }
    }

    /// The index if it is at hand, without building it.
    fn peek(&self) -> Option<&SkillIndex> {
        self.ready.or_else(|| self.built.get())
    }

    fn get(&self) -> Result<&SkillIndex, CoreError> {
        if let Some(index) = self.ready {
            return Ok(index);
        }
        if let Some(index) = self.built.get() {
            return Ok(index);
        }
        let index = match self.build {
            Some(build) => build().inspect_err(|_| self.failed.set(true))?,
            None => SkillIndex::default(),
        };
        Ok(self.built.get_or_init(|| index))
    }

    /// The index, or `None` when it cannot be built: for a heuristic that has
    /// nothing to say without one.
    fn get_or_none(&self) -> Option<&SkillIndex> {
        if self.failed.get() {
            return None;
        }
        self.get().ok()
    }
}

/// What a compile needs to render skill calls for one target.
pub struct CallContext<'a> {
    /// Where the skills a body may name come from.
    pub skills: SkillSource<'a>,
    /// The call token this target writes for a skill, without backticks.
    pub render: &'a dyn Fn(&IndexedSkill) -> Result<String, CoreError>,
    /// The destination's name, for a refusal about what it does not ship.
    pub target: &'a str,
    /// The body is a skill's or a command's, where a plain `[[x]]` to a skill
    /// is a call too.
    pub implicit: bool,
    /// `implicit-skill-call` warnings, deduplicated.
    pub warnings: RefCell<Vec<DiscoveryWarning>>,
    /// A compile done once for every harness: each call and each link is
    /// written as a mark and its site recorded here, for a translator to
    /// spell later. `render` is not called and nothing is refused for not
    /// being shipped — which harness ships what is the translator's question.
    /// See [`crate::model`].
    pub record: Option<&'a RefCell<Recorded>>,
}

/// The calls and links a recording compile met, in the order it met them.
/// The `n`th mark of each kind in the output names the `n`th entry here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recorded {
    /// Every call, resolved to the skill it names.
    pub calls: Vec<CallSite>,
    /// Every surviving link, resolved to where its file lands.
    pub links: Vec<LinkSite>,
}

/// One call, resolved: the skill it names, and what a refusal about it
/// quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// The skill the call names.
    pub skill: IndexedSkill,
    /// The call's target as the author wrote it, fragment and all.
    pub destination: String,
    /// The note the call is in, as a refusal names it.
    pub caller: String,
}

/// One link, resolved: the markdown it renders as, and the file it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSite {
    /// The link's text, escaped for markdown.
    pub label: String,
    /// The relative path from the emitting file to the linked file's output,
    /// encoded, with its fragment.
    pub href: String,
    /// Rendered as an image (`![…](…)`) rather than a link.
    pub image: bool,
    /// The linked file, vault-relative.
    pub target: String,
}

impl LinkSite {
    /// The markdown this link renders as — the bytes an unrecorded compile
    /// writes in its place.
    pub fn markdown(&self) -> String {
        if self.image {
            format!("![{}]({})", self.label, self.href)
        } else {
            format!("[{}]({})", self.label, self.href)
        }
    }
}

/// Opens and closes a recording compile's mark. Private-use code points, so
/// no vault text and no rendered syntax holds one.
pub(crate) const MARK_OPEN: char = '\u{E000}';
pub(crate) const MARK_CLOSE: char = '\u{E001}';

/// The mark for the `index`th call (`kind` `c`) or link (`l`).
fn mark(kind: char, index: usize) -> String {
    format!("{MARK_OPEN}{kind}{index}{MARK_CLOSE}")
}

impl std::fmt::Debug for CallContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallContext")
            .field("target", &self.target)
            .field("implicit", &self.implicit)
            .finish_non_exhaustive()
    }
}

impl CallContext<'_> {
    fn warn(&self, path: String, reason: &str, detail: String) {
        let warning = DiscoveryWarning {
            path,
            reason: reason.to_owned(),
            detail,
        };
        let mut warnings = self.warnings.borrow_mut();
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
}

/// What a compile produces: the entry body flattened, and every resource the
/// body (or the bundle root) reaches, ready to project.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledNote {
    /// The entry note's body with embeds expanded and links rewritten.
    pub body: String,
    /// `(relative_path, bytes, executable)`, in path order — the shape
    /// [`SkillProjection::resources`] already has, so the two connect without
    /// an adapter. Markdown resources are compiled; binaries are verbatim.
    pub resources: Vec<(String, Vec<u8>, bool)>,
    /// Per emitted file, what fed it. Keyed by the same relative path the
    /// resources use, with the entry's own output under `SKILL.md`.
    ///
    /// This is what makes staleness computable. The bytes at a destination can
    /// change because an embedded note changed, without the owning note moving
    /// at all, so a hash over the owner alone would report a skill current
    /// while its published copy was wrong.
    pub closures: BTreeMap<String, Closure>,
}

/// What one emitted file was built from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Closure {
    /// Vault-relative paths of every note whose bytes this file consumed:
    /// the note that owns it, plus everything transitively embedded into it.
    /// Sorted, and never empty — the owner is always a member.
    pub contributing_notes: Vec<String>,
    /// The vault-relative path of the note this file was rendered from: the
    /// entry note for `SKILL.md`, and the resource's own source otherwise.
    /// One of `contributing_notes`, not necessarily the first, since they are
    /// sorted. [`Closure::hash`] reads the whole file for this note.
    pub owner: String,
    /// Link targets in this file that resolved to nothing, as written and
    /// sorted. Recorded rather than refused, because a note appearing at one
    /// of these names later changes the output while leaving the hash over
    /// `contributing_notes` identical.
    pub unresolved: Vec<String>,
    /// Skills this file calls, as `plugin:name` (or `name`), sorted. Nothing
    /// of a called skill is copied, so its identity is what can go stale.
    pub calls: Vec<String>,
}

impl Closure {
    /// The hash that answers *did the source move*.
    ///
    /// Over the bytes the compiler **consumed**, member by member: the whole
    /// file for the note that owns the emitted file, the body only for a note
    /// reached by an embed — embedding pulls a body and never frontmatter, so
    /// a snippet's frontmatter cannot reach the output and must not move this
    /// hash. Hashing whole files throughout would be simpler and would mark
    /// skills stale for edits that cannot change a byte, which trains people
    /// to distrust the status view.
    ///
    /// The unresolved names are folded in too, so one comparison covers the
    /// whole closure: a note appearing at a name that resolved to nothing
    /// changes this hash even before it changes the member list. So are the
    /// calls, so renaming or moving a called skill changes the caller's hash.
    /// Both add nothing when empty.
    pub fn hash(&self, vfs: &dyn Vfs, vault_root: &Path) -> Result<String, CoreError> {
        let mut hasher = Sha256::new();
        for note in &self.contributing_notes {
            let bytes = vfs.read_file(&vault_root.join(note))?;
            let text = String::from_utf8_lossy(&bytes);
            let consumed = if *note == self.owner {
                text.as_ref()
            } else {
                split_frontmatter(&text).1
            };
            // Length-delimited, so two members cannot be confused for one.
            hasher.update(note.as_bytes());
            hasher.update(b"\0");
            hasher.update(consumed.as_bytes());
            hasher.update(b"\0");
        }
        for name in &self.unresolved {
            hasher.update(b"unresolved\0");
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
        }
        for call in &self.calls {
            hasher.update(b"call\0");
            hasher.update(call.as_bytes());
            hasher.update(b"\0");
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

/// Compiles one note body against its vault.
///
/// Seed the bundle, transform the entry body, drain the growing queue of
/// discovered markdown resources (each compiled in turn, so discovery is
/// transitive), read the binaries, then validate that no Obsidian syntax
/// survived into any emitted markdown. Nothing here writes — the caller holds
/// bytes, and what to do with them is the projection's decision.
///
/// # Errors
///
/// All refusals name the skill or the source file so the author can act:
///
/// * `refused: cycle` — the embeds form a cycle, named in vault-relative hops.
/// * `refused: fragment-unresolved` — a `#heading` or `#^block` selected
///   nothing, naming the source file and the fragment.
/// * `refused: link-ambiguous` — a link resolved to several files this pass
///   refuses to choose between. A link resolving to nothing is not refused:
///   it is left as written.
/// * `refused: unsafe-path` — an absolute or `..`-carrying link target.
/// * `refused: path-collision` — two sources claim one output path.
/// * `refused: unresolved-syntax` — the validation pass found Obsidian syntax
///   in an emitted markdown file.
/// * `refused: skill-call-not-skill` / `skill-call-unresolved` /
///   `skill-call-ambiguous` / `skill-call-not-shipped` — with a
///   [`CallContext`], a call named a note that is not a skill, nothing,
///   several skills by name, or a skill the destination does not carry. A
///   broken call sends the reader to a command that does not exist, so unlike
///   a link it blocks.
/// * `refused: literal-skill-call` — a skill or command body, or a note
///   embedded into it, spells a call to a known skill by hand (see
///   `literal_skill_calls`), which is right for one harness only. Every
///   such call is listed in one refusal.
///
/// Every token above except `cycle`, `unsafe-path` and `path-collision` (the
/// publish spec's) is this pass's own; they follow the closed set's morphology
/// so they can join it unchanged.
pub fn compile_note(
    vfs: &dyn Vfs,
    source: &CompileSource<'_>,
    body: &str,
) -> Result<CompiledNote, CoreError> {
    let mut renderer = Renderer::new(vfs, source)?;
    renderer.seed_bundle_resources()?;

    // The entry is on the transclusion stack from the start, so a note that
    // embeds itself is a cycle of one hop rather than a stack overflow.
    let mut stack = vec![(renderer.entry_path.clone(), None)];
    let entry_path = renderer.entry_path.clone();
    let compiled_body = renderer.transform_body(body, &entry_path, "SKILL.md", &mut stack)?;
    renderer.refuse_literal_calls()?;

    // A growing queue: compiling one markdown resource can discover
    // more, and each is compiled exactly once. Indexed iteration because the
    // queue grows while it drains.
    let mut outputs: BTreeMap<String, (Vec<u8>, bool)> = BTreeMap::new();
    let mut queue: Vec<PathBuf> = renderer.markdown_resources.keys().cloned().collect();
    let mut next = 0;
    while next < queue.len() {
        let source_path = queue[next].clone();
        next += 1;
        let output_path = renderer.markdown_resources[&source_path].clone();
        let raw = read_utf8(vfs, &source_path)?;
        // The strict split, not the lenient one: its yaml/shape recompose the
        // file byte-for-byte, so a resource's own frontmatter travels verbatim
        // and only the body changes. Carrying the author's bytes is
        // this core's rule (see `crate::vault`), and it keeps a resource
        // with no links byte-identical to its source.
        let split = frontmatter_lossless::split(&raw);
        let mut stack = vec![(source_path.clone(), None)];
        let compiled =
            renderer.transform_body(split.body, &source_path, &output_path, &mut stack)?;
        let composed = frontmatter_lossless::compose(split.yaml, &compiled, split.shape);
        // The execute bit rides along even for markdown: resources feed the
        // same copy the peers use, and the peers' contract is that bits
        // survive.
        let executable = vfs.is_executable(&source_path);
        outputs.insert(output_path, (composed.into_bytes(), executable));
        for discovered in renderer.markdown_resources.keys() {
            if !queue.contains(discovered) {
                queue.push(discovered.clone());
            }
        }
    }

    for (source_path, output_path) in &renderer.binary_resources {
        let bytes = vfs.read_file(source_path)?;
        let executable = vfs.is_executable(source_path);
        outputs.insert(output_path.clone(), (bytes, executable));
    }

    // Every transformation above either replaced a token or refused, but
    // splicing can move text across fence boundaries and frontmatter is
    // carried untransformed, so the emitted files are scanned once more.
    let shown = &renderer.shown_notation;
    validate_no_obsidian_syntax(&compiled_body, 0, "SKILL.md", source.skill_name, shown)?;
    for (output_path, (bytes, _)) in &outputs {
        if !output_path.to_lowercase().ends_with(".md") {
            continue;
        }
        // Emitted markdown is always this pass's own composition, so it is
        // UTF-8 by construction; the fallback only guards the invariant.
        let text = std::str::from_utf8(bytes)
            .map_err(|e| CoreError::malformed(Path::new(output_path), format!("not UTF-8: {e}")))?;
        validate_no_obsidian_syntax(
            text,
            body_start(text),
            output_path,
            source.skill_name,
            shown,
        )?;
    }

    // Every emitted file's closure contains the file that owns it, so a
    // closure is never empty and a resource with no embeds still hashes over
    // something. `SKILL.md` is owned by the entry note; every other output is
    // owned by the vault file it was rendered from.
    let mut owners: Vec<(String, PathBuf)> = vec![("SKILL.md".to_owned(), entry_path.clone())];
    for (source_path, output_path) in &renderer.markdown_resources {
        owners.push((output_path.clone(), source_path.clone()));
    }
    for (source_path, output_path) in &renderer.binary_resources {
        owners.push((output_path.clone(), source_path.clone()));
    }

    let mut closures: BTreeMap<String, Closure> = BTreeMap::new();
    for (output_path, source_path) in owners {
        let mut members = renderer
            .contributing
            .get(&output_path)
            .cloned()
            .unwrap_or_default();
        let owner = renderer.vault_relative(&source_path)?;
        members.insert(owner.clone());
        closures.insert(
            output_path.clone(),
            Closure {
                contributing_notes: members.into_iter().collect(),
                owner,
                unresolved: renderer
                    .unresolved
                    .get(&output_path)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                calls: renderer
                    .calls
                    .get(&output_path)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            },
        );
    }

    Ok(CompiledNote {
        body: compiled_body,
        resources: outputs
            .into_iter()
            .map(|(path, (bytes, executable))| (path, bytes, executable))
            .collect(),
        closures,
    })
}

/// What one note reaches, split the way the compile pass resolves it.
///
/// Every list is vault-relative, forward-slashed and sorted, except
/// [`Self::unresolved`], which carries link targets as they were written —
/// there is no file to name them after.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelatedSplit {
    /// Notes whose bodies this note's own output consumed: everything it
    /// embeds, transitively, because an embedded note's embeds expand into the
    /// same output. Never the entry itself.
    pub embeds: Vec<String>,
    /// Files a surviving `[[link]]` resolved to — notes and attachments both,
    /// since a non-markdown embed is rewritten as a link too. Never the entry
    /// itself, which a self-reference resolves to.
    pub links: Vec<String>,
    /// Notes this note calls. With a [`CallContext`] these are skills; without
    /// one, whatever note each `/[[…]]` resolved to by path. A call copies
    /// nothing, so a called note is never also in [`Self::links`] for that
    /// call.
    pub calls: Vec<String>,
    /// Link targets that resolved to nothing, as written.
    pub unresolved: Vec<String>,
    /// Files seeded from the bundle root, whether or not the body mentions
    /// them. Empty for a flat note, which has no bundle root.
    pub resources: Vec<String>,
}

/// What [`compile_note`] would reach, without emitting anything.
///
/// The same `Renderer` walk, stopped before the resource queue drains: the
/// entry's body is transformed (and the result thrown away) so the embed
/// expansion, the link collection and the unresolved names are the compiler's
/// own, not a second scanner's answer to the same question. Transitivity comes
/// free with that — an embed expands inside the walk, so a note two hops deep
/// is in [`RelatedSplit::embeds`] for the same reason it is in the closure.
///
/// Calls are tolerant here. A call that names nothing, or several skills, is
/// in [`RelatedSplit::unresolved`]; one that names a note that is not a skill
/// is in [`RelatedSplit::calls`]. A call to a skill the destination does not
/// ship, or with a fragment that selects nothing, is a call like any other.
/// The view shows what a note reaches; a broken call is a row to fix, and a
/// note that shows the notation in code must still have a view.
///
/// # Errors
///
/// The subset of [`compile_note`]'s refusals the walk itself can raise:
/// `cycle`, `fragment-unresolved` (from an embed), `link-ambiguous`,
/// `unsafe-path` and `path-collision`. `unresolved-syntax` and the
/// `skill-call-*` tokens cannot appear.
pub fn related_of(
    vfs: &dyn Vfs,
    source: &CompileSource<'_>,
    body: &str,
) -> Result<RelatedSplit, CoreError> {
    let mut renderer = Renderer::new(vfs, source)?;
    renderer.tolerant_calls = true;
    renderer.seed_bundle_resources()?;
    // Snapshotted before the walk: after it, the two resource maps also hold
    // everything the body linked to, and a bundle peer is not a link.
    let seeded: Vec<PathBuf> = renderer
        .markdown_resources
        .keys()
        .chain(renderer.binary_resources.keys())
        .cloned()
        .collect();

    let entry_path = renderer.entry_path.clone();
    let mut stack = vec![(entry_path.clone(), None)];
    renderer.transform_body(body, &entry_path, "SKILL.md", &mut stack)?;

    let mut links = Vec::new();
    for path in &renderer.linked {
        // `[[#Heading]]` resolves to the note it appears in; a note does not
        // link itself.
        if *path == entry_path {
            continue;
        }
        links.push(renderer.vault_relative(path)?);
    }
    let mut calls = Vec::new();
    for path in &renderer.called_notes {
        calls.push(renderer.vault_relative(path)?);
    }
    let mut resources = Vec::new();
    for path in &seeded {
        resources.push(renderer.vault_relative(path)?);
    }
    resources.sort();
    resources.dedup();

    Ok(RelatedSplit {
        embeds: renderer
            .contributing
            .get("SKILL.md")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect(),
        links,
        calls,
        unresolved: renderer
            .unresolved
            .get("SKILL.md")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect(),
        resources,
    })
}

/// Wires the compile pass into the projection path: the projection's body and
/// resources are replaced with their compiled forms, so a
/// [`crate::projection::project_skill`] over the result emits the flattened
/// skill.
///
/// The resource set is **recomputed** from `source`, not merged: what ships is
/// what the bundle root holds plus what the body reaches, which is the
/// contract. A caller holding peers in a sibling folder passes that
/// folder as [`CompileSource::bundle_root`] or loses them — stated here
/// because the silent version of that mistake is the under-publish this
/// crate exists to prevent.
pub fn compile_projection(
    vfs: &dyn Vfs,
    source: &CompileSource<'_>,
    projection: &mut SkillProjection,
) -> Result<BTreeMap<String, Closure>, CoreError> {
    let compiled = compile_note(vfs, source, &projection.content.body)?;
    projection.content.body = compiled.body;
    projection.resources = compiled.resources;
    Ok(compiled.closures)
}

// --- Extension vocabularies -------------------------------------------------

/// Embeds of these render as images (`![label](path)`); everything else a
/// non-markdown embed reaches renders as an ordinary link.
const IMAGE_EXTENSIONS: &[&str] = &[
    ".avif", ".bmp", ".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp",
];

/// Document-shaped files land under `references/`, everything else under
/// `assets/`. `.md` is in it because a linked note is a
/// reference by definition.
const REFERENCE_EXTENSIONS: &[&str] = &[
    ".csv", ".doc", ".docx", ".html", ".json", ".md", ".odt", ".pdf", ".rtf", ".tsv", ".txt",
    ".xls", ".xlsx", ".yaml", ".yml",
];

/// Directories the vault index never descends into — tooling and trash, not
/// notes.
pub(crate) const EXCLUDED_VAULT_DIRECTORIES: &[&str] =
    &[".git", ".obsidian", ".trash", "node_modules"];

/// Directories a skill's own bundle sweep never ships, at any depth under the
/// bundle root — dev-only fixtures and vendored/tooling trees, never the
/// skill's published content. Also used by `crate::model::load`'s asset reader
/// for a directory `assets:` entry, so a shared tree copied wholesale (a
/// docs directory, a reference bundle) gets the same pruning a skill bundle
/// gets — a vendored `node_modules` under such a tree is exactly as
/// unwanted as one under a skill bundle.
///
/// `tests`/`fixtures` are a fixed convention rather than a project-file
/// setting: every skill gets this for free, with nothing to declare and
/// nothing to forget to declare. Fixture markdown is often literal
/// `[[decision-note]]`-shaped test data, not real Obsidian links; swept as an
/// ordinary bundle resource, it reads as a broken link and fails
/// `unresolved-syntax` on a vault that has nothing wrong with it. The rest
/// mirror [`EXCLUDED_VAULT_DIRECTORIES`] above: no
/// bundle or asset tree should ship a `.git`, `.obsidian`, `.trash` or
/// vendored `node_modules` either.
pub(crate) const EXCLUDED_BUNDLE_DIRECTORIES: &[&str] = &[
    "tests",
    "fixtures",
    ".git",
    ".obsidian",
    ".trash",
    "node_modules",
];

/// The walk compile indexes a vault with, for a caller that must see the
/// same files: every file, dot-named ones too, except under `.git`,
/// `.obsidian`, `.trash` and `node_modules`. A symlink is listed, and a folder
/// that cannot be listed fails the walk.
pub fn vault_walk_options() -> markdown_vault_fs::walk::WalkOptions {
    crate::walk::vault_walk(EXCLUDED_VAULT_DIRECTORIES)
}

/// What a walk with [`vault_walk_options`] cannot read under `vault_root`:
/// each folder it cannot list and each file it cannot stat, in walk order.
/// For a caller that walks with `propagate_errors` off and must say what the
/// walk passed over.
pub fn vault_walk_errors(vfs: &dyn Vfs, vault_root: &Path) -> Vec<crate::vfs::VfsError> {
    crate::walk::vault_walk_errors(vfs, vault_root, EXCLUDED_VAULT_DIRECTORIES)
}

/// Every file under `vault_root`, by vault-relative path, indexed for link
/// resolution, leaving out [`EXCLUDED_VAULT_DIRECTORIES`].
pub(crate) fn vault_link_index(vfs: &dyn Vfs, vault_root: &Path) -> Result<LinkIndex, CoreError> {
    let files = crate::walk::vault_files(vfs, vault_root, EXCLUDED_VAULT_DIRECTORIES)?;
    link_index(vault_root, &files)
}

/// `files`, by their path relative to `vault_root`, indexed for link
/// resolution.
pub(crate) fn link_index(vault_root: &Path, files: &[PathBuf]) -> Result<LinkIndex, CoreError> {
    let mut paths = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(vault_root)
            .map(forward_slashed)
            .map_err(|_| CoreError::PathEscapesRoot(path.clone()))?;
        paths.push(relative);
    }
    Ok(LinkIndex::build(&paths))
}

// --- The renderer -----------------------------------------------------------

/// One skill's compile state: the vault's link index, and the resources
/// claimed so far.
struct Renderer<'a> {
    vfs: &'a dyn Vfs,
    vault_root: PathBuf,
    entry_path: PathBuf,
    bundle_root: Option<PathBuf>,
    skill_name: &'a str,
    /// Every file in the vault, by vault-relative path, for link
    /// resolution. Built once per compile — the vault is the resolution
    /// universe, and walking it lazily would make "not found" depend on what
    /// happened to have been walked.
    index: Cow<'a, LinkIndex>,
    /// Per emitted file, the notes it consumed and the links it could not
    /// resolve. Accumulated as the walk runs; the walk is the only place that
    /// knows either.
    contributing: BTreeMap<String, BTreeSet<String>>,
    unresolved: BTreeMap<String, BTreeSet<String>>,
    /// Every file a surviving link resolved to, as the walk met it. The
    /// resource maps cannot answer this on their own: a bundle peer is in them
    /// before the body is read, so "linked" and "shipped" are different facts.
    /// Read only by [`related_of`]; a compile has no use for it.
    linked: BTreeSet<PathBuf>,
    call_context: Option<&'a CallContext<'a>>,
    /// [`related_of`]'s mode: a call that fails a `skill-call-*` check, or
    /// whose fragment selects nothing, is recorded instead of refused.
    tolerant_calls: bool,
    /// Per emitted file, the [`IndexedSkill::call_id`]s it calls.
    calls: BTreeMap<String, BTreeSet<String>>,
    /// Every note a call resolved to. Read only by [`related_of`].
    called_notes: BTreeSet<PathBuf>,
    /// Hand-written calls found in `SKILL.md`'s sources, as `` `call` in
    /// note (`/[[skill]]`) ``, refused together once the body is built.
    literal_calls: Vec<String>,
    /// Each `/[[x]]` an escape wrote out as text, which validation accepts.
    shown_notation: BTreeSet<String>,
    markdown_resources: BTreeMap<PathBuf, String>,
    /// Source path → output path, for files copied verbatim.
    binary_resources: BTreeMap<PathBuf, String>,
    /// Output path → source path — the collision check. Seeded with
    /// `SKILL.md` so nothing else can claim the entry's own output.
    destinations: BTreeMap<String, PathBuf>,
}

impl<'a> Renderer<'a> {
    fn new(vfs: &'a dyn Vfs, source: &CompileSource<'a>) -> Result<Self, CoreError> {
        // Normalised up front so `==` against index paths (built the same way)
        // means location equality, not spelling equality.
        let vault_root = normalize(source.vault_root);
        let entry_path = normalize(source.entry_path);
        let bundle_root = source.bundle_root.map(normalize);
        // Containment is checked before anything resolves: every output path
        // under `…/vault/` is derived by stripping `vault_root`, and an entry
        // outside it would fail that derivation somewhere deep in a walk with
        // a worse message than this one. Both rules: the derivation strips
        // `vault_root` as spelled, and a path linked out of the vault is
        // outside it on disk.
        let inside = |path: &Path| -> Result<bool, CoreError> {
            Ok(path.starts_with(&vault_root) && within(vfs, &vault_root, path)?)
        };
        if !inside(&entry_path)? {
            return Err(CoreError::PathEscapesRoot(entry_path));
        }
        if let Some(root) = &bundle_root
            && !inside(root)?
        {
            return Err(CoreError::PathEscapesRoot(root.clone()));
        }
        let index = match source.links {
            Some(index) => Cow::Borrowed(index),
            None => Cow::Owned(vault_link_index(vfs, &vault_root)?),
        };
        let mut destinations = BTreeMap::new();
        destinations.insert("SKILL.md".to_owned(), entry_path.clone());
        Ok(Renderer {
            vfs,
            vault_root,
            entry_path,
            bundle_root,
            skill_name: source.skill_name,
            index,
            contributing: BTreeMap::new(),
            unresolved: BTreeMap::new(),
            linked: BTreeSet::new(),
            call_context: source.calls,
            tolerant_calls: false,
            calls: BTreeMap::new(),
            called_notes: BTreeSet::new(),
            literal_calls: Vec::new(),
            shown_notation: BTreeSet::new(),
            markdown_resources: BTreeMap::new(),
            binary_resources: BTreeMap::new(),
            destinations,
        })
    }

    /// Every file under the bundle root ships at its bundle-relative path,
    /// linked or not — the copy set comes from the walk, never from a model
    /// of what the body mentions — except a directory named
    /// [`EXCLUDED_BUNDLE_DIRECTORIES`], which never ships at all.
    fn seed_bundle_resources(&mut self) -> Result<(), CoreError> {
        let Some(bundle_root) = self.bundle_root.clone() else {
            return Ok(());
        };
        for source_path in crate::walk::files(self.vfs, &bundle_root, EXCLUDED_BUNDLE_DIRECTORIES)?
        {
            if source_path == self.entry_path {
                continue;
            }
            let relative = source_path
                .strip_prefix(&bundle_root)
                .map_err(|_| CoreError::PathEscapesRoot(source_path.clone()))?;
            let output_path = forward_slashed(relative);
            if output_path == "SKILL.md" {
                // A bundle file literally named SKILL.md would be overwritten
                // by the rendered entry — the quiet loss `path-collision`
                // exists to make loud.
                return Err(CoreError::refused(
                    "path-collision",
                    format!(
                        "reserved resource path in {}: SKILL.md is the rendered entry's output",
                        self.skill_name
                    ),
                ));
            }
            self.claim_destination(&output_path, &source_path)?;
            if extension_of_path(&source_path).as_deref() == Some(".md") {
                self.markdown_resources.insert(source_path, output_path);
            } else {
                self.binary_resources.insert(source_path, output_path);
            }
        }
        Ok(())
    }

    /// One output path, one source — anything else drops bytes silently.
    fn claim_destination(
        &mut self,
        output_path: &str,
        source_path: &Path,
    ) -> Result<(), CoreError> {
        if let Some(existing) = self.destinations.get(output_path)
            && existing != source_path
        {
            return Err(CoreError::refused(
                "path-collision",
                format!(
                    "resource collision in {}: {} is claimed by both {} and {}",
                    self.skill_name,
                    output_path,
                    existing.display(),
                    source_path.display(),
                ),
            ));
        }
        self.destinations
            .insert(output_path.to_owned(), source_path.to_path_buf());
        Ok(())
    }

    /// Where `source_path` lands in the published skill. See the module doc's
    /// placement table.
    fn output_path_for(&self, source_path: &Path) -> Result<String, CoreError> {
        if source_path == self.entry_path {
            return Ok("SKILL.md".to_owned());
        }
        if let Some(bundle_root) = &self.bundle_root
            && let Ok(relative) = source_path.strip_prefix(bundle_root)
        {
            return Ok(forward_slashed(relative));
        }
        let category = match extension_of_path(source_path) {
            Some(ext) if REFERENCE_EXTENSIONS.contains(&ext.as_str()) => "references",
            _ => "assets",
        };
        Ok(format!(
            "{category}/vault/{}",
            self.vault_relative(source_path)?
        ))
    }

    fn ensure_markdown_resource(&mut self, source_path: &Path) -> Result<String, CoreError> {
        // The entry never becomes a resource of itself: a link back to it is a
        // link to SKILL.md.
        if source_path == self.entry_path {
            return Ok("SKILL.md".to_owned());
        }
        if let Some(existing) = self.markdown_resources.get(source_path) {
            return Ok(existing.clone());
        }
        let output_path = self.output_path_for(source_path)?;
        self.claim_destination(&output_path, source_path)?;
        self.markdown_resources
            .insert(source_path.to_path_buf(), output_path.clone());
        Ok(output_path)
    }

    fn ensure_binary_resource(&mut self, source_path: &Path) -> Result<String, CoreError> {
        if let Some(existing) = self.binary_resources.get(source_path) {
            return Ok(existing.clone());
        }
        let output_path = self.output_path_for(source_path)?;
        self.claim_destination(&output_path, source_path)?;
        self.binary_resources
            .insert(source_path.to_path_buf(), output_path.clone());
        Ok(output_path)
    }

    fn vault_relative(&self, path: &Path) -> Result<String, CoreError> {
        path.strip_prefix(&self.vault_root)
            .map(forward_slashed)
            .map_err(|_| CoreError::PathEscapesRoot(path.to_path_buf()))
    }

    /// A stack entry, spelt for a human: the vault-relative path plus the
    /// fragment, which is how the cycle refusal names its hops.
    fn transclusion_label(&self, path: &Path, fragment: Option<&str>) -> String {
        let relative = self.relative_or_verbatim(path);
        match fragment {
            Some(fragment) => format!("{relative}#{fragment}"),
            None => relative,
        }
    }

    /// The heart of the pass: every wiki token in `body` replaced — embeds of
    /// markdown by their (recursively transformed) bodies, calls by the
    /// harness's invocation, everything else by a relative markdown link to
    /// its collected resource.
    ///
    /// `output_path` is where the text being built will land, because a
    /// relative link's spelling depends on where it is written *from* — the
    /// same note embedded into two resources links its references
    /// differently in each.
    fn transform_body(
        &mut self,
        body: &str,
        source_path: &Path,
        output_path: &str,
        stack: &mut Vec<TransclusionHop>,
    ) -> Result<String, CoreError> {
        let mut tokens = scan_wiki_tokens(body, true);
        let escapes = self.check_literal_calls(body, source_path, output_path)?;
        if !escapes.is_empty() {
            // An escape inside a wiki token is part of that token's text.
            let free: Vec<WikiToken<'_>> = escapes
                .into_iter()
                .filter(|escape| {
                    !tokens
                        .iter()
                        .any(|token| token.start <= escape.start && escape.start < token.end)
                })
                .collect();
            tokens.extend(free);
            tokens.sort_by_key(|token| token.start);
        }
        if tokens.is_empty() {
            return Ok(body.to_owned());
        }
        let mut out = String::with_capacity(body.len());
        let mut cursor = 0;
        for token in tokens {
            out.push_str(&body[cursor..token.start]);
            let raw = &body[token.start..token.end];
            out.push_str(&self.replace_token(&token, raw, source_path, output_path, stack)?);
            cursor = token.end;
        }
        out.push_str(&body[cursor..]);
        Ok(out)
    }

    /// `raw` is the token's text as written.
    fn replace_token(
        &mut self,
        token: &WikiToken<'_>,
        raw: &str,
        source_path: &Path,
        output_path: &str,
        stack: &mut Vec<TransclusionHop>,
    ) -> Result<String, CoreError> {
        match token.kind {
            TokenKind::Escape => {
                // The notation itself, shown: validation lets exactly this
                // text through.
                let shown = &raw[1..];
                self.shown_notation.insert(shown.to_owned());
                return Ok(shown.to_owned());
            }
            TokenKind::Unconverted => {
                if let Some(context) = self.call_context {
                    context.warn(
                        self.relative_or_verbatim(source_path),
                        "unconverted-call",
                        format!(
                            "`{raw}` in code is not at a call position, so it ships as \
                             written; put a space before the `/` to make it a call, or \
                             `\\` to show the notation"
                        ),
                    );
                }
                return Ok(raw.to_owned());
            }
            TokenKind::LiteralEscape => return Ok(String::new()),
            TokenKind::Call { in_code } => {
                return self.replace_call(token, source_path, output_path, in_code);
            }
            TokenKind::Link | TokenKind::Embed => {}
        }
        if token.kind == TokenKind::Link
            && let Some(call) = self.implicit_call(token, source_path, output_path)?
        {
            return Ok(call);
        }
        let embed = token.kind == TokenKind::Embed;
        let Some((resolved, fragment)) = self.resolve(token, source_path)? else {
            // The name is kept so status can see a note appearing at it later,
            // and the reader is left the words rather than a dangling
            // `[[…]]`, which step 3 would refuse as surviving Obsidian syntax.
            self.unresolved
                .entry(output_path.to_owned())
                .or_default()
                .insert(token.target.clone());
            return Ok(text_alias(token.alias.as_deref())
                .unwrap_or(&token.target)
                .to_owned());
        };
        let extension = extension_of_path(&resolved).unwrap_or_default();

        if embed && extension == ".md" {
            let hop = (resolved.clone(), fragment.clone());
            if let Some(at) = stack.iter().position(|entry| *entry == hop) {
                // The refusal prints the loop from its first repeat, ending on
                // the repeated hop, so the author sees the shape rather than
                // the whole approach path.
                let cycle: Vec<String> = stack[at..]
                    .iter()
                    .chain([&hop])
                    .map(|(path, fragment)| self.transclusion_label(path, fragment.as_deref()))
                    .collect();
                return Err(CoreError::refused(
                    "cycle",
                    format!(
                        "circular transclusion in {}: {}",
                        self.skill_name,
                        cycle.join(" -> ")
                    ),
                ));
            }
            let raw = read_utf8(self.vfs, &resolved)?;
            // The lenient split: embedding pulls a note's *body*, and a note
            // whose frontmatter is slightly off should still embed — this is
            // a read, not a rewrite, so strictness buys nothing here.
            let (_, note_body) = split_frontmatter(&raw);
            let label = self.vault_relative(&resolved)?;
            // The closure grows here and nowhere else: a *linked* note gets
            // its own emitted file and its own entry, so its edits are already
            // caught by that file's own comparison. Only embedding folds one
            // note's bytes into another's output and hides them behind the
            // other's hash.
            self.contributing
                .entry(output_path.to_owned())
                .or_default()
                .insert(label.clone());
            let selected = select_fragment(note_body, fragment.as_deref(), &label)?;
            stack.push(hop);
            let expanded = self.transform_body(&selected, &resolved, output_path, stack)?;
            stack.pop();
            return Ok(expanded.trim().to_owned());
        }

        self.linked.insert(resolved.clone());
        let resource_output = if extension == ".md" {
            self.ensure_markdown_resource(&resolved)?
        } else {
            self.ensure_binary_resource(&resolved)?
        };
        let site = LinkSite {
            label: markdown_link::escape_label(&display_label(token.alias.as_deref(), &resolved)),
            href: markdown_link::relative_link(output_path, &resource_output, fragment.as_deref()),
            image: embed && IMAGE_EXTENSIONS.contains(&extension.as_str()),
            target: String::new(),
        };
        match self.call_context.and_then(|context| context.record) {
            Some(record) => {
                let site = LinkSite {
                    target: self.vault_relative(&resolved)?,
                    ..site
                };
                let mut record = record.borrow_mut();
                record.links.push(site);
                Ok(mark('l', record.links.len() - 1))
            }
            None => Ok(site.markdown()),
        }
    }

    /// A `/[[call]]`: the target's invocation in this harness's spelling,
    /// with nothing of the target copied.
    fn replace_call(
        &mut self,
        token: &WikiToken<'_>,
        source_path: &Path,
        output_path: &str,
        in_code: bool,
    ) -> Result<String, CoreError> {
        let resolved = self.resolve(token, source_path)?;
        let target = token.target.as_str();
        if let Some(context) = self.call_context {
            match self.render_call(context, token, resolved.clone(), source_path, output_path) {
                Ok(call) => return Ok(format_call(&call, in_code)),
                Err(CoreError::Refused(reason))
                    if self.tolerant_calls && reason.starts_with("skill-call-") => {}
                Err(e) => return Err(e),
            }
        }
        // Without a context, or in the related view, a call that names no
        // skill is still a row: the note it resolved to by path, or the name
        // as written.
        match resolved {
            Some((path, _)) => {
                self.called_notes.insert(path);
            }
            None => {
                self.unresolved
                    .entry(output_path.to_owned())
                    .or_default()
                    .insert(target.to_owned());
            }
        }
        Ok(format_call(target, in_code))
    }

    /// The invocation a call renders as, or the `skill-call-*` refusal.
    fn render_call(
        &mut self,
        context: &CallContext<'_>,
        token: &WikiToken<'_>,
        resolved: Option<(PathBuf, Option<String>)>,
        source_path: &Path,
        output_path: &str,
    ) -> Result<String, CoreError> {
        let (target, written_fragment) = (token.target.as_str(), token.fragment.clone());
        let skills = context.skills.get()?;
        let caller = self.relative_or_verbatim(source_path);
        let (skill, fragment) = match resolved {
            Some((path, fragment)) => match skills.by_path(&path) {
                Some(skill) => (skill, fragment),
                None => {
                    return Err(CoreError::refused(
                        "skill-call-not-skill",
                        format!(
                            "call in {caller} names {}, which is not a skill: /[[{}]]",
                            self.relative_or_verbatim(&path),
                            token.destination
                        ),
                    ));
                }
            },
            // A path that resolves to no note may still be a skill's name: a
            // bundle skill's entry is `x/SKILL.md`, which `[[x]]` never
            // resolves to.
            None => match skills.by_name(basename(target)) {
                NameMatch::One(skill) => (skill, written_fragment),
                NameMatch::None => {
                    return Err(CoreError::refused(
                        "skill-call-unresolved",
                        format!(
                            "call in {caller} names no skill: /[[{}]]",
                            token.destination
                        ),
                    ));
                }
                NameMatch::Many(candidates) => {
                    let candidates: Vec<String> = candidates
                        .iter()
                        .map(|skill| self.relative_or_verbatim(&skill.entry_path))
                        .collect();
                    return Err(CoreError::refused(
                        "skill-call-ambiguous",
                        format!(
                            "call in {caller} names several skills: /[[{}]] could be {}; \
                             write the path of the one meant",
                            token.destination,
                            candidates.join(", ")
                        ),
                    ));
                }
            },
        };
        self.check_call_shipped(context, skill, &caller, token.destination)?;
        self.check_call_fragment(skill, fragment.as_deref())?;
        self.record_call(skill, output_path);
        render_or_mark(context, skill, token.destination, caller)
    }

    /// A plain `[[x]]` in a skill or command body that names another skill
    /// is a call: copying one skill into another is never what was meant.
    /// `None` leaves the token to the link path, as does a name several
    /// skills share, or an index that cannot be built: the reading is a
    /// heuristic, so a fault elsewhere in the vault must not block a link.
    fn implicit_call(
        &mut self,
        token: &WikiToken<'_>,
        source_path: &Path,
        output_path: &str,
    ) -> Result<Option<String>, CoreError> {
        let Some(context) = self.call_context.filter(|context| context.implicit) else {
            return Ok(None);
        };
        let (target, written_fragment) = (token.target.as_str(), token.fragment.clone());
        let resolved = self.resolve(token, source_path)?;
        let Some(skills) = context.skills.get_or_none() else {
            return Ok(None);
        };
        let (skill, fragment) = match resolved {
            Some((path, fragment)) => (skills.by_path(&path), fragment),
            None => match skills.by_name(basename(target)) {
                NameMatch::One(skill) => (Some(skill), written_fragment),
                NameMatch::None | NameMatch::Many(_) => (None, None),
            },
        };
        // A link back to the entry is a link to its own SKILL.md.
        let Some(skill) = skill.filter(|skill| skill.entry_path != self.entry_path) else {
            return Ok(None);
        };
        let caller = self.relative_or_verbatim(source_path);
        self.check_call_shipped(context, skill, &caller, token.destination)?;
        self.check_call_fragment(skill, fragment.as_deref())?;
        context.warn(
            caller.clone(),
            "implicit-skill-call",
            format!(
                "`[[{}]]` names skill `{}`, so it renders as a call; write `/[[{}]]` to say so",
                token.destination,
                skill.call_id(),
                token.destination
            ),
        );
        self.record_call(skill, output_path);
        let call = render_or_mark(context, skill, token.destination, caller)?;
        Ok(Some(format_call(&call, false)))
    }

    /// A call to a skill the destination does not carry would name a command
    /// that is not there, so it is refused rather than rendered.
    fn check_call_shipped(
        &self,
        context: &CallContext<'_>,
        skill: &IndexedSkill,
        caller: &str,
        destination: &str,
    ) -> Result<(), CoreError> {
        if skill.shipped || self.tolerant_calls || context.record.is_some() {
            return Ok(());
        }
        Err(CoreError::refused(
            "skill-call-not-shipped",
            format!(
                "call `{destination}` in {caller} names skill `{}`, which {} does not ship",
                skill.call_id(),
                context.target
            ),
        ))
    }

    /// A call's fragment addresses a heading or block in the called skill. It
    /// is checked, then dropped: an invocation has no fragment.
    fn check_call_fragment(
        &self,
        skill: &IndexedSkill,
        fragment: Option<&str>,
    ) -> Result<(), CoreError> {
        let Some(fragment) = fragment.filter(|_| !self.tolerant_calls) else {
            return Ok(());
        };
        let raw = read_utf8(self.vfs, &skill.entry_path)?;
        let (_, body) = split_frontmatter(&raw);
        select_fragment(
            body,
            Some(fragment),
            &self.relative_or_verbatim(&skill.entry_path),
        )?;
        Ok(())
    }

    fn record_call(&mut self, skill: &IndexedSkill, output_path: &str) {
        self.calls
            .entry(output_path.to_owned())
            .or_default()
            .insert(skill.call_id());
        self.called_notes.insert(skill.entry_path.clone());
    }

    /// Finds the hand-written calls to known skills in `body`. Each unescaped
    /// one in a body bound for `SKILL.md` is held for
    /// [`Self::refuse_literal_calls`]. Each escaped one comes back as the
    /// token that drops its `\`; an escape that names no known skill, or
    /// that has no index to check against, is left as written. Only a skill
    /// or command body is checked.
    fn check_literal_calls(
        &mut self,
        body: &str,
        source_path: &Path,
        output_path: &str,
    ) -> Result<Vec<WikiToken<'static>>, CoreError> {
        let Some(context) = self.call_context.filter(|context| context.implicit) else {
            return Ok(Vec::new());
        };
        let literals = literal_skill_calls(body);
        if literals.is_empty() {
            return Ok(Vec::new());
        }
        let mut escapes = Vec::new();
        for literal in literals {
            if literal.escaped {
                let known = context
                    .skills
                    .get_or_none()
                    .and_then(|skills| self.known_literal(skills, &literal));
                if known.is_some() {
                    escapes.push(WikiToken {
                        start: literal.start,
                        end: literal.start + 1,
                        kind: TokenKind::LiteralEscape,
                        destination: "",
                        target: String::new(),
                        fragment: None,
                        alias: None,
                    });
                }
                continue;
            }
            let Some(skill) = self.literal_target(context, &literal)? else {
                continue;
            };
            if output_path == "SKILL.md" {
                let found = format!(
                    "`{}` in {} (`/[[{}]]`)",
                    literal.text,
                    self.relative_or_verbatim(source_path),
                    skill.name
                );
                if !self.literal_calls.contains(&found) {
                    self.literal_calls.push(found);
                }
            }
        }
        Ok(escapes)
    }

    /// The known skill an unescaped hand-written call names, if any. A bare
    /// `/skill` counts only when the index is at hand and has plugins: a
    /// standalone publish's skills have none, so the bare form never applies
    /// there and is never worth a vault scan.
    fn literal_target(
        &self,
        context: &'a CallContext<'a>,
        literal: &LiteralCall<'_>,
    ) -> Result<Option<&'a IndexedSkill>, CoreError> {
        let skills = match literal.form {
            LiteralForm::Bare { .. } => match context.skills.peek() {
                Some(skills) if skills.has_plugins() => skills,
                _ => return Ok(None),
            },
            LiteralForm::Qualified { .. } | LiteralForm::Pi { .. } => context.skills.get()?,
        };
        Ok(self.known_literal(skills, literal))
    }

    /// The skill in `skills` a hand-written call names. With plugins in the
    /// index, `/plugin:x` and `$plugin:x` name `x` in `plugin`, Pi's
    /// `/skill:plugin-x` names the same, and a bare `/x` names `x` in the
    /// entry's own plugin. With none, a standalone publish's, every form
    /// names a skill by name alone.
    fn known_literal(
        &self,
        skills: &'a SkillIndex,
        literal: &LiteralCall<'_>,
    ) -> Option<&'a IndexedSkill> {
        if !skills.has_plugins() {
            let name = match literal.form {
                LiteralForm::Qualified { skill, .. } | LiteralForm::Bare { skill } => skill,
                LiteralForm::Pi { prefixed } => prefixed,
            };
            return match skills.by_name(name) {
                NameMatch::One(found) => Some(found),
                NameMatch::Many(found) => found.first().copied(),
                NameMatch::None => None,
            };
        }
        match literal.form {
            LiteralForm::Qualified { plugin, skill } => skills.qualified(plugin, skill),
            LiteralForm::Pi { prefixed } => skills.prefixed(prefixed),
            LiteralForm::Bare { skill } => {
                let plugin = skills.by_path(&self.entry_path)?.plugin.as_deref()?;
                skills.qualified(plugin, skill)
            }
        }
    }

    /// Refuses every hand-written call [`Self::check_literal_calls`] held: its
    /// spelling is right for one harness only.
    fn refuse_literal_calls(&self) -> Result<(), CoreError> {
        if self.literal_calls.is_empty() {
            return Ok(());
        }
        Err(CoreError::refused(
            "literal-skill-call",
            format!(
                "{} literal skill call{} in {}: {}; write each as the `/[[skill]]` call shown \
                 so each harness gets its own, or put `\\` before one meant literally",
                self.literal_calls.len(),
                if self.literal_calls.len() == 1 {
                    ""
                } else {
                    "s"
                },
                self.skill_name,
                self.literal_calls.join(", ")
            ),
        ))
    }

    /// One destination string to one vault file, in order:
    /// vault-relative exact match, then relative to the linking file, then a
    /// unique basename match across the vault.
    ///
    /// `Ok(None)` is a link that resolved to nothing — a reportable fact, not
    /// a refusal. Several matches are refused `link-ambiguous`: picking one
    /// would publish bytes the author may not have meant, which is a different
    /// failure from a name that is simply not there yet.
    fn resolve(
        &self,
        token: &WikiToken<'_>,
        from_file: &Path,
    ) -> Result<Option<(PathBuf, Option<String>)>, CoreError> {
        let (destination, target) = (token.destination, token.target.as_str());
        let fragment = token.fragment.clone();
        if target.is_empty() {
            // `[[#Heading]]` — a self-reference; the fragment addresses the
            // note it appears in.
            return Ok(Some((from_file.to_path_buf(), fragment)));
        }
        if target.starts_with('/') || target.split(['/', '\\']).any(|segment| segment == "..") {
            return Err(CoreError::refused(
                "unsafe-path",
                format!(
                    "unsafe Obsidian link in {}: {destination}",
                    self.relative_or_verbatim(from_file)
                ),
            ));
        }

        // `markdown-link`'s resolver, refusing to guess: an exact path, the
        // path from the linking note's folder, or the only file the name
        // matches. A bare name addresses a note (`[[photo]]` is not
        // `photo.jpg`); a name with an extension addresses that file.
        let from = self.vault_relative(from_file).unwrap_or_default();
        match self.index.resolve_strict(target, &from) {
            Ok(hit) => Ok(Some((self.vault_root.join(hit), fragment))),
            // Not a refusal. The compile spec and the publish spec
            // both rule that an unresolved link is reported and the rest of
            // the skill publishes; the name is recorded in the manifest's
            // `unresolved` list so a note appearing there later counts as a
            // change. Refusing here would also make the manifest's own field
            // unreachable — nothing could ever be written into it.
            Err(ResolveError::NotFound) => Ok(None),
            Err(ResolveError::Ambiguous(candidates)) => Err(CoreError::refused(
                "link-ambiguous",
                format!(
                    "ambiguous Obsidian link from {}: {destination} ({})",
                    self.relative_or_verbatim(from_file),
                    candidates.join(", ")
                ),
            )),
        }
    }

    /// For messages only: the vault-relative spelling when there is one, the
    /// verbatim path when there is not — an error about a path must never
    /// itself error.
    fn relative_or_verbatim(&self, path: &Path) -> String {
        self.vault_relative(path)
            .unwrap_or_else(|_| path.display().to_string())
    }
}

/// One embed on the transclusion stack: the file and its fragment. Two embeds
/// of one file with different fragments are different hops, because they
/// expand different slices and only an exact repeat can loop.
type TransclusionHop = (PathBuf, Option<String>);

/// No Obsidian syntax may survive into emitted markdown: a destination has no
/// vault to resolve it against, so anything left is a broken reference the
/// author would only find after installing.
///
/// That includes a `[[` that is not a link (see [`scan_wiki`]): it would ship
/// as literal brackets. Only one at or after `body_start` counts, so stray
/// brackets in a resource's frontmatter, carried as written, are not refused.
/// A link there still is, as before.
///
/// The one exception is `shown`: the `/[[x]]` texts an escape wrote out on
/// purpose. Only that exact text, `/` included, passes.
fn validate_no_obsidian_syntax(
    text: &str,
    body_start: usize,
    output_path: &str,
    skill_name: &str,
    shown: &BTreeSet<String>,
) -> Result<(), CoreError> {
    // Calls are not rescanned: every live one was replaced or refused, and
    // what is left in code is content.
    let (links, leftovers) = scan_wiki(text, false);
    let link = links
        .iter()
        .find(|token| {
            let notation = token
                .start
                .checked_sub(1)
                .filter(|&slash| token.kind == TokenKind::Link && text.as_bytes()[slash] == b'/')
                .map(|slash| &text[slash..token.end]);
            !notation.is_some_and(|notation| shown.contains(notation))
        })
        .map(|token| (token.start, token.end, None));
    let leftover = leftovers
        .iter()
        .filter(|leftover| leftover.start >= body_start)
        .min_by_key(|leftover| leftover.start)
        .map(|leftover| (leftover.start, leftover.end, Some(leftover.why)));
    let Some((start, end, why)) = link.into_iter().chain(leftover).min_by_key(|first| first.0)
    else {
        return Ok(());
    };
    // An unclosed `[[` runs to the end of the text; its line is enough.
    let written = &text[start..end];
    let written = written.split('\n').next().unwrap_or(written);
    let why = why.map_or_else(String::new, |why| format!(" (not a link: {why})"));
    Err(CoreError::refused(
        "unresolved-syntax",
        format!(
            "compiled skill {skill_name} still contains Obsidian syntax in {output_path}: \
             {written}{why}"
        ),
    ))
}

/// Where a composed file's body starts: past its frontmatter, as the strict
/// split that carried it finds it.
fn body_start(text: &str) -> usize {
    text.len() - frontmatter_lossless::split(text).body.len()
}

// --- Destinations, extensions, labels ---------------------------------------

/// The extension of a link target as written, lowercased with its dot —
/// `None` when the basename has no dot or only a leading one. This decides
/// whether a target is addressed as a file or as a note.
///
/// `pub(crate)` for [`crate::adopt`], whose reverse rewrite applies the same
/// convention deciding which wikilink targets keep their extension.
pub(crate) fn extension_of_target(target: &str) -> Option<String> {
    let base = basename(target);
    match base.rfind('.') {
        Some(0) | None => None,
        Some(at) => Some(base[at..].to_lowercase()),
    }
}

/// The extension of a resolved path, same convention.
fn extension_of_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(extension_of_target)
}

pub(crate) fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// What a rewritten link displays: the alias when it is text (see
/// [`text_alias`]), otherwise the resolved file's name.
fn display_label(alias: Option<&str>, resolved: &Path) -> String {
    if let Some(alias) = text_alias(alias) {
        return alias.to_owned();
    }
    resolved
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The alias as words to display, or `None` when there is none or it is an
/// image size spec like `100` or `640x480`, which Obsidian reads as geometry.
fn text_alias(alias: Option<&str>) -> Option<&str> {
    alias.filter(|alias| !markdown_link::is_size_spec(alias))
}

// --- The scanner ------------------------------------------------------------

/// What a scanned token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    /// `[[x]]` — cite.
    Link,
    /// `![[x]]` — embed.
    Embed,
    /// `/[[x]]` — call. `in_code` renders it bare rather than in backticks.
    Call { in_code: bool },
    /// `\/[[x]]`: written out as the text `/[[x]]`, in prose and code alike.
    Escape,
    /// A `/[[x]]` in code whose `/` is not a sigil: left as written, with a
    /// warning, because raw Obsidian syntax in code is rarely meant.
    Unconverted,
    /// The `\` of an escaped hand-written call to a known skill, such as
    /// `\/plugin:skill`: dropped, so the call is written as it stands.
    LiteralEscape,
}

/// One `[[…]]`, `![[…]]` or `/[[…]]` occurrence (or one `\/` escape), with
/// the byte span replacement needs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WikiToken<'a> {
    /// Byte offset of the opening `[` (or `!`, `/`, `\`), inclusive.
    start: usize,
    /// Byte offset just past the closing `]]`, exclusive.
    end: usize,
    kind: TokenKind,
    /// The text before any `|`, trimmed, as written. What a refusal quotes.
    destination: &'a str,
    /// What resolution runs against, as `markdown-link` reads it: the text
    /// before any `#`, trimmed. Empty for a link to the note it is in.
    target: String,
    /// The `#heading` path, or `^block` with its caret.
    fragment: Option<String>,
    /// The text after the first `|`, trimmed; `None` when empty or absent.
    alias: Option<String>,
}

impl<'a> WikiToken<'a> {
    /// A token over `start..end` from a link `markdown-link` read out of
    /// `inner`, the text between its brackets.
    fn new(
        (start, end): (usize, usize),
        kind: TokenKind,
        inner: &'a str,
        target: String,
        fragment: Option<String>,
        alias: Option<String>,
    ) -> Self {
        WikiToken {
            start,
            end,
            kind,
            destination: written_destination(inner),
            target,
            fragment,
            alias: alias.filter(|alias| !alias.is_empty()),
        }
    }
}

/// How this pass reads a body's links: code by `skill-export`'s rules,
/// links in HTML comments skipped, and `[[#Heading]]` a link to the note it
/// is written in.
const SCAN: ScanOptions = ScanOptions {
    code_rules: CodeRules::SkillExport,
    skip_html_comments: true,
    self_links: true,
};

/// Every wiki token in `markdown` that is *live* — not inside a fenced code
/// block, an inline code span, or an HTML comment, where the same characters
/// are content rather than references.
///
/// The links and embeds are `markdown-link`'s, read under [`SCAN`]. Calls are
/// this crate's layer over them: with `calls`, a link whose `[[` follows a
/// `/` sigil is a `/[[x]]` call, and one whose `/` follows a `\` is a
/// `\/[[x]]` escape. See [`sigil_at`] for where a `/` is a sigil. Calls are
/// also found in code, where a usage line most often sits; in code, a
/// `/[[x]]` whose `/` is not a sigil is found as [`TokenKind::Unconverted`].
///
/// This is both halves of the pass in one function: the transform replaces
/// exactly what it finds, and the validation refuses anything it still finds
/// afterwards. One scanner means the two cannot disagree about what counts.
fn scan_wiki_tokens(markdown: &str, calls: bool) -> Vec<WikiToken<'_>> {
    scan_wiki(markdown, calls).0
}

/// A `[[` in a body that is not a link, so would ship as literal brackets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Leftover {
    /// Byte offset of the `[[`, or of the `!` before it.
    start: usize,
    /// Byte offset just past what the scanner read.
    end: usize,
    /// Why it is not a link, for a refusal.
    why: &'static str,
}

/// [`scan_wiki_tokens`], and each `[[` that is not a token: one
/// `markdown-link` gave up on, and a link with code in its target, which
/// this pass leaves as written.
fn scan_wiki(markdown: &str, calls: bool) -> (Vec<WikiToken<'_>>, Vec<Leftover>) {
    let bytes = markdown.as_bytes();
    let scan = scan_body_with(markdown, &SCAN);
    let mut leftovers: Vec<Leftover> = scan
        .abandoned
        .iter()
        // `[[WIP] Fix](url)` and `[[Note](Note.md)]` are Markdown links with
        // a `[` beside them, not wikilinks gone wrong.
        .filter(|abandoned| {
            let at = abandoned.byte_start + usize::from(abandoned.embed);
            abandoned.reason != AbandonReason::Bracket
                || !(opens_markdown_link(markdown, at) || opens_markdown_link(markdown, at + 1))
        })
        .map(|abandoned| Leftover {
            start: abandoned.byte_start,
            end: abandoned.byte_end,
            why: match abandoned.reason {
                AbandonReason::Bracket => "a `[` or `]` comes before its closing `]]`",
                AbandonReason::Code => "its target is only code",
                AbandonReason::EmptyTarget => "it has no target",
                AbandonReason::Unclosed => "it has no closing `]]`",
            },
        })
        .collect();
    let mut tokens: Vec<WikiToken<'_>> = scan
        .links
        .into_iter()
        .filter_map(|link| {
            let (kind, opener) = match link.kind {
                BodyLinkKind::Wikilink => (TokenKind::Link, 2),
                BodyLinkKind::Embed => (TokenKind::Embed, 3),
                BodyLinkKind::Markdown => return None,
            };
            let at = link.byte_start;
            let (kind, start) = match kind {
                TokenKind::Link
                    if calls
                        && at >= 2
                        && &bytes[at - 2..at] == b"\\/"
                        && sigil_at(bytes, at - 2) =>
                {
                    (TokenKind::Escape, at - 2)
                }
                TokenKind::Link if calls && at >= 1 && sigil_at(bytes, at - 1) => {
                    (TokenKind::Call { in_code: false }, at - 1)
                }
                _ => (kind, at),
            };
            let inner = &markdown[at + opener..link.byte_end - 2];
            // `raw` has code blanked. Code before the alias would leave a
            // target that is not what was written, so the link is left as
            // written, as one that is all code is.
            if !inner.starts_with(link.raw.split('|').next().unwrap_or_default()) {
                leftovers.push(Leftover {
                    start: link.byte_start,
                    end: link.byte_end,
                    why: "its target has code in it",
                });
                return None;
            }
            Some(WikiToken::new(
                (start, link.byte_end),
                kind,
                inner,
                link.target,
                fragment_of(link.heading, link.block),
                link.alias,
            ))
        })
        .collect();
    if calls {
        for (from, to) in code_regions(markdown) {
            scan_code_calls(markdown, from, to, &mut tokens);
        }
        tokens.sort_by_key(|token| token.start);
        // A call in code inside a link's alias is part of that alias.
        let mut end = 0;
        tokens.retain(|token| {
            let free = token.start >= end;
            if free {
                end = token.end;
            }
            free
        });
    }
    (tokens, leftovers)
}

/// Whether the `[` at byte `at` opens a Markdown link: its brackets balance
/// before a blank line, and the closing `]` is followed by `(` (inline) or
/// `[` (reference). A `\`-escaped bracket does not count.
fn opens_markdown_link(text: &str, at: usize) -> bool {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'[') {
        return false;
    }
    let mut depth = 0usize;
    let mut i = at;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'\n' if is_blank_line(&bytes[i + 1..]) => return false,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return matches!(bytes.get(i + 1), Some(b'(' | b'['));
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Whether `rest`, the text after a `\n`, starts with a blank line: one of
/// only spaces, tabs and a `\r`, which ends a paragraph.
fn is_blank_line(rest: &[u8]) -> bool {
    rest.iter().find(|b| !matches!(b, b' ' | b'\t' | b'\r')) == Some(&b'\n')
}

/// A link's text before any `|`, trimmed, as written, without the `\` that
/// escapes the pipe in a table cell.
fn written_destination(inner: &str) -> &str {
    inner
        .split_once('|')
        .map_or(inner, |(before, _)| {
            before.strip_suffix('\\').unwrap_or(before)
        })
        .trim()
}

/// `markdown-link`'s heading or block as the fragment selectors read it: a
/// heading path as written, a block with its `^`, nothing for an empty one.
fn fragment_of(heading: Option<String>, block: Option<String>) -> Option<String> {
    match (heading, block) {
        (_, Some(block)) => Some(format!("^{block}")),
        (Some(heading), None) => (!heading.is_empty()).then_some(heading),
        (None, None) => None,
    }
}

/// The byte ranges of `markdown` that are code: the inside of each inline
/// span and each fenced line, outside HTML comments.
///
/// Fences and inline spans are [`markdown_link::code`]'s lexer under
/// [`CodeRules::SkillExport`], the rules [`SCAN`] gives the link scanner. A
/// comment runs to `-->` or the end of input. All markers are ASCII, so the
/// byte-wise scan can never split a UTF-8 character.
fn code_regions(markdown: &str) -> Vec<(usize, usize)> {
    let bytes = markdown.as_bytes();
    let mut regions = Vec::new();
    let mut offset = 0usize;
    let mut fences = Fences::new(CodeRules::SkillExport);
    let mut spans = Spans::new(CodeRules::SkillExport, bytes);

    while offset < bytes.len() {
        let line_start = offset == 0 || bytes[offset - 1] == b'\n';
        if line_start {
            let line_end = markdown[offset..]
                .find('\n')
                .map_or(bytes.len(), |at| offset + at);
            let line = &bytes[offset..line_end];
            match fences.line(line) {
                FenceLine::Outside => spans.line(line),
                FenceLine::Marker => {
                    offset = advance_past(line_end, bytes.len());
                    continue;
                }
                FenceLine::Inside => {
                    regions.push((offset, line_end));
                    offset = advance_past(line_end, bytes.len());
                    continue;
                }
            }
        }

        if bytes[offset..].starts_with(b"<!--") {
            offset = match markdown[offset + 4..].find("-->") {
                Some(at) => offset + 4 + at + 3,
                None => bytes.len(),
            };
            continue;
        }

        if bytes[offset] == b'`' {
            let (run, close) = spans.close(offset);
            offset = match close {
                Some(at) => {
                    regions.push((offset + run, at));
                    at + run
                }
                None => offset + run,
            };
            continue;
        }

        offset += 1;
    }
    regions
}

/// The calls, escapes and unconverted calls in a code span or fenced line,
/// `from..to`.
fn scan_code_calls<'a>(markdown: &'a str, from: usize, to: usize, tokens: &mut Vec<WikiToken<'a>>) {
    let bytes = markdown.as_bytes();
    let mut offset = from;
    while offset < to {
        let token = if sigil_at(bytes, offset) {
            if bytes[offset] == b'\\' {
                code_token(markdown, offset, to, TokenKind::Escape)
            } else {
                code_token(markdown, offset, to, TokenKind::Call { in_code: true })
            }
        } else if bytes[offset..to].starts_with(b"/[[")
            && offset
                .checked_sub(1)
                .is_none_or(|before| bytes[before] != b'\\')
        {
            code_token(markdown, offset, to, TokenKind::Unconverted)
        } else {
            None
        };
        match token {
            Some(token) => {
                offset = token.end;
                tokens.push(token);
            }
            None => offset += 1,
        }
    }
}

/// A `/[[` or `\/[[` at `offset`, where the `/` (or `\`) starts a line or
/// follows whitespace or one of `` ( " ' ` * > _ | [ = ``. Anything else
/// before it makes it part of a word or path, so `[[A]]/[[B]]`, `dir/[[x]]`
/// and `./[[x]]` stay links.
fn sigil_at(bytes: &[u8], offset: usize) -> bool {
    let rest = &bytes[offset..];
    if !rest.starts_with(b"/[[") && !rest.starts_with(b"\\/[[") {
        return false;
    }
    match offset.checked_sub(1).map(|before| bytes[before]) {
        None => true,
        Some(before) => {
            before.is_ascii_whitespace()
                || matches!(
                    before,
                    b'(' | b'"' | b'\'' | b'`' | b'*' | b'>' | b'_' | b'|' | b'[' | b'='
                )
        }
    }
}

/// A call in code opening at `offset` (`/[[`, or `\/[[` for an escape),
/// closed by the first `]]` ending at or before `limit` and on the same line:
/// a wikilink never crosses a line, so a stray `[[` stays text. The text
/// between the brackets is read by `markdown-link`'s `parse_inner_self`.
fn code_token(
    markdown: &str,
    offset: usize,
    limit: usize,
    kind: TokenKind,
) -> Option<WikiToken<'_>> {
    let opener = if kind == TokenKind::Escape { 4 } else { 3 };
    let content_start = offset + opener;
    let rest = markdown.get(content_start..)?;
    let line_len = rest.find('\n').unwrap_or(rest.len());
    let bound = limit.checked_sub(content_start)?.min(line_len);
    let close = content_start + rest.get(..bound)?.find("]]")?;
    // Both boundaries sit beside ASCII brackets, so the str slice is valid
    // UTF-8 territory.
    let inner = &markdown[content_start..close];
    let link = parse_inner_self(inner, false)?;
    Some(WikiToken::new(
        (offset, close + 2),
        kind,
        inner,
        link.target,
        fragment_of(link.heading, link.block),
        link.alias,
    ))
}

/// A call as written into output: bare in code, in backticks in prose.
/// A call's invocation: `render`'s, or in a recording compile the mark
/// standing in for it.
fn render_or_mark(
    context: &CallContext<'_>,
    skill: &IndexedSkill,
    destination: &str,
    caller: String,
) -> Result<String, CoreError> {
    let Some(record) = context.record else {
        return (context.render)(skill);
    };
    let mut record = record.borrow_mut();
    record.calls.push(CallSite {
        skill: skill.clone(),
        destination: destination.to_owned(),
        caller,
    });
    Ok(mark('c', record.calls.len() - 1))
}

fn format_call(call: &str, in_code: bool) -> String {
    if in_code {
        call.to_owned()
    } else {
        format!("`{call}`")
    }
}

/// How a hand-written call spells its skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralForm<'a> {
    /// `/plugin:skill` (Claude Code) or `$plugin:skill` (Codex).
    Qualified { plugin: &'a str, skill: &'a str },
    /// `/skill:plugin-skill` (Pi).
    Pi { prefixed: &'a str },
    /// `/skill` (Cursor).
    Bare { skill: &'a str },
}

/// A skill call written out by hand, as [`literal_skill_calls`] finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiteralCall<'a> {
    /// Byte offset of the `\` when escaped, else of the sigil.
    start: usize,
    /// The call as written, without the `\`.
    text: &'a str,
    /// Written `\/…` or `\$…`: meant literally.
    escaped: bool,
    form: LiteralForm<'a>,
}

/// Every hand-written call shape in `body`, in prose and code alike: whether
/// it names a known skill is the caller's question.
///
/// `/plugin:skill`, `$plugin:skill` and `/skill:plugin-skill` count unless
/// the sigil continues a word or path, so `${CLAUDE_PLUGIN_ROOT}/bin/tool`
/// and `a/b:c` are not calls. A bare `/skill` is riskier, so its `/` must
/// start a line or follow whitespace or one of `` ( " ' ` * > `` (but not the
/// `(` of a markdown link destination, `[site](/docs)`), and the name must not
/// run on into a path (`/skill/x`, `/skill.md`). A `:` ends the name unless a
/// name follows it. A `\` before the sigil marks the call as meant literally;
/// the byte before the `\` must pass the same test.
fn literal_skill_calls(body: &str) -> Vec<LiteralCall<'_>> {
    let bytes = body.as_bytes();
    let is_name = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-';
    let name_end = |from: usize| {
        let mut at = from;
        while at < bytes.len() && is_name(bytes[at]) {
            at += 1;
        }
        at
    };
    let mut found = Vec::new();
    for (at, &sigil) in bytes.iter().enumerate() {
        if sigil != b'/' && sigil != b'$' {
            continue;
        }
        let escaped = at > 0 && bytes[at - 1] == b'\\';
        let start = if escaped { at - 1 } else { at };
        let before = start.checked_sub(1).map(|before| bytes[before]);
        let first_end = name_end(at + 1);
        if first_end == at + 1 {
            continue;
        }
        // A `:` with no name after it ends the word, as in `Run /x: …`.
        let qualified = bytes.get(first_end) == Some(&b':')
            && bytes.get(first_end + 1).is_some_and(|b| is_name(*b));
        if qualified {
            if before.is_some_and(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'}' | b'/' | b'.' | b'-' | b'$')
            }) {
                continue;
            }
            let second_end = name_end(first_end + 1);
            if second_end == first_end + 1
                || bytes
                    .get(second_end)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                continue;
            }
            let first = &body[at + 1..first_end];
            let second = &body[first_end + 1..second_end];
            let form = if sigil == b'/' && first == "skill" {
                LiteralForm::Pi { prefixed: second }
            } else {
                LiteralForm::Qualified {
                    plugin: first,
                    skill: second,
                }
            };
            found.push(LiteralCall {
                start,
                text: &body[at..second_end],
                escaped,
                form,
            });
            continue;
        }
        // A `(` right after `]` opens a markdown link's destination, a path.
        let link_destination = before == Some(b'(') && start >= 2 && bytes[start - 2] == b']';
        let boundary = !link_destination
            && before.is_none_or(|b| {
                b.is_ascii_whitespace() || matches!(b, b'(' | b'"' | b'\'' | b'`' | b'*' | b'>')
            });
        let runs_on = match bytes.get(first_end) {
            None => false,
            Some(b'.') => bytes
                .get(first_end + 1)
                .is_some_and(|b| b.is_ascii_alphanumeric()),
            Some(b) => b.is_ascii_alphanumeric() || matches!(b, b'_' | b'/' | b'@'),
        };
        if sigil == b'/' && boundary && !runs_on {
            found.push(LiteralCall {
                start,
                text: &body[at..first_end],
                escaped,
                form: LiteralForm::Bare {
                    skill: &body[at + 1..first_end],
                },
            });
        }
    }
    found
}

/// One past a line's `\n`, saturating at the end of input.
fn advance_past(line_end: usize, len: usize) -> usize {
    if line_end >= len { len } else { line_end + 1 }
}

// --- Fragment selection -----------------------------------------------------

/// The slice of `body` a fragment addresses: `#^id` a block, anything else a
/// heading path, no fragment the whole body — Obsidian's rules, as
/// `markdown-contract`'s `select` reads them. `source_label` names the file
/// being selected *from* in a refusal, because the author's fix is there —
/// not in the file that asked.
fn select_fragment(
    body: &str,
    fragment: Option<&str>,
    source_label: &str,
) -> Result<String, CoreError> {
    markdown_contract::select::fragment(body, fragment).ok_or_else(|| {
        let missing = match fragment.and_then(|f| f.strip_prefix('^')) {
            Some(block_id) => format!("block not found in {source_label}: ^{block_id}"),
            None => format!(
                "heading not found in {source_label}: {}",
                fragment.unwrap_or_default()
            ),
        };
        CoreError::refused("fragment-unresolved", missing)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;

    /// Writes one file, creating its directories — the shape every vault
    /// fixture below is built from.
    fn write(vfs: &MemVfs, path: &str, content: &[u8]) {
        let path = Path::new(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(path, content).unwrap();
    }

    fn source<'a>(bundle_root: Option<&'a Path>) -> CompileSource<'a> {
        CompileSource {
            vault_root: Path::new("/vault"),
            entry_path: Path::new("/vault/plugins/demo/demo.md"),
            bundle_root,
            skill_name: "demo",
            calls: None,
            links: None,
        }
    }

    /// A demo vault reused by several tests below.
    fn demo_vault() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/plugins/demo/demo.md",
            b"---\nname: demo\ndescription: d\n---\n\n# Demo\n",
        );
        write(
            &vfs,
            "/vault/Shared.md",
            b"---\ntag: fragment\n---\n\n## Shared instructions\n\n- Preserve this list.\n",
        );
        write(
            &vfs,
            "/vault/Sections.md",
            b"# Other\n\nIgnore this.\n\n## Chosen\n\nKeep this section.\n\n### Nested\n\nKeep the nested section too.\n\n## Later\n\nDo not include this.\n",
        );
        write(
            &vfs,
            "/vault/Blocks.md",
            b"Before.\n\nBlock content for export. ^example\n\nAfter.\n",
        );
        write(
            &vfs,
            "/vault/plugins/demo/references/local.md",
            b"# Local\n\n![[Shared]]\n",
        );
        write(
            &vfs,
            "/vault/plugins/demo/scripts/tool.sh",
            b"#!/bin/sh\necho ok\n",
        );
        vfs.set_executable(Path::new("/vault/plugins/demo/scripts/tool.sh"), true)
            .unwrap();
        write(&vfs, "/vault/Docs/manual.pdf", b"%PDF-test");
        write(&vfs, "/vault/Media/pic.png", b"PNG-test");
        vfs
    }

    /// The demo entry body.
    const DEMO_BODY: &str = "# Demo\n\n![[Shared]]\n\n![[Sections#Chosen]]\n\n![[Blocks#^example]]\n\nRead [[references/local|Local reference]].\n\n![[Docs/manual.pdf#page=2]]\n\n![[Media/pic.png|100]]\n\nKeep `![[Missing inline example]]` unchanged.\n\n```md\n![[Missing fenced example]]\n```\n";

    fn resource<'a>(compiled: &'a CompiledNote, path: &str) -> &'a (String, Vec<u8>, bool) {
        compiled
            .resources
            .iter()
            .find(|(p, _, _)| p == path)
            .unwrap_or_else(|| panic!("no resource at {path}: {:?}", paths(compiled)))
    }

    fn paths(compiled: &CompiledNote) -> Vec<&str> {
        compiled
            .resources
            .iter()
            .map(|(p, _, _)| p.as_str())
            .collect()
    }

    // --- The closure ---------------------------------------------------------

    #[test]
    fn an_embedded_note_joins_the_closure_of_the_file_it_lands_in() {
        let vfs = demo_vault();
        let compiled = compile_note(&vfs, &source(None), "# Demo\n\n![[Shared]]\n").unwrap();

        assert_eq!(
            compiled.closures["SKILL.md"].contributing_notes,
            vec!["Shared.md".to_owned(), "plugins/demo/demo.md".to_owned()]
        );
    }

    // The asymmetry that makes the closure worth computing at all. An embed
    // folds another note's bytes into this file, so it has to be hashed here.
    // A link produces its own file with its own entry, so its edits are caught
    // by that file's own comparison and it is not a member here.
    #[test]
    fn a_linked_note_gets_its_own_closure_rather_than_joining_the_skills() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        let compiled = compile_note(
            &vfs,
            &source(Some(bundle)),
            "# Demo\n\nRead [[references/local|Local reference]].\n",
        )
        .unwrap();

        let skill = &compiled.closures["SKILL.md"].contributing_notes;
        assert_eq!(skill, &vec!["plugins/demo/demo.md".to_owned()]);
        assert!(
            !skill.contains(&"plugins/demo/references/local.md".to_owned()),
            "a linked note must not join the linking file's closure: {skill:?}"
        );

        // …and the linked file's own closure carries what *it* embeds.
        let local = &compiled.closures["references/local.md"].contributing_notes;
        assert_eq!(
            local,
            &vec![
                "Shared.md".to_owned(),
                "plugins/demo/references/local.md".to_owned()
            ]
        );
    }

    #[test]
    fn embedding_is_transitive_through_the_closure() {
        let vfs = demo_vault();
        write(&vfs, "/vault/Deep.md", b"Deepest.\n");
        write(&vfs, "/vault/Middle.md", b"Middle.\n\n![[Deep]]\n");

        let compiled = compile_note(&vfs, &source(None), "# Demo\n\n![[Middle]]\n").unwrap();

        assert_eq!(
            compiled.closures["SKILL.md"].contributing_notes,
            vec![
                "Deep.md".to_owned(),
                "Middle.md".to_owned(),
                "plugins/demo/demo.md".to_owned()
            ]
        );
    }

    // A closure is never empty, so a hash over it always covers something —
    // including for a peer resource, whose closure is itself.
    #[test]
    fn every_emitted_file_has_a_closure_holding_at_least_its_own_owner() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        let compiled = compile_note(&vfs, &source(Some(bundle)), DEMO_BODY).unwrap();

        for (path, _, _) in &compiled.resources {
            let closure = compiled
                .closures
                .get(path)
                .unwrap_or_else(|| panic!("no closure for {path}"));
            assert!(
                !closure.contributing_notes.is_empty(),
                "{path} has an empty closure"
            );
        }
        assert_eq!(
            compiled.closures["scripts/tool.sh"].contributing_notes,
            vec!["plugins/demo/scripts/tool.sh".to_owned()]
        );
    }

    // Recorded against the file it appears in, not against the skill as a
    // whole — that is what lets the cause be named.
    #[test]
    fn an_unresolved_name_is_recorded_against_the_file_it_appears_in() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        write(
            &vfs,
            "/vault/plugins/demo/references/local.md",
            b"# Local\n\nSee [[AlsoMissing]].\n",
        );

        let compiled = compile_note(
            &vfs,
            &source(Some(bundle)),
            "# Demo\n\nSee [[Missing]]. Read [[references/local]].\n",
        )
        .unwrap();

        assert_eq!(
            compiled.closures["SKILL.md"].unresolved,
            vec!["Missing".to_owned()]
        );
        assert_eq!(
            compiled.closures["references/local.md"].unresolved,
            vec!["AlsoMissing".to_owned()]
        );
    }

    #[test]
    fn related_separates_an_embed_from_a_link_and_keeps_the_name_that_resolved_to_nothing() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        let related = related_of(
            &vfs,
            &source(Some(bundle)),
            "# Demo\n\n![[Shared]]\n\nRead [[references/local]] and [[Sections]], then [[Missing]].\n",
        )
        .unwrap();

        assert_eq!(related.embeds, vec!["Shared.md".to_owned()]);
        assert_eq!(
            related.links,
            vec![
                "Sections.md".to_owned(),
                "plugins/demo/references/local.md".to_owned()
            ]
        );
        assert_eq!(related.unresolved, vec!["Missing".to_owned()]);
    }

    // AC-2's property, and the reason this reuses the compiler's walk rather
    // than scanning the body once: the second hop is only visible from inside
    // the expansion.
    #[test]
    fn related_embeds_reach_two_hops_deep() {
        let vfs = demo_vault();
        write(&vfs, "/vault/Deep.md", b"Deepest.\n");
        write(&vfs, "/vault/Middle.md", b"Middle.\n\n![[Deep]]\n");

        let related = related_of(&vfs, &source(None), "# Demo\n\n![[Middle]]\n").unwrap();

        assert_eq!(
            related.embeds,
            vec!["Deep.md".to_owned(), "Middle.md".to_owned()]
        );
    }

    // The bundle peers are the walk's seed, not something the body reached, so
    // they stay out of `links` — and a peer nothing mentions is still shipped.
    #[test]
    fn related_lists_bundle_peers_as_resources_and_never_as_links() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        let related = related_of(&vfs, &source(Some(bundle)), "# Demo\n").unwrap();

        assert_eq!(
            related.resources,
            vec![
                "plugins/demo/references/local.md".to_owned(),
                "plugins/demo/scripts/tool.sh".to_owned()
            ]
        );
        assert!(related.links.is_empty(), "{:?}", related.links);
    }

    // --- Exporter tests -----------------------------------------------------

    #[test]
    fn a_demo_skill_compiles_end_to_end() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        let compiled = compile_note(&vfs, &source(Some(bundle)), DEMO_BODY).unwrap();

        // A whole-note embed pulls the body, never the frontmatter.
        assert!(compiled.body.contains("## Shared instructions"));
        assert!(compiled.body.contains("- Preserve this list."));
        assert!(!compiled.body.contains("tag: fragment"));
        // A heading fragment keeps its nested subsections and stops at the
        // next sibling heading.
        assert!(compiled.body.contains("## Chosen"));
        assert!(compiled.body.contains("### Nested"));
        assert!(!compiled.body.contains("## Later"));
        // A block fragment selects the labelled text without its marker.
        assert!(compiled.body.contains("Block content for export."));
        assert!(!compiled.body.contains("^example"));
        // A bundle-relative link keeps the bundle layout; alias wins.
        assert!(
            compiled
                .body
                .contains("[Local reference](references/local.md)")
        );
        // A binary embed with a fragment becomes a link carrying it.
        assert!(
            compiled
                .body
                .contains("[manual.pdf](references/vault/Docs/manual.pdf#page=2)")
        );
        // An image embed renders as an image; a size-spec alias is geometry,
        // not a label.
        assert!(
            compiled
                .body
                .contains("![pic.png](assets/vault/Media/pic.png)")
        );
        // Code is content: inline and fenced Obsidian syntax survives
        // untouched and does not fail validation.
        assert!(compiled.body.contains("`![[Missing inline example]]`"));
        assert!(compiled.body.contains("![[Missing fenced example]]"));

        // The bundle markdown resource is itself compiled — its embed
        // expanded — and the external files land under their category roots.
        let (_, local, _) = resource(&compiled, "references/local.md");
        assert!(String::from_utf8_lossy(local).contains("## Shared instructions"));
        let (_, pdf, _) = resource(&compiled, "references/vault/Docs/manual.pdf");
        assert_eq!(pdf, b"%PDF-test");
        let (_, png, _) = resource(&compiled, "assets/vault/Media/pic.png");
        assert_eq!(png, b"PNG-test");
        // The execute bit survives the copy.
        let (_, _, executable) = resource(&compiled, "scripts/tool.sh");
        assert!(executable);
    }

    #[test]
    fn the_committed_fixture_compiles_with_its_bundled_embed_inlined() {
        // A real filesystem, because the fixture is real files.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent-skills-vault");
        let entry = root.join(
            "marketplaces/house/plugins/tools/skills/workflow-checklist/workflow-checklist.md",
        );
        let bundle = root.join("marketplaces/house/plugins/tools/skills/workflow-checklist");
        let vfs = crate::vfs::NativeVfs;

        let raw = read_utf8(&vfs, &entry).unwrap();
        let (_, body) = split_frontmatter(&raw);
        let compile_source = CompileSource {
            vault_root: &root,
            entry_path: &entry,
            bundle_root: Some(&bundle),
            skill_name: "workflow-checklist",
            calls: None,
            links: None,
        };
        let compiled = compile_note(&vfs, &compile_source, body).unwrap();

        assert!(compiled.body.contains("# Workflow Checklist"));
        assert!(compiled.body.contains("# Steps"));
        assert!(compiled.body.contains("Understand the requested outcome."));
        assert!(!compiled.body.contains("![[references/steps]]"));
        // The embedded note also ships, at its bundle-relative path.
        let (_, steps, _) = resource(&compiled, "references/steps.md");
        assert!(
            String::from_utf8_lossy(&steps.clone()).contains("Understand the requested outcome.")
        );
    }

    /// An entry or bundle root must be inside the vault both as spelled and
    /// on disk: one linked out of the vault, or spelled through an alias of
    /// it, is refused before the walk.
    #[cfg(unix)]
    #[test]
    fn an_entry_or_bundle_root_outside_the_vault_by_link_or_spelling_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().join("vault");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(vault.join("skills/demo")).unwrap();
        std::fs::create_dir_all(outside.join("demo")).unwrap();
        std::fs::write(vault.join("skills/demo/demo.md"), "# Demo\n").unwrap();
        std::fs::write(outside.join("demo/demo.md"), "# Demo\n").unwrap();
        std::os::unix::fs::symlink(&outside, vault.join("linked")).unwrap();
        let alias = tmp.path().join("alias");
        std::os::unix::fs::symlink(&vault, &alias).unwrap();

        let vfs = crate::vfs::NativeVfs;
        let inside_entry = vault.join("skills/demo/demo.md");
        let inside_bundle = vault.join("skills/demo");
        let linked_entry = vault.join("linked/demo/demo.md");
        let linked_bundle = vault.join("linked/demo");
        let aliased_entry = alias.join("skills/demo/demo.md");
        let aliased_bundle = alias.join("skills/demo");
        let cases: [(&Path, Option<&Path>); 4] = [
            (&linked_entry, None),
            (&aliased_entry, None),
            (&inside_entry, Some(&linked_bundle)),
            (&inside_entry, Some(&aliased_bundle)),
        ];
        for (entry, bundle) in cases {
            let compile_source = CompileSource {
                vault_root: &vault,
                entry_path: entry,
                bundle_root: bundle,
                skill_name: "demo",
                calls: None,
                links: None,
            };
            let err = compile_note(&vfs, &compile_source, "# Demo\n").unwrap_err();
            // The early check names the root it refused; a later failure in
            // the walk would name a file under it instead.
            let expected = bundle.unwrap_or(entry);
            assert!(
                matches!(&err, CoreError::PathEscapesRoot(p) if p == expected),
                "{} / {bundle:?}: {err}",
                entry.display()
            );
        }

        // The same tree, spelled inside the vault, compiles.
        let compile_source = CompileSource {
            vault_root: &vault,
            entry_path: &inside_entry,
            bundle_root: Some(&inside_bundle),
            skill_name: "demo",
            calls: None,
            links: None,
        };
        compile_note(&vfs, &compile_source, "# Demo\n").unwrap();
    }

    #[test]
    fn a_transclusion_cycle_is_refused_naming_the_cycle() {
        let vfs = demo_vault();
        write(&vfs, "/vault/A.md", b"![[B]]\n");
        write(&vfs, "/vault/B.md", b"![[A]]\n");

        let err = compile_note(&vfs, &source(None), "# Demo\n\n![[A]]\n").unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused(_)), "{message}");
        assert!(message.contains("cycle"), "{message}");
        assert!(message.contains("demo"), "{message}");
        // The loop, spelt in vault-relative hops from its first repeat.
        assert!(message.contains("A.md -> B.md -> A.md"), "{message}");
    }

    #[test]
    fn a_cycle_through_a_heading_ending_in_a_hash_names_the_whole_heading() {
        let vfs = demo_vault();
        write(&vfs, "/vault/A.md", b"# Learn C#\n\n![[A#Learn C#]]\n");

        let err = compile_note(&vfs, &source(None), "![[A#Learn C#]]\n").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("A.md#Learn C# -> A.md#Learn C#"),
            "{message}"
        );
    }

    #[test]
    fn a_note_embedding_itself_is_the_one_hop_cycle() {
        let vfs = demo_vault();
        let err = compile_note(&vfs, &source(None), "![[demo]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("cycle"), "{message}");
        assert!(
            message.contains("plugins/demo/demo.md -> plugins/demo/demo.md"),
            "{message}"
        );
    }

    #[test]
    fn an_ambiguous_link_is_refused_naming_every_candidate() {
        let vfs = demo_vault();
        write(&vfs, "/vault/One/Shared2.md", b"One.\n");
        write(&vfs, "/vault/Two/Shared2.md", b"Two.\n");

        let err = compile_note(&vfs, &source(None), "![[Shared2]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("link-ambiguous"), "{message}");
        assert!(message.contains("One/Shared2.md"), "{message}");
        assert!(message.contains("Two/Shared2.md"), "{message}");
    }

    #[test]
    fn an_exact_path_beats_the_basename_ladder() {
        // Root-level Shared.md and One/Shared.md share a stem, but the exact
        // vault-relative match answers before the basename map is consulted —
        // so no ambiguity refusal, and no guessing either.
        let vfs = demo_vault();
        write(&vfs, "/vault/One/Shared.md", b"The nested one.\n");

        let compiled =
            compile_note(&vfs, &source(None), "![[Shared]]\n\n![[One/Shared]]\n").unwrap();
        assert!(
            compiled.body.contains("Preserve this list."),
            "{}",
            compiled.body
        );
        assert!(
            compiled.body.contains("The nested one."),
            "{}",
            compiled.body
        );
    }

    // --- Resolution at the edges, as `resolve_strict` reads them ------------

    #[test]
    fn a_partial_path_names_the_one_file_whose_path_ends_with_it() {
        // The bare name `x` is shared, but only one path ends with `sub/x`.
        let vfs = demo_vault();
        write(&vfs, "/vault/One/sub/x.md", b"One sub.\n");
        write(&vfs, "/vault/Two/x.md", b"Two.\n");
        let compiled = compile_note(&vfs, &source(None), "![[sub/x]]\n").unwrap();
        assert_eq!(compiled.body, "One sub.\n");

        // Two paths ending with it are still ambiguous.
        write(&vfs, "/vault/Two/sub/x.md", b"Two sub.\n");
        let message = compile_note(&vfs, &source(None), "![[sub/x]]\n")
            .unwrap_err()
            .to_string();
        assert!(message.contains("link-ambiguous"), "{message}");
        assert!(
            message.contains("(One/sub/x.md, Two/sub/x.md)"),
            "{message}"
        );
    }

    #[test]
    fn a_dot_slash_target_is_relative_to_the_linking_note() {
        let vfs = demo_vault();
        write(&vfs, "/vault/near.md", b"At the root.\n");
        write(&vfs, "/vault/plugins/demo/near.md", b"Beside the entry.\n");
        let compiled = compile_note(&vfs, &source(None), "![[near]]\n\n![[./near]]\n").unwrap();
        // The bare name is the exact vault path; `./` is the linking note's
        // folder.
        assert_eq!(compiled.body, "At the root.\n\nBeside the entry.\n");
    }

    #[test]
    fn a_dot_dot_target_is_refused_before_it_resolves() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/up.md", b"One up.\n");
        let message = compile_note(&vfs, &source(None), "![[../up]]\n")
            .unwrap_err()
            .to_string();
        assert!(message.contains("unsafe-path"), "{message}");
        assert!(message.contains("../up"), "{message}");
    }

    #[test]
    fn a_percent_encoded_target_is_decoded() {
        let vfs = demo_vault();
        write(&vfs, "/vault/My Note.md", b"Spaced.\n");
        let compiled = compile_note(&vfs, &source(None), "![[My%20Note]]\n").unwrap();
        assert_eq!(compiled.body, "Spaced.\n");
    }

    #[test]
    fn ambiguous_candidates_are_listed_shortest_path_first() {
        let vfs = demo_vault();
        write(&vfs, "/vault/Deep/er/Shared2.md", b"Deep.\n");
        write(&vfs, "/vault/Two/Shared2.md", b"Two.\n");
        let message = compile_note(&vfs, &source(None), "[[Shared2]]\n")
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("(Two/Shared2.md, Deep/er/Shared2.md)"),
            "{message}"
        );
    }

    // The compile spec: "An unresolved link is not a blocker, and that is
    // deliberate. The rest of the skill publishes. The name is reported
    // against the note it appears in and recorded in the manifest's
    // `unresolved` list."
    #[test]
    fn a_link_to_nothing_is_recorded_rather_than_refused() {
        let vfs = demo_vault();
        let compiled = compile_note(&vfs, &source(None), "See [[Nope]].\n").unwrap();

        assert_eq!(compiled.body, "See Nope.\n");
        assert_eq!(
            compiled.closures["SKILL.md"].unresolved,
            vec!["Nope".to_owned()]
        );
    }

    #[test]
    fn an_unresolved_link_keeps_its_alias_as_the_words_on_the_page() {
        let vfs = demo_vault();
        let compiled =
            compile_note(&vfs, &source(None), "See [[Nope|the house style]].\n").unwrap();

        assert_eq!(compiled.body, "See the house style.\n");
        // The recorded name is the target, not the alias: it is the name a
        // note would have to appear under to change this output.
        assert_eq!(
            compiled.closures["SKILL.md"].unresolved,
            vec!["Nope".to_owned()]
        );
    }

    #[test]
    fn an_unsafe_link_target_is_refused() {
        let vfs = demo_vault();
        for body in ["[[../escape]]\n", "[[/etc/passwd]]\n", "![[a/../../b]]\n"] {
            let err = compile_note(&vfs, &source(None), body).unwrap_err();
            let message = err.to_string();
            assert!(message.contains("unsafe-path"), "{body}: {message}");
        }
    }

    #[test]
    fn a_missing_fragment_is_a_refusal_naming_source_and_fragment() {
        let vfs = demo_vault();

        let err = compile_note(&vfs, &source(None), "![[Sections#Missing]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("fragment-unresolved"), "{message}");
        assert!(message.contains("Sections.md"), "{message}");
        assert!(message.contains("Missing"), "{message}");

        let err = compile_note(&vfs, &source(None), "![[Blocks#^nope]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("fragment-unresolved"), "{message}");
        assert!(message.contains("Blocks.md"), "{message}");
        assert!(message.contains("^nope"), "{message}");
    }

    // --- Resources, collisions, wiring --------------------------------------

    #[test]
    fn resource_discovery_is_transitive() {
        let vfs = demo_vault();
        write(&vfs, "/vault/One.md", b"See [[Two]].\n");
        write(&vfs, "/vault/Two.md", b"See [[Three]].\n");
        write(&vfs, "/vault/Three.md", b"End.\n");

        let compiled = compile_note(&vfs, &source(None), "Start at [[One]].\n").unwrap();
        // Each hop was collected, and each collected body was compiled: One's
        // link to Two is already a relative markdown link, spelt from One's
        // own output directory.
        let (_, one, _) = resource(&compiled, "references/vault/One.md");
        assert!(String::from_utf8_lossy(one).contains("[Two.md](Two.md)"));
        let (_, two, _) = resource(&compiled, "references/vault/Two.md");
        assert!(String::from_utf8_lossy(two).contains("[Three.md](Three.md)"));
        assert!(paths(&compiled).contains(&"references/vault/Three.md"));
    }

    #[test]
    fn bundle_root_none_collects_only_what_links_reach() {
        let vfs = demo_vault();
        let compiled = compile_note(&vfs, &source(None), "Read [[Blocks]].\n").unwrap();
        // No seeded bundle files — not local.md, not tool.sh — only the
        // linked note.
        assert_eq!(paths(&compiled), vec!["references/vault/Blocks.md"]);
    }

    #[test]
    fn markdown_resources_keep_their_frontmatter_and_execute_bits() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/Extra.md",
            b"---\ntag: x\n---\nExtra body with [[Blocks]].\n",
        );
        vfs.set_executable(Path::new("/vault/Extra.md"), true)
            .unwrap();

        let compiled = compile_note(&vfs, &source(None), "See [[Extra]].\n").unwrap();
        let (_, extra, executable) = resource(&compiled, "references/vault/Extra.md");
        // The author's frontmatter travels byte-for-byte; only the body's
        // links were rewritten.
        assert_eq!(
            String::from_utf8_lossy(extra),
            "---\ntag: x\n---\nExtra body with [Blocks.md](Blocks.md).\n"
        );
        assert!(executable);
    }

    #[test]
    fn two_sources_claiming_one_output_path_are_refused() {
        // A bundle file that happens to sit at references/vault/X.md collides
        // with the vault-collected copy of a root-level X.md — one path, two
        // sources, and either winner drops the other's bytes.
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/plugins/demo/references/vault/X.md",
            b"bundled\n",
        );
        write(&vfs, "/vault/X.md", b"vault-wide\n");

        let bundle = Path::new("/vault/plugins/demo");
        let err = compile_note(&vfs, &source(Some(bundle)), "See [[X]].\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("path-collision"), "{message}");
        assert!(message.contains("demo"), "{message}");
        assert!(message.contains("references/vault/X.md"), "{message}");
    }

    #[test]
    fn a_bundle_file_named_skill_md_is_refused() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/SKILL.md", b"impostor\n");

        let bundle = Path::new("/vault/plugins/demo");
        let err = compile_note(&vfs, &source(Some(bundle)), "# Demo\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("path-collision"), "{message}");
        assert!(message.contains("SKILL.md"), "{message}");
    }

    #[test]
    fn tests_and_fixtures_directories_under_the_bundle_root_never_ship() {
        // The motivating case: a fixture shipping literal `[[...]]` test data,
        // which would otherwise read as an unresolved link and fail
        // `unresolved-syntax` on a vault that has nothing wrong with it.
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/plugins/demo/tests/fixtures/case/some-note.md",
            b"---\nid: X\n---\n\n[[not-a-real-note]]\n",
        );
        write(
            &vfs,
            "/vault/plugins/demo/tests/run_evals.test.ts",
            b"// not a vault note either\n",
        );
        // A directory named `fixtures` directly under the bundle root, not
        // only nested under `tests/`, is excluded the same way.
        write(
            &vfs,
            "/vault/plugins/demo/fixtures/other.md",
            b"[[also-not-real]]\n",
        );

        let bundle = Path::new("/vault/plugins/demo");
        let compiled = compile_note(&vfs, &source(Some(bundle)), "# Demo\n").unwrap();
        for (path, _, _) in &compiled.resources {
            assert!(!path.starts_with("tests/"), "{path} shipped from tests/");
            assert!(
                !path.starts_with("fixtures/"),
                "{path} shipped from fixtures/"
            );
        }
    }

    #[test]
    fn a_wikilink_in_resource_frontmatter_fails_validation() {
        // The transform never touches frontmatter, so a wikilink there rides
        // into the emitted file — where the validation pass, which scans the
        // whole emitted text, refuses it.
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/Fm.md",
            b"---\nrelated: \"[[Blocks]]\"\n---\nBody.\n",
        );

        let err = compile_note(&vfs, &source(None), "See [[Fm]].\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("unresolved-syntax"), "{message}");
        assert!(message.contains("demo"), "{message}");
        assert!(message.contains("references/vault/Fm.md"), "{message}");
    }

    #[test]
    fn compile_projection_feeds_project_skill_with_compiled_output() {
        // The wiring: vault read → compile → projection. What lands at the
        // destination is the flattened body, the collected references, and
        // the bundle files with their bits — through the same project_skill
        // every publish already uses.
        use crate::namespace::Namespace;
        use crate::projection::project_skill;
        use crate::vault::read_vault_skill;

        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/skills/demo.md",
            b"---\ndescription: d\n---\nIntro.\n\n![[Shared]]\n\nSee [[Ref]].\n",
        );
        write(&vfs, "/vault/Shared.md", b"Shared body.\n");
        write(&vfs, "/vault/Ref.md", b"Ref body.\n");
        write(&vfs, "/vault/skills/demo/scripts/run.sh", b"#!/bin/sh\n");
        vfs.set_executable(Path::new("/vault/skills/demo/scripts/run.sh"), true)
            .unwrap();

        let ns = Namespace::default();
        let mut note = read_vault_skill(&vfs, Path::new("/vault/skills/demo.md"), &ns).unwrap();
        let compile_source = CompileSource {
            vault_root: Path::new("/vault"),
            entry_path: Path::new("/vault/skills/demo.md"),
            // The vault-canonical peer folder is this note's bundle root.
            bundle_root: Some(Path::new("/vault/skills/demo")),
            skill_name: &note.projection.content.name.clone(),
            calls: None,
            links: None,
        };
        compile_projection(&vfs, &compile_source, &mut note.projection).unwrap();

        let files = project_skill(&note.projection, &["claude-code".to_owned()], &ns).unwrap();
        let by_path: std::collections::BTreeMap<&str, &crate::projection::ProjectedFile> = files
            .iter()
            .map(|f| (f.relative_path.as_str(), f))
            .collect();

        let skill_md =
            String::from_utf8(by_path[".claude/skills/demo/SKILL.md"].content.clone()).unwrap();
        assert!(skill_md.contains("Shared body."), "{skill_md}");
        assert!(
            skill_md.contains("[Ref.md](references/vault/Ref.md)"),
            "{skill_md}"
        );
        assert!(!skill_md.contains("![["), "{skill_md}");
        assert!(by_path.contains_key(".claude/skills/demo/references/vault/Ref.md"));
        assert!(by_path[".claude/skills/demo/scripts/run.sh"].executable);
    }

    #[test]
    fn compiling_twice_yields_identical_output() {
        let vfs = demo_vault();
        let bundle = Path::new("/vault/plugins/demo");
        let first = compile_note(&vfs, &source(Some(bundle)), DEMO_BODY).unwrap();
        let second = compile_note(&vfs, &source(Some(bundle)), DEMO_BODY).unwrap();
        assert_eq!(first, second);
    }

    // --- Link spelling -------------------------------------------------------

    #[test]
    fn labels_prefer_aliases_and_escape_markdown_delimiters() {
        let vfs = demo_vault();
        let compiled = compile_note(
            &vfs,
            &source(None),
            "See [[Docs/manual.pdf|notes \\ 3]].\n\n![[Media/pic.png|Diagram]]\n\n![[Media/pic.png|640x480]]\n",
        )
        .unwrap();
        // A `\` in an alias is escaped so the label cannot break the link
        // syntax around it. (A `[` or `]` cannot reach a label: a wikilink
        // holding one is not a link.)
        assert!(
            compiled
                .body
                .contains("[notes \\\\ 3](references/vault/Docs/manual.pdf)"),
            "{}",
            compiled.body
        );
        // A real alias labels an image; a geometry alias does not.
        assert!(
            compiled
                .body
                .contains("![Diagram](assets/vault/Media/pic.png)")
        );
        assert!(
            compiled
                .body
                .contains("![pic.png](assets/vault/Media/pic.png)")
        );
    }

    #[test]
    fn a_wikilink_holding_a_bracket_or_code_or_no_target_is_refused() {
        // markdown-link's reading: `[[` does not nest, a `[` or `]` inside
        // ends the attempt, code inside is blanked, and a link needs a target
        // or a `#` fragment. None is a link, so each would ship as literal
        // brackets, and each is refused as leftover syntax.
        let vfs = demo_vault();
        const BRACKET: &str = "a `[` or `]` comes before its closing `]]`";
        for (body, written, why) in [
            ("A [[Shared|see [1]]] B\n", "[[Shared|see ", BRACKET),
            ("A [[x]y]] B\n", "[[x", BRACKET),
            (
                "A [[`Shared`]] B\n",
                "[[`Shared`]]",
                "its target is only code",
            ),
            ("A [[]] B\n", "[[]]", "it has no target"),
            ("A [[ |alone]] B\n", "[[ |alone]]", "it has no target"),
            ("A ![[TODO\nB [[ later\n", "![[TODO", BRACKET),
            ("A [[TODO\nB\n", "[[TODO", "it has no closing `]]`"),
        ] {
            let err = compile_note(&vfs, &source(None), body)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unresolved-syntax"), "{body:?}: {err}");
            assert!(
                err.contains(&format!("in SKILL.md: {written} (not a link: {why})")),
                "{body:?}: {err}"
            );
        }
    }

    #[test]
    fn a_markdown_link_beside_a_bracket_is_not_a_leftover() {
        // CommonMark link text may start with a `[`, and a wikilink after a
        // literal `[` compiles to a Markdown link inside that `[`. Neither is
        // a wikilink gone wrong.
        let vfs = demo_vault();
        for body in [
            "See [[WIP] Fix parser](https://example.com/pull/1).\n",
            "See [[RFC] Design][rfc].\n\n[rfc]: https://example.com\n",
            "See [[[Shared]]].\n",
        ] {
            let compiled = compile_note(&vfs, &source(None), body);
            assert!(compiled.is_ok(), "{body:?}: {:?}", compiled.err());
        }
        // A bracket that opens no Markdown link is still refused, and a blank
        // line ends the search whatever its line endings or spaces.
        for body in [
            "See [[WIP] Fix parser.\n",
            "See [[WIP] x\r\n\r\nthen](https://example.com)\r\n",
            "See [[WIP] x\n  \t\nthen](https://example.com)\n",
        ] {
            let err = compile_note(&vfs, &source(None), body)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unresolved-syntax"), "{body:?}: {err}");
        }
    }

    #[test]
    fn a_wikilink_with_code_in_part_of_its_target_is_refused() {
        // markdown-link blanks code before it reads a target, so a target
        // with code in it would resolve against the blanked text: the code
        // would vanish, or the link would reach another note. It is left as
        // written, so it is refused as leftover syntax.
        let vfs = demo_vault();
        for (body, written) in [
            ("A [[My `code` note]]\n", "[[My `code` note]]"),
            ("B [[Shared `v2`]]\n", "[[Shared `v2`]]"),
        ] {
            let err = compile_note(&vfs, &source(None), body)
                .unwrap_err()
                .to_string();
            assert!(
                err.contains(&format!(
                    "in SKILL.md: {written} (not a link: its target has code in it)"
                )),
                "{body:?}: {err}"
            );
        }
    }

    #[test]
    fn a_leftover_bracket_in_code_or_a_comment_or_a_resources_frontmatter_passes() {
        // Code and HTML comments are content, and a resource's frontmatter
        // travels as written: none of them is read as markdown links.
        assert!(
            validate_no_obsidian_syntax(
                "`a[[0` and\n\n```\n[[TODO\n```\n<!-- [[draft -->\n",
                0,
                "SKILL.md",
                "demo",
                &BTreeSet::new(),
            )
            .is_ok()
        );
        let resource = "---\nmatrix: [[1, 2], [3]]\n---\nBody.\n";
        assert!(
            validate_no_obsidian_syntax(
                resource,
                body_start(resource),
                "references/x.md",
                "demo",
                &BTreeSet::new()
            )
            .is_ok()
        );
        let err =
            validate_no_obsidian_syntax(resource, 0, "references/x.md", "demo", &BTreeSet::new())
                .unwrap_err()
                .to_string();
        assert!(err.contains("unresolved-syntax"), "{err}");
    }

    #[test]
    fn a_call_in_code_in_a_links_alias_is_part_of_that_alias() {
        // The link spans the code, so a call found inside that code is not
        // a token of its own.
        let vfs = demo_vault();
        let compiled = compile_note(&vfs, &source(None), "[[Shared|`/[[called]]`]]\n").unwrap();
        assert!(
            compiled
                .body
                .contains("[`/\\[\\[called\\]\\]`](references/vault/Shared.md)"),
            "{}",
            compiled.body
        );
    }

    #[test]
    fn a_refusal_quotes_a_table_cells_target_without_the_pipes_escape() {
        let vfs = demo_vault();
        let message = compile_note(&vfs, &source(None), "| [[../up\\|Up]] |\n")
            .unwrap_err()
            .to_string();
        assert!(message.contains("unsafe-path"), "{message}");
        assert!(message.ends_with("../up"), "{message}");
    }

    #[test]
    fn a_pipe_escaped_for_a_table_cell_is_not_part_of_the_target() {
        let vfs = demo_vault();
        let compiled = compile_note(
            &vfs,
            &source(None),
            "| Doc |\n|---|\n| [[Shared\\|the shared note]] |\n",
        )
        .unwrap();
        assert!(
            compiled
                .body
                .contains("| [the shared note](references/vault/Shared.md) |"),
            "{}",
            compiled.body
        );
    }

    #[test]
    fn a_block_fragment_is_read_without_the_space_after_its_caret() {
        let vfs = demo_vault();
        let compiled = compile_note(&vfs, &source(None), "![[Blocks#^ example]]\n").unwrap();
        assert_eq!(compiled.body, "Block content for export.\n");
    }

    #[test]
    fn paths_and_fragments_are_percent_encoded() {
        let vfs = demo_vault();
        write(&vfs, "/vault/My Note.md", b"# A B\n\nText.\n");

        let compiled = compile_note(
            &vfs,
            &source(None),
            "See [[My Note#A B]] and [[Sections#Chosen#Nested]].\n",
        )
        .unwrap();
        assert!(
            compiled
                .body
                .contains("[My Note.md](references/vault/My%20Note.md#A%20B)"),
            "{}",
            compiled.body
        );
        // A heading path keeps its inner `#`s in the fragment — escaped, so
        // the URL still has exactly one fragment separator.
        assert!(
            compiled
                .body
                .contains("[Sections.md](references/vault/Sections.md#Chosen%23Nested)"),
            "{}",
            compiled.body
        );
    }

    #[test]
    fn a_self_referencing_fragment_link_addresses_skill_md() {
        let vfs = demo_vault();
        let compiled = compile_note(&vfs, &source(None), "See [[#Steps|the steps]].\n").unwrap();
        assert!(
            compiled.body.contains("[the steps](SKILL.md#Steps)"),
            "{}",
            compiled.body
        );
    }

    // --- The scanner and the selectors, directly -----------------------------

    #[test]
    fn the_scanner_skips_inline_and_fenced_code() {
        let markdown = "![[Real]] `![[Inline]]`\n\n```md\n![[Fenced]]\n```\n";
        let destinations: Vec<&str> = scan_wiki_tokens(markdown, true)
            .iter()
            .map(|t| t.destination)
            .collect();
        assert_eq!(destinations, vec!["Real"]);
    }

    #[test]
    fn the_scanner_knows_tilde_fences_comments_and_closing_lengths() {
        // Tilde fences and HTML comments hide syntax exactly as backtick
        // fences do; a shorter closing run does not close a longer opener.
        let markdown = "~~~\n[[a]]\n~~~\n<!-- [[b]] -->\n````\n[[c]]\n```\n[[d]]\n````\n[[live]]\n";
        let destinations: Vec<&str> = scan_wiki_tokens(markdown, true)
            .iter()
            .map(|t| t.destination)
            .collect();
        assert_eq!(destinations, vec!["live"]);
    }

    #[test]
    fn the_scanner_reports_spans_embeds_and_aliases() {
        let markdown = "a ![[x|alias]] b [[y]]";
        let tokens = scan_wiki_tokens(markdown, true);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Embed);
        assert_eq!(tokens[0].destination, "x");
        assert_eq!(tokens[0].alias.as_deref(), Some("alias"));
        assert_eq!(&markdown[tokens[0].start..tokens[0].end], "![[x|alias]]");
        assert_eq!(tokens[1].kind, TokenKind::Link);
        assert_eq!(tokens[1].alias, None);
        assert_eq!(&markdown[tokens[1].start..tokens[1].end], "[[y]]");
    }

    #[test]
    fn heading_and_block_extraction_follow_obsidian() {
        let body = "# Parent\n\n## Selected\n\nText.\n\n### Child\n\nChild text.\n\n## Later\n\n- Item one\n- Item two\n\n^list-block\n";
        let section = select_fragment(body, Some("Parent#Selected"), "fixture.md").unwrap();
        assert!(section.contains("### Child"));
        assert!(!section.contains("## Later"));
        assert_eq!(
            select_fragment(body, Some("^list-block"), "fixture.md").unwrap(),
            "- Item one\n- Item two"
        );
    }

    #[test]
    fn a_heading_inside_a_fence_neither_ends_nor_starts_a_section() {
        let body = "## PR body\n\nTemplate:\n\n```md\n## Summary\n<why>\n\n~~~\n# Not closed by this\n~~~\n```\n\nAfter the fence.\n\n## Next\n\nOther.\n";
        let section = select_fragment(body, Some("PR body"), "pr-body.md").unwrap();
        assert!(section.contains("## Summary"), "{section}");
        assert!(section.ends_with("After the fence."), "{section}");
        assert_eq!(section.matches("```").count(), 2, "{section}");
        // A heading only in code is not an address.
        assert!(select_fragment(body, Some("Summary"), "pr-body.md").is_err());
    }

    #[test]
    fn a_closing_hash_run_is_stripped_only_after_whitespace() {
        let text_of = |line: &str| {
            markdown_contract::select::atx_heading(line).map(|(_, text)| text.to_owned())
        };
        for (line, expected) in [
            ("## foo ##", "foo"),
            ("###   bar    ###", "bar"),
            ("# foo ##################################", "foo"),
            ("##### foo ##", "foo"),
            ("### foo ###     ", "foo"),
            ("### foo ### b", "foo ### b"),
            ("# foo#", "foo#"),
            ("### foo \\###", "foo \\###"),
            ("## foo #\\##", "foo #\\##"),
            ("# foo \\#", "foo \\#"),
            ("# #", ""),
            ("## Learn C#", "Learn C#"),
        ] {
            assert_eq!(text_of(line).as_deref(), Some(expected), "{line}");
        }
    }

    #[test]
    fn a_heading_ending_in_a_hash_is_not_selected_by_its_hashless_prefix() {
        let body = "## Learn C#\n\nbody\n\n## Next\n";
        assert!(select_fragment(body, Some("Learn C"), "b.md").is_err());
    }

    #[test]
    fn a_selector_naming_a_heading_that_ends_in_a_hash_resolves() {
        let body = "## Learn C#\n\nbody\n\n## Next\n";
        assert_eq!(
            select_fragment(body, Some("Learn C#"), "b.md").unwrap(),
            "## Learn C#\n\nbody"
        );
    }

    #[test]
    fn a_whole_selector_match_wins_over_the_path_reading() {
        let body = "# A\n\npath\n\n## B\n\npath child\n\n# A#B\n\nwhole\n";
        assert_eq!(
            select_fragment(body, Some("A#B"), "b.md").unwrap(),
            "# A#B\n\nwhole"
        );
    }

    #[test]
    fn a_block_marker_on_a_list_item_selects_that_item_alone() {
        let body = "- one ^here\n- two\n";
        assert_eq!(
            select_fragment(body, Some("^here"), "b.md").unwrap(),
            "- one"
        );
    }

    #[test]
    fn a_heading_selector_disambiguates_by_ancestry() {
        // Two `Details` headings; the path picks the one under Second.
        let body =
            "# First\n\n## Details\n\nfirst details\n\n# Second\n\n## Details\n\nsecond details\n";
        let section = select_fragment(body, Some("Second#Details"), "b.md").unwrap();
        assert!(section.contains("second details"), "{section}");
        assert!(!section.contains("first details"), "{section}");
    }

    // --- Calls ---------------------------------------------------------------

    fn indexed(path: &str, name: &str) -> IndexedSkill {
        IndexedSkill {
            entry_path: PathBuf::from(path),
            plugin: Some("tools".to_owned()),
            name: name.to_owned(),
            shipped: true,
        }
    }

    /// `demo` (the entry) and `called` are skills; `Shared` is not.
    fn call_index(called_name: &str) -> SkillIndex {
        SkillIndex::new([
            indexed("/vault/plugins/demo/demo.md", "demo"),
            indexed("/vault/plugins/demo/called.md", called_name),
        ])
    }

    fn try_compile_with(
        vfs: &MemVfs,
        skills: SkillSource<'_>,
        body: &str,
    ) -> Result<(CompiledNote, Vec<DiscoveryWarning>), CoreError> {
        let render = |skill: &IndexedSkill| Ok(format!("/{}", skill.call_id()));
        let calls = CallContext {
            skills,
            render: &render,
            target: "test",
            implicit: true,
            warnings: RefCell::new(Vec::new()),
            record: None,
        };
        let source = CompileSource {
            calls: Some(&calls),
            ..source(None)
        };
        let compiled = compile_note(vfs, &source, body)?;
        Ok((compiled, calls.warnings.into_inner()))
    }

    fn compile_with_calls(vfs: &MemVfs, skills: &SkillIndex, body: &str) -> CompiledNote {
        try_compile_with(vfs, SkillSource::ready(skills), body)
            .unwrap()
            .0
    }

    fn call_refusal(skills: &SkillIndex, body: &str) -> String {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        try_compile_with(&vfs, SkillSource::ready(skills), body)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn an_escaped_sigil_before_a_skill_shows_the_notation() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let (compiled, warnings) = try_compile_with(
            &vfs,
            SkillSource::ready(&call_index("called")),
            "Esc \\/[[called]] here, `\\/[[called|alias]]` there.\n",
        )
        .unwrap();
        assert_eq!(
            compiled.body,
            "Esc /[[called]] here, `/[[called|alias]]` there.\n"
        );
        assert!(compiled.resources.is_empty(), "{:?}", paths(&compiled));
        assert!(compiled.closures["SKILL.md"].calls.is_empty());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn only_an_escape_lets_the_notation_through_validation() {
        let shown = BTreeSet::from(["/[[called]]".to_owned()]);
        assert!(
            validate_no_obsidian_syntax("Esc /[[called]].", 0, "SKILL.md", "demo", &shown).is_ok()
        );
        for leftover in ["Esc [[called]].", "Esc /[[other]].", "Esc x[[called]]."] {
            let err = validate_no_obsidian_syntax(leftover, 0, "SKILL.md", "demo", &shown)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unresolved-syntax"), "{leftover}: {err}");
        }
    }

    #[test]
    fn a_name_several_skills_share_is_refused_as_ambiguous() {
        let skills = SkillIndex::new([
            indexed("/vault/plugins/demo/demo.md", "demo"),
            indexed("/vault/plugins/a/rev/SKILL.md", "rev"),
            indexed("/vault/plugins/b/rev/SKILL.md", "rev"),
        ]);
        let err = call_refusal(&skills, "Run /[[rev]].\n");
        assert!(err.contains("skill-call-ambiguous"), "{err}");
        assert!(err.contains("plugins/a/rev/SKILL.md"), "{err}");
        assert!(err.contains("plugins/b/rev/SKILL.md"), "{err}");
    }

    #[test]
    fn a_call_to_a_skill_the_destination_does_not_ship_is_refused() {
        let mut called = indexed("/vault/plugins/demo/called.md", "called");
        called.shipped = false;
        let skills = SkillIndex::new([indexed("/vault/plugins/demo/demo.md", "demo"), called]);
        for body in ["Run /[[called]].\n", "See [[called]].\n"] {
            let err = call_refusal(&skills, body);
            assert!(err.contains("skill-call-not-shipped"), "{err}");
            assert!(err.contains("tools:called"), "{err}");
            assert!(err.contains("test does not ship"), "{err}");
        }
    }

    #[test]
    fn the_index_is_built_only_for_a_body_that_needs_it() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let build = || -> Result<SkillIndex, CoreError> {
            Err(CoreError::refused(
                "path-collision",
                "elsewhere in the vault",
            ))
        };
        // A bare `/name` never builds the index: see `literal_target`.
        let plain = "No calls, no links, `/not-known` in code.\n";
        let (compiled, _) = try_compile_with(&vfs, SkillSource::lazy(&build), plain).unwrap();
        assert_eq!(compiled.body, plain);

        for needs_it in ["Run /[[called]].\n", "Run `/not:known`.\n"] {
            let err = try_compile_with(&vfs, SkillSource::lazy(&build), needs_it)
                .unwrap_err()
                .to_string();
            assert!(err.contains("path-collision"), "{err}");
        }
    }

    #[test]
    fn a_stray_backtick_does_not_swallow_the_next_block() {
        let markdown = "a `stray\n\n[[A]] /[[B]]\n\nb `oops\n```\n[[C]]\n```\n[[D]] `x/[[E]]`\n";
        let kinds: Vec<(&str, TokenKind)> = scan_wiki_tokens(markdown, true)
            .iter()
            .map(|t| (t.destination, t.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("A", TokenKind::Link),
                ("B", TokenKind::Call { in_code: false }),
                ("D", TokenKind::Link),
                ("E", TokenKind::Unconverted),
            ]
        );
        // A span may still cross a single line break.
        let tokens = scan_wiki_tokens("`one\n[[hidden]] two` [[live]]", true);
        let destinations: Vec<&str> = tokens.iter().map(|t| t.destination).collect();
        assert_eq!(destinations, vec!["live"]);
    }

    #[test]
    fn renaming_a_called_skill_changes_the_callers_hash() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let body = "Run /[[called]].\n";

        let before = compile_with_calls(&vfs, &call_index("called"), body);
        let after = compile_with_calls(&vfs, &call_index("renamed"), body);

        assert_eq!(before.body, "Run `/tools:called`.\n");
        assert_eq!(before.closures["SKILL.md"].calls, vec!["tools:called"]);
        assert_eq!(after.closures["SKILL.md"].calls, vec!["tools:renamed"]);
        assert_ne!(
            before.closures["SKILL.md"]
                .hash(&vfs, Path::new("/vault"))
                .unwrap(),
            after.closures["SKILL.md"]
                .hash(&vfs, Path::new("/vault"))
                .unwrap(),
        );
    }

    #[test]
    fn a_link_to_the_entry_itself_stays_a_link() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let compiled = compile_with_calls(&vfs, &call_index("called"), "Back to [[demo]].\n");
        assert_eq!(compiled.body, "Back to [demo.md](SKILL.md).\n");
        assert!(compiled.closures["SKILL.md"].calls.is_empty());
    }

    #[test]
    fn without_a_context_a_call_renders_its_target_and_is_related() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let body = "Run /[[called]] and `/[[missing]] now`.\n";

        let compiled = compile_note(&vfs, &source(None), body).unwrap();
        assert_eq!(compiled.body, "Run `called` and `missing now`.\n");
        assert!(compiled.resources.is_empty(), "{:?}", paths(&compiled));
        assert_eq!(compiled.closures["SKILL.md"].unresolved, vec!["missing"]);

        let related = related_of(&vfs, &source(None), body).unwrap();
        assert_eq!(related.calls, vec!["plugins/demo/called.md".to_owned()]);
        assert!(related.links.is_empty(), "{:?}", related.links);
    }

    #[test]
    fn a_slash_after_a_word_or_a_link_is_not_a_sigil() {
        let markdown = "[[A]]/[[B]] and dir/[[C]] and (/[[D]]) and `x/[[E]]`";
        let kinds: Vec<(&str, TokenKind)> = scan_wiki_tokens(markdown, true)
            .iter()
            .map(|t| (t.destination, t.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("A", TokenKind::Link),
                ("B", TokenKind::Link),
                ("C", TokenKind::Link),
                ("D", TokenKind::Call { in_code: false }),
                ("E", TokenKind::Unconverted),
            ]
        );
    }

    #[test]
    fn a_slash_is_a_sigil_only_after_a_boundary() {
        let not_calls = "./[[a]] ~/[[b]] x]/[[c]] -/[[d]] :/[[e]] é/[[f]]";
        let kinds: Vec<TokenKind> = scan_wiki_tokens(not_calls, true)
            .iter()
            .map(|t| t.kind)
            .collect();
        assert_eq!(kinds, vec![TokenKind::Link; 6]);

        let calls = "/[[a]] x\t/[[b]] \"/[[c]]\" '/[[d]]' */[[e]]* >/[[f]]\n/[[g]] \
                     _/[[i]]_ |/[[j]]| [/[[k]]] =/[[l]] `/[[h]]`";
        let kinds: Vec<TokenKind> = scan_wiki_tokens(calls, true)
            .iter()
            .map(|t| t.kind)
            .collect();
        let mut expected = vec![TokenKind::Call { in_code: false }; 11];
        expected.push(TokenKind::Call { in_code: true });
        assert_eq!(kinds, expected);
    }

    #[test]
    fn emphasis_a_table_cell_and_an_option_value_carry_a_call() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let compiled = compile_with_calls(
            &vfs,
            &call_index("called"),
            "Use _/[[called]]_ or |/[[called]]| or `--next=/[[called]]`.\n",
        );
        assert_eq!(
            compiled.body,
            "Use _`/tools:called`_ or |`/tools:called`| or `--next=/tools:called`.\n"
        );
    }

    #[test]
    fn a_call_left_raw_in_code_is_warned_about() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let (compiled, warnings) = try_compile_with(
            &vfs,
            SkillSource::ready(&call_index("called")),
            "Run `x/[[called]]` or `x\\/[[called]]`.\n",
        )
        .unwrap();
        assert_eq!(compiled.body, "Run `x/[[called]]` or `x\\/[[called]]`.\n");
        let reasons: Vec<(&str, &str)> = warnings
            .iter()
            .map(|w| (w.reason.as_str(), w.detail.as_str()))
            .collect();
        assert_eq!(reasons.len(), 1, "{reasons:?}");
        assert_eq!(reasons[0].0, "unconverted-call");
        assert!(reasons[0].1.contains("`/[[called]]`"), "{reasons:?}");
    }

    /// `demo` and `called` in plugin `tools`; `away` in plugin `other`.
    fn literal_index() -> SkillIndex {
        let mut away = indexed("/vault/plugins/other/away.md", "away");
        away.plugin = Some("other".to_owned());
        SkillIndex::new([
            indexed("/vault/plugins/demo/demo.md", "demo"),
            indexed("/vault/plugins/demo/called.md", "called"),
            away,
        ])
    }

    fn literal_texts(body: &str) -> Vec<(&str, bool)> {
        literal_skill_calls(body)
            .iter()
            .map(|literal| (literal.text, literal.escaped))
            .collect()
    }

    fn call_refusal_in(vfs: &MemVfs, skills: &SkillIndex, body: &str) -> String {
        try_compile_with(vfs, SkillSource::ready(skills), body)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn every_literal_call_is_listed_in_one_refusal() {
        let err = call_refusal(
            &literal_index(),
            "Run `/tools:called`, then $other:away, /skill:tools-called and /called.\n",
        );
        assert!(err.contains("literal-skill-call"), "{err}");
        assert!(err.contains("4 literal skill calls in demo"), "{err}");
        for literal in [
            "/tools:called",
            "$other:away",
            "/skill:tools-called",
            "/called",
        ] {
            assert!(err.contains(&format!("`{literal}`")), "{literal}: {err}");
        }
        assert!(err.contains("(`/[[away]]`)"), "{err}");
    }

    #[test]
    fn a_fenced_literal_call_is_refused_too() {
        let err = call_refusal(&literal_index(), "```sh\n/tools:called --now\n```\n");
        assert!(err.contains("`/tools:called`"), "{err}");
    }

    #[test]
    fn a_bare_call_counts_only_for_the_entrys_own_plugin() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        // `away` is another plugin's skill; `nope` is no skill at all.
        let compiled = compile_with_calls(&vfs, &literal_index(), "Run /away or /nope.\n");
        assert_eq!(compiled.body, "Run /away or /nope.\n");
    }

    #[test]
    fn a_path_or_url_is_not_a_bare_call() {
        let body = "src/called and ~/called, /called/x, /called.md, \
                    https://x.io/called, [[a]]/called, /called@2";
        assert_eq!(literal_texts(body), vec![]);

        let body = "/called (/called) \"/called\" `/called` */called* >/called, /called.";
        assert_eq!(literal_texts(body).len(), 7, "{:?}", literal_texts(body));
    }

    #[test]
    fn an_escaped_literal_call_loses_its_backslash_and_is_not_refused() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let compiled = compile_with_calls(
            &vfs,
            &literal_index(),
            "Claude only: `/loop \\/tools:called`, \\$other:away, \\/called.\n\n\
             ```sh\n\\/skill:tools-called\n```\n\nKept: `\\/nope` and src\\/called.\n",
        );
        assert_eq!(
            compiled.body,
            "Claude only: `/loop /tools:called`, $other:away, /called.\n\n\
             ```sh\n/skill:tools-called\n```\n\nKept: `\\/nope` and src\\/called.\n"
        );
    }

    #[test]
    fn an_escape_that_names_no_known_skill_is_left_as_written() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let body = "Regex: `s=(\\/tmp)=` and \\/away, \\$other:called.\n\n\
                    ```sh\nchown \"\\$owner:staff\" f\n```\n";
        let compiled = compile_with_calls(&vfs, &literal_index(), body);
        assert_eq!(compiled.body, body);

        // With no index to check against, every escape is left as written.
        let build = || -> Result<SkillIndex, CoreError> {
            Err(CoreError::refused("path-collision", "two `dup` skills"))
        };
        let (compiled, _) = try_compile_with(
            &vfs,
            SkillSource::lazy(&build),
            "Claude only: \\/tools:called.\n",
        )
        .unwrap();
        assert_eq!(compiled.body, "Claude only: \\/tools:called.\n");
    }

    /// A standalone publish's index: skills with no plugin, built lazily.
    fn standalone_index() -> SkillIndex {
        SkillIndex::new(literal_index().by_path.into_values().map(|mut skill| {
            skill.plugin = None;
            skill
        }))
    }

    #[test]
    fn a_standalone_publish_matches_by_skill_name() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let build = || -> Result<SkillIndex, CoreError> { Ok(standalone_index()) };
        let (compiled, _) = try_compile_with(
            &vfs,
            SkillSource::lazy(&build),
            "Claude only: `/loop \\/tools:orchestrate`, \\$any:called, \\/called.\n",
        )
        .unwrap();
        // `orchestrate` names no skill here, so its escape stays.
        assert_eq!(
            compiled.body,
            "Claude only: `/loop \\/tools:orchestrate`, $any:called, /called.\n"
        );

        let err = try_compile_with(
            &vfs,
            SkillSource::lazy(&build),
            "Run `/other:called`, `/other:unknown` and /called.\n",
        )
        .unwrap_err()
        .to_string();
        // The plugin is not checked, and a bare call has no plugin to match.
        assert!(err.contains("1 literal skill call in demo"), "{err}");
        assert!(err.contains("`/other:called`"), "{err}");
    }

    #[test]
    fn a_link_destination_is_not_a_bare_call_and_a_colon_ends_one() {
        assert_eq!(
            literal_texts("[site](/called) and [x]( /called)"),
            vec![("/called", false)]
        );
        assert_eq!(
            literal_texts("Run /called: then /called:x"),
            vec![("/called", false), ("/called:x", false)]
        );
        let err = call_refusal(&literal_index(), "Run /called: now.\n");
        assert!(err.contains("`/called`"), "{err}");
    }

    #[test]
    fn a_plain_link_does_not_need_the_index() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let build = || -> Result<SkillIndex, CoreError> {
            Err(CoreError::refused(
                "path-collision",
                "two `dup` skills elsewhere",
            ))
        };
        let (compiled, warnings) = try_compile_with(
            &vfs,
            SkillSource::lazy(&build),
            "See [[called]]. Also [[nowhere]].\n",
        )
        .unwrap();
        assert!(
            compiled.body.starts_with("See [called.md]("),
            "{}",
            compiled.body
        );
        assert!(warnings.is_empty(), "{warnings:?}");

        let err = try_compile_with(
            &vfs,
            SkillSource::lazy(&build),
            "See [[called]], run /[[called]].\n",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("path-collision"), "{err}");
    }

    #[test]
    fn the_related_view_records_a_broken_call_instead_of_refusing() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        let skills = call_index("called");
        let render = |skill: &IndexedSkill| Ok(skill.name.clone());
        let calls = CallContext {
            skills: SkillSource::ready(&skills),
            render: &render,
            target: "the vault",
            implicit: false,
            warnings: RefCell::new(Vec::new()),
            record: None,
        };
        let source = CompileSource {
            calls: Some(&calls),
            ..source(None)
        };
        let body = "Run /[[called#Missing]], /[[nope]] and /[[Shared]].\n\n\
                    Notation: `/[[task-work]] T-1` example.\n";
        let related = related_of(&vfs, &source, body).unwrap();
        assert_eq!(related.calls, vec!["Shared.md", "plugins/demo/called.md"]);
        assert_eq!(related.unresolved, vec!["nope", "task-work"]);

        // A build still refuses.
        let err = compile_note(&vfs, &source, body).unwrap_err().to_string();
        assert!(err.contains("fragment-unresolved"), "{err}");
    }

    #[test]
    fn a_literal_call_in_an_embedded_note_is_refused() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/called.md", b"# called\n");
        write(&vfs, "/vault/Snippet.md", b"Then run `/tools:called`.\n");
        let err = call_refusal_in(&vfs, &literal_index(), "# Demo\n\n![[Snippet]]\n");
        assert!(err.contains("`/tools:called` in Snippet.md"), "{err}");

        // The escape works inside an embed as well.
        write(&vfs, "/vault/Snippet.md", b"Then run `\\/tools:called`.\n");
        let compiled = compile_with_calls(&vfs, &literal_index(), "# Demo\n\n![[Snippet]]\n");
        assert_eq!(compiled.body, "# Demo\n\nThen run `/tools:called`.\n");
    }

    // --- Wikilink parsing and resolution -------------------------------------

    /// Obsidian wikilinks never cross lines: a stray `[[` is refused on its
    /// own, and the refusal does not take in the lines after it.
    #[test]
    fn an_unclosed_wikilink_does_not_swallow_the_lines_after_it() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/note.md", b"# Note\n");
        let err = compile_note(
            &vfs,
            &source(None),
            "Start [[foo\n\n| a | b |\n|---|---|\n| x | [[note]] |\n",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("[[foo"), "{err}");
        assert!(!err.contains("| a |"), "{err}");
        assert!(!err.contains("[[note"), "{err}");
    }

    /// A dot in a note's name is not an extension: `[[v1.2 notes]]` finds
    /// `v1.2 notes.md`, as Obsidian does.
    #[test]
    fn a_note_name_with_a_dot_resolves_to_the_note() {
        let vfs = demo_vault();
        write(&vfs, "/vault/plugins/demo/v1.2 notes.md", b"# Notes\n");
        let compiled = compile_note(&vfs, &source(None), "See [[v1.2 notes]].\n").unwrap();
        assert!(
            compiled.body.starts_with("See [v1.2 notes.md]("),
            "{}",
            compiled.body
        );
        assert!(compiled.closures["SKILL.md"].unresolved.is_empty());
        // A real extension that names nothing is still unresolved.
        let compiled = compile_note(&vfs, &source(None), "See [[gone.pdf]].\n").unwrap();
        assert_eq!(compiled.body, "See gone.pdf.\n");
    }

    /// A size alias is image geometry, not words for the page.
    #[test]
    fn an_unresolved_embed_with_a_size_alias_shows_the_target() {
        let vfs = demo_vault();
        let compiled = compile_note(
            &vfs,
            &source(None),
            "A ![[missing.png|100]] and ![[gone.png|640x480]] here.\n",
        )
        .unwrap();
        assert_eq!(compiled.body, "A missing.png and gone.png here.\n");
    }
}
