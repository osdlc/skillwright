//! The `Renderer`: one compile's state, and the pass that replaces each
//! token of a body with an embed's text or a mark.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::fence::{IMAGE_EXTENSIONS, Line, fenced_block};
use super::resolve::{PackTarget, component_note_path, extension_of_path};
use super::scan::{
    TokenKind, WikiToken, literal_skill_calls, markdown_link_tokens, plugin_root_tokens,
    scan_wiki_tokens,
};
use super::validate::{ESCAPED, refuse_marks};
use super::{CallContext, CallSite, CompileSource, Recorded, mark};
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::guard::within;
use crate::hooks_file::CANONICAL_ROOT;
use crate::layout::{Layout, MARKETPLACES_DIR, Place, SharedDir};
use crate::model::{LinkForm, PluginRootPath, ResolvedRef};
use crate::note::read_utf8;
use crate::refusal;
use crate::vfs::forward_slashed;
use crate::vfs::{Vfs, normalize};

/// One compile's state: the layout links resolve in, and the resources
/// claimed so far.
pub(super) struct Renderer<'a> {
    pub(super) vfs: &'a dyn Vfs,
    pub(super) entry_path: PathBuf,
    pub(super) skill_name: &'a str,
    pub(super) layout: &'a Layout,
    /// The entry's marketplace and plugin: a link resolves inside them.
    pub(super) market: String,
    pub(super) plugin: Option<String>,
    /// Where the entry lands in a pack laid out as Claude Code's:
    /// `skills/<s>/SKILL.md`, `commands/<c>.md`, or a shared file's own path.
    pub(super) entry_pack: String,
    /// `skills/<s>`, where a skill's own files land.
    pub(super) skill_base: Option<String>,
    pub(super) call_context: &'a CallContext<'a>,
    /// Hand-written calls found in any text this compile builds, each as
    /// the refusal names it, refused together once every file is built.
    pub(super) literal_calls: Vec<String>,
    /// Source path → output path, for compiled Markdown.
    pub(super) markdown_resources: BTreeMap<PathBuf, String>,
    /// Source path → output path, for files copied verbatim.
    pub(super) binary_resources: BTreeMap<PathBuf, String>,
    /// What the marks written so far stand for.
    pub(super) record: Recorded,
}

impl<'a> Renderer<'a> {
    pub(super) fn new(vfs: &'a dyn Vfs, source: &CompileSource<'a>) -> Result<Self, CoreError> {
        // Normalised up front so `==` against layout paths (built the same
        // way) means location equality, not spelling equality.
        let layout = source.layout;
        let entry_path = normalize(source.entry_path);
        if !(entry_path.starts_with(&layout.root) && within(vfs, &layout.root, &entry_path)?) {
            return Err(CoreError::PathEscapesRoot(entry_path));
        }
        let Some(entry) = layout.file(&entry_path) else {
            return Err(CoreError::refused(
                refusal::LAYOUT,
                format!(
                    "{} is not a component note or a shared file under `{MARKETPLACES_DIR}/`",
                    entry_path.display()
                ),
            ));
        };
        let (entry_pack, skill_base) = match &entry.place {
            Place::ComponentNote { kind, name } => (
                component_note_path(*kind, name),
                (*kind == EntityKind::Skill).then(|| format!("skills/{name}")),
            ),
            Place::Shared {
                skill: Some(skill),
                path,
                ..
            } => (
                format!("skills/{skill}/{path}"),
                Some(format!("skills/{skill}")),
            ),
            Place::Shared { path, .. } => (path.clone(), None),
            _ => (entry.rel.clone(), None),
        };
        let (market, plugin) = (entry.marketplace.clone(), entry.plugin.clone());
        Ok(Renderer {
            vfs,
            entry_path,
            skill_name: source.skill_name,
            layout,
            market,
            plugin,
            entry_pack,
            skill_base,
            call_context: source.calls,
            literal_calls: Vec::new(),
            markdown_resources: BTreeMap::new(),
            binary_resources: BTreeMap::new(),
            record: Recorded::default(),
        })
    }

