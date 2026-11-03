//! Line-level Markdown helpers shared by the text passes: fenced code, code spans, headings and
//! link targets. They cover the CommonMark subset chapters are written in (`tooling.md` §5), with
//! no regex dependency.

/// For each line, whether it belongs to a fenced code block (the fences included).
pub fn code_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = Vec::with_capacity(lines.len());
    let mut fence: Option<(char, usize)> = None;
    for line in lines {
        let trimmed = line.trim_start();
        let opening = fence_of(trimmed);
        match fence {
            Some((ch, len)) => {
                mask.push(true);
                if let Some((c, l)) = opening {
                    if c == ch && l >= len && trimmed.trim_start_matches(c).trim().is_empty() {
                        fence = None;
                    }
                }
            }
            None => {
                if let Some(open) = opening {
                    fence = Some(open);
                    mask.push(true);
                } else {
                    mask.push(false);
                }
            }
        }
    }
    mask
}

/// The fence character and length when the line opens or closes a fenced block.
fn fence_of(trimmed: &str) -> Option<(char, usize)> {
    for ch in ['`', '~'] {
        let len = trimmed.chars().take_while(|c| *c == ch).count();
        if len >= 3 {
            return Some((ch, len));
        }
    }
    None
}

/// The line with its code spans blanked out, so that syntax inside backticks is not seen.
pub fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            let run = chars[i..].iter().take_while(|c| **c == '`').count();
            // Find the matching closing run of the same length.
            let mut j = i + run;
            let mut closed = None;
            while j < chars.len() {
                if chars[j] == '`' {
                    let r = chars[j..].iter().take_while(|c| **c == '`').count();
                    if r == run {
                        closed = Some(j + r);
                        break;
                    }
                    j += r;
                } else {
                    j += 1;
                }
            }
            match closed {
                Some(end) => {
                    out.extend(std::iter::repeat(' ').take(end - i));
                    i = end;
                }
                None => {
                    out.extend(&chars[i..]);
                    break;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// An ATX heading: its level and text, with closing `#`s removed.
pub fn heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim_end().to_string();
    Some((level, text))
}

/// A heading's text without a leading section number (`4.3`, `4.3.`, `1.`, `A.`).
pub fn strip_number(text: &str) -> &str {
    let text = text.trim();
    let first = match text.split_whitespace().next() {
        Some(first) => first,
        None => return text,
    };
    let numeric = first.chars().all(|c| c.is_ascii_digit() || c == '.')
        && first.chars().any(|c| c.is_ascii_digit());
    let letter = first.len() == 2
        && first.ends_with('.')
        && first
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false);
    if numeric || letter {
        text[first.len()..].trim_start()
    } else {
        text
    }
}

/// The targets of the inline links and images on a line (code spans excluded), and of a
/// reference definition (`[label]: target`).
pub fn link_targets(line: &str) -> Vec<String> {
    let line = without_code_spans(line);
    let mut targets = Vec::new();
    let trimmed = line.trim_start();
    if trimmed.starts_with('[') {
        if let Some(close) = trimmed.find("]:") {
            let target = trimmed[close + 2..].split_whitespace().next().unwrap_or("");
            if !target.is_empty() {
                targets.push(target.trim_matches(|c| c == '<' || c == '>').to_string());
            }
        }
    }
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let start = i + 2;
            let mut depth = 1;
            let mut j = start;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            let inside = &line[start..j.min(line.len())];
            let target = inside.split_whitespace().next().unwrap_or("");
            if !target.is_empty() {
                targets.push(target.trim_matches(|c| c == '<' || c == '>').to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    targets
}

/// Whether a link target is absolute: it has a URL scheme.
pub fn is_absolute_url(target: &str) -> bool {
    match target.find(':') {
        Some(pos) => {
            let scheme = &target[..pos];
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        }
        None => false,
    }
}

/// Splits `---` YAML front matter from the body. Returns the YAML (if any), the body, and the
/// number of lines before the body, so findings in the body can report file line numbers.
pub fn split_front_matter(text: &str) -> (Option<&str>, &str, usize) {
    let rest = match text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    {
        Some(rest) => rest,
        None => return (None, text, 0),
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            let lines_before = 2 + yaml.lines().count();
            return (Some(yaml), body, lines_before);
        }
        offset += line.len();
    }
    (None, text, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_and_spans() {
        let lines = vec!["a", "```rust", "# not a heading", "```", "b"];
        assert_eq!(code_mask(&lines), vec![false, true, true, true, false]);
        assert_eq!(without_code_spans("x `{.c}` y"), "x        y");
    }

    #[test]
    fn headings_and_numbers() {
        assert_eq!(
            heading("### 4.3 Argument defaults"),
            Some((3, "4.3 Argument defaults".to_string()))
        );
        assert_eq!(heading("#hashtag"), None);
        assert_eq!(strip_number("4.3 Argument defaults"), "Argument defaults");
        assert_eq!(strip_number("1. Installing"), "Installing");
        assert_eq!(strip_number("A. Query syntax"), "Query syntax");
        assert_eq!(strip_number("Argument defaults"), "Argument defaults");
    }

    #[test]
    fn links() {
        assert_eq!(
            link_targets("see [a](x.md#y) and ![i](img.png \"t\") but not `[c](d)`"),
            vec!["x.md#y".to_string(), "img.png".to_string()]
        );
        assert_eq!(
            link_targets("[ref]: https://e.org"),
            vec!["https://e.org".to_string()]
        );
        assert!(is_absolute_url("https://e.org"));
        assert!(!is_absolute_url("../specs/x.md"));
    }

    #[test]
    fn front_matter() {
        let (yaml, body, before) = split_front_matter("---\ntitle: X\nstatus: draft\n---\n# X\n");
        assert_eq!(yaml, Some("title: X\nstatus: draft\n"));
        assert_eq!(body, "# X\n");
        assert_eq!(before, 4);
        assert_eq!(split_front_matter("# X\n").0, None);
    }
}
