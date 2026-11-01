//! The pipeline: read the guides, expand and check every chapter, and write the build tree that
//! mdBook and Pandoc render (`specs/design/user-documentation-guides/phase2-architecture.md`).
//!
//! The tree, under `out/`:
//! - `<guide>/book.toml`, `<guide>/theme/liquers.css` and `<guide>/src/` (with `SUMMARY.md`): an
//!   mdBook book per guide;
//! - `<guide>.md`: the guide as one Pandoc document with a title block, for EPUB and PDF;
//! - `home/`: the home book, from `docs/index.md` and `docs/guides/index.md`;
//! - `guides.tsv`: one line per guide, `name<TAB>slug<TAB>title`, for the build script.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use liquers_core::error::{Error, ErrorType};
use liquers_core::parse::parse_key;
use serde::Serialize;

use crate::chapter::{Chapter, ChapterMeta, ChapterStatus};
use crate::env::{build_environment, expand_chapter};
use crate::guide::{ChapterKind, GuideConfig};
use crate::markdown::{code_mask, heading, is_absolute_url, link_targets, split_front_matter};
use crate::markers::{apply_markers, notes, BuildMode, MarkerNote};
use crate::neutral::check_neutral;
use crate::{Finding, Severity};

/// Where the site is published; cross-guide links in EPUB and PDF point here.
pub const SITE_URL: &str = "https://orest-d.github.io/liquers";
/// The path of the site under its host, for mdBook's `site-url`.
pub const SITE_PATH: &str = "/liquers";
/// The repository, for mdBook's edit and repository links.
pub const REPOSITORY_URL: &str = "https://github.com/orest-d/liquers";

/// Files in a guide folder that are not chapters.
const NOT_CHAPTERS: [&str; 2] = ["HISTORY.md", "README.md"];

/// One processed chapter.
#[derive(Debug, Clone, Serialize)]
pub struct ChapterOutput {
    /// The file name, e.g. `05-commands.md`.
    pub file: String,
    pub meta: ChapterMeta,
    /// The first level-1 heading, e.g. `5. Commands`.
    pub heading: String,
    /// The expanded body after the marker rules: what the renderers get.
    #[serde(skip)]
    pub text: String,
    /// The markers of the source body.
    #[serde(skip)]
    pub notes: Vec<MarkerNote>,
}

/// One processed guide.
#[derive(Debug, Clone, Serialize)]
pub struct GuideOutput {
    /// The folder name: `rust`, `python` or `web`.
    pub name: String,
    pub config: GuideConfig,
    pub chapters: Vec<ChapterOutput>,
    /// Other files of the guide folder (images), copied into the book as they are.
    #[serde(skip)]
    pub assets: Vec<PathBuf>,
}

/// Everything the build writes.
#[derive(Debug, Clone)]
pub struct BuildTree {
    pub mode: BuildMode,
    pub guides: Vec<GuideOutput>,
    /// The home page (`docs/index.md`) and the guides index (`docs/guides/index.md`).
    pub home: Vec<(String, String)>,
    /// `docs/theme/liquers.css`, when present.
    pub theme: Option<String>,
}

fn io_error(path: &Path, e: std::io::Error) -> Error {
    Error::from_error(ErrorType::General, format!("{}: {e}", path.display()))
}

fn finding(file: &str, line: usize, severity: Severity, message: String) -> Finding {
    Finding {
        file: file.to_string(),
        line,
        severity,
        message,
    }
}

