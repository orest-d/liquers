//! [`ValueDisplay`]: the value an element shows while a new one is evaluated.
//!
//! # Purpose
//!
//! A view that re-evaluates a query should neither flicker nor leave the person guessing. The
//! display keeps the value it shows until the new one is there: a result that arrives within the
//! quiet threshold (250 ms) simply replaces it; a slower one dims it and shows a status bar with
//! the status, progress and the latest log message; an error keeps it, dimmed, with the error in
//! the status bar. A value shown as a *preview* (evaluated from an edit that is not applied) is
//! framed in the theme's preview colour, and a value whose asset changed upstream is dimmed with a
//! note until it is recalculated (design `query-update-modes`).
//!
//! # Main components
//!
//! - [`ValueDisplay`]: the state machine. [`ValueDisplay::request`] starts waiting for a query
//!   and returns the generation its timers carry; [`ValueDisplay::on_snapshot`] takes the
//!   runner's [`AssetSnapshot`]s, ignoring a late one of a query the element itself asked for
//!   earlier, and taking one of any other query as the element being re-pointed from outside
//!   (an editor showing its preview, or its applied query, in the view it follows);
//!   [`ValueDisplay::on_timer`] takes the element's own timer events ([`DisplayTimer`]).
//! - [`DisplayPhase`] and [`SnapshotOutcome`]: where the display is, and what a snapshot did.
//! - [`StatusLine`]: what the status bar says, for both renderers.
//!
//! # Where it is used
//!
//! - [`crate::ui::widgets::QueryConsoleElement`] draws its value through it.
//! - The element schedules the timers with [`crate::ui::schedule_event`]; nothing here reads a
//!   clock, so it works the same on wasm32.
//!
//! # Related concepts
//!
//! - `ui::runner`: the snapshots, which name the query they answer (`AssetSnapshot::query`).
//! - `ui::editor::UpdateMode`: when an element asks for a new value.
//!
//! # Examples
//!
//! ```
//! # use liquers_core::metadata::{Metadata, Status};
//! # use liquers_lib::ui::{AssetSnapshot, DisplayTimer, SnapshotOutcome, ValueDisplay};
//! # use liquers_lib::value::Value;
//! # use std::sync::Arc;
//! #
//! # let snapshot = |query: &str, value: Option<&str>, status| AssetSnapshot {
//! #     value: value.map(|v| Arc::new(Value::from(v))),
//! #     metadata: Metadata::new(),
//! #     error: None,
//! #     status,
//! #     query: Some(query.to_string()),
//! #     preview: false,
//! # };
//! # let mut display = ValueDisplay::default();
//! # display.request("a", false);
//! # display.on_snapshot(&snapshot("a", Some("old"), Status::Ready));
//!
//! let generation = display.request("b", false);
//! assert_eq!(display.on_snapshot(&snapshot("b", None, Status::Processing)), SnapshotOutcome::Progress);
//! assert!(display.value().is_some(), "the old value stays");
//! display.on_timer(generation, DisplayTimer::Quiet);
//! assert!(display.is_dimmed() && display.status_line().is_some(), "slow: dimmed, with a status bar");
//! assert_eq!(display.on_snapshot(&snapshot("a", Some("late"), Status::Ready)), SnapshotOutcome::Ignored);
//! assert_eq!(display.on_snapshot(&snapshot("b", Some("new"), Status::Ready)), SnapshotOutcome::Settled);
//! assert!(!display.is_dimmed() && display.status_line().is_none());
//! ```
//!
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-lib/src/ui/widgets/value_display.rs):
//!   the fast, slow, error, out-of-order, slow-hint and preview cases.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, ProgressEntry, Status};

use crate::ui::element::UIElement;
use crate::ui::message::AssetSnapshot;
use crate::value::Value;

/// The quiet threshold: a value arriving sooner replaces the old one with no status bar (AC-3).
pub const QUIET_MS: u64 = 250;
/// After this long, the status bar offers to switch to On demand (AC-21).
pub const SLOW_MS: u64 = 3000;

/// Where a [`ValueDisplay`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayPhase {
    /// The shown value is the requested one (or nothing was requested).
    #[default]
    Settled,
    /// A new value is awaited, within the quiet threshold.
    Waiting,
    /// A new value is awaited, past the quiet threshold: dimmed, with a status bar.
    Slow,
    /// The requested evaluation failed; the previous value, if any, stays dimmed.
    Failed,
}

