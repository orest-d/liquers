//! The build tree over a fixture repository: `SUMMARY.md` follows `guide.yaml`, `book.toml` has
//! search on, the Pandoc file holds the chapters in order with a title block, and the findings
//! name what is wrong.

use std::path::PathBuf;

use liquers_docs::build::{build, check};
use liquers_docs::markers::BuildMode;
use liquers_docs::Severity;

type Res = Result<(), Box<dyn std::error::Error>>;

fn fixture(name: &str, files: &[(&str, &str)]) -> Result<PathBuf, std::io::Error> {
    let root = std::env::temp_dir().join(format!("liquers-docs-build-{name}-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    for (path, text) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    Ok(root)
}

const GUIDE: &str = "title: Liquers for Python\nlanguage: python\nauthors: [Orest Dubay]\nchapters: [index.md, 01-introduction.md, 02-installing.md, a-query-syntax.md]\n";

fn files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("docs/index.md", "# Liquers\n\n[Guides](guides/index.md)\n"),
        ("docs/guides/index.md", "# User guides\n"),
        ("docs/theme/liquers.css", ":root { --x: 1; }\n"),
        ("docs/guides/python/guide.yaml", GUIDE),
        ("docs/guides/python/HISTORY.md", "# History\n\n| Date | Chapter | Change |\n|---|---|---|\n| 2026-11-01 | 01-introduction.md | reviewed |\n"),
        ("docs/guides/python/index.md", "---\ntitle: Preface\nstatus: ready\n---\n# Preface\n\nWelcome.\n"),
        ("docs/guides/python/01-introduction.md", "---\ntitle: Introduction\nstatus: ready\nreviewed: 2026-11-01\n---\n# 1. Introduction\n\nPURPOSE: orient the reader.\n\nSee [installing](02-installing.md).\n"),
        ("docs/guides/python/02-installing.md", "---\ntitle: Installing\nstatus: draft\n---\n# 2. Installing\n\nTBD: pip.\n"),
        ("docs/guides/python/a-query-syntax.md", "---\ntitle: Query syntax\nstatus: outline\n---\n# A. Query syntax\n\n{{> ../_shared/syntax.md}}\n"),
        ("docs/guides/_shared/syntax.md", "Segments are separated by `/-/`."),
        ("specs/README.md", "# Specs\n"),
    ]
}

#[tokio::test]
async fn build_tree_feeds_both_renderers() -> Res {
    let root = fixture("tree", &files())?;
    let out = root.join("target/docs-build");
    let findings = build(&root, &out, BuildMode::Draft).await?;
    assert!(findings.iter().all(|f| f.severity == Severity::Warning), "{findings:?}");

    let summary = std::fs::read_to_string(out.join("python/src/SUMMARY.md"))?;
    assert_eq!(
        summary,
        "# Summary\n\n[Preface](index.md)\n\n- [Introduction](01-introduction.md)\n\
         - [Installing](02-installing.md)\n\n---\n\n[A. Query syntax](a-query-syntax.md)\n"
    );
    let book = std::fs::read_to_string(out.join("python/book.toml"))?;
    assert!(book.contains("title = \"Liquers for Python\""));
    assert!(book.contains("site-url = \"/liquers/guides/python/\""));
    assert!(book.contains("enable = true"));
    assert!(out.join("python/theme/liquers.css").is_file());

    let appendix = std::fs::read_to_string(out.join("python/src/a-query-syntax.md"))?;
    assert!(appendix.contains("Segments are separated by `/-/`."), "{appendix}");
    let intro = std::fs::read_to_string(out.join("python/src/01-introduction.md"))?;
    assert!(intro.contains("> **PURPOSE:** orient the reader."), "{intro}");
    assert!(!intro.starts_with("---"), "front matter is removed: {intro}");

    let pandoc = std::fs::read_to_string(out.join("python.md"))?;
    assert!(pandoc.starts_with("---\n"));
    assert!(pandoc.contains("title: Liquers for Python"));
    assert!(pandoc.contains("- Orest Dubay"));
    let preface = pandoc.find("# Preface {#index}").ok_or("preface")?;
    let intro = pandoc.find("# 1. Introduction {#01-introduction}").ok_or("intro")?;
    let appendix = pandoc.find("# A. Query syntax {#a-query-syntax}").ok_or("appendix")?;
    assert!(preface < intro && intro < appendix);
    assert!(pandoc.contains("[installing](#02-installing)"));

    let tsv = std::fs::read_to_string(out.join("guides.tsv"))?;
    assert_eq!(tsv, "python\tliquers-for-python\tLiquers for Python\n");
    assert!(out.join("home/src/index.md").is_file());
    assert!(out.join("home/src/guides/index.md").is_file());

    let release = build(&root, &out, BuildMode::Release).await?;
    assert!(release.iter().all(|f| f.severity == Severity::Warning), "{release:?}");
    let installing = std::fs::read_to_string(out.join("python/src/02-installing.md"))?;
    assert_eq!(installing, "# 2. Installing\n\n*This chapter is in preparation.*\n");
    let intro = std::fs::read_to_string(out.join("python/src/01-introduction.md"))?;
    assert!(!intro.contains("PURPOSE"));
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[tokio::test]
async fn findings_name_the_problem() -> Res {
    let mut files = files();
    files.push(("docs/guides/python/notes.md", "loose notes"));
    files[6] = (
        "docs/guides/python/01-introduction.md",
        "---\ntitle: Introduction\nstatus: ready\nreviewed: 2026-12-24\n---\n# 1. Introduction\n\nTBD: write it.\n\nSee [specs](../../../specs/README.md) and [gone](03-gone.md).\n\n::: note\n",
    );
    let root = fixture("findings", &files)?;
    let draft = check(&root, BuildMode::Draft).await?;
    let errors: Vec<String> = draft
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{}:{} {}", f.file, f.line, f.message))
        .collect();
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().any(|e| e.contains(":10 link '../../../specs/README.md' points into specs/")));
    assert!(errors.iter().any(|e| e.contains("link '03-gone.md' does not resolve")));
    assert!(errors.iter().any(|e| e.contains(":12 a fenced div")));
    let warnings: Vec<String> = draft
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .map(|f| format!("{}:{} {}", f.file, f.line, f.message))
        .collect();
    assert!(warnings.iter().any(|w| w.contains("notes.md") && w.contains("not listed")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("2026-12-24 has no row")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains(":8 TBD:")), "{warnings:?}");

    let release = check(&root, BuildMode::Release).await?;
    assert!(release
        .iter()
        .any(|f| f.severity == Severity::Error && f.line == 8 && f.message.starts_with("TBD:")));
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}
