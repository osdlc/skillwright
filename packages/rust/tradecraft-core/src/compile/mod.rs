//! The compile pass — a note's body flattened for publication.
//!
//! 1. `![[embed]]` of a markdown note expands to that note's *body* in place,
//!    recursively, with a `#heading` or `#^block` fragment selecting the
//!    addressed slice. A cycle is refused naming the cycle path. An image
//!    embed is an image link; any other file is inlined as a fenced block,
//!    held behind a mark so nothing in it is read again.
//! 2. Every surviving `[[link]]`, and every relative Markdown link, is
//!    resolved in the scope of spec section 4 and pointed at where its file
//!    lands in the pack. Nothing is copied for a link: every file a link may
//!    reach ships on its own. A `/[[call]]` is checked against the skills
//!    and commands it may name ([`CallContext`]). Each call, link, inlined
//!    file and plugin-root path is written as a mark and recorded
//!    ([`Recorded`]), for each harness to write its own way.
//! 3. The output is validated: any Obsidian syntax remaining in an emitted
//!    markdown file is a refusal, because a harness has no resolver to
//!    give it meaning. A `[[` after an odd run of `\` is escaped: it is neither a
//!    link nor a leftover, and ships without the `\`. A resource's
//!    frontmatter ships as written; it is read as YAML, and a string in it
//!    that holds a link is refused.
//!
//! Code is literal: nothing in a code span or fenced block is a link, an
//! embed or a call, and it ships as written.
//!
//! [`crate::note::read_vault_note`] still reads the body verbatim — the
//! vault's bytes are canonical and a read must not rewrite them. Compilation
//! is a separate pass between the read and the projection:
//! [`compile_note`] takes the body the read produced and returns it compiled,
//! with the resources that ship beside it.
//!
//! # Where a link resolves
//!
//! A name, `[[guide]]`, is looked up first in the linking file's component
//! folder, at any depth, then in the plugin's shared `references/`,
//! `scripts/` and `assets/` and among its components by name. A file with no
//! component, such as a shared reference, starts at the second step. A link
//! with a path resolves relative to the linking file. Either way the file
//! must be one the layout read, in the linking file's plugin, and not in a
//! never-ship folder. A link is recorded with the pack path of the file it is
//! written in and of the file it reaches, laid out as Claude Code's pack, so
//! a translator writes it for its own folder names.
//!
//! # What a skill carries comes from the layout
//!
//! A skill's `references/`, `scripts/` and `assets/` are seeded as resources
//! at their paths in the skill's folder, as the layout walk classified them
//! ([`crate::layout::Place::Shared`]). Markdown under `references/` is
//! compiled; every other file ships byte for byte (spec section 9 rule 8).
//!
//! # What comes from the shared crates, and what stays here
//!
//! The links come from `markdown-link`: its scanner finds each `[[link]]` and
//! `![[embed]]` (code by [`markdown_link::code::CodeRules::SkillExport`]; links in HTML comments
//! skipped; `[[#Heading]]` a link to the note it is in), and its
//! `escape_label` and URI encoders write them back out. `markdown-contract`'s
//! `select` cuts a `#heading` or `#^block` out of an embedded note.
//!
//! What stays here is skillwright's own: link scope, `/[[call]]` calls and
//! `\/[[x]]` escapes, read as a layer over `markdown-link`'s links; the `\[[`
//! escape; hand-written calls; and the validation that no Obsidian syntax
//! survives.

mod fence;
mod render;
mod resolve;
mod scan;
mod validate;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::discover::DiscoveryWarning;
use crate::error::CoreError;
use crate::layout::Layout;
use crate::model::{PluginRootPath, ResolvedRef, SkillId};
use crate::note::read_utf8;
use crate::vfs::{Vfs, normalize};
use render::Renderer;

/// Where a compile reads from: the vault's layout, which says what the skill
/// carries and what a link may reach, the note being compiled, and the
/// skills its calls may name. `entry_path` is absolute, and must lie under
/// the layout's root and be a file of the layout.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompileSource<'a> {
    /// The vault's layout.
    pub layout: &'a Layout,
    /// The note whose body is being compiled. Its own output is `SKILL.md`.
    pub entry_path: &'a Path,
    /// The skill's name, used only to say *which* skill a refusal is about.
    pub skill_name: &'a str,
    /// What `/[[x]]` calls may name.
    pub calls: &'a CallContext<'a>,
}

impl<'a> CompileSource<'a> {
    /// Compile the note at `entry_path`, named `skill_name`, in `layout`.
    pub fn new(
        layout: &'a Layout,
        entry_path: &'a Path,
        skill_name: &'a str,
        calls: &'a CallContext<'a>,
    ) -> Self {
        Self {
            layout,
            entry_path,
            skill_name,
            calls,
        }
    }
}

/// Every component a compile's calls may name, keyed by entry path.
#[derive(Debug, Clone, Default)]
pub struct SkillIndex {
    by_path: BTreeMap<PathBuf, SkillId>,
}

impl SkillIndex {
    /// Builds an index from the given components.
    pub fn new(skills: impl IntoIterator<Item = SkillId>) -> Self {
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

    /// Every component named `name`, in path order.
    pub(super) fn by_name(&self, name: &str) -> Vec<&SkillId> {
        self.by_path
            .values()
            .filter(|skill| skill.name == name)
            .collect()
    }
}

/// What a compile's calls may name, shared by every compile of a load.
#[derive(Debug)]
pub struct CallContext<'a> {
    /// The skills a body may name.
    pub skills: &'a SkillIndex,
    /// The call strings a body may not write by hand, each mapped to the
    /// name to write as `/[[name]]` instead (spec section 4 rule 8). Empty
    /// checks nothing.
    pub hand_written: BTreeMap<String, String>,
    /// What the compile warned about, deduplicated.
    pub warnings: RefCell<Vec<DiscoveryWarning>>,
}

