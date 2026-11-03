//! Section addresses: `1.2.3`, `phase~.2.solution`, `_.8`, `3..2`.

use std::fmt;

use liquers_core::error::Error;

use crate::section::Section;

/// One step of an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A section number: the `n`-th subsection, counted from 1 in document order.
    Number(usize),
    /// Heading text, matched without case on word boundaries: exact, then prefix, then contained.
    Text(String),
    /// `_`: any one subsection.
    Any,
}

/// A path of steps, each choosing among the subsections of the previous one (the first among
/// the top-level sections).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address(pub Vec<Step>);

fn invalid(text: &str, why: &str) -> Error {
    Error::general_error(format!("md: invalid address '{text}': {why}"))
}

impl Address {
    /// Parses an address: steps separated by `.`, `..` a literal dot; a digits-only step is a
    /// number, `_` alone is the placeholder, `__` a literal underscore, any other `_` reserved.
    pub fn parse(text: &str) -> Result<Address, Error> {
        if text.is_empty() {
            return Err(invalid(text, "the address is empty"));
        }
        let mut raw_steps: Vec<String> = vec![String::new()];
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '.' {
                if chars.peek() == Some(&'.') {
                    chars.next();
                    if let Some(s) = raw_steps.last_mut() {
                        s.push('\u{0}');
                    }
                } else {
                    raw_steps.push(String::new());
                }
            } else if let Some(s) = raw_steps.last_mut() {
                s.push(c);
            }
        }
        let mut steps = Vec::with_capacity(raw_steps.len());
        for raw in raw_steps {
            if raw.is_empty() {
                return Err(invalid(text, "a step is empty (a leading, trailing or doubled separator)"));
            }
            if raw == "_" {
                steps.push(Step::Any);
                continue;
            }
            if raw.chars().all(|c| c.is_ascii_digit()) {
                let n: usize = raw.parse().map_err(|_| invalid(text, "a section number is too large"))?;
                if n == 0 {
                    return Err(invalid(text, "section numbers start at 1"));
                }
                steps.push(Step::Number(n));
                continue;
            }
            let mut out = String::with_capacity(raw.len());
            let mut chars = raw.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '\u{0}' => out.push('.'),
                    '_' if chars.peek() == Some(&'_') => {
                        chars.next();
                        out.push('_');
                    }
                    '_' => return Err(invalid(text, "'_' is reserved inside a step (write '__' for an underscore)")),
                    c => out.push(c),
                }
            }
            steps.push(Step::Text(out));
        }
        Ok(Address(steps))
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, step) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            match step {
                Step::Number(n) => write!(f, "{n}")?,
                Step::Any => f.write_str("_")?,
                Step::Text(t) => f.write_str(&t.replace('_', "__").replace('.', ".."))?,
            }
        }
        Ok(())
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn is_word(c: Option<char>) -> bool {
    c.is_some_and(char::is_alphanumeric)
}

/// How well `step` (normalized) matches `heading`: 0 exact, 1 prefix, 2 contained.
pub(crate) fn match_tier(heading: &str, number: Option<&str>, step: &str) -> Option<u8> {
    let h = normalize(heading);
    let unnumbered = number
        .map(|n| h[n.len()..].trim_start_matches('.').trim_start().to_string())
        .unwrap_or_else(|| h.clone());
    if h == step || unnumbered == step {
        return Some(0);
    }
    let first = step.chars().next();
    let last = step.chars().last();
    let right_ok = |text: &str, at: usize| !is_word(last) || !is_word(text[at..].chars().next());
    let left_ok = |text: &str, at: usize| !is_word(first) || !is_word(text[..at].chars().last());
    for candidate in [&h, &unnumbered] {
        if candidate.starts_with(step) && right_ok(candidate, step.len()) {
            return Some(1);
        }
    }
    let mut from = 0;
    while let Some(i) = h[from..].find(step) {
        let at = from + i;
        if left_ok(&h, at) && right_ok(&h, at + step.len()) {
            return Some(2);
        }
        from = at + h[at..].chars().next().map(char::len_utf8).unwrap_or(1);
    }
    None
}

fn describe(sections: &[Section], ids: &[usize]) -> String {
    ids.iter()
        .map(|&i| format!("{} ({}, line {})", sections[i].address, sections[i].heading, sections[i].span.line))
        .collect::<Vec<_>>()
        .join("; ")
}

fn descendants(sections: &[Section], ids: &[usize], out: &mut Vec<usize>) {
    for &i in ids {
        out.push(i);
        descendants(sections, &sections[i].children, out);
    }
}

