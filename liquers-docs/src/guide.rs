//! A guide's manifest, `docs/guides/<guide>/guide.yaml`.

use std::path::Path;

use liquers_core::error::Error;
use liquers_core::validate::registry::from_json_or_yaml;
use serde::{Deserialize, Serialize};

/// The manifest of one guide: its title, language and chapter order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuideConfig {
    /// The book title, e.g. "Liquers for Python".
    pub title: String,
    /// `rust`, `python` or `web`.
    pub language: String,
    /// Chapter file names in reading order. `index.md` is the preface; names starting with a
    /// letter and a dash (`a-query-syntax.md`) are appendices.
    pub chapters: Vec<String>,
    /// The authors named on the EPUB and PDF title page.
    #[serde(default)]
    pub authors: Vec<String>,
}

impl GuideConfig {
    /// Parses a manifest; `source` names it in errors.
    pub fn parse(source: &str, text: &str) -> Result<Self, Error> {
        from_json_or_yaml(source, text)
    }

    /// Reads `<dir>/guide.yaml`.
    pub fn load(dir: &Path) -> Result<Self, Error> {
        let path = dir.join("guide.yaml");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::general_error(format!("Cannot read {}: {e}", path.display())))?;
        Self::parse(&path.display().to_string(), &text)
    }
}

/// What a chapter file is, from its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterKind {
    /// `index.md`.
    Preface,
    /// `NN-name.md`.
    Numbered,
    /// `x-name.md`, a letter then a dash.
    Appendix,
}

impl ChapterKind {
    /// Classifies a chapter file name.
    pub fn of(file: &str) -> Self {
        let bytes = file.as_bytes();
        if file == "index.md" {
            ChapterKind::Preface
        } else if bytes.len() > 2 && bytes[0].is_ascii_lowercase() && bytes[1] == b'-' {
            ChapterKind::Appendix
        } else {
            ChapterKind::Numbered
        }
    }

    /// The appendix letter, upper case.
    pub fn appendix_letter(file: &str) -> Option<char> {
        match Self::of(file) {
            ChapterKind::Appendix => file.chars().next().map(|c| c.to_ascii_uppercase()),
            ChapterKind::Preface | ChapterKind::Numbered => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_and_kinds() {
        let config = GuideConfig::parse(
            "guide.yaml",
            "title: Liquers for Python\nlanguage: python\nchapters: [index.md, 01-introduction.md, a-query-syntax.md]\n",
        )
        .expect("parses");
        assert_eq!(config.chapters.len(), 3);
        assert!(config.authors.is_empty());
        assert_eq!(ChapterKind::of("index.md"), ChapterKind::Preface);
        assert_eq!(ChapterKind::of("01-introduction.md"), ChapterKind::Numbered);
        assert_eq!(ChapterKind::of("a-query-syntax.md"), ChapterKind::Appendix);
        assert_eq!(
            ChapterKind::appendix_letter("b-command-library.md"),
            Some('B')
        );
    }
}