/// What a snapshot did to a [`ValueDisplay`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotOutcome {
    /// It answers a query the display no longer waits for.
    Ignored,
    /// The evaluation goes on: status and progress changed, the shown value did not.
    Progress,
    /// The requested value (or its error) is there.
    Settled,
}

/// The element's own timers, delivered back to it as `lq:timer` events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayTimer {
    /// The quiet threshold passed ([`QUIET_MS`]).
    Quiet,
    /// The evaluation is slow ([`SLOW_MS`]).
    Slow,
}

impl DisplayTimer {
    /// The name the `timer` field of an `lq:timer` event carries.
    pub fn name(self) -> &'static str {
        match self {
            DisplayTimer::Quiet => "quiet",
            DisplayTimer::Slow => "slow",
        }
    }

    /// The timer named `name`.
    pub fn from_name(name: &str) -> Option<DisplayTimer> {
        match name {
            "quiet" => Some(DisplayTimer::Quiet),
            "slow" => Some(DisplayTimer::Slow),
            _ => None,
        }
    }

    /// How long after a request the timer fires, in milliseconds.
    pub fn after_ms(self) -> u64 {
        match self {
            DisplayTimer::Quiet => QUIET_MS,
            DisplayTimer::Slow => SLOW_MS,
        }
    }
}

/// What the status bar says.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusLine {
    pub status: Status,
    /// The primary progress, when the evaluation reports one.
    pub progress: Option<ProgressEntry>,
    /// The latest log message.
    pub log: Option<String>,
    pub error: Option<String>,
    /// The shown value is a preview, not applied (AC-22).
    pub preview: bool,
    /// The asset changed upstream and the value waits for ✓ (AC-19).
    pub changed_upstream: bool,
    /// The evaluation is slow: offer to switch to On demand (AC-21).
    pub suggest_on_demand: bool,
}

/// The value an element shows while a new one is evaluated. See the module docs.
#[derive(Debug, Clone, Default)]
pub struct ValueDisplay {
    value: Option<Arc<Value>>,
    metadata: Option<Metadata>,
    error: Option<Error>,
    status: Status,
    ui_element: Option<Box<dyn UIElement>>,
    /// The query awaited, or last settled.
    requested: Option<String>,
    /// The request is a preview.
    requested_preview: bool,
    /// The shown value is a preview.
    preview: bool,
    phase: DisplayPhase,
    changed_upstream: bool,
    suggest_on_demand: bool,
    generation: u64,
    /// The element's own earlier requests, most recent last: a late snapshot of one is stale.
    superseded: Vec<(String, bool)>,
}

/// How many earlier requests a display remembers as stale.
const SUPERSEDED_LIMIT: usize = 8;

impl ValueDisplay {
    /// A new query is awaited; the shown value stays. Returns the generation its timers carry.
    pub fn request(&mut self, query: &str, preview: bool) -> u64 {
        if let Some(previous) = self.requested.take() {
            self.superseded.push((previous, self.requested_preview));
            if self.superseded.len() > SUPERSEDED_LIMIT {
                self.superseded.remove(0);
            }
        }
        self.superseded.retain(|(q, p)| !(q == query && *p == preview));
        self.generation += 1;
        self.requested = Some(query.to_string());
        self.requested_preview = preview;
        self.phase = DisplayPhase::Waiting;
        self.changed_upstream = false;
        self.suggest_on_demand = false;
        self.generation
    }

    /// The current generation: the one the latest request's timers carry.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The query awaited or last settled.
    pub fn requested(&self) -> Option<&str> {
        self.requested.as_deref()
    }

    /// Whether the query awaited or last settled is a preview.
    pub fn requested_preview(&self) -> bool {
        self.requested_preview
    }

