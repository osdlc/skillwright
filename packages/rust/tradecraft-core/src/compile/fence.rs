//! A file embedded as text: an image link, or the file inlined as a fenced
//! block that keeps the containers its line opens (spec section 5 rules 9 to
//! 14).

// --- Extension vocabularies -------------------------------------------------

/// Embeds of these render as images (`![label](path)`); any other file
/// that is not a note is inlined as a fenced block.
pub(super) const IMAGE_EXTENSIONS: &[&str] = &[
    ".avif", ".bmp", ".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp",
];

/// The info string of a file inlined as a fenced block, by extension (spec
/// section 5 rule 10). An extension not listed gives none.
const INFO_STRINGS: &[(&str, &str)] = &[
    (".bash", "bash"),
    (".c", "c"),
    (".cjs", "javascript"),
    (".cpp", "cpp"),
    (".cs", "csharp"),
    (".css", "css"),
    (".csv", "csv"),
    (".diff", "diff"),
    (".fish", "fish"),
    (".go", "go"),
    (".h", "c"),
    (".hpp", "cpp"),
    (".html", "html"),
    (".ini", "ini"),
    (".java", "java"),
    (".js", "javascript"),
    (".json", "json"),
    (".jsonc", "jsonc"),
    (".jsx", "jsx"),
    (".kt", "kotlin"),
    (".lua", "lua"),
    (".mjs", "javascript"),
    (".patch", "diff"),
    (".php", "php"),
    (".pl", "perl"),
    (".ps1", "powershell"),
    (".py", "python"),
    (".rb", "ruby"),
    (".rs", "rust"),
    (".sh", "bash"),
    (".sql", "sql"),
    (".swift", "swift"),
    (".toml", "toml"),
    (".ts", "typescript"),
    (".tsx", "tsx"),
    (".xml", "xml"),
    (".yaml", "yaml"),
    (".yml", "yaml"),
    (".zsh", "zsh"),
];

/// The text on a token's line: before it, and whether text follows it.
#[derive(Clone, Copy)]
pub(super) struct Line<'t> {
    before: &'t str,
    text_after: bool,
}

impl<'t> Line<'t> {
    pub(super) fn around(body: &'t str, start: usize, end: usize) -> Self {
        let line_start = body[..start].rfind('\n').map_or(0, |at| at + 1);
        let rest = &body[end..];
        let after = &rest[..rest.find('\n').unwrap_or(rest.len())];
        Line {
            before: &body[line_start..start],
            text_after: !after.trim().is_empty(),
        }
    }

    /// The indent that keeps a block in the containers that open the line,
    /// list items and block quotes, and whether text before the token means
    /// the block must start on a line of its own.
    fn container(self) -> (String, bool) {
        let bytes = self.before.as_bytes();
        let mut at = 0;
        loop {
            while bytes.get(at).is_some_and(|b| *b == b' ' || *b == b'\t') {
                at += 1;
            }
            let marker = match bytes.get(at) {
                Some(b'>') => 1,
                Some(b'-' | b'*' | b'+') => 1,
                Some(b'0'..=b'9') => {
                    let digits = bytes[at..]
                        .iter()
                        .take_while(|b| b.is_ascii_digit())
                        .count();
                    match bytes.get(at + digits) {
                        Some(b'.' | b')') if digits <= 9 => digits + 1,
                        _ => 0,
                    }
                }
                _ => 0,
            };
            let ends = matches!(bytes.get(at + marker), None | Some(b' ' | b'\t'));
            if marker == 0 || !ends {
                break;
            }
            at += marker;
        }
        let indent = self.before[..at]
            .chars()
            .map(|c| {
                if matches!(c, ' ' | '\t' | '>') {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        (indent, !self.before[at..].trim().is_empty())
    }
}

/// `content` as a fenced code block (spec section 5 rules 9 to 12): backticks
/// one longer than the longest run that opens a line after up to three
/// spaces, never fewer than three, and every line after the first indented to
/// the containers `line` opens. The content itself is not changed.
pub(super) fn fenced_block(content: &str, extension: Option<&str>, line: Line<'_>) -> String {
    let longest = content
        .lines()
        .map(|text| {
            let indent = text.len() - text.trim_start_matches(' ').len();
            if indent > 3 {
                0
            } else {
                text[indent..].bytes().take_while(|b| *b == b'`').count()
            }
        })
        .max()
        .unwrap_or(0);
    let fence = "`".repeat((longest + 1).max(3));
    let info = extension
        .and_then(|extension| INFO_STRINGS.iter().find(|(ext, _)| *ext == extension))
        .map_or("", |(_, info)| info);
    let (indent, break_before) = line.container();
    let mut out = String::new();
    if break_before {
        out.push('\n');
        out.push_str(&indent);
    }
    out.push_str(&fence);
    out.push_str(info);
    out.push('\n');
    for text in content.split_inclusive('\n') {
        if text.trim_end_matches(['\r', '\n']).is_empty() {
            out.push_str(text);
        } else {
            out.push_str(&indent);
            out.push_str(text);
        }
    }
    if !content.is_empty() && !content.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&indent);
    out.push_str(&fence);
    if line.text_after {
        out.push('\n');
        out.push_str(&indent);
    }
    out
}
