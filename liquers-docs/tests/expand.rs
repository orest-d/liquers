//! Chapter expansion over a fixture repository on disk: partials from `_shared/`, includes from
//! `specs/`, and the recorded dependency on the included document.

use std::path::{Path, PathBuf};

use liquers_core::assets::AssetManager;
use liquers_core::metadata::DependencyKey;
use liquers_core::parse::{parse_key, parse_query};
use liquers_docs::env::{build_environment, expand_chapter};

type Res = Result<(), Box<dyn std::error::Error>>;

/// A fresh fixture repository under the system temp folder.
pub fn fixture(name: &str, files: &[(&str, &str)]) -> Result<PathBuf, std::io::Error> {
    let root = std::env::temp_dir().join(format!("liquers-docs-{name}-{}", std::process::id()));
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

const CHAPTER: &str = "---\ntitle: Commands\nstatus: draft\n---\n# 5. Commands\n\n{{> ../_shared/q.md}}\n\n## 5.2 Defaults\n\n{{$ -R/specs/reference/DECL.md/-/ns-docs/include-Argument~.defaults}}\n";
const DECL: &str = "# Declaration\n\n## 4. Arguments\n\n### 4.3 Argument defaults\n\nA default is used when the argument is omitted.\n\n#### Links\n\nA link default is a query.\n\n### 4.4 Other\n\nNot included.\n";

fn files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("docs/guides/python/05-commands.md", CHAPTER),
        ("docs/guides/_shared/q.md", "A query is a path."),
        ("specs/reference/DECL.md", DECL),
    ]
}

#[tokio::test]
async fn chapter_expands_partials_and_includes() -> Res {
    let root = fixture("expand", &files())?;
    let env = build_environment(&root)?;
    let key = parse_key("docs/guides/python/05-commands.md")?;
    let text = expand_chapter(&env, &key).await?;
    assert!(text.contains("A query is a path."), "{text}");
    assert!(text.contains("A default is used when the argument is omitted."), "{text}");
    assert!(text.contains("#### Links"), "{text}");
    assert!(!text.contains("Not included."), "{text}");
    assert!(!text.contains("{{"), "{text}");

    let query = parse_query("-R/docs/guides/python/05-commands.md/-/ns-template/expand--strict-docs~/guides~/python")?;
    let asset = env.get_asset_manager().get_asset(&query).await?;
    let dependencies: Vec<DependencyKey> = asset
        .get_metadata()
        .await?
        .get_dependencies()
        .iter()
        .map(|d| d.key.clone())
        .collect();
    let printed: Vec<String> = dependencies.iter().map(|d| d.to_string()).collect();
    assert!(
        printed.iter().any(|d| d.contains("specs/reference/DECL.md")),
        "the include is a recorded dependency: {printed:?}"
    );
    cleanup(&root);
    Ok(())
}

#[tokio::test]
async fn missing_section_fails_the_expansion() -> Res {
    let mut files = files();
    files[2] = ("specs/reference/DECL.md", "# Declaration\n\nRenamed.\n");
    let root = fixture("missing", &files)?;
    let env = build_environment(&root)?;
    let key = parse_key("docs/guides/python/05-commands.md")?;
    let error = expand_chapter(&env, &key).await.expect_err("missing section");
    assert!(error.to_string().contains("Argument defaults"), "{error}");
    cleanup(&root);
    Ok(())
}

fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}
