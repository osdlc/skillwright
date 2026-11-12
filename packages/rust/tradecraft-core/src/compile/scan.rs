//! The scanner: every link, embed, call, escape, Markdown link and
//! plugin-root path in a body, and every `[[` that is none of them.

use std::collections::BTreeMap;

use markdown_link::code::{CodeRules, FenceLine, Fences, Spans};
use markdown_link::{
    AbandonReason, AbandonedLink, BodyLink, BodyLinkKind, ScanOptions, scan_body_with,
};

use crate::hooks_file::{RootKind, plugin_root_refs};

// --- The scanner ------------------------------------------------------------

/// What a scanned token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenKind {
    /// `[[x]]` — cite.
    Link,
    /// `![[x]]` — embed.
    Embed,
    /// `/[[x]]` — call.
    Call,
    /// `\[[`, `\![[` or `\/[[`, through the `[[`: written out without the
    /// `\`.
    Escape,
    /// A Markdown link's destination, `x.md` in `[t](x.md)`.
    MarkdownLink,
    /// `${PLUGIN_ROOT}`, its path in `target`, empty for none.
    PluginRoot,
    /// `${CLAUDE_PLUGIN_ROOT}`.
    ClaudeRoot,
}

/// One `[[…]]`, `![[…]]` or `/[[…]]` occurrence (or one escape), with the
/// byte span replacement needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WikiToken<'a> {
    /// Byte offset of the opening `[` (or `!`, `/`, `\`), inclusive.
    pub(super) start: usize,
    /// Byte offset just past the closing `]]`, exclusive.
    pub(super) end: usize,
    pub(super) kind: TokenKind,
    /// The text before any `|`, trimmed, as written. What a refusal quotes.
    pub(super) destination: &'a str,
    /// What resolution runs against, as `markdown-link` reads it: the text
    /// before any `#`, trimmed. Empty for a link to the note it is in.
    pub(super) target: String,
    /// The `#heading` path, or `^block` with its caret.
    pub(super) fragment: Option<String>,
    /// The text after the first `|`, trimmed; `None` when empty or absent.
    pub(super) alias: Option<String>,
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
/// `/` sigil is a `/[[x]]` call, and each escape, `\[[`, `\![[` or a `\/[[`
/// at a sigil, is a [`TokenKind::Escape`]. See [`sigil_at`] for where a `/`
/// is a sigil. Without `calls`, an escape is neither a token nor a leftover.
///
/// This is both halves of the pass in one function: the transform replaces
/// exactly what it finds, and the validation refuses anything it still finds
/// afterwards. One scanner means the two cannot disagree about what counts.
pub(super) fn scan_wiki_tokens(markdown: &str, calls: bool) -> Vec<WikiToken<'_>> {
    scan_wiki(markdown, calls).0
}

/// A `[[` in a body that is not a link, so would ship as literal brackets.
/// An escaped one, `\[[`, is not a leftover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Leftover {
    /// Byte offset of the `[[`, or of the `!` before it.
    pub(super) start: usize,
    /// Byte offset just past what the scanner read.
    pub(super) end: usize,
    /// Why it is not a link, for a refusal.
    pub(super) why: &'static str,
}

/// An escape token over `start..end`, the `\` through the `[[`.
fn escape_token(start: usize, end: usize) -> WikiToken<'static> {
    WikiToken {
        start,
        end,
        kind: TokenKind::Escape,
        destination: "",
        target: String::new(),
        fragment: None,
        alias: None,
    }
}

/// [`scan_wiki_tokens`], and each `[[` that is not a token: one
/// `markdown-link` gave up on, and a link with code in its target, which
/// this pass leaves as written. A `[[` or `![[` after an odd run of `\`
/// (see [`escaped_at`]) is in neither list unless `calls` asks for escapes.
pub(super) fn scan_wiki(markdown: &str, calls: bool) -> (Vec<WikiToken<'_>>, Vec<Leftover>) {
    let scan = scan_body_with(markdown, &SCAN);
    let mut tokens: Vec<WikiToken<'_>> = Vec::new();
    let mut leftovers: Vec<Leftover> = Vec::new();
    read_abandoned(
        markdown,
        &scan.abandoned,
        calls,
        &mut tokens,
        &mut leftovers,
    );
    read_links(markdown, scan.links, calls, &mut tokens, &mut leftovers);
    tokens.sort_by_key(|token| token.start);
    (tokens, leftovers)
}