/// The guides under `docs/guides/`: folders with a `guide.yaml`, by name.
pub fn discover_guides(repo_root: &Path) -> Result<Vec<String>, Error> {
    let dir = repo_root.join("docs/guides");
    let mut names = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| io_error(&dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io_error(&dir, e))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with('_') && entry.path().join("guide.yaml").is_file() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// A file name as a URL-safe slug: `Liquers for Python` → `liquers-for-python`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Normalises `base/relative` without touching the file system; `None` when it climbs above the
/// root.
fn join_normalised(base: &Path, relative: &str) -> Option<PathBuf> {
    let mut parts: Vec<String> = Vec::new();
    for component in base.join(relative).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    Some(parts.iter().collect())
}

/// Checks a chapter's links: none into `specs/`, and every relative one resolving inside
/// `docs/`. `file` is the chapter's path relative to the repository root.
fn check_links(repo_root: &Path, file: &str, body: &str, offset: usize) -> Vec<Finding> {
    let mut findings = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mask = code_mask(&lines);
    let dir = Path::new(file).parent().unwrap_or(Path::new(""));
    for (i, line) in lines.iter().enumerate() {
        if mask[i] {
            continue;
        }
        for target in link_targets(line) {
            if is_absolute_url(&target) || target.starts_with('#') {
                continue;
            }
            let path = target.split('#').next().unwrap_or("");
            let resolved = join_normalised(dir, path);
            let line_no = offset + i + 1;
            match resolved {
                Some(resolved) if resolved.starts_with("specs") => findings.push(finding(
                    file,
                    line_no,
                    Severity::Error,
                    format!("link '{target}' points into specs/, which is not published; include the text or link the published page"),
                )),
                Some(resolved) if !resolved.starts_with("docs") => findings.push(finding(
                    file,
                    line_no,
                    Severity::Error,
                    format!("link '{target}' leaves docs/"),
                )),
                Some(resolved) => {
                    if !repo_root.join(&resolved).exists() {
                        findings.push(finding(
                            file,
                            line_no,
                            Severity::Error,
                            format!("link '{target}' does not resolve"),
                        ));
                    }
                }
                None => findings.push(finding(
                    file,
                    line_no,
                    Severity::Error,
                    format!("link '{target}' climbs above the repository"),
                )),
            }
        }
    }
    findings
}

/// Whether the guide's `HISTORY.md` has a row naming this chapter and date.
fn history_has(history: &str, file: &str, date: &str) -> bool {
    history
        .lines()
        .any(|line| line.contains(date) && line.contains(file))
}

/// Reads, expands and checks every guide.
pub async fn process(repo_root: &Path, mode: BuildMode) -> Result<(BuildTree, Vec<Finding>), Error> {
    let env = build_environment(repo_root)?;
    let mut findings = Vec::new();
    let mut guides = Vec::new();
    for name in discover_guides(repo_root)? {
        let dir = repo_root.join("docs/guides").join(&name);
        let rel_dir = format!("docs/guides/{name}");
        let config = GuideConfig::load(&dir)?;
        let history = std::fs::read_to_string(dir.join("HISTORY.md")).unwrap_or_default();
        let listed: BTreeSet<&str> = config.chapters.iter().map(String::as_str).collect();
        let mut assets = Vec::new();
        let entries = std::fs::read_dir(&dir).map_err(|e| io_error(&dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| io_error(&dir, e))?;
            let file = entry.file_name().to_string_lossy().to_string();
            if file.ends_with(".md") {
                if !listed.contains(file.as_str()) && !NOT_CHAPTERS.contains(&file.as_str()) {
                    findings.push(finding(
                        &format!("{rel_dir}/{file}"),
                        0,
                        Severity::Warning,
                        "not listed in guide.yaml, so not published".to_string(),
                    ));
                }
            } else if file != "guide.yaml" && entry.path().is_file() {
                assets.push(entry.path());
            }
        }
        assets.sort();

        let mut chapters = Vec::new();
        for file in &config.chapters {
            let rel = format!("{rel_dir}/{file}");
            let path = repo_root.join(&rel);
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(_) => {
                    findings.push(finding(
                        &rel,
                        0,
                        Severity::Error,
                        format!("listed in {rel_dir}/guide.yaml but missing"),
                    ));
                    continue;
                }
            };
            let chapter = match Chapter::parse(&rel, &raw) {
                Ok(chapter) => chapter,
                Err(e) => {
                    findings.push(finding(&rel, 1, Severity::Error, e.to_string()));
                    continue;
                }
            };
            let offset = chapter.body_offset;
            let shift = |mut f: Finding| {
                f.file = rel.clone();
                f.line += offset;
                f
            };
            findings.extend(check_neutral(&chapter.body).into_iter().map(shift));
            let (_, marker_findings) = apply_markers(&chapter.body, chapter.meta.status, mode);
            findings.extend(marker_findings.into_iter().map(shift));
            findings.extend(check_links(repo_root, &rel, &chapter.body, offset));
            if let Some(date) = &chapter.meta.reviewed {
                if !history_has(&history, file, date) {
                    findings.push(finding(
                        &rel,
                        0,
                        Severity::Warning,
                        format!("reviewed: {date} has no row in {rel_dir}/HISTORY.md"),
                    ));
                }
            }

            let key = parse_key(&rel)?;
            let expanded = match expand_chapter(&env, &key).await {
                Ok(text) => split_front_matter(&text).1.to_string(),
                Err(e) => {
                    findings.push(finding(
                        &rel,
                        0,
                        Severity::Error,
                        format!("template expansion failed: {e}"),
                    ));
                    chapter.body.clone()
                }
            };
            let (text, _) = apply_markers(&expanded, chapter.meta.status, mode);
            chapters.push(ChapterOutput {
                file: file.clone(),
                heading: chapter.heading(),
                notes: notes(&chapter.body),
                meta: chapter.meta,
                text,
            });
        }
        guides.push(GuideOutput {
            name,
            config,
            chapters,
            assets,
        });
    }

    let mut home = Vec::new();
    for (source, target) in [("docs/index.md", "index.md"), ("docs/guides/index.md", "guides/index.md")] {
        let path = repo_root.join(source);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                findings.extend(check_neutral(&text).into_iter().map(|mut f| {
                    f.file = source.to_string();
                    f
                }));
                home.push((target.to_string(), text));
            }
            Err(_) => findings.push(finding(source, 0, Severity::Error, "missing".to_string())),
        }
    }
    let theme = std::fs::read_to_string(repo_root.join("docs/theme/liquers.css")).ok();
    Ok((
        BuildTree {
            mode,
            guides,
            home,
            theme,
        },
        findings,
    ))
}