    /// Takes a snapshot pushed by the runner. A late snapshot of one of the element's own
    /// earlier requests is ignored (AC-6). A snapshot of any other query means the element was
    /// re-pointed from outside (the runner monitors one query per element, the latest asked
    /// for): the display adopts it as the query it now waits for. One without a query (from an
    /// older sender) is taken.
    pub fn on_snapshot(&mut self, snapshot: &AssetSnapshot) -> SnapshotOutcome {
        if let Some(theirs) = &snapshot.query {
            let ours = self.requested.as_deref() == Some(theirs.as_str())
                && snapshot.preview == self.requested_preview;
            if !ours {
                if self.superseded.iter().any(|(q, p)| q == theirs && *p == snapshot.preview) {
                    return SnapshotOutcome::Ignored;
                }
                // Re-pointed from outside: its timers are not ours, and our earlier requests
                // are no longer the ones that could arrive late.
                self.superseded.clear();
                self.requested = Some(theirs.clone());
                self.requested_preview = snapshot.preview;
                self.generation += 1;
                self.phase = DisplayPhase::Waiting;
                self.changed_upstream = false;
                self.suggest_on_demand = false;
            }
        }
        self.metadata = Some(snapshot.metadata.clone());
        self.status = snapshot.status;
        if let Some(error) = &snapshot.error {
            self.error = Some(error.clone());
            self.phase = DisplayPhase::Failed;
            return SnapshotOutcome::Settled;
        }
        match &snapshot.value {
            Some(value) => {
                use crate::value::ExtValueInterface;
                self.value = Some(value.clone());
                self.ui_element = value.as_ui_element().ok().map(|e| e.clone_boxed());
                self.error = None;
                self.preview = snapshot.preview;
                self.phase = DisplayPhase::Settled;
                self.changed_upstream = false;
                self.suggest_on_demand = false;
                SnapshotOutcome::Settled
            }
            None => SnapshotOutcome::Progress,
        }
    }

    /// Takes one of the element's timers; one of an older generation does nothing.
    pub fn on_timer(&mut self, generation: u64, timer: DisplayTimer) {
        if generation != self.generation {
            return;
        }
        match (timer, self.phase) {
            (DisplayTimer::Quiet, DisplayPhase::Waiting) => self.phase = DisplayPhase::Slow,
            (DisplayTimer::Slow, DisplayPhase::Waiting) | (DisplayTimer::Slow, DisplayPhase::Slow) => {
                self.phase = DisplayPhase::Slow;
                self.suggest_on_demand = true;
            }
            (DisplayTimer::Quiet, DisplayPhase::Settled)
            | (DisplayTimer::Quiet, DisplayPhase::Slow)
            | (DisplayTimer::Quiet, DisplayPhase::Failed)
            | (DisplayTimer::Slow, DisplayPhase::Settled)
            | (DisplayTimer::Slow, DisplayPhase::Failed) => {}
        }
    }

    /// The asset of the shown value changed upstream and is not recalculated yet (AC-19).
    pub fn mark_changed_upstream(&mut self) {
        self.changed_upstream = true;
    }

    /// Shows `value` at once, as the settled value of `query` (a value the element computed
    /// itself, or a test).
    pub fn set_value(&mut self, query: &str, value: Option<Arc<Value>>) {
        use crate::value::ExtValueInterface;
        self.requested = Some(query.to_string());
        self.ui_element = value.as_ref().and_then(|v| v.as_ui_element().ok()).map(|e| e.clone_boxed());
        self.value = value;
        self.phase = DisplayPhase::Settled;
    }

    /// Forgets the value and everything about it.
    pub fn clear(&mut self) {
        let generation = self.generation;
        *self = ValueDisplay::default();
        self.generation = generation;
    }

    pub fn phase(&self) -> DisplayPhase {
        self.phase
    }
    pub fn value(&self) -> Option<&Arc<Value>> {
        self.value.as_ref()
    }
    pub fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }
    pub fn status(&self) -> Status {
        self.status
    }
    /// The value as a UI element, when it is one.
    pub fn ui_element_mut(&mut self) -> Option<&mut Box<dyn UIElement>> {
        self.ui_element.as_mut()
    }
    /// The shown value is a preview (AC-22).
    pub fn is_preview(&self) -> bool {
        self.preview
    }
    /// The asset changed upstream (AC-19).
    pub fn is_changed_upstream(&self) -> bool {
        self.changed_upstream
    }

    /// The shown value is stale: a slow evaluation, a failed one, or an upstream change (AC-4,
    /// AC-5, AC-19). A preview is framed instead; a slow preview is both.
    pub fn is_dimmed(&self) -> bool {
        let stale = match self.phase {
            DisplayPhase::Slow => true,
            DisplayPhase::Failed => self.value.is_some(),
            DisplayPhase::Settled | DisplayPhase::Waiting => false,
        };
        stale || self.changed_upstream
    }

    /// What the status bar says, or `None` when it is hidden: a settled value, or one awaited
    /// within the quiet threshold (AC-3), shows none.
    pub fn status_line(&self) -> Option<StatusLine> {
        let shown = match self.phase {
            DisplayPhase::Slow | DisplayPhase::Failed => true,
            DisplayPhase::Settled | DisplayPhase::Waiting => false,
        } || self.changed_upstream
            || self.preview;
        if !shown {
            return None;
        }
        let progress = self.metadata.as_ref().map(Metadata::primary_progress).filter(|p| !p.is_off());
        let log = self.metadata.as_ref().and_then(last_log);
        Some(StatusLine {
            status: self.status,
            progress,
            log,
            error: match self.phase {
                DisplayPhase::Failed => self.error.as_ref().map(|e| e.to_string()),
                DisplayPhase::Settled | DisplayPhase::Waiting | DisplayPhase::Slow => None,
            },
            preview: self.preview,
            changed_upstream: self.changed_upstream,
            suggest_on_demand: self.suggest_on_demand,
        })
    }
}