    /// Seeds the skill's `references/`, `scripts/` and `assets/` from the
    /// layout, linked or not, each at its path in the skill's folder.
    /// Markdown under `references/` is compiled; everything else is copied.
    pub(super) fn seed_bundle_resources(&mut self) {
        let Some(entry) = self.layout.file(&self.entry_path) else {
            return;
        };
        let Place::ComponentNote {
            kind: EntityKind::Skill,
            name,
        } = &entry.place
        else {
            return;
        };
        let carried: Vec<(PathBuf, String, bool)> = self
            .layout
            .files_of(&entry.marketplace)
            .filter(|file| file.plugin == entry.plugin)
            .filter_map(|file| match &file.place {
                Place::Shared {
                    skill: Some(skill),
                    dir,
                    path,
                } if skill == name => Some((
                    file.path.clone(),
                    path.clone(),
                    *dir == SharedDir::References && path.to_lowercase().ends_with(".md"),
                )),
                _ => None,
            })
            .collect();
        for (source, output, markdown) in carried {
            if markdown {
                self.markdown_resources.insert(source, output);
            } else {
                self.binary_resources.insert(source, output);
            }
        }
    }

    /// Where the output `output_path` lands in a pack laid out as Claude
    /// Code's.
    fn pack_path_of(&self, output_path: &str) -> String {
        match (&self.skill_base, output_path) {
            (_, "SKILL.md") => self.entry_pack.clone(),
            (Some(base), _) => format!("{base}/{output_path}"),
            (None, _) => output_path.to_owned(),
        }
    }

