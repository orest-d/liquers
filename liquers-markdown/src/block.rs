//! The block layer: a flat list of blocks that tiles the input, every byte in exactly one block.

/// A byte range of the document with its lines (1-based, inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub end_line: usize,
}

/// The syntax of a front-matter block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontMatterFormat {
    /// `---` … `---` (or `...`)
    Yaml,
    /// `+++` … `+++`
    Toml,
}

/// What a block is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    FrontMatter(FrontMatterFormat),
    /// An ATX (`## x`) or setext (`x` over `===`/`---`) heading; `text` is the heading's own text.
    Heading { level: u8, setext: bool, text: Span },
    Paragraph,
    /// A whole list, nested items included.
    List,
    /// A whole block quote.
    BlockQuote,
    /// A pipe table: a header line, a delimiter row and its rows.
    Table,
    FencedCode { info: String },
    IndentedCode,
    Html,
    ThematicBreak,
    /// One or more blank lines.
    Blank,
}

/// One block and its span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub span: Span,
}

/// One line: `start` of the line, `content_end` before `\r\n`/`\n`, `end` after it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line {
    pub start: usize,
    pub content_end: usize,
    pub end: usize,
}

pub(crate) fn split_lines(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    let bytes = text.as_bytes();
    while pos < text.len() {
        let rest = &bytes[pos..];
        let (content_end, end) = match rest.iter().position(|&b| b == b'\n') {
            Some(i) => {
                let mut ce = pos + i;
                if ce > pos && bytes[ce - 1] == b'\r' {
                    ce -= 1;
                }
                (ce, pos + i + 1)
            }
            None => (text.len(), text.len()),
        };
        out.push(Line { start: pos, content_end, end });
        pos = end;
    }
    out
}

/// Leading indentation in columns (a tab advances to the next multiple of 4) and its byte length.
fn indent(line: &str) -> (usize, usize) {
    let mut cols = 0;
    let mut bytes = 0;
    for c in line.chars() {
        match c {
            ' ' => cols += 1,
            '\t' => cols += 4 - cols % 4,
            _ => break,
        }
        bytes += 1;
    }
    (cols, bytes)
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// The end of the front matter at byte 0 (after the closing delimiter line), as
/// `liquers_records::search::front_matter_end` reads it, and its format.
pub(crate) fn front_matter(text: &str, lines: &[Line]) -> Option<(FrontMatterFormat, usize)> {
    let first = lines.first()?;
    let bom = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let head = text[first.start + bom..first.end].trim_end_matches(['\n', '\r']).trim_end();
    let (format, closers): (FrontMatterFormat, &[&str]) = match head {
        "---" => (FrontMatterFormat::Yaml, &["---", "..."]),
        "+++" => (FrontMatterFormat::Toml, &["+++"]),
        _ => return None,
    };
    for (i, line) in lines.iter().enumerate().skip(1) {
        let content = text[line.start..line.end].trim_end_matches(['\n', '\r']).trim_end();
        if closers.contains(&content) {
            return Some((format, i));
        }
    }
    None
}

/// `(char, length, info)` of a fence opening line with up to 3 spaces of indent.
fn fence_open(line: &str) -> Option<(char, usize, String)> {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return None;
    }
    let rest = &line[bytes..];
    let c = rest.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = rest.chars().take_while(|&x| x == c).count();
    if n < 3 {
        return None;
    }
    let info = rest[n..].trim();
    if c == '`' && info.contains('`') {
        return None;
    }
    Some((c, n, info.to_string()))
}

fn fence_close(line: &str, c: char, n: usize) -> bool {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return false;
    }
    let rest = &line[bytes..];
    let m = rest.chars().take_while(|&x| x == c).count();
    m >= n && rest[m * c.len_utf8()..].trim().is_empty()
}