/// The latest log message of `metadata`.
fn last_log(metadata: &Metadata) -> Option<String> {
    match metadata {
        Metadata::MetadataRecord(record) => record.log.last().map(|entry| entry.message.clone()),
        Metadata::LegacyMetadata(_) => None,
    }
}

impl StatusLine {
    /// The status bar's text, for both renderers.
    pub fn text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(error) = &self.error {
            parts.push(format!("Error: {error}"));
        } else if self.preview && matches!(self.status, Status::Ready) {
            // Said once, beside the preview frame (AC-22).
        } else {
            parts.push(format!("{:?}", self.status));
        }
        if let Some(progress) = &self.progress {
            if progress.total > 0 {
                parts.push(format!("{}/{} {}", progress.done, progress.total, progress.message));
            } else if !progress.message.is_empty() {
                parts.push(progress.message.clone());
            }
        }
        if let Some(log) = &self.log {
            parts.push(log.clone());
        }
        if self.preview {
            parts.push("Preview of unapplied edits — ✓ applies, Revert or Undo discards".to_string());
        }
        if self.changed_upstream {
            parts.push("Changed upstream — ✓ recalculates".to_string());
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::metadata::{LogEntry, MetadataRecord};

    fn snapshot(query: &str, value: Option<&str>, status: Status) -> AssetSnapshot {
        AssetSnapshot {
            value: value.map(|v| Arc::new(Value::from(v))),
            metadata: Metadata::new(),
            error: None,
            status,
            query: Some(query.to_string()),
            preview: false,
        }
    }

    fn settled(query: &str, value: &str) -> ValueDisplay {
        let mut display = ValueDisplay::default();
        display.request(query, false);
        display.on_snapshot(&snapshot(query, Some(value), Status::Ready));
        display
    }

    fn shown(display: &ValueDisplay) -> String {
        use liquers_core::value::ValueInterface;
        display.value().map(|v| v.try_into_string().expect("text")).unwrap_or_default()
    }

    /// query-update-modes AC-3
    #[test]
    fn fast_result_replaces_without_an_empty_frame() {
        let mut display = settled("a", "old");
        display.request("b", false);
        for status in [Status::Submitted, Status::Dependencies, Status::Processing] {
            assert_eq!(display.on_snapshot(&snapshot("b", None, status)), SnapshotOutcome::Progress);
            assert_eq!(shown(&display), "old", "no empty frame while {status:?}");
            assert!(display.status_line().is_none() && !display.is_dimmed());
        }
        assert_eq!(display.on_snapshot(&snapshot("b", Some("new"), Status::Ready)), SnapshotOutcome::Settled);
        assert_eq!(shown(&display), "new");
        // The quiet timer of that request, arriving later, changes nothing.
        display.on_timer(display.generation(), DisplayTimer::Quiet);
        assert!(display.status_line().is_none());
    }

    /// query-update-modes AC-4
    #[test]
    fn slow_result_dims_and_shows_progress() {
        let mut display = settled("a", "old");
        let generation = display.request("b", false);
        let mut record = MetadataRecord::new();
        record.set_primary_progress(&ProgressEntry {
            message: "rows".to_string(),
            done: 3,
            total: 10,
            timestamp: String::new(),
            eta: None,
        });
        record.log.push(LogEntry::info("reading sales.csv".to_string()));
        let mut progress = snapshot("b", None, Status::Processing);
        progress.metadata = Metadata::MetadataRecord(record);
        display.on_snapshot(&progress);
        display.on_timer(generation, DisplayTimer::Quiet);
        assert!(display.is_dimmed());
        assert_eq!(shown(&display), "old");
        let line = display.status_line().expect("status bar");
        assert_eq!(line.status, Status::Processing);
        assert_eq!(line.progress.as_ref().map(|p| p.done), Some(3));
        assert_eq!(line.log.as_deref(), Some("reading sales.csv"));
        assert!(line.text().contains("3/10 rows") && line.text().contains("reading sales.csv"), "{}", line.text());
    }

    /// query-update-modes AC-5
    #[test]
    fn error_keeps_the_previous_value() {
        let mut display = settled("a", "old");
        display.request("b", false);
        let mut failed = snapshot("b", None, Status::Error);
        failed.error = Some(Error::general_error("no such file".to_string()));
        assert_eq!(display.on_snapshot(&failed), SnapshotOutcome::Settled);
        assert_eq!(shown(&display), "old");
        assert!(display.is_dimmed());
        let line = display.status_line().expect("status bar");
        assert!(line.text().contains("no such file"), "{}", line.text());
    }

    /// query-update-modes AC-6
    #[test]
    fn snapshot_of_another_query_is_ignored() {
        let mut display = settled("a", "old");
        display.request("b", false);
        assert_eq!(display.on_snapshot(&snapshot("a", Some("late a"), Status::Ready)), SnapshotOutcome::Ignored);
        assert_eq!(shown(&display), "old");
        assert_eq!(display.status(), Status::Ready, "the status of `a` did not leak either");
        // A preview of an earlier own request is stale too; a snapshot of a query never asked
        // for is the element re-pointed from outside, and is taken.
        let mut stale = snapshot("a", Some("preview a"), Status::Ready);
        stale.preview = true;
        display.request("a", true);
        display.request("b", false);
        assert_eq!(display.on_snapshot(&stale), SnapshotOutcome::Ignored);
        let mut preview = snapshot("b/-/edit", Some("preview"), Status::Ready);
        preview.preview = true;
        assert_eq!(display.on_snapshot(&preview), SnapshotOutcome::Settled);
        assert!(display.is_preview() && display.requested() == Some("b/-/edit"));
        // Asking again for a query once superseded makes it current, not stale.
        display.request("a", false);
        assert_eq!(display.on_snapshot(&snapshot("a", Some("again"), Status::Ready)), SnapshotOutcome::Settled);
        // A timer of the older request does nothing.
        display.request("c", false);
        display.on_timer(display.generation() - 1, DisplayTimer::Quiet);
        assert_eq!(display.phase(), DisplayPhase::Waiting);
    }

    /// query-update-modes AC-21
    #[test]
    fn slow_timer_offers_on_demand() {
        let mut display = settled("a", "old");
        let generation = display.request("b", false);
        display.on_timer(generation, DisplayTimer::Quiet);
        assert!(!display.status_line().expect("bar").suggest_on_demand);
        display.on_timer(generation, DisplayTimer::Slow);
        assert!(display.status_line().expect("bar").suggest_on_demand);
        // Settling drops the suggestion; the display never changes a mode itself.
        display.on_snapshot(&snapshot("b", Some("new"), Status::Ready));
        assert!(display.status_line().is_none());
    }

    /// query-update-modes AC-22
    #[test]
    fn preview_is_framed_and_stale_is_dimmed() {
        let mut display = settled("a", "applied");
        let generation = display.request("a/-/edit", true);
        display.on_timer(generation, DisplayTimer::Quiet);
        assert!(display.is_dimmed() && !display.is_preview(), "still the applied value, stale");
        let mut preview = snapshot("a/-/edit", Some("previewed"), Status::Ready);
        preview.preview = true;
        display.on_snapshot(&preview);
        assert!(display.is_preview() && !display.is_dimmed());
        let line = display.status_line().expect("a preview always says so");
        assert!(line.preview && line.text().contains("✓ applies, Revert or Undo discards"), "{}", line.text());
        // Changed upstream: dimmed, with its note.
        let mut display = settled("a", "applied");
        display.mark_changed_upstream();
        assert!(display.is_dimmed());
        assert!(display.status_line().expect("bar").text().contains("Changed upstream"));
    }
}