/// The calls, links, inlined files and plugin-root paths a compile met, in
/// the order it met them. A compile is done once for every harness: each is
/// written as a mark, for a translator to spell later (see [`crate::model`]).
/// The `n`th mark of each kind in the output names the `n`th entry here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recorded {
    /// Every call, resolved to the skill it names.
    pub calls: Vec<CallSite>,
    /// Every surviving link, resolved to where its file lands.
    pub links: Vec<ResolvedRef>,
    /// Every file inlined as a fenced block (spec section 5 rules 9 to 14),
    /// fence and indent included. Its text ships as is: nothing in it is
    /// interpreted.
    pub literals: Vec<String>,
    /// Every `${PLUGIN_ROOT}/<path>` (spec section 4 rule 15).
    pub roots: Vec<PluginRootPath>,
}

/// One call, resolved: the skill it names, and what a refusal about it
/// quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// The skill the call names.
    pub skill: SkillId,
    /// The call's target as the author wrote it, fragment and all.
    pub written: String,
    /// The note the call is in, as a refusal names it.
    pub caller: String,
}

/// Opens and closes a compile's mark. Unicode noncharacters, which
/// are for internal use; [`refuse_marks`] refuses a source that holds one.
pub(crate) const MARK_OPEN: char = '\u{FDD0}';
pub(crate) const MARK_CLOSE: char = '\u{FDD1}';

/// The mark for the `index`th call (`kind` `c`), link (`l`), literal (`v`)
/// or plugin-root path (`p`).
pub(super) fn mark(kind: char, index: usize) -> String {
    format!("{MARK_OPEN}{kind}{index}{MARK_CLOSE}")
}