/// `(level, text start, text end)` within `line` of an ATX heading.
pub(crate) fn atx_heading(line: &str) -> Option<(u8, usize, usize)> {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return None;
    }
    let rest = &line[bytes..];
    let level = rest.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let after = &rest[level..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    let start = bytes + level + (after.len() - after.trim_start().len());
    let mut end = bytes + level + after.trim_end().len();
    // A closing sequence of `#` preceded by a space (or making up the whole text) is not text.
    let text = &line[start.min(end)..end];
    let hashes = text.chars().rev().take_while(|&c| c == '#').count();
    if hashes > 0 {
        let before = &text[..text.len() - hashes];
        if before.is_empty() {
            end = start;
        } else if before.ends_with(' ') || before.ends_with('\t') {
            end = start + before.trim_end().len();
        }
    }
    Some((level as u8, start.min(end), end))
}

fn thematic_break(line: &str) -> bool {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return false;
    }
    let rest = line[bytes..].trim_end();
    let Some(c) = rest.chars().next() else { return false };
    if c != '-' && c != '*' && c != '_' {
        return false;
    }
    let mut n = 0;
    for x in rest.chars() {
        if x == c {
            n += 1;
        } else if x != ' ' && x != '\t' {
            return false;
        }
    }
    n >= 3
}

/// The level of a setext underline (`===` 1, `---` 2).
fn setext_underline(line: &str) -> Option<u8> {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return None;
    }
    let rest = line[bytes..].trim_end();
    let c = rest.chars().next()?;
    if (c == '=' || c == '-') && rest.chars().all(|x| x == c) {
        Some(if c == '=' { 1 } else { 2 })
    } else {
        None
    }
}

fn list_marker(line: &str) -> bool {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return false;
    }
    let rest = &line[bytes..];
    let followed = |r: &str| r.is_empty() || r.starts_with(' ') || r.starts_with('\t');
    if let Some(c) = rest.chars().next() {
        if c == '-' || c == '+' || c == '*' {
            return followed(&rest[1..]);
        }
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        if after.starts_with('.') || after.starts_with(')') {
            return followed(&after[1..]);
        }
    }
    false
}

/// A list item interrupts a paragraph when it is a bullet or an ordered item numbered 1 with
/// content (CommonMark).
fn list_interrupts_paragraph(line: &str) -> bool {
    if !list_marker(line) {
        return false;
    }
    let rest = line.trim_start();
    let body = match rest.chars().next() {
        Some('-' | '+' | '*') => &rest[1..],
        _ if rest.starts_with("1.") || rest.starts_with("1)") => &rest[2..],
        _ => return false,
    };
    !body.trim().is_empty()
}

fn quote_marker(line: &str) -> bool {
    let (cols, bytes) = indent(line);
    cols <= 3 && line[bytes..].starts_with('>')
}

/// CommonMark's block-level HTML tag names (start condition 6).
const BLOCK_TAGS: &[&str] = &[
    "address", "article", "aside", "base", "basefont", "blockquote", "body", "caption", "center", "col",
    "colgroup", "dd", "details", "dialog", "dir", "div", "dl", "dt", "fieldset", "figcaption", "figure",
    "footer", "form", "frame", "frameset", "h1", "h2", "h3", "h4", "h5", "h6", "head", "header", "hr",
    "html", "iframe", "legend", "li", "link", "main", "menu", "menuitem", "nav", "noframes", "ol",
    "optgroup", "option", "p", "param", "pre", "script", "section", "source", "style", "summary",
    "table", "tbody", "td", "textarea", "tfoot", "th", "thead", "title", "tr", "track", "ul",
];

