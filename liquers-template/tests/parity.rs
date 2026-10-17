//! Parity with core's query parser (AC-14): for a query without placeholders,
//! `QueryTemplate::from(&parse_query(q)) == parse_query_template(q)` and
//! `parse_query_template(q).try_into_query() == Some(parse_query(q))`, with positions compared
//! separately; a text core rejects is rejected at the same position.
#![cfg(not(target_arch = "wasm32"))]

use liquers_core::parse::parse_query;
use liquers_core::query::{ActionParameter, Position, Query, QuerySegment};
use liquers_template::{parse_query_template, QueryTemplate};

fn positions(query: &Query, out: &mut Vec<Position>) {
    for segment in &query.segments {
        match segment {
            QuerySegment::Resource(r) => {
                if let Some(h) = &r.header {
                    out.push(h.position.clone());
                }
                out.extend(r.key.0.iter().map(|n| n.position.clone()));
            }
            QuerySegment::Transform(t) => {
                if let Some(h) = &t.header {
                    out.push(h.position.clone());
                }
                for action in &t.query {
                    out.push(action.position.clone());
                    for p in &action.parameters {
                        match p {
                            ActionParameter::String(_, pos) => out.push(pos.clone()),
                            ActionParameter::Link(link, pos) => {
                                out.push(pos.clone());
                                positions(link, out);
                            }
                        }
                    }
                }
                if let Some(f) = &t.filename {
                    out.push(f.position.clone());
                }
            }
        }
    }
}

/// Checks one text; returns whether core accepted it.
fn check(text: &str) -> bool {
    match (parse_query(text), parse_query_template(text)) {
        (Ok(query), Ok(template)) => {
            assert_eq!(QueryTemplate::from(&query), template, "lifted AST differs for {text:?}");
            let back = template.try_into_query();
            assert_eq!(back.as_ref(), Some(&query), "query differs for {text:?}");
            let (mut a, mut b) = (Vec::new(), Vec::new());
            positions(&query, &mut a);
            if let Some(back) = &back {
                positions(back, &mut b);
            }
            assert_eq!(a, b, "positions differ for {text:?}");
            assert_eq!(template.encode(), query.encode(), "encoding differs for {text:?}");
            true
        }
        (Err(a), Err(b)) => {
            assert_eq!(a.position, b.position, "rejected at different positions: {text:?}");
            assert_eq!(a.message, b.message, "rejected differently: {text:?}");
            false
        }
        (Ok(_), Err(e)) => panic!("core accepts {text:?}, the template parser rejects it: {e}"),
        (Err(e), Ok(_)) => panic!("core rejects {text:?} ({e}), the template parser accepts it"),
    }
}

/// String literals passed to `parse_query(…)` in a Rust source.
fn literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in source.match_indices("parse_query(\"") {
        let start = i + "parse_query(\"".len();
        let rest = &source[start..];
        let Some(end) = rest.find('"') else { continue };
        let text = &rest[..end];
        if !text.contains('\\') {
            out.push(text.to_string());
        }
    }
    out
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn parity_core_parser_corpus() {
    let mut files = Vec::new();
    rust_files(&root().join("liquers-core/src"), &mut files);
    rust_files(&root().join("liquers-core/tests"), &mut files);
    let mut accepted = 0;
    let mut total = 0;
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap_or_default();
        for text in literals(&source) {
            if text.contains("{{") {
                continue;
            }
            total += 1;
            if check(&text) {
                accepted += 1;
            }
        }
    }
    assert!(total > 200 && accepted > 200, "corpus too small: {accepted} of {total}");
}

#[test]
fn parity_specs_and_registry_corpus() {
    let mut texts = Vec::new();
    let mut stack = vec![root().join("specs")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                // Code spans that look like queries: they hold a `/` or a `-`, and no space.
                for span in text.split('`').skip(1).step_by(2) {
                    if !span.is_empty()
                        && span.len() < 300
                        && !span.contains(char::is_whitespace)
                        && !span.contains("{{")
                        && (span.contains('/') || span.contains('-'))
                    {
                        texts.push(span.to_string());
                    }
                }
            }
        }
    }
    let registry = std::fs::read_to_string(root().join("specs/command_registry.yaml")).unwrap_or_default();
    let mut namespace = String::new();
    for line in registry.lines() {
        let line = line.trim();
        if let Some(ns) = line.strip_prefix("namespace: ") {
            namespace = ns.trim_matches(|c| c == '\'' || c == '"').to_string();
        }
        if let Some(name) = line.strip_prefix("name: ") {
            let name = name.trim_matches(|c| c == '\'' || c == '"');
            texts.push(if namespace.is_empty() {
                name.to_string()
            } else {
                format!("ns-{namespace}/{name}")
            });
        }
    }
    let accepted = texts.iter().filter(|t| check(t)).count();
    assert!(accepted > 300, "only {accepted} of {} accepted", texts.len());
}

#[test]
fn parity_generated_queries() {
    let resources = ["", "-R/data/a.csv", "-R-key/specs/issues", "/-R/x/y.txt", "-R/./a", "-R-cwd/t/-/", "a/b"];
    let headers = ["", "-/", "-q/", "-store_config/"];
    let actions = [
        "cmd",
        "cmd-1-2",
        "cmd-a~_b",
        "f-~X~-R/k.txt~E",
        "ns-pl/head-5",
        "to_text",
        "s-~ncolon~x~.~_y",
        "a-~X~b-~X~-R/c~E~E",
    ];
    let filenames = ["", "/out.html", "/data.csv"];
    let mut count = 0;
    for r in resources {
        for h in headers {
            for (i, a) in actions.iter().enumerate() {
                for f in filenames {
                    let second = actions[(i * 3 + 1) % actions.len()];
                    for text in [
                        format!("{r}{}{h}{a}{f}", if r.is_empty() || r.ends_with('/') { "" } else { "/" }),
                        format!("{h}{a}/{second}{f}"),
                        format!("{r}/-/{a}/{second}"),
                    ] {
                        check(&text);
                        count += 1;
                    }
                }
            }
        }
    }
    assert!(count > 1000);
}

#[test]
fn parity_rejects_at_same_position() {
    for text in [
        "cmd-~", "-R/a b", "a/-/-/b", "cmd-~X~x", "~X~", "f-~U110000~", "-R-", "x//y", "a-~X~-R/~E/~", "ns-",
        "cmd-~nnosuchentity~", "/-/", "a.b/c", "\u{0}",
    ] {
        check(text);
    }
}