/// Resolves `address` to exactly one section.
pub(crate) fn resolve(sections: &[Section], address: &Address) -> Result<usize, Error> {
    let top: Vec<usize> = (0..sections.len()).filter(|&i| sections[i].parent.is_none()).collect();
    // `None` stands for the document itself, whose children are the top-level sections.
    let mut current: Vec<Option<usize>> = vec![None];
    let children = |c: &Option<usize>| -> Vec<usize> {
        match c {
            Some(i) => sections[*i].children.clone(),
            None => top.clone(),
        }
    };
    for (n, step) in address.0.iter().enumerate() {
        let pool: Vec<usize> = current.iter().flat_map(&children).collect();
        let next: Vec<usize> = match step {
            Step::Any => pool.clone(),
            Step::Number(k) => current.iter().filter_map(|c| children(c).get(k - 1).copied()).collect(),
            Step::Text(t) => {
                let t = normalize(t);
                let tiers: Vec<(usize, u8)> = pool
                    .iter()
                    .filter_map(|&i| match_tier(&sections[i].heading, sections[i].number.as_deref(), &t).map(|m| (i, m)))
                    .collect();
                let best = tiers.iter().map(|(_, m)| *m).min();
                tiers.into_iter().filter(|(_, m)| Some(*m) == best).map(|(i, _)| i).collect()
            }
        };
        if next.is_empty() {
            let shown = Address(address.0[..=n].to_vec());
            let mut message = format!("md: no section '{address}': step {} ('{shown}') matches no section", n + 1);
            if let Step::Text(t) = step {
                let t = normalize(t);
                let mut deeper = Vec::new();
                let below: Vec<usize> = pool.iter().flat_map(|&i| sections[i].children.clone()).collect();
                descendants(sections, &below, &mut deeper);
                deeper.retain(|&i| match_tier(&sections[i].heading, sections[i].number.as_deref(), &t).is_some());
                if !deeper.is_empty() {
                    message.push_str(&format!("; deeper sections match: {}", describe(sections, &deeper)));
                }
            }
            if let Step::Number(k) = step {
                let counts: Vec<String> = current.iter().map(|c| children(c).len().to_string()).collect();
                message.push_str(&format!(" (number {k}; subsections available: {})", counts.join(", ")));
            }
            return Err(Error::general_error(message));
        }
        current = next.into_iter().map(Some).collect();
    }
    let found: Vec<usize> = current.into_iter().flatten().collect();
    match found.as_slice() {
        [one] => Ok(*one),
        many => Err(Error::general_error(format!(
            "md: address '{address}' matches {} sections: {}",
            many.len(),
            describe(sections, many)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn text(t: &str) -> Step {
        Step::Text(t.to_string())
    }

    #[test]
    fn address_parse_steps() {
        assert_eq!(Address::parse("1.2.3").ok(), Some(Address(vec![Step::Number(1), Step::Number(2), Step::Number(3)])));
        assert_eq!(Address::parse("phase 2.solution").ok(), Some(Address(vec![text("phase 2"), text("solution")])));
        assert_eq!(Address::parse("3..2").ok(), Some(Address(vec![text("3.2")])));
        assert_eq!(Address::parse("_.8").ok(), Some(Address(vec![Step::Any, Step::Number(8)])));
        assert_eq!(Address::parse("my__var").ok(), Some(Address(vec![text("my_var")])));
        for bad in ["", "1.", ".1", "1...", "my_var", "_x", "0"] {
            assert!(Address::parse(bad).is_err(), "{bad:?} should be refused");
        }
        assert_eq!(Address::parse("a...b").ok(), Some(Address(vec![text("a."), text("b")])));
    }

    #[test]
    fn address_display_round_trips() {
        for a in ["1.2.3", "phase 2.solution", "3..2", "_.8", "my__var", "a...b"] {
            let parsed = Address::parse(a).expect("parses");
            assert_eq!(parsed.to_string(), a);
            assert_eq!(Address::parse(&parsed.to_string()).ok(), Some(parsed));
        }
    }

    #[test]
    fn address_text_prefers_exact_then_prefix() {
        let doc = Document::parse("# Phase 10\n# Phase 1\n## Examples\n# Phase 1 notes\n# Section 3.2\n# 8. Storage\n");
        let at = |a: &str| doc.resolve(&Address::parse(a).expect("parses")).map(|i| doc.sections()[i].heading.clone());
        assert_eq!(at("phase 1").ok().as_deref(), Some("Phase 1"));
        assert_eq!(at("phase 1.examples").ok().as_deref(), Some("Examples"));
        assert_eq!(at("phase 10").ok().as_deref(), Some("Phase 10"));
        assert_eq!(at("3..2").ok().as_deref(), Some("Section 3.2"));
        assert_eq!(at("storage").ok().as_deref(), Some("8. Storage"));
        assert_eq!(at("notes").ok().as_deref(), Some("Phase 1 notes"));
        assert!(at("phase 2").is_err());
    }

    #[test]
    fn address_placeholder_matches_any() {
        let doc = Document::parse("# Title\n## A\n## B\n### History\n");
        let at = |a: &str| doc.resolve(&Address::parse(a).expect("parses")).map(|i| doc.sections()[i].address.clone());
        assert_eq!(at("_.2").ok().as_deref(), Some("1.2"));
        assert_eq!(at("_._.history").ok().as_deref(), Some("1.2.1"));
        assert!(at("_._").is_err());
    }

    #[test]
    fn address_ambiguous_lists_candidates() {
        let doc = Document::parse("# A\n## Notes\n# B\n## Notes\n");
        let error = doc.resolve(&Address::parse("_.notes").expect("parses")).expect_err("ambiguous");
        let message = error.to_string();
        assert!(message.contains("1.1 (Notes, line 2)") && message.contains("2.1 (Notes, line 4)"), "{message}");
    }

    #[test]
    fn address_missing_names_deeper_match() {
        let doc = Document::parse("# Agent Memory Service\n## 8. Storage\n");
        let message = doc.resolve(&Address::parse("storage").expect("parses")).expect_err("missing").to_string();
        assert!(message.contains("deeper sections match: 1.1 (8. Storage, line 2)"), "{message}");
    }

    #[test]
    fn address_number_out_of_range() {
        let doc = Document::parse("# A\n## B\n");
        let message = doc.resolve(&Address::parse("1.5").expect("parses")).expect_err("missing").to_string();
        assert!(message.contains("number 5; subsections available: 1"), "{message}");
    }
}
