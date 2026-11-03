//! AC-7: every Markdown document under `specs/` parses into blocks that tile it exactly.

use std::path::{Path, PathBuf};

use liquers_markdown::{BlockKind, Document};

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

#[test]
fn every_spec_document_round_trips() {
    let specs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../specs");
    let mut files = Vec::new();
    markdown_files(&specs, &mut files);
    assert!(files.len() > 100, "found only {} files under {}", files.len(), specs.display());
    let mut headings = 0;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("readable UTF-8");
        let doc = Document::parse(&text);
        let mut rebuilt = String::with_capacity(text.len());
        let mut pos = 0;
        let mut line = 1;
        for block in doc.blocks() {
            assert_eq!(block.span.start, pos, "{}: gap before line {}", file.display(), block.span.line);
            assert_eq!(block.span.line, line, "{}: line count drifts", file.display());
            rebuilt.push_str(&text[block.span.start..block.span.end]);
            pos = block.span.end;
            line = block.span.end_line + 1;
            if let BlockKind::Heading { .. } = block.kind {
                headings += 1;
            }
        }
        assert_eq!(rebuilt, text, "{}", file.display());
        for section in doc.sections() {
            let address = liquers_markdown::Address::parse(&section.address).expect("address parses");
            assert_eq!(doc.resolve(&address).ok().map(|i| &doc.sections()[i].address), Some(&section.address));
        }
        let _ = doc.links();
    }
    assert!(headings > 1000);
}

/// The addresses `specs/reference/MARKDOWN.md` and the design use as examples resolve against the
/// documents they name (liquers-validate checks only that the queries parse).
#[test]
fn documented_addresses_resolve() {
    let specs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../specs");
    for (file, address, heading) in [
        ("reference/AGENT_MEMORY_SERVICE.md", "_.storage", "8. Storage"),
        ("reference/AGENT_MEMORY_SERVICE.md", "_.8", "8. Storage"),
        ("reference/AGENT_MEMORY_SERVICE.md", "1.8", "8. Storage"),
        ("reference/ASSETS.md", "_.history", "History"),
        ("design/runtime-volatility/DESIGN.md", "_.phase 2.solution", "Solution"),
    ] {
        let text = std::fs::read_to_string(specs.join(file)).expect("readable");
        let doc = Document::parse(&text);
        let id = doc
            .resolve(&liquers_markdown::Address::parse(address).expect("parses"))
            .unwrap_or_else(|e| panic!("{file} {address}: {e}"));
        assert_eq!(doc.sections()[id].heading, heading, "{file} {address}");
    }
}