/// Whether `line` opens an HTML block: `Some(true)` for a comment (it runs to `-->`),
/// `Some(false)` for a block tag, or a line that is one complete tag (both run to a blank line).
/// An autolink (`<https://…>`) or prose such as `<T>` in a sentence does not open one.
fn html_open(line: &str) -> Option<bool> {
    let (cols, bytes) = indent(line);
    if cols > 3 {
        return None;
    }
    let rest = line[bytes..].trim_end();
    if rest.starts_with("<!--") {
        return Some(true);
    }
    let inner = rest.strip_prefix('<')?;
    let inner = inner.strip_prefix('/').unwrap_or(inner);
    let name: String = inner.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let after = &inner[name.len()..];
    let boundary = after.is_empty() || after.starts_with([' ', '\t', '>']) || after.starts_with("/>");
    if !boundary {
        return None;
    }
    if BLOCK_TAGS.contains(&name.to_ascii_lowercase().as_str()) {
        return Some(false);
    }
    // Any other tag opens a block only as a complete tag alone on its line.
    (rest.ends_with('>') && rest.matches('<').count() == 1).then_some(false)
}

fn pipe_cells(line: &str) -> usize {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').count()
}

fn delimiter_row(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('-') {
        return false;
    }
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let cells: Vec<&str> = t.split('|').collect();
    (line.contains('|') || cells.len() > 1)
        && cells.iter().all(|cell| {
            let c = cell.trim();
            let c = c.strip_prefix(':').unwrap_or(c);
            let c = c.strip_suffix(':').unwrap_or(c);
            !c.is_empty() && c.chars().all(|x| x == '-')
        })
}

fn table_start(lines: &[&str], i: usize) -> bool {
    let (cols, _) = indent(lines[i]);
    cols <= 3
        && lines[i].contains('|')
        && i + 1 < lines.len()
        && delimiter_row(lines[i + 1])
        && pipe_cells(lines[i]) == pipe_cells(lines[i + 1])
}

/// Whether line `i` starts a block that ends a paragraph (or a list or quote at indent 0).
fn interrupts(lines: &[&str], i: usize) -> bool {
    let line = lines[i];
    atx_heading(line).is_some()
        || fence_open(line).is_some()
        || thematic_break(line)
        || quote_marker(line)
        || matches!(html_open(line), Some(true))
        || table_start(lines, i)
}

