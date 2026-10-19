//! The L0/L1 bridge: where an agent's two short tiers of a document come from.
//!
//! L0 and L1 serve agents finding a resource; `title` and `description` serve humans. By default
//! the bridge reads L0 from `title` and L1 from `description`, which the mount's store pipeline
//! fills (`front_matter`, then `markdown_tiers`). Changing what agents see is an edit of
//! `memory.yaml`, not of code.

use crate::search::md_split::{first_heading, first_paragraph, section_paragraph};

use super::front_matter::FrontMatterDoc;
use super::layout::{TierSource, TierSources};

/// Whether resolving `sources` needs the document's text, not only its metadata.
pub fn needs_text(sources: &TierSources) -> bool {
    sources.l0.iter().chain(sources.l1.iter()).any(|source| match source {
        TierSource::Meta(_) => false,
        TierSource::Field(_) | TierSource::Section(_) | TierSource::Heading | TierSource::Paragraph => {
            true
        }
    })
}

/// A document's `(l0, l1)`: for each tier, the first source that gives a non-empty text. `meta`
/// reads a field of the store's metadata record; `text` is the document, when it was read.
pub fn resolve(
    sources: &TierSources,
    meta: &dyn Fn(&str) -> Option<String>,
    text: Option<&str>,
) -> (String, String) {
    let doc = text.and_then(|text| FrontMatterDoc::parse(text).ok());
    let tier = |list: &[TierSource]| {
        list.iter()
            .find_map(|source| {
                let value = match source {
                    TierSource::Meta(name) => meta(name),
                    TierSource::Field(name) => doc.as_ref().map(|doc| doc.get_text(name)),
                    TierSource::Section(name) => text.and_then(|text| section_paragraph(text, name)),
                    TierSource::Heading => text.and_then(first_heading),
                    TierSource::Paragraph => text.and_then(first_paragraph),
                };
                value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
            })
            .unwrap_or_default()
    };
    (tier(&sources.l0), tier(&sources.l1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\nl0: Agent summary\n---\n# Heading\n\nFirst paragraph.\n\n## Problem\n\nThe problem.\n";

    fn meta(title: &'static str, description: &'static str) -> impl Fn(&str) -> Option<String> {
        move |name| match name {
            "title" => Some(title.to_string()),
            "description" => Some(description.to_string()),
            _ => None,
        }
    }

    #[test]
    fn default_tiers_read_title_and_description() {
        let sources = TierSources::default();
        assert!(!needs_text(&sources));
        let (l0, l1) = resolve(&sources, &meta("A title", "A description"), None);
        assert_eq!((l0.as_str(), l1.as_str()), ("A title", "A description"));
    }

    #[test]
    fn first_nonempty_source_wins() {
        let sources = TierSources {
            l0: vec![TierSource::Field("missing".into()), TierSource::Meta("title".into()), TierSource::Heading],
            l1: vec![TierSource::Field("l0".into()), TierSource::Paragraph],
        };
        assert!(needs_text(&sources));
        let (l0, l1) = resolve(&sources, &meta("", ""), Some(DOC));
        assert_eq!(l0, "Heading");
        assert_eq!(l1, "Agent summary");
    }

    #[test]
    fn section_source_takes_first_paragraph() {
        let sources = TierSources {
            l0: vec![TierSource::Section("Absent".into()), TierSource::Paragraph],
            l1: vec![TierSource::Section("Problem".into())],
        };
        let (l0, l1) = resolve(&sources, &meta("", ""), Some(DOC));
        assert_eq!(l0, "First paragraph.");
        assert_eq!(l1, "The problem.");
    }
}
