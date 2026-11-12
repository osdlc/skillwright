//! The checks around a compile: no source holds a code point the build uses
//! as its own mark, and no Obsidian syntax survives into an emitted file.

use std::borrow::Cow;
use std::collections::BTreeMap;

use super::render::Renderer;
use super::scan::scan_wiki;
use super::{MARK_CLOSE, MARK_OPEN};
use crate::error::CoreError;
use crate::refusal;

impl Renderer<'_> {
    /// Checks the compiled entry `body` and every compiled Markdown file in
    /// `outputs`, writes each escape back as `[[`, and returns the body.
    pub(super) fn validate_outputs(
        &self,
        body: &str,
        outputs: &mut BTreeMap<String, (Vec<u8>, bool)>,
    ) -> Result<String, CoreError> {
        // Every transformation above either replaced a token or refused, but
        // splicing can move text across fence boundaries and frontmatter is
        // carried untransformed, so the emitted files are scanned once more. An
        // escape wrote [`ESCAPED`] for its `[[`, so the scan skips it.
        let entry_label = self.relative_or_verbatim(&self.entry_path);
        validate_no_obsidian_syntax(body, 0, &entry_label)?;
        // Only the compiled Markdown: a copied file ships as written.
        for (source_path, output_path) in &self.markdown_resources {
            let (bytes, _) = outputs
                .get_mut(output_path)
                .expect("each compiled resource has an output");
            // Emitted markdown is this pass's own composition: UTF-8 by construction.
            let text = std::str::from_utf8(bytes).expect("emitted markdown is UTF-8");
            let split = frontmatter_lossless::split(text);
            let label = self.relative_or_verbatim(source_path);
            validate_no_obsidian_syntax(text, text.len() - split.body.len(), &label)?;
            if let Some(yaml) = split.yaml {
                validate_frontmatter_links(yaml, output_path, self.skill_name)?;
            }
            *bytes = unescape(text).into_owned().into_bytes();
        }
        Ok(unescape(body).into_owned())
    }
}

/// No Obsidian syntax may survive into emitted markdown: a harness has no
/// vault to resolve it against, so anything left is a broken reference the
/// author would only find after installing (spec section 4 rule 13).
///
/// That includes a `[[` that is not a link (see [`scan_wiki`]): it would ship
/// as literal brackets, so its refusal says to write `\[[` for that. Code is
/// not checked, and neither is text an escape produced: it holds
/// [`ESCAPED`], not `[[`. Only links and leftovers at or after `body_start`
/// count: a resource's frontmatter is YAML, carried as written, and
/// [`validate_frontmatter_links`] reads it as YAML instead. `source` names
/// the file the text was built from.
pub(super) fn validate_no_obsidian_syntax(
    text: &str,
    body_start: usize,
    source: &str,
) -> Result<(), CoreError> {
    let (links, leftovers) = scan_wiki(text, false);
    let link = links
        .iter()
        .filter(|token| token.start >= body_start)
        .map(|token| (token.start, token.end, None));
    let leftover = leftovers
        .iter()
        .filter(|leftover| leftover.start >= body_start)
        .map(|leftover| (leftover.start, leftover.end, Some(leftover.why)));
    let Some((start, end, why)) = link.chain(leftover).min_by_key(|first| first.0) else {
        return Ok(());
    };
    // An unclosed `[[` runs to the end of the text; its line is enough.
    let written = unescape(&text[start..end]);
    let written = written.split('\n').next().unwrap_or(&written);
    let why = why.map_or_else(String::new, |why| {
        format!(" (not a link: {why}); write \\[[ to keep the brackets as text")
    });
    Err(CoreError::refused(
        refusal::UNRESOLVED_SYNTAX,
        format!("{source}: `{written}` is left in the built text{why} (section 4 rule 13)"),
    ))
}

/// Stands in for the `[[` of an escape, `\[[`, `\![[` or `\/[[`, until the
/// built text is validated. A noncharacter, as [`MARK_OPEN`] is.
pub(super) const ESCAPED: char = '\u{FDD2}';

/// Refuses `text`, read from `file`, when it holds [`MARK_OPEN`],
/// [`MARK_CLOSE`] or [`ESCAPED`]: the build would read one as its own mark
/// (spec section 4 rule 13).
pub(super) fn refuse_marks(text: &str, file: &str) -> Result<(), CoreError> {
    match text
        .chars()
        .find(|c| [MARK_OPEN, MARK_CLOSE, ESCAPED].contains(c))
    {
        None => Ok(()),
        Some(c) => Err(CoreError::refused(
            refusal::NONCHARACTER,
            format!(
                "{file} holds U+{:04X}, a Unicode noncharacter the build uses internally \
                 (section 4 rule 13)",
                u32::from(c)
            ),
        )),
    }
}

/// `text` with each [`ESCAPED`] written back as `[[`.
fn unescape(text: &str) -> Cow<'_, str> {
    if text.contains(ESCAPED) {
        Cow::Owned(text.replace(ESCAPED, "[["))
    } else {
        Cow::Borrowed(text)
    }
}

/// A resource's frontmatter is YAML and ships as written, so a link in one of
/// its strings would ship unresolved. Every string key and value is read for
/// a live `[[link]]` or `![[embed]]`. YAML structure such as `tags: [[a]]` is
/// a nested list, not a link, and a `[[` in a string that is not a link is
/// text. Frontmatter that does not parse is not this pass's to check.
fn validate_frontmatter_links(
    yaml: &str,
    output_path: &str,
    skill_name: &str,
) -> Result<(), CoreError> {
    fn first_link(value: &serde_yaml::Value) -> Option<&str> {
        match value {
            serde_yaml::Value::String(text) => scan_wiki(text, false)
                .0
                .first()
                .map(|token| &text[token.start..token.end]),
            serde_yaml::Value::Sequence(items) => items.iter().find_map(first_link),
            serde_yaml::Value::Mapping(map) => map
                .iter()
                .find_map(|(key, value)| first_link(key).or_else(|| first_link(value))),
            serde_yaml::Value::Tagged(tagged) => first_link(&tagged.value),
            _ => None,
        }
    }
    if !yaml.contains("[[") {
        return Ok(());
    }
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(yaml) else {
        return Ok(());
    };
    match first_link(&value) {
        None => Ok(()),
        Some(written) => Err(CoreError::refused(
            refusal::UNRESOLVED_SYNTAX,
            format!(
                "compiled skill {skill_name} has a link in the frontmatter of {output_path}: \
                 {written}; frontmatter ships as written, so the link is not resolved; \
                 move it into the body"
            ),
        )),
    }
}
