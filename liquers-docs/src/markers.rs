//! Editorial markers (`guide-outlines.md` §4): lines that start with `TBD:`, `PURPOSE:`,
//! `SOURCE:`, `EDITOR:` or `CHECK:` are notes to writers, not reader text.
//!
//! A note is the marker line plus the lines that continue it, up to a blank line, a heading or
//! the next marker. Markers inside fenced code are text.

use crate::chapter::ChapterStatus;
use crate::markdown::{code_mask, heading};
use crate::{Finding, Severity};

/// An editorial marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// Content still to be written.
    Tbd,
    /// What the chapter enables.
    Purpose,
    /// Where to draft from.
    Source,
    /// A note to editors.
    Editor,
    /// A claim to verify before release.
    Check,
}

impl Marker {
    /// Every marker, in report order.
    pub const ALL: [Marker; 5] = [
        Marker::Tbd,
        Marker::Purpose,
        Marker::Source,
        Marker::Editor,
        Marker::Check,
    ];

    /// The word as written, without the colon.
    pub fn word(self) -> &'static str {
        match self {
            Marker::Tbd => "TBD",
            Marker::Purpose => "PURPOSE",
            Marker::Source => "SOURCE",
            Marker::Editor => "EDITOR",
            Marker::Check => "CHECK",
        }
    }

    /// Whether the marker blocks a `ready` chapter (`TBD`, `CHECK`); the others are stripped.
    pub fn blocks_release(self) -> bool {
        match self {
            Marker::Tbd | Marker::Check => true,
            Marker::Purpose | Marker::Source | Marker::Editor => false,
        }
    }

    /// The marker a line starts with, and the rest of the line.
    pub fn parse(line: &str) -> Option<(Marker, &str)> {
        let trimmed = line.trim_start();
        for marker in Marker::ALL {
            if let Some(rest) = trimmed.strip_prefix(marker.word()) {
                if let Some(rest) = rest.strip_prefix(':') {
                    return Some((marker, rest.trim_start()));
                }
            }
        }
        None
    }
}

/// Draft or release build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    /// Everything is published; markers are shown as quoted notes.
    Draft,
    /// Only `ready` chapters are published; writer notes are removed.
    Release,
}

/// One marker found in a chapter body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerNote {
    pub marker: Marker,
    /// 1-based line of the marker in the body.
    pub line: usize,
    /// The note's text, continuation lines joined by spaces.
    pub text: String,
}

/// Lists the notes of a body.
pub fn notes(text: &str) -> Vec<MarkerNote> {
    let lines: Vec<&str> = text.lines().collect();
    let mask = code_mask(&lines);
    let mut notes: Vec<MarkerNote> = Vec::new();
    let mut open = false;
    for (i, line) in lines.iter().enumerate() {
        if mask[i] {
            open = false;
            continue;
        }
        if let Some((marker, rest)) = Marker::parse(line) {
            notes.push(MarkerNote {
                marker,
                line: i + 1,
                text: rest.to_string(),
            });
            open = true;
        } else if open && !line.trim().is_empty() && heading(line).is_none() {
            if let Some(note) = notes.last_mut() {
                note.text.push(' ');
                note.text.push_str(line.trim());
            }
        } else {
            open = false;
        }
    }
    notes
}