/// Each `[[` `markdown-link` gave up on: an escape, a `[` beside a Markdown
/// link, or a leftover.
fn read_abandoned(
    markdown: &str,
    abandoned: &[AbandonedLink],
    calls: bool,
    tokens: &mut Vec<WikiToken<'_>>,
    leftovers: &mut Vec<Leftover>,
) {
    let bytes = markdown.as_bytes();
    for abandoned in abandoned {
        let opens = abandoned.byte_start + usize::from(abandoned.embed);
        if let Some(start) = escape_start(bytes, abandoned.byte_start) {
            if calls {
                tokens.push(escape_token(start, opens + 2));
            }
            continue;
        }
        let at = abandoned.byte_start;
        if calls
            && !abandoned.embed
            && at >= 2
            && &bytes[at - 2..at] == b"\\/"
            && sigil_at(bytes, at - 2)
        {
            tokens.push(escape_token(at - 2, at + 2));
            continue;
        }
        // `[[WIP] Fix](url)` and `[[Note](Note.md)]` are Markdown links with
        // a `[` beside them, not wikilinks gone wrong.
        if abandoned.reason == AbandonReason::Bracket
            && (opens_markdown_link(markdown, opens) || opens_markdown_link(markdown, opens + 1))
        {
            continue;
        }
        leftovers.push(Leftover {
            start: abandoned.byte_start,
            end: abandoned.byte_end,
            why: match abandoned.reason {
                AbandonReason::Bracket => "a `[` or `]` comes before its closing `]]`",
                AbandonReason::Code => "its target is only code",
                AbandonReason::EmptyTarget => "it has no target",
                AbandonReason::Unclosed => "it has no closing `]]`",
            },
        });
    }
}

/// Each wikilink and embed `markdown-link` read: an escape, a call, a link,
/// or a leftover when code is in its target.
fn read_links<'m>(
    markdown: &'m str,
    links: Vec<BodyLink>,
    calls: bool,
    tokens: &mut Vec<WikiToken<'m>>,
    leftovers: &mut Vec<Leftover>,
) {
    let bytes = markdown.as_bytes();
    for link in links {
        let (kind, opener) = match link.kind {
            BodyLinkKind::Wikilink => (TokenKind::Link, 2),
            BodyLinkKind::Embed => (TokenKind::Embed, 3),
            BodyLinkKind::Markdown => continue,
        };
        let at = link.byte_start;
        if let Some(start) = escape_start(bytes, at) {
            if calls {
                tokens.push(escape_token(start, at + opener));
            }
            continue;
        }
        let (kind, start) = match kind {
            TokenKind::Link
                if calls && at >= 2 && &bytes[at - 2..at] == b"\\/" && sigil_at(bytes, at - 2) =>
            {
                tokens.push(escape_token(at - 2, at + 2));
                continue;
            }
            TokenKind::Link if calls && at >= 1 && sigil_at(bytes, at - 1) => {
                (TokenKind::Call, at - 1)
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
            continue;
        }
        tokens.push(WikiToken::new(
            (start, link.byte_end),
            kind,
            inner,
            link.target,
            fragment_of(link.heading, link.block),
            link.alias,
        ));
    }
}

/// A token for the destination of each live Markdown link in `markdown`. Its
/// `destination` is the whole link, which a refusal quotes, and its `target`
/// the destination it replaces.
pub(super) fn markdown_link_tokens(markdown: &str) -> Vec<WikiToken<'_>> {
    markdown_destinations(markdown)
        .into_iter()
        .map(|(destination, link)| {
            // Each destination is a slice of `markdown`.
            let start = destination.as_ptr() as usize - markdown.as_ptr() as usize;
            WikiToken {
                start,
                end: start + destination.len(),
                kind: TokenKind::MarkdownLink,
                destination: link,
                target: destination.to_owned(),
                fragment: None,
                alias: None,
            }
        })
        .collect()
}

/// A token for each `${PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_ROOT}` in
/// `markdown`, code included (spec section 4 rules 15 and 25).
pub(super) fn plugin_root_tokens(markdown: &str) -> Vec<WikiToken<'_>> {
    plugin_root_refs(markdown)
        .into_iter()
        .map(|found| {
            let (kind, target) = match found.kind {
                RootKind::Path(path) => (TokenKind::PluginRoot, path),
                RootKind::Lone => (TokenKind::PluginRoot, ""),
                RootKind::Claude => (TokenKind::ClaudeRoot, ""),
            };
            WikiToken {
                start: found.start,
                end: found.end,
                kind,
                destination: &markdown[found.start..found.end],
                target: target.to_owned(),
                fragment: None,
                alias: None,
            }
        })
        .collect()
}