/// The title shown in the table of contents.
fn toc_title(chapter: &ChapterOutput, mode: BuildMode) -> String {
    let title = match ChapterKind::appendix_letter(&chapter.file) {
        Some(letter) => format!("{letter}. {}", chapter.meta.title),
        None => chapter.meta.title.clone(),
    };
    if mode == BuildMode::Release && chapter.meta.status != ChapterStatus::Ready {
        format!("{title} (in preparation)")
    } else {
        title
    }
}

/// `SUMMARY.md`: the preface as a prefix chapter, numbered chapters, then the appendices as
/// unnumbered suffix chapters after a separator.
pub fn summary(guide: &GuideOutput, mode: BuildMode) -> String {
    let mut out = String::from("# Summary\n\n");
    let mut separated = false;
    for chapter in &guide.chapters {
        let title = toc_title(chapter, mode);
        match ChapterKind::of(&chapter.file) {
            ChapterKind::Preface => out.push_str(&format!("[{title}]({})\n\n", chapter.file)),
            ChapterKind::Numbered => out.push_str(&format!("- [{title}]({})\n", chapter.file)),
            ChapterKind::Appendix => {
                if !separated {
                    out.push_str("\n---\n\n");
                    separated = true;
                }
                out.push_str(&format!("[{title}]({})\n", chapter.file));
            }
        }
    }
    out
}

/// A guide's `book.toml`.
pub fn book_toml(title: &str, site_path: &str, search: bool) -> String {
    format!(
        "# Generated by liquers-docs; edit docs/guides/<guide>/guide.yaml instead.\n\
         [book]\ntitle = {title:?}\nsrc = \"src\"\nlanguage = \"en\"\n\n\
         [output.html]\nsite-url = {site_path:?}\ngit-repository-url = {REPOSITORY_URL:?}\n\
         additional-css = [\"theme/liquers.css\"]\n\n\
         [output.html.search]\nenable = {search}\n"
    )
}

/// Rewrites the links of a chapter for the single-document Pandoc file: a chapter of the same
/// guide becomes an anchor, a chapter of another guide a link to the published site.
fn pandoc_links(text: &str, guide: &GuideOutput) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mask = code_mask(&lines);
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let mut line = line.to_string();
        if !mask[i] {
            for target in link_targets(&line) {
                if is_absolute_url(&target) || target.starts_with('#') {
                    continue;
                }
                let (path, fragment) = match target.split_once('#') {
                    Some((p, f)) => (p, Some(f)),
                    None => (target.as_str(), None),
                };
                let replacement = if guide.chapters.iter().any(|c| c.file == path) {
                    match fragment {
                        Some(f) => format!("#{f}"),
                        None => format!("#{}", path.trim_end_matches(".md")),
                    }
                } else if let Some(rest) = path.strip_prefix("../") {
                    let html = rest.trim_end_matches(".md");
                    let mut url = format!("{SITE_URL}/guides/{html}.html");
                    if let Some(f) = fragment {
                        url.push('#');
                        url.push_str(f);
                    }
                    url
                } else {
                    continue;
                };
                line = line.replace(&format!("]({target}"), &format!("]({replacement}"));
            }
        }
        out.push(line);
    }
    out.join("\n")
}