    fn vault_relative(&self, path: &Path) -> Result<String, CoreError> {
        path.strip_prefix(&self.layout.root)
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

    /// The heart of the pass: every token in `body` replaced — embeds of
    /// markdown by their (recursively transformed) bodies, and calls, links
    /// and plugin-root paths by marks.
    ///
    /// `output_path` is where the text being built will land, because a
    /// relative link's spelling depends on where it is written *from* — the
    /// same note embedded into two resources links differently in each.
    pub(super) fn transform_body(
        &mut self,
        body: &str,
        source_path: &Path,
        output_path: &str,
        stack: &mut Vec<TransclusionHop>,
    ) -> Result<String, CoreError> {
        refuse_marks(body, &self.relative_or_verbatim(source_path))?;
        let mut tokens = scan_wiki_tokens(body, true);
        // A destination that is a plugin-root path is that path, checked by
        // rules 15 to 22 and not again as a relative link.
        let roots = plugin_root_tokens(body);
        tokens.extend(
            markdown_link_tokens(body)
                .into_iter()
                .filter(|link| !roots.iter().any(|root| root.start == link.start)),
        );
        tokens.extend(roots);
        self.check_literal_calls(body, source_path);
        tokens.sort_by_key(|token| token.start);
        // A token inside another, such as an escape in a link's alias, is
        // part of that token's text.
        let mut end = 0;
        tokens.retain(|token| {
            let free = token.start >= end;
            if free {
                end = token.end;
            }
            free
        });
        if tokens.is_empty() {
            return Ok(body.to_owned());
        }
        let mut out = String::with_capacity(body.len());
        let mut cursor = 0;
        for token in tokens {
            out.push_str(&body[cursor..token.start]);
            let raw = &body[token.start..token.end];
            let line = Line::around(body, token.start, token.end);
            out.push_str(&self.replace_token(
                &token,
                raw,
                line,
                source_path,
                output_path,
                stack,
            )?);
            cursor = token.end;
        }
        out.push_str(&body[cursor..]);
        Ok(out)
    }

    /// `raw` is the token's text as written, and `line` the text around it.
    fn replace_token(
        &mut self,
        token: &WikiToken<'_>,
        raw: &str,
        line: Line<'_>,
        source_path: &Path,
        output_path: &str,
        stack: &mut Vec<TransclusionHop>,
    ) -> Result<String, CoreError> {
        match token.kind {
            TokenKind::Escape => {
                // `raw` runs from the `\` through the `[[`: the text between
                // ships, and the `[[` is held as [`ESCAPED`] until validation
                // is done.
                return Ok(format!("{}{ESCAPED}", &raw[1..raw.len() - 2]));
            }
            TokenKind::Call => {
                return self.replace_call(token, source_path);
            }
            TokenKind::MarkdownLink => {
                return self.replace_markdown_link(
                    raw,
                    token.destination,
                    source_path,
                    output_path,
                );
            }
            TokenKind::PluginRoot | TokenKind::ClaudeRoot => {
                return self.replace_plugin_root(token, raw, source_path, output_path);
            }
            TokenKind::Link | TokenKind::Embed => {}
        }
        let embed = token.kind == TokenKind::Embed;
        let (resolved, target) = self.resolve_wiki(token, raw, source_path)?;
        let fragment = token.fragment.clone();
        let extension = extension_of_path(&resolved).unwrap_or_default();

        if embed && extension == ".md" {
            let hop = (resolved, fragment);
            return self.expand_embed(hop, raw, source_path, output_path, stack);
        }

        let image = IMAGE_EXTENSIONS.contains(&extension.as_str());
        if embed && fragment.is_some() {
            return Err(self.link_error(
                refusal::EMBED_FRAGMENT,
                source_path,
                raw,
                "has a fragment, and only a note's sections can be embedded",
                "section 5 rule 13",
            ));
        }
        if embed && !image {
            return self.inline_file(&resolved, raw, line, source_path);
        }
        let suffix = fragment
            .as_deref()
            .map(|fragment| format!("#{}", markdown_link::encode_uri_fragment(fragment)))
            .unwrap_or_default();
        let form = if embed {
            LinkForm::Image
        } else {
            LinkForm::Link
        };
        let link = LinkTo {
            written: raw,
            resolved: &resolved,
            target,
            suffix,
            form,
            label: markdown_link::escape_label(&display_label(token)),
        };
        self.write_link(link, source_path, output_path)
    }

    /// An embed of a note: the fragment's slice of its body, with its own
    /// tokens replaced. A hop already on `stack` is the `cycle` refusal.
    fn expand_embed(
        &mut self,
        hop: TransclusionHop,
        raw: &str,
        source_path: &Path,
        output_path: &str,
        stack: &mut Vec<TransclusionHop>,
    ) -> Result<String, CoreError> {
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
                refusal::CYCLE,
                format!(
                    "circular transclusion in {}: {}",
                    self.skill_name,
                    cycle.join(" -> ")
                ),
            ));
        }
        let (resolved, fragment) = &hop;
        let text = read_utf8(self.vfs, resolved)?;
        let note_body = self.embedded_body(&text, resolved, raw, source_path)?;
        let label = self.vault_relative(resolved)?;
        let selected = select_fragment(note_body, fragment.as_deref(), &label)?;
        let resolved = resolved.clone();
        stack.push(hop);
        let expanded = self.transform_body(&selected, &resolved, output_path, stack)?;
        stack.pop();
        Ok(expanded.trim().to_owned())
    }

    /// An embedded note's body. Its frontmatter is dropped, and frontmatter
    /// that does not close or is not YAML is an error naming both notes
    /// (spec section 5 rule 4): with a broken fence the body's start is
    /// unknown.
    fn embedded_body<'t>(
        &self,
        text: &'t str,
        note: &Path,
        written: &str,
        from_file: &Path,
    ) -> Result<&'t str, CoreError> {
        let split = frontmatter_lossless::split(text);
        let problem = if split.unterminated {
            Some("its `---` block never closes".to_owned())
        } else {
            split.yaml.and_then(|yaml| {
                serde_yaml::from_str::<serde_yaml::Value>(yaml)
                    .err()
                    .map(|err| format!("it is not YAML: {err}"))
            })
        };
        match problem {
            None => Ok(split.body),
            Some(problem) => Err(self.link_error(
                refusal::EMBED_FRONTMATTER,
                from_file,
                written,
                format!(
                    "embeds {}, whose frontmatter is malformed: {problem}",
                    self.relative_or_verbatim(note)
                ),
                "section 5 rule 4",
            )),
        }
    }

    /// A text file inlined as a fenced code block (spec section 5 rules 9
    /// to 14). The block is held behind a literal mark, so nothing in it is
    /// read again: not as a link, a call, a leftover or a plugin-root path.
    fn inline_file(
        &mut self,
        file: &Path,
        written: &str,
        line: Line<'_>,
        source_path: &Path,
    ) -> Result<String, CoreError> {
        let bytes = self.vfs.read_file(file)?;
        let Ok(content) = String::from_utf8(bytes) else {
            return Err(self.link_error(
                refusal::EMBED_BINARY,
                source_path,
                written,
                "embeds a file that is not UTF-8 text",
                "section 5 rule 13",
            ));
        };
        let label = self.vault_relative(file)?;
        refuse_marks(&content, &label)?;
        let block = fenced_block(&content, extension_of_path(file).as_deref(), line);
        self.record.literals.push(block);
        Ok(mark('v', self.record.literals.len() - 1))
    }

    /// A plugin-root variable, recorded as a mark for each harness to write
    /// (spec section 4 rules 15 to 22).
    fn replace_plugin_root(
        &mut self,
        token: &WikiToken<'_>,
        written: &str,
        source_path: &Path,
        output_path: &str,
    ) -> Result<String, CoreError> {
        if token.kind == TokenKind::ClaudeRoot {
            return Err(self.link_error(
                refusal::CLAUDE_PLUGIN_ROOT,
                source_path,
                written,
                format_args!("is Claude Code's own; write `{CANONICAL_ROOT}`"),
                "section 4 rule 22",
            ));
        }
        if token.target.is_empty() {
            return Err(self.link_error(
                refusal::PLUGIN_ROOT_NO_PATH,
                source_path,
                written,
                format_args!(
                    "names no file, and only a path can be written for every harness; \
                     write `{CANONICAL_ROOT}/<path>`"
                ),
                "section 4 rule 21",
            ));
        }
        let site = PluginRootPath {
            path: token.target.clone(),
            from: self.pack_path_of(output_path),
            written: written.to_owned(),
            caller: self.relative_or_verbatim(source_path),
        };
        self.record.roots.push(site);
        Ok(mark('p', self.record.roots.len() - 1))
    }

    /// A Markdown link's `destination`: a relative path is resolved like a
    /// wikilink with a path and rewritten to where its file lands; a URL with
    /// a scheme and a `#fragment` ship as written; an absolute path and a
    /// `file:` URL are refused. `written` is the whole link, which an error
    /// quotes.
    fn replace_markdown_link(
        &mut self,
        destination: &str,
        written: &str,
        source_path: &Path,
        output_path: &str,
    ) -> Result<String, CoreError> {
        let (path, suffix) =
            destination.split_at(destination.find(['#', '?']).unwrap_or(destination.len()));
        if path.is_empty() {
            return Ok(destination.to_owned());
        }
        let scheme = path
            .split_once(':')
            .filter(|(before, _)| !before.contains('/'))
            .map(|(before, _)| before);
        if scheme.is_some_and(|scheme| !scheme.eq_ignore_ascii_case("file")) {
            return Ok(destination.to_owned());
        }
        if scheme.is_some() || path.starts_with('/') {
            return Err(self.link_error(
                refusal::UNSAFE_PATH,
                source_path,
                written,
                "names a path outside the vault",
                "section 4, the link table",
            ));
        }
        let decoded = markdown_link::percent_decode(path);
        let resolved = self.resolve_path(source_path, &decoded, written, false)?;
        let link = LinkTo {
            written,
            resolved: &resolved,
            target: self.pack_target(source_path, &resolved, written)?,
            suffix: suffix.to_owned(),
            form: LinkForm::Href,
            label: String::new(),
        };
        self.write_link(link, source_path, output_path)
    }

    /// A resolved link, recorded and written as a mark. A link from one
    /// component to another's note warns, since a call was more likely meant
    /// (section 4 rule 7).
    fn write_link(
        &mut self,
        link: LinkTo<'_>,
        source_path: &Path,
        output_path: &str,
    ) -> Result<String, CoreError> {
        let LinkTo {
            written,
            resolved,
            target,
            suffix,
            form,
            label,
        } = link;
        let caller = self.relative_or_verbatim(source_path);
        if let (Some(component), true) = (&target.component, target.note)
            && matches!(component.kind, EntityKind::Skill | EntityKind::Command)
            && self.component_folder(source_path) != resolved.parent().map(Path::to_path_buf)
        {
            self.call_context.warn(
                caller.clone(),
                "link-not-call",
                format!(
                    "`{written}` links to the note of {} `{}` and ships as a file link. Did \
                     you mean to call /[[{}]]?",
                    component.kind.as_str(),
                    component.name,
                    component.name
                ),
            );
        }
        let link = ResolvedRef {
            path: self.vault_relative(resolved)?,
            label,
            from: self.pack_path_of(output_path),
            to: target.path,
            suffix,
            form,
            component: target.component,
            written: written.to_owned(),
            caller,
        };
        self.record.links.push(link);
        Ok(mark('l', self.record.links.len() - 1))
    }

    /// A `/[[call]]`: the mark for the target's invocation, in backticks,
    /// after the call's words when it has them. Nothing of the target is
    /// copied.
    fn replace_call(
        &mut self,
        token: &WikiToken<'_>,
        source_path: &Path,
    ) -> Result<String, CoreError> {
        let call = self.record_call(token, source_path)?;
        Ok(match text_alias(token.alias.as_deref()) {
            Some(words) => format!("{words} (`{call}`)"),
            None => format!("`{call}`"),
        })
    }

    /// The mark a call is written as, or the `skill-call-*` refusal. A call
    /// names a skill or command of the marketplace by its name.
    fn record_call(
        &mut self,
        token: &WikiToken<'_>,
        source_path: &Path,
    ) -> Result<String, CoreError> {
        let written = format!("/[[{}]]", token.destination);
        if token.fragment.is_some() {
            return Err(self.call_error(
                refusal::SKILL_CALL_FRAGMENT,
                source_path,
                &written,
                "has a fragment, and a call cannot target a section; link or embed the \
                 section instead",
            ));
        }
        let skill = match self.call_context.skills.by_name(&token.target).as_slice() {
            [] => {
                return Err(self.call_error(
                    refusal::SKILL_CALL_UNRESOLVED,
                    source_path,
                    &written,
                    format!("names no skill or command in marketplace `{}`", self.market),
                ));
            }
            [one] => *one,
            many => {
                let named: Vec<String> = many
                    .iter()
                    .map(|skill| {
                        format!(
                            "{} `{}` of plugin `{}`",
                            skill.kind.as_str(),
                            skill.name,
                            skill.plugin
                        )
                    })
                    .collect();
                return Err(self.call_error(
                    refusal::SKILL_CALL_AMBIGUOUS,
                    source_path,
                    &written,
                    format!("names {}; rename one", named.join(" and ")),
                ));
            }
        };
        if skill.kind == EntityKind::Agent {
            return Err(self.call_error(
                refusal::SKILL_CALL_NOT_SHIPPED,
                source_path,
                &written,
                format!("names agent `{}`, and no pack contains agents", skill.name),
            ));
        }
        self.record.calls.push(CallSite {
            skill: skill.clone(),
            destination: token.destination.to_owned(),
            caller: self.relative_or_verbatim(source_path),
        });
        Ok(mark('c', self.record.calls.len() - 1))
    }

    /// A call error naming the file, the call as written and section 4 rule
    /// 8.
    fn call_error(
        &self,
        token: refusal::Token,
        source_path: &Path,
        written: &str,
        what: impl std::fmt::Display,
    ) -> CoreError {
        self.link_error(token, source_path, written, what, "section 4 rule 8")
    }

    /// Holds each hand-written call in `body`'s prose for
    /// [`Self::refuse_literal_calls`].
    fn check_literal_calls(&mut self, body: &str, source_path: &Path) {
        for (text, name) in literal_skill_calls(body, &self.call_context.hand_written) {
            let found = format!(
                "{}: `{text}`, write `/[[{name}]]`",
                self.relative_or_verbatim(source_path)
            );
            if !self.literal_calls.contains(&found) {
                self.literal_calls.push(found);
            }
        }
    }

    /// Refuses every hand-written call [`Self::check_literal_calls`] held: its
    /// spelling is right for one harness only.
    pub(super) fn refuse_literal_calls(&self) -> Result<(), CoreError> {
        if self.literal_calls.is_empty() {
            return Ok(());
        }
        Err(CoreError::refused(
            refusal::LITERAL_SKILL_CALL,
            format!(
                "{} call{} written by hand, which is right for one harness only: {} \
                 (section 4 rule 8)",
                self.literal_calls.len(),
                if self.literal_calls.len() == 1 {
                    ""
                } else {
                    "s"
                },
                self.literal_calls.join("; ")
            ),
        ))
    }

    /// A link to nothing (section 4 rule 11), or an embed of nothing
    /// (section 5 rule 3).
    pub(super) fn missing(&self, from_file: &Path, written: &str) -> CoreError {
        let (what, rule) = if written.starts_with("![[") {
            ("embeds nothing", "section 5 rule 3")
        } else {
            ("links to nothing", "section 4 rule 11")
        };
        self.link_error(refusal::LINK_UNRESOLVED, from_file, written, what, rule)
    }

    /// A link error naming the linking file, the link as written and the
    /// rule it broke (section 4 rule 14).
    pub(super) fn link_error(
        &self,
        token: refusal::Token,
        from_file: &Path,
        written: &str,
        what: impl std::fmt::Display,
        rule: &str,
    ) -> CoreError {
        crate::layout::link_refusal(
            token,
            &self.relative_or_verbatim(from_file),
            written,
            what,
            rule,
        )
    }

    /// For messages only: the vault-relative spelling when there is one, the
    /// verbatim path when there is not — an error about a path must never
    /// itself error.
    pub(super) fn relative_or_verbatim(&self, path: &Path) -> String {
        self.vault_relative(path)
            .unwrap_or_else(|_| path.display().to_string())
    }
}