/// The destination of each inline Markdown link and reference definition in
/// `markdown` that is live: not in code, not in an HTML comment. Each comes
/// with the whole link as written, such as `[t](x.md)` or `[t]: x.md`.
fn markdown_destinations(markdown: &str) -> Vec<(&str, &str)> {
    let code = code_regions(markdown);
    let live = |at: usize| {
        !code.iter().any(|(from, to)| (*from..*to).contains(&at))
            && markdown[..at].rfind("<!--") <= markdown[..at].rfind("-->")
    };
    let inline = markdown
        .match_indices("](")
        .map(|(at, _)| (at, at + 2, link_open(markdown, at)))
        .filter(|(at, _, _)| !markdown[..*at].ends_with('\\'));
    let mut offset = 0;
    let mut definitions = Vec::new();
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[')
            && let Some(close) = trimmed.find("]:")
            && is_definition(&trimmed[1..close], &trimmed[close + 2..])
        {
            let open = offset + (line.len() - trimmed.len());
            let at = open + close;
            definitions.push((at, at + 2, Some(open)));
        }
        offset += line.len();
    }
    inline
        .chain(definitions)
        .filter(|(at, _, _)| live(*at))
        .filter_map(|(at, from, open)| {
            let rest = markdown[from..].trim_start_matches([' ', '\t']);
            let url = match rest.strip_prefix('<') {
                Some(angled) => angled.split_once('>').map(|(url, _)| url),
                None => rest
                    .split([' ', '\t', '\r', '\n', ')'])
                    .next()
                    .filter(|url| !url.is_empty()),
            }?;
            let url_end = url.as_ptr() as usize - markdown.as_ptr() as usize + url.len();
            // An inline link runs to its `)`; a definition ends with its URL.
            let end = if markdown[at..].starts_with("](") {
                markdown[url_end..]
                    .find([')', '\n'])
                    .filter(|found| markdown[url_end + found..].starts_with(')'))
                    .map_or(url_end, |found| url_end + found + 1)
            } else {
                url_end + usize::from(markdown[url_end..].starts_with('>'))
            };
            let link = open.map_or(url, |open| &markdown[open..end]);
            Some((url, link))
        })
        .collect()
}

/// Whether `[label]: rest` is a link reference definition: a label with no
/// brackets in it, then a URL, then nothing or a title in `"…"`, `'…'` or
/// `(…)`.
fn is_definition(label: &str, rest: &str) -> bool {
    if label.trim().is_empty() || label.contains(['[', ']']) {
        return false;
    }
    let rest = rest.trim();
    let after = match rest.strip_prefix('<') {
        Some(angled) => match angled.split_once('>') {
            Some((_, after)) => after,
            None => return false,
        },
        None => rest.split_once([' ', '\t']).map_or("", |(_, after)| after),
    };
    let title = after.trim();
    title.is_empty()
        || [('"', '"'), ('\'', '\''), ('(', ')')]
            .iter()
            .any(|(open, close)| {
                title.len() >= 2 && title.starts_with(*open) && title.ends_with(*close)
            })
}

/// The `[` that the `]` at byte `close` closes, through nested brackets, with
/// a `!` before it for an image. `None` when it is not on that line.
fn link_open(markdown: &str, close: usize) -> Option<usize> {
    let bytes = markdown.as_bytes();
    let mut depth = 0usize;
    for at in (0..=close).rev() {
        let escaped = at > 0 && bytes[at - 1] == b'\\';
        match bytes[at] {
            b'\n' => return None,
            b']' if !escaped => depth += 1,
            b'[' if !escaped => {
                depth -= 1;
                if depth == 0 {
                    return Some(if at > 0 && bytes[at - 1] == b'!' {
                        at - 1
                    } else {
                        at
                    });
                }
            }
            _ => {}
        }
    }
    None
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

/// Where the escaping `\` is when the `[[` at `offset` is escaped: right
/// before it, or before a `!` that sits right before it (`\![[x]]`).
fn escape_start(bytes: &[u8], offset: usize) -> Option<usize> {
    if escaped_at(bytes, offset) {
        return Some(offset - 1);
    }
    (offset >= 1 && bytes[offset - 1] == b'!' && escaped_at(bytes, offset - 1)).then(|| offset - 2)
}

/// Whether the byte at `offset` follows an odd run of `\`: `\[[x]]` and
/// `\![[x]]` are escaped, `\\[[x]]` is an escaped backslash before a link.
fn escaped_at(bytes: &[u8], offset: usize) -> bool {
    let run = bytes[..offset]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    run % 2 == 1
}

/// A `/[[` or `\/[[` at `offset`, where the `/` (or `\`) starts a line or
/// follows whitespace or one of `( " ' * > _ | [ =` (spec section 4 rule 8).
/// Anything else before it makes it part of a word or path, so
/// `[[A]]/[[B]]`, `dir/[[x]]` and `./[[x]]` stay links.
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
                    b'(' | b'"' | b'\'' | b'*' | b'>' | b'_' | b'|' | b'[' | b'='
                )
        }
    }
}