/// The guide as one Pandoc document: a YAML title block, then the chapters in order, each first
/// heading given its file name as identifier so links between chapters resolve.
pub fn pandoc_document(guide: &GuideOutput) -> String {
    let title_block = serde_yaml::to_string(&serde_json::json!({
        "title": guide.config.title,
        "author": guide.config.authors,
        "lang": "en",
    }))
    .unwrap_or_default();
    let mut out = format!("---\n{title_block}---\n\n");
    for chapter in &guide.chapters {
        let id = chapter.file.trim_end_matches(".md");
        let mut tagged = false;
        let text = pandoc_links(&chapter.text, guide);
        for line in text.lines() {
            if !tagged {
                if let Some((1, _)) = heading(line) {
                    out.push_str(&format!("{} {{#{id}}}\n", line.trim_end()));
                    tagged = true;
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

fn write(path: &Path, text: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    }
    std::fs::write(path, text).map_err(|e| io_error(path, e))
}

fn remove(path: &Path) -> Result<(), Error> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| io_error(path, e))
    } else if path.is_file() {
        std::fs::remove_file(path).map_err(|e| io_error(path, e))
    } else {
        Ok(())
    }
}

/// Writes the build tree under `out`, replacing what an earlier build wrote there.
pub fn write_tree(tree: &BuildTree, out: &Path) -> Result<(), Error> {
    let theme = tree.theme.clone().unwrap_or_default();
    let mut index = String::new();
    for guide in &tree.guides {
        let book = out.join(&guide.name);
        remove(&book)?;
        remove(&out.join(format!("{}.md", guide.name)))?;
        let site_path = format!("{SITE_PATH}/guides/{}/", guide.name);
        write(&book.join("book.toml"), &book_toml(&guide.config.title, &site_path, true))?;
        write(&book.join("theme/liquers.css"), &theme)?;
        write(&book.join("src/SUMMARY.md"), &summary(guide, tree.mode))?;
        for chapter in &guide.chapters {
            write(&book.join("src").join(&chapter.file), &chapter.text)?;
        }
        for asset in &guide.assets {
            if let Some(name) = asset.file_name() {
                let target = book.join("src").join(name);
                std::fs::copy(asset, &target).map_err(|e| io_error(&target, e))?;
            }
        }
        write(&out.join(format!("{}.md", guide.name)), &pandoc_document(guide))?;
        index.push_str(&format!(
            "{}\t{}\t{}\n",
            guide.name,
            slug(&guide.config.title),
            guide.config.title
        ));
    }
    let home = out.join("home");
    remove(&home)?;
    write(&home.join("book.toml"), &book_toml("Liquers", &format!("{SITE_PATH}/"), true))?;
    write(&home.join("theme/liquers.css"), &theme)?;
    let mut summary = String::from("# Summary\n\n[Liquers](index.md)\n");
    for (file, text) in &tree.home {
        write(&home.join("src").join(file), text)?;
        if file != "index.md" {
            summary.push_str(&format!("[User guides]({file})\n"));
        }
    }
    write(&home.join("src/SUMMARY.md"), &summary)?;
    write(&out.join("guides.tsv"), &index)?;
    Ok(())
}

/// Checks everything and writes nothing; the findings, errors first.
pub async fn check(repo_root: &Path, mode: BuildMode) -> Result<Vec<Finding>, Error> {
    let (_, mut findings) = process(repo_root, mode).await?;
    sort_findings(&mut findings);
    Ok(findings)
}

/// Checks everything and writes the build tree; the findings, errors first. The tree is written
/// even when there are errors, so a draft can be looked at; the caller fails on them.
pub async fn build(repo_root: &Path, out: &Path, mode: BuildMode) -> Result<Vec<Finding>, Error> {
    let (tree, mut findings) = process(repo_root, mode).await?;
    write_tree(&tree, out)?;
    sort_findings(&mut findings);
    Ok(findings)
}

fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        let rank = |s: Severity| match s {
            Severity::Error => 0,
            Severity::Warning => 1,
        };
        (rank(a.severity), &a.file, a.line).cmp(&(rank(b.severity), &b.file, b.line))
    });
}

