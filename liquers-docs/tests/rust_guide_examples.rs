//! Every Rust example of the Rust user guide is code that the tests compile and run.
//!
//! A ```rust block of `docs/guides/rust/*.md` must be a contiguous slice of
//! `liquers-lib/tests/user_guide_GUIDE.rs` or `liquers-axum/tests/user_guide_GUIDE.rs`, compared
//! line by line with indentation, blank lines and the `// --- name` section markers ignored. A
//! block whose first line starts with `// Sketch` shows a shape and is exempt.
//!
//! When this fails, the chapter and the example file have drifted apart: change them together.

use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The lines that count: trimmed, without blank lines and section markers.
fn significant(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("// --- "))
        .map(str::to_string)
        .collect()
}

fn contains_slice(haystack: &[String], needle: &[String]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|window| window == needle)
}

/// The ```rust blocks of a chapter, each with the line number of its opening fence.
fn rust_blocks(text: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, Vec<&str>)> = None;
    for (index, line) in text.lines().enumerate() {
        match current.take() {
            Some((start, lines)) if line.trim() == "```" => blocks.push((start, lines.join("\n"))),
            Some((start, mut lines)) => {
                lines.push(line);
                current = Some((start, lines));
            }
            None if line.trim() == "```rust" => current = Some((index + 1, Vec::new())),
            None => {}
        }
    }
    blocks
}

#[test]
fn every_rust_block_of_the_rust_guide_is_tested_code() {
    let root = repository();
    let sources: Vec<Vec<String>> = [
        "liquers-lib/tests/user_guide_GUIDE.rs",
        "liquers-axum/tests/user_guide_GUIDE.rs",
    ]
    .iter()
    .map(|path| significant(&std::fs::read_to_string(root.join(path)).unwrap()))
    .collect();

    let mut chapters: Vec<PathBuf> = std::fs::read_dir(root.join("docs/guides/rust"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    chapters.sort();

    let mut checked = 0;
    let mut problems = Vec::new();
    for chapter in &chapters {
        let text = std::fs::read_to_string(chapter).unwrap();
        for (line, block) in rust_blocks(&text) {
            let lines = significant(&block);
            if lines.first().is_some_and(|first| first.starts_with("// Sketch")) {
                continue;
            }
            checked += 1;
            if !sources.iter().any(|source| contains_slice(source, &lines)) {
                problems.push(format!(
                    "{}:{line}: this example is not in the guide's example tests (first line: {})",
                    chapter.strip_prefix(&root).unwrap_or(chapter).display(),
                    lines.first().cloned().unwrap_or_default(),
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(checked >= 40, "only {checked} Rust examples found; the block parser may be broken");
}

#[test]
fn a_changed_line_is_noticed() {
    let source = significant("fn a() {\n    let x = 1;\n\n    // --- part\n    let y = 2;\n}\n");
    assert!(contains_slice(&source, &significant("let x = 1;\nlet y = 2;")));
    assert!(!contains_slice(&source, &significant("let x = 1;\nlet y = 3;")));
    assert_eq!(rust_blocks("a\n```rust\nlet x = 1;\n```\nb\n```text\nq\n```\n").len(), 1);
}