/// Splits `text` into blocks. Never fails: every line ends up in exactly one block.
pub(crate) fn parse_blocks(text: &str) -> Vec<Block> {
    let lines = split_lines(text);
    let content: Vec<&str> = lines.iter().map(|l| &text[l.start..l.content_end]).collect();
    let mut blocks = Vec::new();
    let span = |a: usize, b: usize| Span { start: lines[a].start, end: lines[b].end, line: a + 1, end_line: b + 1 };
    let mut i = 0;
    if let Some((format, close)) = front_matter(text, &lines) {
        blocks.push(Block { kind: BlockKind::FrontMatter(format), span: span(0, close) });
        i = close + 1;
    }
    let n = lines.len();
    while i < n {
        let line = content[i];
        // Blank lines.
        if is_blank(line) {
            let mut j = i;
            while j + 1 < n && is_blank(content[j + 1]) {
                j += 1;
            }
            blocks.push(Block { kind: BlockKind::Blank, span: span(i, j) });
            i = j + 1;
            continue;
        }
        let (cols, _) = indent(line);
        // Indented code (cannot interrupt a paragraph: a paragraph consumes its lazy lines).
        if cols >= 4 {
            let mut j = i;
            let mut k = i;
            while k + 1 < n && (is_blank(content[k + 1]) || indent(content[k + 1]).0 >= 4) {
                k += 1;
                if !is_blank(content[k]) {
                    j = k;
                }
            }
            blocks.push(Block { kind: BlockKind::IndentedCode, span: span(i, j) });
            i = j + 1;
            continue;
        }
        if let Some((c, len, info)) = fence_open(line) {
            let mut j = i + 1;
            while j < n && !fence_close(content[j], c, len) {
                j += 1;
            }
            let last = j.min(n - 1);
            blocks.push(Block { kind: BlockKind::FencedCode { info }, span: span(i, last) });
            i = last + 1;
            continue;
        }
        if let Some((level, a, b)) = atx_heading(line) {
            let base = lines[i].start;
            let text_span = Span { start: base + a, end: base + b, line: i + 1, end_line: i + 1 };
            blocks.push(Block {
                kind: BlockKind::Heading { level, setext: false, text: text_span },
                span: span(i, i),
            });
            i += 1;
            continue;
        }
        if thematic_break(line) {
            blocks.push(Block { kind: BlockKind::ThematicBreak, span: span(i, i) });
            i += 1;
            continue;
        }
        if let Some(comment) = html_open(line) {
            let mut j = i;
            if comment {
                while j < n && !content[j].contains("-->") {
                    j += 1;
                }
                j = j.min(n - 1);
            } else {
                while j + 1 < n && !is_blank(content[j + 1]) {
                    j += 1;
                }
            }
            blocks.push(Block { kind: BlockKind::Html, span: span(i, j) });
            i = j + 1;
            continue;
        }
        if table_start(&content, i) {
            let mut j = i + 1;
            while j + 1 < n && !is_blank(content[j + 1]) && !interrupts(&content, j + 1) {
                j += 1;
            }
            blocks.push(Block { kind: BlockKind::Table, span: span(i, j) });
            i = j + 1;
            continue;
        }
        let quote = quote_marker(line);
        if quote || list_marker(line) {
            let mut j = i;
            loop {
                if j + 1 >= n {
                    break;
                }
                let next = content[j + 1];
                if is_blank(next) {
                    if quote {
                        break;
                    }
                    // A list continues past blank lines when the next line is indented or an item.
                    let mut k = j + 1;
                    while k < n && is_blank(content[k]) {
                        k += 1;
                    }
                    if k < n && (indent(content[k]).0 >= 2 || list_marker(content[k])) {
                        j = k;
                        continue;
                    }
                    break;
                }
                let next_cols = indent(next).0;
                let interrupted = if quote {
                    !quote_marker(next) && (interrupts(&content, j + 1) || list_marker(next))
                } else {
                    next_cols < 2 && !list_marker(next) && interrupts(&content, j + 1)
                };
                if interrupted {
                    break;
                }
                j += 1;
            }
            let kind = if quote { BlockKind::BlockQuote } else { BlockKind::List };
            blocks.push(Block { kind, span: span(i, j) });
            i = j + 1;
            continue;
        }
        // A paragraph, possibly turned into a setext heading by its underline.
        let mut j = i;
        let mut setext = None;
        while j + 1 < n {
            let next = content[j + 1];
            if is_blank(next) {
                break;
            }
            if let Some(level) = setext_underline(next) {
                setext = Some((level, j + 1));
                break;
            }
            if interrupts(&content, j + 1) || list_interrupts_paragraph(next) {
                break;
            }
            j += 1;
        }
        match setext {
            Some((level, under)) => {
                let a = lines[i].start + indent(content[i]).1;
                let b = lines[j].start + content[j].trim_end().len();
                let text_span = Span { start: a, end: b.max(a), line: i + 1, end_line: j + 1 };
                blocks.push(Block {
                    kind: BlockKind::Heading { level, setext: true, text: text_span },
                    span: span(i, under),
                });
                i = under + 1;
            }
            None => {
                blocks.push(Block { kind: BlockKind::Paragraph, span: span(i, j) });
                i = j + 1;
            }
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<String> {
        parse_blocks(text)
            .iter()
            .map(|b| match &b.kind {
                BlockKind::Heading { level, setext, .. } => format!("H{level}{}", if *setext { "s" } else { "" }),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn assert_tiles(text: &str) {
        let blocks = parse_blocks(text);
        let mut pos = 0;
        for b in &blocks {
            assert_eq!(b.span.start, pos, "gap or overlap before {b:?} in {text:?}");
            assert!(b.span.end > b.span.start);
            pos = b.span.end;
        }
        assert_eq!(pos, text.len());
    }

    #[test]
    fn blocks_tile_the_input() {
        let text = "---\na: 1\n---\n# T\n\npara\nmore\n\n- a\n- b\n\n  cont\n\n> q\n> r\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\ncode\n```\n\n    indented\n\n<div>\nx\n</div>\n\n***\n";
        assert_tiles(text);
        assert_eq!(
            kinds(text),
            vec![
                "FrontMatter(Yaml)", "H1", "Blank", "Paragraph", "Blank", "List", "Blank", "BlockQuote", "Blank",
                "Table", "Blank", "FencedCode { info: \"rust\" }", "Blank", "IndentedCode", "Blank", "Html",
                "Blank", "ThematicBreak"
            ]
        );
    }

    #[test]
    fn setext_headings_are_sections() {
        let text = "Title\n=====\n\nSub\nline\n---\ntext\n";
        assert_tiles(text);
        assert_eq!(kinds(text), vec!["H1s", "Blank", "H2s", "Paragraph"]);
        let blocks = parse_blocks(text);
        let BlockKind::Heading { text: t, .. } = &blocks[2].kind else { panic!("not a heading") };
        assert_eq!(&text[t.start..t.end], "Sub\nline");
    }

    #[test]
    fn hash_lines_in_code_html_and_front_matter_are_not_headings() {
        let text = "---\n# yaml comment\n---\n```\n# fenced\n```\n\n    # indented\n\n<div>\n# in html\n</div>\n\n<!--\n# in comment\n-->\n# Real\n";
        assert_tiles(text);
        let headings: Vec<_> = kinds(text).into_iter().filter(|k| k.starts_with('H') && k != "Html").collect();
        assert_eq!(headings, vec!["H1"]);
    }

    #[test]
    fn unclosed_fence_runs_to_end() {
        let text = "a\n\n```\n# no\nmore";
        assert_tiles(text);
        assert_eq!(kinds(text), vec!["Paragraph", "Blank", "FencedCode { info: \"\" }"]);
    }

    #[test]
    fn degenerate_inputs_round_trip() {
        for text in ["", "\n", "no heading", "# only", "a\r\n# b\r\n\r\nc", "   \n\t\n", "#\n##x\n# #\n", "|a|\n|-|\n", "---\nunclosed"] {
            assert_tiles(text);
        }
        assert_eq!(kinds("##x\n"), vec!["Paragraph"]);
        assert_eq!(kinds("# #\n"), vec!["H1"]);
    }

    #[test]
    fn atx_heading_text_drops_closing_hashes() {
        assert_eq!(atx_heading("## Title ##"), Some((2, 3, 8)));
        assert_eq!(atx_heading("# C#"), Some((1, 2, 4)));
        assert_eq!(atx_heading("    # code"), None);
    }

    #[test]
    fn autolinks_and_inline_tags_do_not_open_html_blocks() {
        assert_eq!(kinds("<https://example.org>\n# H\n"), vec!["Paragraph", "H1"]);
        assert_eq!(kinds("<T> is generic\n"), vec!["Paragraph"]);
        assert_eq!(kinds("<details>\n# in html\n</details>\n"), vec!["Html"]);
        assert_eq!(kinds("<custom-tag>\n# in html\n"), vec!["Html"]);
        assert_eq!(kinds("text\n    lazy\n"), vec!["Paragraph"]);
    }

    #[test]
    fn table_interrupts_a_paragraph() {
        let text = "Intro:\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        assert_tiles(text);
        assert_eq!(kinds(text), vec!["Paragraph", "Table"]);
    }

    #[test]
    fn front_matter_agrees_with_records() {
        for text in [
            "---\na: 1\n---\nbody", "+++\na = 1\n+++\n", "---\na: 1\n...\n", "\u{feff}---\nx: 1\n---\n",
            "---\nunclosed\n", "no front matter", "---", "---\r\na: 1\r\n---\r\nb",
        ] {
            let lines = split_lines(text);
            let ours = front_matter(text, &lines).map(|(_, close)| lines[close].end).unwrap_or(0);
            assert_eq!(ours, liquers_records::search::front_matter_end(text), "{text:?}");
        }
    }
}
