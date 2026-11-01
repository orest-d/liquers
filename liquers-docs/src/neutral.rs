//! The tool-neutral rule (`tooling.md` §5): a chapter is CommonMark with tables and footnotes,
//! plus Liquers templates. Syntax that only one renderer understands is refused, so the renderer
//! can be replaced without touching a chapter.

use crate::markdown::{code_mask, without_code_spans};
use crate::{Finding, Severity};

/// mdBook's own `{{#…}}` helpers. They look like Mustache sections and are refused until Liquers
/// templates read them (`TEMPLATE-HAS-NO-MDBOOK-INCLUDE-SYNTAX`).
const MDBOOK_HELPERS: [&str; 4] = ["include", "rustdoc_include", "playground", "title"];

/// Finds renderer-specific syntax in a chapter body. Code blocks and code spans are not checked,
/// except a fence whose info string is a MyST directive (```` ```{note} ````).
pub fn check_neutral(text: &str) -> Vec<Finding> {
    let lines: Vec<&str> = text.lines().collect();
    let mask = code_mask(&lines);
    let mut findings = Vec::new();
    let mut in_code = false;
    for (i, raw) in lines.iter().enumerate() {
        let mut push = |message: String| {
            findings.push(Finding {
                file: String::new(),
                line: i + 1,
                severity: Severity::Error,
                message,
            })
        };
        if mask[i] {
            let trimmed = raw.trim_start();
            if !in_code {
                let info = trimmed.trim_start_matches(['`', '~']).trim_start();
                if info.starts_with('{') {
                    push(format!("a MyST directive fence ({}) is renderer syntax", trimmed));
                }
            }
            in_code = true;
            // The closing fence ends the block; the mask marks it as code too.
            if i + 1 >= lines.len() || !mask[i + 1] {
                in_code = false;
            }
            continue;
        }
        in_code = false;
        // A MyST role is written {name}`text`; look for it before code spans are blanked.
        if let Some(role) = myst_role(raw) {
            push(format!("a MyST role ({{{role}}}`…`) is renderer syntax"));
        }
        let line = without_code_spans(raw);
        let trimmed = line.trim();
        for helper in MDBOOK_HELPERS {
            if line.contains(&format!("{{{{#{helper}")) {
                push(format!(
                    "mdBook's {{{{#{helper}}}}} is renderer syntax; use a Liquers partial or query tag"
                ));
            }
        }
        if line.contains("{{<") {
            push("a Quarto shortcode ({{< … >}}) is renderer syntax".to_string());
        }
        if trimmed.starts_with(":::") {
            push("a fenced div (:::) is renderer syntax".to_string());
        }
        if trimmed.starts_with('(') && trimmed.ends_with(")=") {
            push(format!("a MyST target ({trimmed}) is renderer syntax"));
        }
        if let Some(attr) = attribute_block(&line) {
            push(format!("an attribute block ({attr}) is renderer syntax"));
        }
        if let Some(reference) = cross_reference(&line) {
            push(format!(
                "a cross-reference ({reference}) is renderer syntax; use a relative link"
            ));
        }
    }
    findings
}

/// `{.class}`, `{#id}` or `{key=value}` written with single braces.
fn attribute_block(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    for i in 0..chars.len() {
        if chars[i] != '{' || (i > 0 && chars[i - 1] == '{') {
            continue;
        }
        if i + 1 < chars.len() && chars[i + 1] == '{' {
            continue;
        }
        let next = match chars.get(i + 1) {
            Some(c) => *c,
            None => continue,
        };
        if !(next == '.' || next == '#') {
            continue;
        }
        if !chars
            .get(i + 2)
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(len) = chars[i..].iter().position(|c| *c == '}') {
            let end = i + len;
            if chars.get(end + 1) == Some(&'}') {
                continue;
            }
            return Some(chars[i..=end].iter().collect());
        }
    }
    None
}

/// A Pandoc or Quarto cross-reference or citation: `@name` at a word start.
fn cross_reference(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    for i in 0..chars.len() {
        if chars[i] != '@' {
            continue;
        }
        let starts_word = i == 0 || matches!(chars[i - 1], ' ' | '\t' | '[' | '(' | ';');
        let named = chars
            .get(i + 1)
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false);
        if starts_word && named {
            let end = chars[i + 1..]
                .iter()
                .position(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ':'))
                .map(|p| i + 1 + p)
                .unwrap_or(chars.len());
            return Some(chars[i..end].iter().collect());
        }
    }
    None
}

/// The name of a MyST role, `{name}` immediately followed by a backtick.
fn myst_role(line: &str) -> Option<String> {
    let mut rest = line;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            let name = &after[..end];
            let followed = after[end + 1..].starts_with('`');
            let preceded = start > 0 && rest.as_bytes()[start - 1] == b'{';
            if followed
                && !preceded
                && !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':')
            {
                return Some(name.to_string());
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(text: &str) -> Vec<String> {
        check_neutral(text).into_iter().map(|f| f.message).collect()
    }

    #[test]
    fn renderer_syntax_is_refused() {
        assert_eq!(messages("{{#include ../x.md}}").len(), 1);
        assert_eq!(messages("::: note").len(), 1);
        assert_eq!(messages("## Heading {.unnumbered}").len(), 1);
        assert_eq!(messages("As shown in @fig-plot.").len(), 1);
        assert_eq!(messages("See {ref}`target`.").len(), 1);
        assert_eq!(messages("(my-label)=").len(), 1);
        assert_eq!(messages("```{note}\ntext\n```").len(), 1);
        assert_eq!(messages("{{< include x.qmd >}}").len(), 1);
    }

    #[test]
    fn plain_markdown_and_templates_pass() {
        let ok = "# 5. Commands\n\nA [link](x.md), mail me at a@b.org, `{.c}` in code.\n\
                  {{#items}}{{name}}{{/items}} and {{$ -R/specs/x.md/-/ns-docs/include-Intro}}\n\
                  {{> ../_shared/q.md}}\n\n```python\n@command\ndef f(): ...\n```\n";
        assert!(messages(ok).is_empty(), "{:?}", messages(ok));
    }
}