/// One chapter in the report.
#[derive(Debug, Clone, Serialize)]
pub struct ChapterReport {
    pub guide: String,
    pub file: String,
    pub title: String,
    pub status: ChapterStatus,
    pub sources: Vec<String>,
    pub reviewed: Option<String>,
    /// Marker counts, by marker word.
    pub markers: std::collections::BTreeMap<String, usize>,
}

/// The editors' report: every chapter's status, sources and open markers.
pub async fn report(repo_root: &Path) -> Result<Vec<ChapterReport>, Error> {
    let (tree, _) = process(repo_root, BuildMode::Draft).await?;
    let mut rows = Vec::new();
    for guide in &tree.guides {
        for chapter in &guide.chapters {
            let mut markers = std::collections::BTreeMap::new();
            for note in &chapter.notes {
                *markers.entry(note.marker.word().to_string()).or_insert(0) += 1;
            }
            rows.push(ChapterReport {
                guide: guide.name.clone(),
                file: chapter.file.clone(),
                title: chapter.meta.title.clone(),
                status: chapter.meta.status,
                sources: chapter.meta.sources.clone(),
                reviewed: chapter.meta.reviewed.clone(),
                markers,
            });
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(file: &str, title: &str, status: ChapterStatus, text: &str) -> ChapterOutput {
        ChapterOutput {
            file: file.to_string(),
            meta: ChapterMeta {
                title: title.to_string(),
                status,
                sources: vec![],
                reviewed: None,
            },
            heading: title.to_string(),
            text: text.to_string(),
            notes: vec![],
        }
    }

    fn guide() -> GuideOutput {
        GuideOutput {
            name: "python".to_string(),
            config: GuideConfig {
                title: "Liquers for Python".to_string(),
                language: "python".to_string(),
                chapters: vec![],
                authors: vec![],
            },
            chapters: vec![
                chapter("index.md", "Preface", ChapterStatus::Ready, "# Preface\n"),
                chapter(
                    "01-introduction.md",
                    "Introduction",
                    ChapterStatus::Ready,
                    "# 1. Introduction\n\nSee [commands](05-commands.md#defaults) and [Rust](../rust/05-commands.md).\n",
                ),
                chapter("05-commands.md", "Commands", ChapterStatus::Draft, "# 5. Commands\n"),
                chapter("a-query-syntax.md", "Query syntax", ChapterStatus::Draft, "# A. Query syntax\n"),
            ],
            assets: vec![],
        }
    }

    #[test]
    fn summary_follows_the_manifest() {
        let summary = summary(&guide(), BuildMode::Release);
        assert_eq!(
            summary,
            "# Summary\n\n[Preface](index.md)\n\n- [Introduction](01-introduction.md)\n\
             - [Commands (in preparation)](05-commands.md)\n\n---\n\n\
             [A. Query syntax (in preparation)](a-query-syntax.md)\n"
        );
    }

    #[test]
    fn pandoc_document_links_chapters() {
        let doc = pandoc_document(&guide());
        assert!(doc.starts_with("---\n"));
        assert!(doc.contains("title: Liquers for Python"));
        assert!(doc.contains("# 1. Introduction {#01-introduction}"));
        assert!(doc.contains("[commands](#defaults)"));
        assert!(doc.contains(&format!("[Rust]({SITE_URL}/guides/rust/05-commands.html)")));
    }

    #[test]
    fn slugs_and_paths() {
        assert_eq!(slug("Liquers for the Web"), "liquers-for-the-web");
        assert_eq!(
            join_normalised(Path::new("docs/guides/python"), "../../../specs/x.md"),
            Some(PathBuf::from("specs/x.md"))
        );
        assert_eq!(join_normalised(Path::new("docs"), "../../x"), None);
        assert!(book_toml("T", "/liquers/", false).contains("enable = false"));
    }
}
