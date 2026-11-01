//! A chapter's front matter and status.

use liquers_core::error::Error;
use liquers_core::validate::registry::from_json_or_yaml;
use serde::{Deserialize, Serialize};

use crate::markdown::split_front_matter;

/// A chapter's front matter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterMeta {
    /// The chapter title without its number, as the table of contents shows it.
    pub title: String,
    /// How far the chapter has got; a release build publishes only `ready` chapters.
    pub status: ChapterStatus,
    /// The `specs/` documents (relative to `specs/`) the chapter is written from.
    #[serde(default)]
    pub sources: Vec<String>,
    /// The date of the last review against `sources`, with a row in the guide's `HISTORY.md`.
    #[serde(default)]
    pub reviewed: Option<String>,
}

/// A chapter's status (`guide-outlines.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChapterStatus {
    /// Skeleton only.
    Outline,
    /// Text written; markers allowed.
    Draft,
    /// Waiting for a person.
    Review,
    /// Published in release builds; no `TBD:` or `CHECK:`.
    Ready,
}

impl ChapterStatus {
    /// The lower-case name used in front matter and reports.
    pub fn name(self) -> &'static str {
        match self {
            ChapterStatus::Outline => "outline",
            ChapterStatus::Draft => "draft",
            ChapterStatus::Review => "review",
            ChapterStatus::Ready => "ready",
        }
    }
}

/// A chapter split into front matter and body.
#[derive(Debug, Clone)]
pub struct Chapter {
    pub meta: ChapterMeta,
    pub body: String,
    /// Lines before the body, to turn body line numbers into file line numbers.
    pub body_offset: usize,
}

impl Chapter {
    /// Parses a chapter; `source` names it in errors. Front matter is required.
    pub fn parse(source: &str, text: &str) -> Result<Self, Error> {
        let (yaml, body, body_offset) = split_front_matter(text);
        let yaml = yaml.ok_or_else(|| {
            Error::general_error(format!(
                "{source}: a chapter must start with front matter (---)"
            ))
        })?;
        let meta: ChapterMeta = from_json_or_yaml(source, yaml)?;
        Ok(Chapter {
            meta,
            body: body.to_string(),
            body_offset,
        })
    }

    /// The text of the first level-1 heading, or the front-matter title.
    pub fn heading(&self) -> String {
        for line in self.body.lines() {
            if let Some((1, text)) = crate::markdown::heading(line) {
                return text;
            }
        }
        self.meta.title.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_front_matter() {
        let chapter = Chapter::parse(
            "05-commands.md",
            "---\ntitle: Commands\nstatus: outline\nsources: [guides/PYTHON_GUIDE.md]\nreviewed:\n---\n# 5. Commands\n",
        )
        .expect("parses");
        assert_eq!(chapter.meta.status, ChapterStatus::Outline);
        assert_eq!(chapter.meta.reviewed, None);
        assert_eq!(chapter.heading(), "5. Commands");
        assert_eq!(chapter.body_offset, 6);
        assert!(Chapter::parse("x.md", "# no front matter\n").is_err());
        assert!(Chapter::parse("x.md", "---\ntitle: X\nstatus: finished\n---\n").is_err());
    }
}