/// Every hand-written call in `body`'s prose: each string of `forms` found
/// where its sigil does not continue a word or path and the character after
/// it is not `a` to `z`, a digit or `-`, with the name to write instead
/// (spec section 4 rule 8). Code spans and fenced blocks are not read, and a
/// `\` before the sigil does not escape it.
pub(super) fn literal_skill_calls<'b>(
    body: &'b str,
    forms: &BTreeMap<String, String>,
) -> Vec<(&'b str, String)> {
    if forms.is_empty() {
        return Vec::new();
    }
    let bytes = body.as_bytes();
    let code = code_regions(body);
    let in_code = |at: usize| code.iter().any(|(from, to)| (*from..*to).contains(&at));
    let is_name = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-';
    let mut found = Vec::new();
    for (at, &sigil) in bytes.iter().enumerate() {
        if sigil != b'/' && sigil != b'$' {
            continue;
        }
        let continues = at.checked_sub(1).is_some_and(|before| {
            let b = bytes[before];
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'}' | b'/' | b'.' | b'-' | b'$')
        });
        if continues || in_code(at) {
            continue;
        }
        let mut end = at + 1;
        while end < bytes.len() && (is_name(bytes[end]) || bytes[end] == b':') {
            end += 1;
        }
        // The longest form that ends where a name does: at the run's end or
        // before one of its `:`.
        let longest = (at + 2..=end)
            .rev()
            .filter(|&stop| stop == end || bytes[stop] == b':')
            .find_map(|stop| {
                let text = &body[at..stop];
                forms.get(text).map(|name| (text, name.clone()))
            });
        if let Some(call) = longest {
            found.push(call);
        }
    }
    found
}

/// One past a line's `\n`, saturating at the end of input.
fn advance_past(line_end: usize, len: usize) -> usize {
    if line_end >= len { len } else { line_end + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::tests::test_forms;

    #[test]
    fn a_reference_definition_needs_a_plain_label_and_nothing_after_its_url() {
        let found = |text| markdown_destinations(text);
        assert_eq!(found("[a]: x.md\n"), [("x.md", "[a]: x.md")]);
        assert_eq!(found("[a]: x.md \"T\"\n"), [("x.md", "[a]: x.md")]);
        assert_eq!(found("[a]: <x.md> 'T'\n"), [("x.md", "[a]: <x.md>")]);
        assert_eq!(found("[a]: x.md (T)"), [("x.md", "[a]: x.md")]);
        assert!(found("[[guide]]: read this first.\n").is_empty());
        assert!(found("[Note]: keep this short.\n").is_empty());
        assert!(found("[a]: x.md \"open\n").is_empty());
    }

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
                ("D", TokenKind::Call),
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
        assert_eq!(kinds, vec![TokenKind::Call; 11]);
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
                ("B", TokenKind::Call),
                ("D", TokenKind::Link),
            ]
        );
        // A span may still cross a single line break.
        let tokens = scan_wiki_tokens("`one\n[[hidden]] two` [[live]]", true);
        let destinations: Vec<&str> = tokens.iter().map(|t| t.destination).collect();
        assert_eq!(destinations, vec!["live"]);
    }

    fn literal_texts(body: &str) -> Vec<&str> {
        literal_skill_calls(body, &test_forms())
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    #[test]
    fn a_literal_call_in_code_is_not_read() {
        let body = "Run `/tools:called`.\n\n```sh\n/tools:called --now\n```\n";
        assert!(literal_texts(body).is_empty());
    }

    #[test]
    fn a_path_or_url_is_not_a_literal_call() {
        let body = "src/tools:called https://x.io/tools:called a/tools:called \
                    x$tools:called ${X}/tools:called ./tools-called";
        assert!(literal_texts(body).is_empty(), "{:?}", literal_texts(body));
    }

    #[test]
    fn a_literal_call_ends_before_a_name_character() {
        assert_eq!(
            literal_texts("/tools:called: /tools:called.md /tools:called-x /tools:calledx"),
            vec!["/tools:called", "/tools:called"]
        );
    }
}
