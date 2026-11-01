//! The real `docs/guides/`: the three guides are parallel, and the skeleton follows the outline
//! convention (`specs/design/user-documentation-guides/guide-outlines.md`).

use std::path::{Path, PathBuf};

use liquers_docs::build::{check, discover_guides};
use liquers_docs::chapter::{Chapter, ChapterStatus};
use liquers_docs::guide::GuideConfig;
use liquers_docs::markers::{notes, BuildMode, Marker};
use liquers_docs::Severity;

type Res = Result<(), Box<dyn std::error::Error>>;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn guides_are_parallel() -> Res {
    let root = repo_root();
    let guides = discover_guides(&root)?;
    assert_eq!(guides, vec!["python", "rust", "web"]);
    let mut first: Option<Vec<String>> = None;
    for guide in &guides {
        let dir = root.join("docs/guides").join(guide);
        let config = GuideConfig::load(&dir)?;
        assert_eq!(&config.language, guide);
        assert_eq!(config.chapters.first().map(String::as_str), Some("index.md"));
        match &first {
            Some(chapters) => assert_eq!(chapters, &config.chapters, "{guide} differs"),
            None => first = Some(config.chapters.clone()),
        }
        for file in &config.chapters {
            let text = std::fs::read_to_string(dir.join(file))?;
            let chapter = Chapter::parse(file, &text)?;
            if chapter.meta.status == ChapterStatus::Outline {
                let found = notes(&chapter.body);
                assert!(
                    found.iter().any(|n| n.marker == Marker::Purpose),
                    "{guide}/{file} has no PURPOSE:"
                );
                assert!(
                    found.iter().any(|n| n.marker == Marker::Tbd) || text.contains("{{"),
                    "{guide}/{file} has no TBD: and no generated content"
                );
            }
        }
    }
    assert_eq!(first.map(|c| c.len()), Some(20));
    Ok(())
}

#[tokio::test]
async fn the_guides_check_clean() -> Res {
    let findings = check(&repo_root(), BuildMode::Draft).await?;
    let errors: Vec<String> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    Ok(())
}
