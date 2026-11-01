//! Builds the Liquers user guides (`docs/guides/`): expands each chapter as a Liquers template,
//! checks it, and writes the sources mdBook and Pandoc render.
//!
//! # Main components
//!
//! - [`build::build`] and [`build::check`]: the whole pipeline, writing a build tree or only
//!   reporting [`Finding`]s.
//! - [`env::build_environment`] and [`env::expand_chapter`]: the Liquers environment, with
//!   `docs/` and `specs/` mounted read-only, and chapter expansion through `ns-template/expand`.
//! - [`markers::apply_markers`]: the editorial markers (`TBD:`, `PURPOSE:`, …) for draft and
//!   release builds.
//! - [`neutral::check_neutral`]: refuses renderer-specific syntax, so chapters stay tool-neutral.
//! - [`include::include_section`]: the `ns-docs/include` command, a `specs/` section by heading.
//!
//! # Where it is used
//!
//! - `scripts/build-docs.sh` runs `liquers-docs build`, then mdBook and Pandoc over the tree.
//! - `.github/workflows/docs-site.yml` runs `liquers-docs check` on pull requests and the
//!   release build on `main`.
//! - Editors run `liquers-docs check` and `report` locally
//!   (`specs/guides/USER_DOCS_EDITING_GUIDE.md`).
//!
//! # Related concepts
//!
//! - `ns-template` (`liquers-lib/src/template/`) does the template work; this crate only chooses
//!   what to expand and checks the result.
//! - `ns-mem/read` also reads a section by heading, but returns one heading's own text; an
//!   include needs the heading's whole subtree, which `ns-docs/include` returns.
//! - `scripts/docs_index.py` indexes the chapters with the `specs/` documents; this crate builds
//!   them.
//!
//! # Examples
//!
//! ```
//! use liquers_docs::chapter::ChapterStatus;
//! use liquers_docs::markers::{apply_markers, BuildMode};
//!
//! let body = "# 5. Commands\n\nPURPOSE: write a command.\n\nText.\n";
//! let (draft, _) = apply_markers(body, ChapterStatus::Draft, BuildMode::Draft);
//! assert!(draft.contains("> **PURPOSE:** write a command."));
//! let (release, _) = apply_markers(body, ChapterStatus::Draft, BuildMode::Release);
//! assert!(release.contains("in preparation"));
//! ```
//!
//! More: the tests in `liquers-docs/tests/`, and the design in
//! `specs/design/user-documentation-guides/`.

pub mod build;
pub mod chapter;
pub mod env;
pub mod guide;
pub mod include;
pub mod markdown;
pub mod markers;
pub mod neutral;

use serde::Serialize;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Reported; does not fail the build.
    Warning,
    /// Fails `check` and `build`.
    Error,
}

/// A problem found in a chapter or a guide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// The file, relative to the repository root.
    pub file: String,
    /// 1-based line in the file; 0 when the finding is about the whole file.
    pub line: usize,
    pub severity: Severity,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        if self.line > 0 {
            write!(f, "{severity}: {}:{}: {}", self.file, self.line, self.message)
        } else {
            write!(f, "{severity}: {}: {}", self.file, self.message)
        }
    }
}