/// A resolved link, for [`Renderer::write_link`] to record: the link as
/// written, the file it reaches and where that lands, and how to write it.
struct LinkTo<'l> {
    written: &'l str,
    resolved: &'l Path,
    target: PackTarget,
    /// The `#fragment` or `?query`, encoded, or empty.
    suffix: String,
    form: LinkForm,
    /// The text a link displays. Empty for a Markdown link, which keeps its own.
    label: String,
}

/// One embed on the transclusion stack: the file and its fragment. Two embeds
/// of one file with different fragments are different hops, because they
/// expand different slices and only an exact repeat can loop.
pub(super) type TransclusionHop = (PathBuf, Option<String>);

/// What a rewritten link displays: the alias when it is text (see
/// [`text_alias`]), otherwise the target as written, or the fragment of a
/// link to the file it is in.
fn display_label(token: &WikiToken<'_>) -> String {
    if let Some(alias) = text_alias(token.alias.as_deref()) {
        return alias.to_owned();
    }
    if token.target.is_empty() {
        return token.fragment.clone().unwrap_or_default();
    }
    token.target.clone()
}

/// The alias as words to display, or `None` when there is none or it is an
/// image size spec like `100` or `640x480`, which Obsidian reads as geometry.
fn text_alias(alias: Option<&str>) -> Option<&str> {
    alias.filter(|alias| !markdown_link::is_size_spec(alias))
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
        CoreError::refused(refusal::FRAGMENT_UNRESOLVED, missing)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