impl CallContext<'_> {
    pub(super) fn warn(&self, path: String, reason: &str, detail: String) {
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

/// What a compile produces: the entry body flattened, every resource the
/// body (or the bundle root) reaches, and what their marks stand for.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledNote {
    /// The entry note's body with embeds expanded, and each call, link,
    /// inlined file and plugin-root path a mark.
    pub body: String,
    /// `(relative_path, bytes, executable)`, in path order — the shape
    /// [`crate::content::SkillProjection::resources`] already has, so the two
    /// connect without an adapter. Markdown resources are compiled; binaries
    /// are verbatim.
    pub resources: Vec<(String, Vec<u8>, bool)>,
    /// What the marks in the body and the Markdown resources stand for.
    pub recorded: Recorded,
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
/// * `refused: link-ambiguous` — a name matched several files.
/// * `refused: link-unresolved` — a link or embed reached nothing.
/// * `refused: link-out-of-scope` — a link left the marketplace or reached
///   another plugin.
/// * `refused: excluded-link` — a link reached a file that never ships.
/// * `refused: unsafe-path` — an absolute link target.
/// * `refused: path-collision` — two sources claim one output path.
/// * `refused: noncharacter` — a source holds a code point the build uses
///   as its own mark (U+FDD0 to U+FDD2).
/// * `refused: unresolved-syntax` — the validation pass found Obsidian syntax
///   in an emitted markdown file's body, or a link in a string of a
///   resource's frontmatter.
/// * `refused: skill-call-unresolved` / `skill-call-ambiguous` /
///   `skill-call-not-shipped` / `skill-call-fragment` — a call named no
///   skill or command, several, or an agent, or had a fragment. A broken
///   call sends the reader to a command that does not exist, so it blocks.
///   Whether a harness carries what a call names is asked when the call is
///   written ([`crate::model::Call::check_shipped`]).
/// * `refused: literal-skill-call` — prose spells one of
///   [`CallContext::hand_written`]'s call strings by hand, which is right
///   for one harness only. Every such call is listed in one refusal.
///
/// Every token above except `cycle`, `unsafe-path` and `path-collision` is
/// this pass's own.
pub fn compile_note(
    vfs: &dyn Vfs,
    source: &CompileSource<'_>,
    body: &str,
) -> Result<CompiledNote, CoreError> {
    let mut renderer = Renderer::new(vfs, source)?;
    renderer.seed_bundle_resources();

    // The entry is on the transclusion stack from the start, so a note that
    // embeds itself is a cycle of one hop rather than a stack overflow.
    let mut stack = vec![(renderer.entry_path.clone(), None)];
    let entry_path = renderer.entry_path.clone();
    let compiled_body = renderer.transform_body(body, &entry_path, "SKILL.md", &mut stack)?;

    let mut outputs = renderer.compile_resources()?;

    renderer.refuse_literal_calls()?;

    let compiled_body = renderer.validate_outputs(&compiled_body, &mut outputs)?;
    for (source_path, output_path) in &renderer.binary_resources {
        let bytes = vfs.read_file(source_path)?;
        let executable = vfs.is_executable(source_path);
        outputs.insert(output_path.clone(), (bytes, executable));
    }

    Ok(CompiledNote {
        body: compiled_body,
        resources: outputs
            .into_iter()
            .map(|(path, (bytes, executable))| (path, bytes, executable))
            .collect(),
        recorded: renderer.record,
    })
}

impl Renderer<'_> {
    /// Compiles every Markdown resource, frontmatter kept, to its bytes and
    /// execute bit. A growing queue: compiling one can discover more, and
    /// each is compiled exactly once. Indexed iteration because the queue
    /// grows while it drains.
    fn compile_resources(&mut self) -> Result<BTreeMap<String, (Vec<u8>, bool)>, CoreError> {
        let mut outputs = BTreeMap::new();
        let mut queue: Vec<PathBuf> = self.markdown_resources.keys().cloned().collect();
        let mut next = 0;
        while next < queue.len() {
            let source_path = queue[next].clone();
            next += 1;
            let output_path = self.markdown_resources[&source_path].clone();
            let raw = read_utf8(self.vfs, &source_path)?;
            // The strict split, not the lenient one: its yaml/shape recompose
            // the file byte-for-byte, so a resource's own frontmatter travels
            // verbatim and only the body changes. Carrying the author's bytes
            // is this core's rule (see `crate::note`), and it keeps a
            // resource with no links byte-identical to its source.
            let split = frontmatter_lossless::split(&raw);
            let mut stack = vec![(source_path.clone(), None)];
            let compiled =
                self.transform_body(split.body, &source_path, &output_path, &mut stack)?;
            let composed = frontmatter_lossless::compose(split.yaml, &compiled, split.shape);
            // The execute bit rides along even for markdown: resources feed
            // the same copy the peers use, and the peers' contract is that
            // bits survive.
            let executable = self.vfs.is_executable(&source_path);
            outputs.insert(output_path, (composed.into_bytes(), executable));
            for discovered in self.markdown_resources.keys() {
                if !queue.contains(discovered) {
                    queue.push(discovered.clone());
                }
            }
        }
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::validate::validate_no_obsidian_syntax;
    use super::*;
    use crate::frontmatter::EntityKind;
    use crate::vfs::MemVfs;

    /// Writes one file, creating its directories, and the root, marketplace
    /// and plugin notes a vault needs.
    fn write(vfs: &MemVfs, path: &str, content: &[u8]) {
        for (note, text) in [
            ("/vault/skillwright.md", "---\n---\n"),
            ("/vault/marketplaces/m/m.md", "---\n---\n"),
            ("/vault/marketplaces/m/plugins/p/p.md", "---\n---\n"),
        ] {
            let note = Path::new(note);
            if !vfs.exists(note) {
                vfs.create_dir_all(note.parent().unwrap()).unwrap();
                vfs.write_file(note, text.as_bytes()).unwrap();
            }
        }
        let path = Path::new(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(path, content).unwrap();
    }

    /// Skill `demo` compiled with the test call context over `skills`, and
    /// what it warned. The body and each Markdown resource are written back
    /// from their segments in Claude Code's spelling, a call as
    /// `/<plugin:name>`.
    fn try_compile_with(
        vfs: &MemVfs,
        skills: &SkillIndex,
        body: &str,
    ) -> Result<(CompiledNote, Vec<DiscoveryWarning>), CoreError> {
        let layout = crate::layout::read_layout(vfs, Path::new("/vault"))?;
        let calls = test_calls(skills);
        let entry = Path::new("/vault/marketplaces/m/plugins/p/skills/demo/demo.md");
        let source = CompileSource::new(&layout, entry, "demo", &calls);
        let mut compiled = compile_note(vfs, &source, body)?;
        compiled.body = written(&compiled.body, &compiled.recorded);
        for (path, bytes, _) in &mut compiled.resources {
            if path.ends_with(".md") {
                let text = std::str::from_utf8(bytes).unwrap();
                *bytes = written(text, &compiled.recorded).into_bytes();
            }
        }
        Ok((compiled, calls.warnings.into_inner()))
    }

    /// [`try_compile_with`] over [`call_index`], without the warnings.
    fn compile(vfs: &MemVfs, body: &str) -> Result<CompiledNote, CoreError> {
        try_compile_with(vfs, &call_index("called"), body).map(|(compiled, _)| compiled)
    }

    /// `text`'s segments, each mark written as Claude Code's pack would.
    fn written(text: &str, recorded: &Recorded) -> String {
        crate::model::segments(text, recorded)
            .segments
            .into_iter()
            .map(|segment| match segment {
                crate::model::Segment::Text { text } | crate::model::Segment::Literal { text } => {
                    text
                }
                crate::model::Segment::Link { target } => target.markdown(),
                crate::model::Segment::Call { call } => format!("/{}", call.skill.call_id()),
                crate::model::Segment::PluginRoot { path } => path.written,
            })
            .collect()
    }

    /// A demo vault reused by several tests below.
    fn demo_vault() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/demo.md",
            b"---\ndescription: d\n---\n\n# Demo\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Shared.md",
            b"---\ntag: fragment\n---\n\n## Shared instructions\n\n- Preserve this list.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Sections.md",
            b"# Other\n\nIgnore this.\n\n## Chosen\n\nKeep this section.\n\n### Nested\n\nKeep the nested section too.\n\n## Later\n\nDo not include this.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Blocks.md",
            b"Before.\n\nBlock content for export. ^example\n\nAfter.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/local.md",
            b"# Local\n\n![[Shared]]\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/scripts/tool.sh",
            b"#!/bin/sh\necho ok\n",
        );
        vfs.set_executable(
            Path::new("/vault/marketplaces/m/plugins/p/skills/demo/scripts/tool.sh"),
            true,
        )
        .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Docs/manual.pdf",
            b"%PDF-test",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Media/pic.png",
            b"PNG-test",
        );
        vfs
    }

    /// The demo entry body.
    const DEMO_BODY: &str = "# Demo\n\n![[Shared]]\n\n![[Sections#Chosen]]\n\n![[Blocks#^example]]\n\nRead [[references/local|Local reference]].\n\n[[manual.pdf#page=2]]\n\n![[pic.png|100]]\n\nKeep `![[Missing inline example]]` unchanged.\n\n```md\n![[Missing fenced example]]\n```\n";

    /// What the layout seeds for the demo skill.
    const SEEDED: [&str; 2] = ["references/local.md", "scripts/tool.sh"];

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

    #[test]
    fn a_demo_skill_compiles_end_to_end() {
        let vfs = demo_vault();
        let compiled = compile(&vfs, DEMO_BODY).unwrap();

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
        // A link to a file keeps its fragment.
        assert!(
            compiled
                .body
                .contains("[manual.pdf](../../references/Docs/manual.pdf#page=2)")
        );
        // An image embed renders as an image; a size-spec alias is geometry,
        // not a label.
        assert!(
            compiled
                .body
                .contains("![pic.png](../../references/Media/pic.png)")
        );
        // Code is content: inline and fenced Obsidian syntax survives
        // untouched and does not fail validation.
        assert!(compiled.body.contains("`![[Missing inline example]]`"));
        assert!(compiled.body.contains("![[Missing fenced example]]"));

        // The bundle markdown resource is itself compiled, its embed
        // expanded. A linked shared file is not copied: it ships on its own.
        let (_, local, _) = resource(&compiled, "references/local.md");
        assert!(String::from_utf8_lossy(local).contains("## Shared instructions"));
        assert_eq!(paths(&compiled), SEEDED);
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
        let vfs = crate::vfs::NativeVfs;
        let layout = crate::layout::read_layout(&vfs, &root).unwrap();

        let raw = read_utf8(&vfs, &entry).unwrap();
        let (_, body) = crate::note::split_frontmatter(&raw);
        let skills = SkillIndex::default();
        let calls = test_calls(&skills);
        let compile_source = CompileSource::new(&layout, &entry, "workflow-checklist", &calls);
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

    /// An entry must be inside the vault both as spelled and on disk: one
    /// linked out of the vault, or spelled through an alias of it, is
    /// refused before the walk.
    #[cfg(unix)]
    #[test]
    fn an_entry_outside_the_vault_by_link_or_spelling_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().join("vault");
        let outside = tmp.path().join("outside");
        let skill = "marketplaces/m/plugins/p/skills/demo";
        std::fs::create_dir_all(vault.join(skill)).unwrap();
        std::fs::create_dir_all(outside.join("demo")).unwrap();
        for note in [
            "skillwright.md",
            "marketplaces/m/m.md",
            "marketplaces/m/plugins/p/p.md",
        ] {
            std::fs::write(vault.join(note), "---\n---\n").unwrap();
        }
        std::fs::write(outside.join("demo/demo.md"), "# Demo\n").unwrap();
        std::fs::write(vault.join(skill).join("demo.md"), "# Demo\n").unwrap();
        let alias = tmp.path().join("alias");
        std::os::unix::fs::symlink(&vault, &alias).unwrap();

        let vfs = crate::vfs::NativeVfs;
        std::os::unix::fs::symlink(&outside, vault.join("linked")).unwrap();
        let layout = crate::layout::read_layout(&vfs, &vault).unwrap();
        let skills = SkillIndex::default();
        let calls = test_calls(&skills);
        for entry in [
            vault.join("linked/demo/demo.md"),
            alias.join(skill).join("demo.md"),
        ] {
            let compile_source = CompileSource::new(&layout, &entry, "demo", &calls);
            let err = compile_note(&vfs, &compile_source, "# Demo\n").unwrap_err();
            assert!(
                matches!(&err, CoreError::PathEscapesRoot(p) if *p == entry),
                "{}: {err}",
                entry.display()
            );
        }
    }

    #[test]
    fn a_transclusion_cycle_is_refused_naming_the_cycle() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/A.md",
            b"![[B]]\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/B.md",
            b"![[A]]\n",
        );

        let err = compile(&vfs, "# Demo\n\n![[A]]\n").unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused { .. }), "{message}");
        assert!(message.contains("cycle"), "{message}");
        assert!(message.contains("demo"), "{message}");
        // The loop, spelt in vault-relative hops from its first repeat.
        assert!(
            message.contains("A.md -> marketplaces/m/plugins/p/references/B.md -> marketplaces/m/plugins/p/references/A.md"),
            "{message}"
        );
    }

    #[test]
    fn a_cycle_through_a_heading_ending_in_a_hash_names_the_whole_heading() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/A.md",
            b"# Learn C#\n\n![[A#Learn C#]]\n",
        );

        let err = compile(&vfs, "![[A#Learn C#]]\n").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("A.md#Learn C# -> marketplaces/m/plugins/p/references/A.md#Learn C#"),
            "{message}"
        );
    }

    #[test]
    fn a_note_embedding_itself_is_the_one_hop_cycle() {
        let vfs = demo_vault();
        let err = compile(&vfs, "![[demo]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("cycle"), "{message}");
        assert!(
            message.contains("skills/demo/demo.md -> marketplaces/m/plugins/p/skills/demo/demo.md"),
            "{message}"
        );
    }

    #[test]
    fn an_ambiguous_link_is_refused_naming_every_candidate() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/One/Shared2.md",
            b"One.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Two/Shared2.md",
            b"Two.\n",
        );

        let err = compile(&vfs, "![[Shared2]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("link-ambiguous"), "{message}");
        assert!(message.contains("One/Shared2.md"), "{message}");
        assert!(message.contains("Two/Shared2.md"), "{message}");
    }

    // --- Resolution at the edges, as `resolve_strict` reads them ------------

    #[test]
    fn a_name_prefers_the_skill_folder_and_a_path_is_relative_to_the_linking_note() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/near.md",
            b"Shared.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/near.md",
            b"Beside the entry.\n",
        );
        let compiled = compile(&vfs, "![[near]]\n\n![[../../references/near]]\n").unwrap();
        assert_eq!(compiled.body, "Beside the entry.\n\nShared.\n");
    }

    #[test]
    fn a_percent_encoded_target_is_decoded() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/My Note.md",
            b"Spaced.\n",
        );
        let compiled = compile(&vfs, "![[My%20Note]]\n").unwrap();
        assert_eq!(compiled.body, "Spaced.\n");
    }

    #[test]
    fn an_unsafe_link_target_is_refused() {
        let vfs = demo_vault();
        for (body, token) in [
            ("[[/etc/passwd]]\n", "unsafe-path"),
            ("[[../../../../../../escape]]\n", "link-out-of-scope"),
            ("![[a/../../../../../../../b]]\n", "link-out-of-scope"),
        ] {
            let err = compile(&vfs, body).unwrap_err();
            let message = err.to_string();
            assert!(message.contains(token), "{body}: {message}");
        }
    }

    #[test]
    fn a_missing_fragment_is_a_refusal_naming_source_and_fragment() {
        let vfs = demo_vault();

        let err = compile(&vfs, "![[Sections#Missing]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("fragment-unresolved"), "{message}");
        assert!(
            message.contains("marketplaces/m/plugins/p/references/Sections.md"),
            "{message}"
        );
        assert!(message.contains("Missing"), "{message}");

        let err = compile(&vfs, "![[Blocks#^nope]]\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("fragment-unresolved"), "{message}");
        assert!(
            message.contains("marketplaces/m/plugins/p/references/Blocks.md"),
            "{message}"
        );
        assert!(message.contains("^nope"), "{message}");
    }

    // --- Resources, collisions, wiring --------------------------------------

    #[test]
    fn markdown_resources_keep_their_frontmatter_and_execute_bits() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/Extra.md",
            b"---\ntag: x\n---\nExtra body with [[Blocks]].\n",
        );
        vfs.set_executable(
            Path::new("/vault/marketplaces/m/plugins/p/skills/demo/references/Extra.md"),
            true,
        )
        .unwrap();

        let compiled = compile(&vfs, "See [[Extra]].\n").unwrap();
        let (_, extra, executable) = resource(&compiled, "references/Extra.md");
        // The author's frontmatter travels byte-for-byte; only the body's
        // links were rewritten.
        assert_eq!(
            String::from_utf8_lossy(extra),
            "---\ntag: x\n---\nExtra body with [Blocks](../../../references/Blocks.md).\n"
        );
        assert!(executable);
    }

    #[test]
    fn tests_and_fixtures_directories_under_the_bundle_root_never_ship() {
        // The motivating case: a fixture shipping literal `[[...]]` test data,
        // which would otherwise read as an unresolved link and fail
        // `unresolved-syntax` on a vault that has nothing wrong with it.
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/tests/fixtures/case/some-note.md",
            b"---\nid: X\n---\n\n[[not-a-real-note]]\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/tests/run_evals.test.ts",
            b"// not a vault note either\n",
        );
        // A directory named `fixtures` directly under the bundle root, not
        // only nested under `tests/`, is excluded the same way.
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/fixtures/other.md",
            b"[[also-not-real]]\n",
        );

        let compiled = compile(&vfs, "# Demo\n").unwrap();
        for (path, _, _) in &compiled.resources {
            assert!(!path.starts_with("tests/"), "{path} shipped from tests/");
            assert!(
                !path.starts_with("fixtures/"),
                "{path} shipped from fixtures/"
            );
        }
    }

    #[test]
    fn compiling_twice_yields_identical_output() {
        let vfs = demo_vault();
        let first = compile(&vfs, DEMO_BODY).unwrap();
        let second = compile(&vfs, DEMO_BODY).unwrap();
        assert_eq!(first, second);
    }

    // --- Link spelling -------------------------------------------------------

    #[test]
    fn labels_prefer_aliases_and_escape_markdown_delimiters() {
        let vfs = demo_vault();
        let compiled = compile(
            &vfs,
            "See [[manual.pdf|notes \\ 3]].\n\n![[pic.png|Diagram]]\n\n![[pic.png|640x480]]\n",
        )
        .unwrap();
        // A `\` in an alias is escaped so the label cannot break the link
        // syntax around it. (A `[` or `]` cannot reach a label: a wikilink
        // holding one is not a link.)
        assert!(
            compiled
                .body
                .contains("[notes \\\\ 3](../../references/Docs/manual.pdf)"),
            "{}",
            compiled.body
        );
        // A real alias labels an image; a geometry alias does not.
        assert!(
            compiled
                .body
                .contains("![Diagram](../../references/Media/pic.png)")
        );
        assert!(
            compiled
                .body
                .contains("![pic.png](../../references/Media/pic.png)")
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
            let err = compile(&vfs, body).unwrap_err().to_string();
            assert_eq!(err, leftover(written, Some(why)), "{body:?}");
        }
    }

    // --- The `\[[` escape ----------------------------------------------------

    const DEMO: &str = "marketplaces/m/plugins/p/skills/demo/demo.md";
    const HINT: &str = r"; write \[[ to keep the brackets as text";

    /// The refusal of `written` left in the demo note, with why it is not a
    /// link.
    fn leftover(written: &str, why: Option<&str>) -> String {
        let why = why.map_or_else(String::new, |why| format!(" (not a link: {why}){HINT}"));
        format!(
            "refused: unresolved-syntax: {DEMO}: `{written}` is left in the built text{why} \
             (section 4 rule 13)"
        )
    }

    #[test]
    fn an_escaped_double_bracket_ships_without_its_backslash() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/note.md",
            b"# Note\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/photo.png",
            b"PNG",
        );
        for (body, shipped) in [
            ("See \\[[TODO later.\n", "See [[TODO later.\n"),
            ("See \\![[TODO later.\n", "See ![[TODO later.\n"),
            (
                "Write \\[[note]] for a link.\n",
                "Write [[note]] for a link.\n",
            ),
            ("\\![[photo.png]]\n", "![[photo.png]]\n"),
            ("!\\[[x]]\n", "![[x]]\n"),
            ("A \\[[x]y]] B\n", "A [[x]y]] B\n"),
            ("A \\[[`x` y]] B\n", "A [[`x` y]] B\n"),
            ("Back to \\[[demo]].\n", "Back to [[demo]].\n"),
        ] {
            let compiled = compile_with_calls(&vfs, &call_index("called"), body);
            assert_eq!(compiled.body, shipped);
            assert_eq!(paths(&compiled), SEEDED, "{body:?}");
            let calls = &compiled.recorded.calls;
            assert!(calls.is_empty(), "{body:?}: {calls:?}");
        }

        // markdown-link gives up on the first `[[` at the `[` after it and
        // reads `[[a]]` from there: that `[[` is not escaped, so it is a link,
        // and the `\` before it stays.
        let compiled = compile_with_calls(&vfs, &call_index("called"), "A \\[[[Shared]] B\n");
        assert_eq!(
            compiled.body,
            "A \\[[Shared](../../references/Shared.md) B\n"
        );

        // In a bundle resource's body as well.
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/x.md",
            b"See \\[[TODO and \\[[note]].\n",
        );
        let compiled = compile(&vfs, "# Demo\n").unwrap();
        assert_eq!(
            compiled_text(&compiled, "references/x.md"),
            "See [[TODO and [[note]].\n"
        );
    }

    #[test]
    fn an_escaped_backslash_before_double_brackets_does_not_escape_them() {
        let vfs = demo_vault();
        let compiled = compile(&vfs, "A \\\\[[Shared]] B\n").unwrap();
        assert_eq!(
            compiled.body,
            "A \\\\[Shared](../../references/Shared.md) B\n"
        );
        let err = compile(&vfs, "A \\\\[[TODO\n").unwrap_err().to_string();
        assert!(err.contains("unresolved-syntax"), "{err}");
    }

    #[test]
    fn a_call_escape_over_a_leftover_ships_the_brackets() {
        let vfs = demo_vault();
        let compiled =
            try_compile_with(&vfs, &call_index("called"), "See \\/[[TODO later.\n").unwrap();
        assert_eq!(compiled.0.body, "See /[[TODO later.\n");
    }

    #[test]
    fn a_surviving_link_is_refused_without_the_escape_hint() {
        let err = validate_no_obsidian_syntax("See [[x]].", 0, DEMO)
            .unwrap_err()
            .to_string();
        assert_eq!(err, leftover("[[x]]", None));
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
            let compiled = compile(&vfs, body);
            assert!(compiled.is_ok(), "{body:?}: {:?}", compiled.err());
        }
        // A bracket that opens no Markdown link is still refused, and a blank
        // line ends the search whatever its line endings or spaces.
        for body in [
            "See [[WIP] Fix parser.\n",
            "See [[WIP] x\r\n\r\nthen](https://example.com)\r\n",
            "See [[WIP] x\n  \t\nthen](https://example.com)\n",
        ] {
            let err = compile(&vfs, body).unwrap_err().to_string();
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
            let err = compile(&vfs, body).unwrap_err().to_string();
            assert_eq!(
                err,
                leftover(written, Some("its target has code in it")),
                "{body:?}"
            );
        }
    }

    #[test]
    fn a_leftover_bracket_in_code_or_a_comment_passes() {
        // Code and HTML comments are content, not markdown links.
        let vfs = demo_vault();
        let body = "`a[[0` and\n\n```\n[[TODO\n```\n<!-- [[draft -->\n";
        let compiled = compile(&vfs, body).unwrap();
        assert_eq!(compiled.body, body);
    }

    /// A linked note `Fm.md` with `frontmatter` and `Body.`, compiled.
    fn compile_with_frontmatter(frontmatter: &str) -> Result<(String, CompiledNote), CoreError> {
        compile_resource(frontmatter, "Body.\n")
    }

    /// A compile of a skill linking `Fm.md`, which has `frontmatter` and `body`.
    fn compile_resource(
        frontmatter: &str,
        body: &str,
    ) -> Result<(String, CompiledNote), CoreError> {
        let vfs = demo_vault();
        let note = format!("---\n{frontmatter}\n---\n{body}");
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/Fm.md",
            note.as_bytes(),
        );
        compile(&vfs, "See [[Fm]].\n").map(|compiled| (note, compiled))
    }

    const FM_LINK: &str = "refused: unresolved-syntax: compiled skill demo has a link in the \
                           frontmatter of references/Fm.md";
    const FM_WHY: &str = "; frontmatter ships as written, so the link is not resolved; \
                          move it into the body";

    #[test]
    fn yaml_structure_in_a_resources_frontmatter_passes() {
        for frontmatter in ["tags: [[a]]", "matrix: [[1, 2], [3]]"] {
            let (note, compiled) = compile_with_frontmatter(frontmatter).unwrap();
            assert_eq!(
                resource(&compiled, "references/Fm.md").1,
                note.as_bytes(),
                "{frontmatter}"
            );
        }
        // The body is still checked.
        let err = compile_resource("tags: [[a]]", "See [[TODO\n")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("references/Fm.md: `[[TODO` is left in the built text (not a link"),
            "{err}"
        );
    }

    #[test]
    fn a_wikilink_in_resource_frontmatter_fails_validation() {
        // The transform never touches frontmatter, so a link there would ship
        // unresolved.
        let err = compile_with_frontmatter("related: \"[[Blocks]]\"")
            .unwrap_err()
            .to_string();
        assert_eq!(err, format!("{FM_LINK}: [[Blocks]]{FM_WHY}"));
    }

    #[test]
    fn a_link_in_a_nested_frontmatter_string_is_refused() {
        for (frontmatter, written) in [
            ("related: [\"[[a]]\", \"[[b]]\"]", "[[a]]"),
            ("x: {y: \"see [[a]]\"}", "[[a]]"),
            ("\"[[a]]\": 1", "[[a]]"),
            ("x:\n  - y: ['![[pic.png]]']", "![[pic.png]]"),
        ] {
            let err = compile_with_frontmatter(frontmatter)
                .unwrap_err()
                .to_string();
            assert_eq!(
                err,
                format!("{FM_LINK}: {written}{FM_WHY}"),
                "{frontmatter}"
            );
        }
    }

    #[test]
    fn a_frontmatter_string_without_a_live_link_passes() {
        for frontmatter in ["title: \"[[WIP] draft\"", "note: '\\[[x]]'"] {
            let compiled = compile_with_frontmatter(frontmatter);
            assert!(compiled.is_ok(), "{frontmatter}: {:?}", compiled.err());
        }
    }

    #[test]
    fn unparsable_resource_frontmatter_is_not_checked() {
        // `\[` is not a double-quoted YAML escape, so this does not parse.
        for frontmatter in ["note: \"\\[[x]]\"", "related: [[Blocks]] ]"] {
            let (note, compiled) = compile_with_frontmatter(frontmatter).unwrap();
            assert_eq!(
                resource(&compiled, "references/Fm.md").1,
                note.as_bytes(),
                "{frontmatter}"
            );
        }
    }

    #[test]
    fn code_in_a_links_alias_is_part_of_that_alias() {
        // The link spans the code, and code is literal.
        let vfs = demo_vault();
        let compiled = compile(&vfs, "[[Shared|`/[[called]]`]]\n").unwrap();
        assert!(
            compiled
                .body
                .contains("[`/\\[\\[called\\]\\]`](../../references/Shared.md)"),
            "{}",
            compiled.body
        );
    }

    #[test]
    fn a_refusal_quotes_a_table_cells_target_without_the_pipes_escape() {
        let vfs = demo_vault();
        let message = compile(&vfs, "| [[../up\\|Up]] |\n")
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("`[[../up\\|Up]]` links to nothing"),
            "{message}"
        );
    }

    #[test]
    fn a_pipe_escaped_for_a_table_cell_is_not_part_of_the_target() {
        let vfs = demo_vault();
        let compiled = compile(&vfs, "| Doc |\n|---|\n| [[Shared\\|the shared note]] |\n").unwrap();
        assert!(
            compiled
                .body
                .contains("| [the shared note](../../references/Shared.md) |"),
            "{}",
            compiled.body
        );
    }

    #[test]
    fn a_block_fragment_is_read_without_the_space_after_its_caret() {
        let vfs = demo_vault();
        let compiled = compile(&vfs, "![[Blocks#^ example]]\n").unwrap();
        assert_eq!(compiled.body, "Block content for export.\n");
    }

    #[test]
    fn paths_and_fragments_are_percent_encoded() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/My Note.md",
            b"# A B\n\nText.\n",
        );

        let compiled = compile(
            &vfs,
            "See [[My Note#A B]] and [[Sections#Chosen#Nested]].\n",
        )
        .unwrap();
        assert!(
            compiled
                .body
                .contains("[My Note](../../references/My%20Note.md#A%20B)"),
            "{}",
            compiled.body
        );
        // A heading path keeps its inner `#`s in the fragment — escaped, so
        // the URL still has exactly one fragment separator.
        assert!(
            compiled
                .body
                .contains("[Sections](../../references/Sections.md#Chosen%23Nested)"),
            "{}",
            compiled.body
        );
    }

    #[test]
    fn a_self_referencing_fragment_link_addresses_skill_md() {
        let vfs = demo_vault();
        let compiled = compile(&vfs, "See [[#Steps|the steps]].\n").unwrap();
        assert!(
            compiled.body.contains("[the steps](SKILL.md#Steps)"),
            "{}",
            compiled.body
        );
    }

    // --- The scanner and the selectors, directly -----------------------------

    // --- Calls ---------------------------------------------------------------

    fn indexed(path: &str, name: &str) -> SkillId {
        SkillId {
            entry_path: PathBuf::from(path),
            plugin: "tools".to_owned(),
            name: name.to_owned(),
            kind: EntityKind::Skill,
        }
    }

    /// `demo` (the entry) and `called` are skills; `Shared` is not.
    fn call_index(called_name: &str) -> SkillIndex {
        SkillIndex::new([
            indexed(
                "/vault/marketplaces/m/plugins/p/skills/demo/demo.md",
                "demo",
            ),
            indexed(
                "/vault/marketplaces/m/plugins/p/skills/called/called.md",
                called_name,
            ),
        ])
    }

    fn compile_with_calls(vfs: &MemVfs, skills: &SkillIndex, body: &str) -> CompiledNote {
        try_compile_with(vfs, skills, body).unwrap().0
    }

    fn call_refusal(skills: &SkillIndex, body: &str) -> String {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/called/called.md",
            b"# called\n",
        );
        call_refusal_in(&vfs, skills, body)
    }

    fn call_refusal_in(vfs: &MemVfs, skills: &SkillIndex, body: &str) -> String {
        try_compile_with(vfs, skills, body).unwrap_err().to_string()
    }

    #[test]
    fn an_escaped_sigil_before_a_skill_shows_the_notation() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/called/called.md",
            b"# called\n",
        );
        let (compiled, warnings) = try_compile_with(
            &vfs,
            &call_index("called"),
            "Esc \\/[[called]] here, `\\/[[called|alias]]` there.\n",
        )
        .unwrap();
        assert_eq!(
            compiled.body,
            "Esc /[[called]] here, `\\/[[called|alias]]` there.\n"
        );
        assert_eq!(paths(&compiled), SEEDED);
        assert!(compiled.recorded.calls.is_empty());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn a_name_several_plugins_share_is_refused_naming_each() {
        let mut a = indexed("/vault/marketplaces/m/plugins/a/skills/rev/rev.md", "rev");
        a.plugin = "a".to_owned();
        let mut b = indexed("/vault/marketplaces/m/plugins/b/skills/rev/rev.md", "rev");
        b.plugin = "b".to_owned();
        let err = call_refusal(&SkillIndex::new([a, b]), "Run /[[rev]].\n");
        assert!(err.contains("skill-call-ambiguous"), "{err}");
        assert!(err.contains("plugin `a`"), "{err}");
        assert!(err.contains("plugin `b`"), "{err}");
    }

    #[test]
    fn a_compile_records_the_skills_it_calls() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/called/called.md",
            b"# called\n",
        );
        let body = "Run /[[called]].\n";

        let before = compile_with_calls(&vfs, &call_index("called"), body);
        let after = compile_with_calls(&vfs, &call_index("renamed"), "Run /[[renamed]].\n");

        assert_eq!(before.body, "Run `/tools:called`.\n");
        let called = |compiled: &CompiledNote| -> Vec<String> {
            let calls = &compiled.recorded.calls;
            calls.iter().map(|call| call.skill.call_id()).collect()
        };
        assert_eq!(called(&before), vec!["tools:called"]);
        assert_eq!(called(&after), vec!["tools:renamed"]);
    }

    /// Each harness's form for `called` of plugin `tools`.
    pub(super) fn test_forms() -> BTreeMap<String, String> {
        [
            "/tools:called",
            "$tools:called",
            "/tools-called",
            "/skill:tools-called",
        ]
        .into_iter()
        .map(|form| (form.to_owned(), "called".to_owned()))
        .collect()
    }

    /// The test call context over `skills`, refusing [`test_forms`].
    fn test_calls(skills: &SkillIndex) -> CallContext<'_> {
        CallContext {
            skills,
            hand_written: test_forms(),
            warnings: RefCell::new(Vec::new()),
        }
    }

    fn compiled_text<'a>(compiled: &'a CompiledNote, path: &str) -> &'a str {
        std::str::from_utf8(&resource(compiled, path).1).unwrap()
    }

    #[test]
    fn a_self_call_still_renders_the_call() {
        let vfs = demo_vault();
        let compiled = compile_with_calls(&vfs, &call_index("called"), "Run /[[demo]].\n");
        assert_eq!(compiled.body, "Run `/tools:demo`.\n");
    }

    #[test]
    fn emphasis_a_table_cell_and_an_option_value_carry_a_call() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/called/called.md",
            b"# called\n",
        );
        let compiled = compile_with_calls(
            &vfs,
            &call_index("called"),
            "Use _/[[called]]_ or |/[[called]]| or --next=/[[called]].\n",
        );
        assert_eq!(
            compiled.body,
            "Use _`/tools:called`_ or |`/tools:called`| or --next=`/tools:called`.\n"
        );
    }

    #[test]
    fn a_call_in_code_ships_as_written_without_a_warning() {
        let vfs = demo_vault();
        let (compiled, warnings) = try_compile_with(
            &vfs,
            &call_index("called"),
            "Run `x/[[called]]` or `/[[nothing]]`.\n",
        )
        .unwrap();
        assert_eq!(compiled.body, "Run `x/[[called]]` or `/[[nothing]]`.\n");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn every_literal_call_is_listed_in_one_refusal() {
        let err = call_refusal(
            &call_index("called"),
            "Run /tools:called, then $tools:called, /skill:tools-called and /tools-called.\n",
        );
        assert!(err.contains("literal-skill-call"), "{err}");
        assert!(err.contains("4 calls written by hand"), "{err}");
        for literal in [
            "/tools:called",
            "$tools:called",
            "/skill:tools-called",
            "/tools-called",
        ] {
            assert!(err.contains(&format!("`{literal}`")), "{literal}: {err}");
        }
        assert!(err.contains("write `/[[called]]`"), "{err}");
    }

    #[test]
    fn a_plain_link_to_a_skill_stays_a_link() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/called/called.md",
            b"# called\n",
        );
        let (compiled, warnings) =
            try_compile_with(&vfs, &call_index("called"), "See [[called]].\n").unwrap();
        assert_eq!(compiled.body, "See [called](../called/SKILL.md).\n");
        assert!(
            warnings.iter().all(|w| w.reason == "link-not-call"),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_literal_call_in_an_embedded_note_is_refused() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/called/called.md",
            b"# called\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/references/Snippet.md",
            b"Then run /tools:called.\n",
        );
        let err = call_refusal_in(&vfs, &call_index("called"), "# Demo\n\n![[Snippet]]\n");
        assert!(
            err.contains("marketplaces/m/plugins/p/references/Snippet.md: `/tools:called`"),
            "{err}"
        );
    }

    // --- Wikilink parsing and resolution -------------------------------------

    /// Obsidian wikilinks never cross lines: a stray `[[` is refused on its
    /// own, and the refusal does not take in the lines after it.
    #[test]
    fn an_unclosed_wikilink_does_not_swallow_the_lines_after_it() {
        let vfs = demo_vault();
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/note.md",
            b"# Note\n",
        );
        let err = compile(
            &vfs,
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
        write(
            &vfs,
            "/vault/marketplaces/m/plugins/p/skills/demo/references/v1.2 notes.md",
            b"# Notes\n",
        );
        let compiled = compile(&vfs, "See [[v1.2 notes]].\n").unwrap();
        assert!(
            compiled.body.starts_with("See [v1.2 notes]("),
            "{}",
            compiled.body
        );
        // A real extension that names nothing links to nothing.
        let err = compile(&vfs, "See [[gone.pdf]].\n").unwrap_err();
        assert!(err.to_string().contains("links to nothing"), "{err}");
    }

    /// A size alias is image geometry, not words for the page.
    #[test]
    fn an_embed_of_nothing_is_refused_naming_it_as_written() {
        let vfs = demo_vault();
        let err = compile(&vfs, "A ![[missing.png|100]] here.\n")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("`![[missing.png|100]]` embeds nothing"),
            "{err}"
        );
    }
}