/// Applies the marker rules to a chapter body.
///
/// - Draft: every note becomes a quoted paragraph with its marker in bold. `TBD:` or `CHECK:` in
///   a `ready` chapter is a warning.
/// - Release: `PURPOSE:`, `SOURCE:` and `EDITOR:` notes are removed. `TBD:` or `CHECK:` in a
///   `ready` chapter is an error. A chapter that is not `ready` is replaced by its first heading
///   and "In preparation." (the caller's [`in_preparation`]).
///
/// Findings carry an empty `file` and body line numbers; the caller fills both in.
pub fn apply_markers(text: &str, status: ChapterStatus, mode: BuildMode) -> (String, Vec<Finding>) {
    let mut findings = Vec::new();
    if status == ChapterStatus::Ready {
        for note in notes(text) {
            if note.marker.blocks_release() {
                findings.push(Finding {
                    file: String::new(),
                    line: note.line,
                    severity: match mode {
                        BuildMode::Draft => Severity::Warning,
                        BuildMode::Release => Severity::Error,
                    },
                    message: format!(
                        "{}: in a ready chapter (a release build fails): {}",
                        note.marker.word(),
                        note.text
                    ),
                });
            }
        }
    }
    if mode == BuildMode::Release && status != ChapterStatus::Ready {
        return (in_preparation(text), findings);
    }

    let lines: Vec<&str> = text.lines().collect();
    let mask = code_mask(&lines);
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    // Some(true) while inside a note that is kept, Some(false) inside one that is dropped.
    let mut note: Option<bool> = None;
    for (i, line) in lines.iter().enumerate() {
        if mask[i] {
            note = None;
            out.push(line.to_string());
            continue;
        }
        if let Some((marker, rest)) = Marker::parse(line) {
            let keep = match mode {
                BuildMode::Draft => true,
                BuildMode::Release => marker.blocks_release(),
            };
            if keep {
                if note.is_some() || out.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                    out.push(String::new());
                }
                out.push(format!("> **{}:** {}", marker.word(), rest));
            }
            note = Some(keep);
            continue;
        }
        let continues = note.is_some() && !line.trim().is_empty() && heading(line).is_none();
        if continues {
            if note == Some(true) {
                out.push(format!("> {}", line.trim()));
            }
            continue;
        }
        if note.is_some() && !line.trim().is_empty() {
            // A heading right after a note: separate it from the quote.
            out.push(String::new());
        }
        note = None;
        out.push(line.to_string());
    }
    let mut result = out.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    (result, findings)
}

/// The "in preparation" stand-in for a chapter that is not `ready`: its first heading, then one
/// line.
pub fn in_preparation(text: &str) -> String {
    let title = text
        .lines()
        .find(|line| matches!(heading(line), Some((1, _))))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| "# Chapter".to_string());
    format!("{title}\n\n*This chapter is in preparation.*\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "# 5. Commands\n\nPURPOSE: The reader can write a command\nand call it.\n\nText.\n\nTBD: 5.1 A function becomes a command.\nSOURCE: guides/PYTHON_GUIDE.md §5\nCHECK: the decorator name.\n\n```\nTBD: inside code\n```\n";

    #[test]
    fn notes_are_found_outside_code() {
        let found = notes(BODY);
        let words: Vec<&str> = found.iter().map(|n| n.marker.word()).collect();
        assert_eq!(words, vec!["PURPOSE", "TBD", "SOURCE", "CHECK"]);
        assert_eq!(found[0].text, "The reader can write a command and call it.");
        assert_eq!(found[1].line, 8);
    }

    #[test]
    fn release_fails_on_open_markers_in_ready() {
        let (_, findings) = apply_markers(BODY, ChapterStatus::Ready, BuildMode::Release);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.severity == Severity::Error));
        assert_eq!(findings[0].line, 8);
        let (_, draft) = apply_markers(BODY, ChapterStatus::Ready, BuildMode::Draft);
        assert_eq!(draft.len(), 2);
        assert!(draft.iter().all(|f| f.severity == Severity::Warning));
        let (_, other) = apply_markers(BODY, ChapterStatus::Draft, BuildMode::Release);
        assert!(other.is_empty());
    }

    #[test]
    fn release_strips_writer_markers() {
        let (release, _) = apply_markers(BODY, ChapterStatus::Ready, BuildMode::Release);
        assert!(!release.contains("PURPOSE"));
        assert!(!release.contains("SOURCE"));
        assert!(!release.contains("and call it."));
        assert!(release.contains("> **TBD:** 5.1 A function becomes a command."));
        assert!(release.contains("TBD: inside code"));
        assert!(release.contains("Text."));

        let (draft, findings) = apply_markers(BODY, ChapterStatus::Draft, BuildMode::Draft);
        assert!(findings.is_empty());
        assert!(draft.contains("> **PURPOSE:** The reader can write a command\n> and call it."));
        assert!(draft.contains("> **TBD:** 5.1 A function becomes a command.\n\n> **SOURCE:**"));
        assert!(draft.contains("TBD: inside code"));
    }

    #[test]
    fn release_replaces_unready_chapter() {
        let (release, _) = apply_markers(BODY, ChapterStatus::Draft, BuildMode::Release);
        assert_eq!(release, "# 5. Commands\n\n*This chapter is in preparation.*\n");
        let (draft, _) = apply_markers(BODY, ChapterStatus::Outline, BuildMode::Draft);
        assert!(draft.contains("Text."));
    }
}
