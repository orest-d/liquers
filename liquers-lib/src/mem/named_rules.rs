//! The named rules of `scripts/docs_index.py --check` that a schema cannot express: a design's
//! lifecycle, its readiness assessment, its document form and its acceptance scenarios
//! (`DOCS_STRUCTURE_GUIDE.md` §5). A collection opts in by listing them in `checks`.
//!
//! These are ports, kept close to the script so the two agree finding for finding; the
//! `liquers-agent` test `check_matches_docs_index_oracle` compares them.

use std::collections::{BTreeMap, BTreeSet};

use super::read::Entry;
use super::rules::{is_date, Violation};

const DESIGN_STATUS: [&str; 8] = [
    "draft",
    "in_review",
    "approved",
    "in_implementation",
    "implemented",
    "complete",
    "superseded",
    "abandoned",
];
const STATUS_NEEDING_PHASE: [&str; 5] =
    ["draft", "in_review", "approved", "in_implementation", "implemented"];
const TERMINAL_STATUS_WITH_PR: [&str; 4] = ["", "complete", "superseded", "abandoned"];
const PHASES: [&str; 5] = ["high-level", "architecture", "examples", "implementation", "documentation"];
const DESIGN_AUTOFIX: [&str; 2] = ["eligible", "not-eligible"];
const DESIGN_FORMS: [&str; 2] = ["full", "compact"];

/// The named rules this module implements.
pub const DESIGN_RULES: [&str; 4] = ["lifecycle", "readiness", "compact_form", "scenarios"];

fn phase_rank(phase: &str) -> usize {
    PHASES.iter().position(|p| *p == phase).map(|i| i + 1).unwrap_or(0)
}

/// `status` and `phase` agree, and a design with a PR keeps a derived or terminal status (§5.1,
/// §5.5).
pub(crate) fn lifecycle(entry: &Entry) -> Vec<Violation> {
    let mut out = Vec::new();
    let status = entry.field("status");
    let phase = entry.field("phase");
    let has_pr = entry
        .doc()
        .map(|d| !super::front_matter::value_items(d.get("gh_pr").as_ref()).is_empty())
        .unwrap_or(false);
    if has_pr {
        if !TERMINAL_STATUS_WITH_PR.contains(&status.as_str()) {
            out.push(Violation::error(
                "status",
                format!(
                    "has gh_pr and status '{status}' — with a PR linked, write nothing (derived) \
                     or a terminal status (§5.5)"
                ),
            ));
        }
    } else if !DESIGN_STATUS.contains(&status.as_str()) {
        out.push(Violation::error("status", format!("status '{status}' not in §5.1")));
    }
    if !status.is_empty() {
        let needs = STATUS_NEEDING_PHASE.contains(&status.as_str());
        if needs && phase.is_empty() {
            out.push(Violation::error("phase", format!("status '{status}' requires a phase (§5.1)")));
        }
        if !needs && !phase.is_empty() {
            out.push(Violation::error("phase", format!("status '{status}' must not carry a phase (§5.1)")));
        }
    }
    out
}

/// A design's `form` is known, and a compact design keeps its phases as `## Phase N` (§5).
pub(crate) fn compact_form(entry: &Entry) -> Vec<Violation> {
    let form = entry.field("form");
    if form.is_empty() {
        return Vec::new();
    }
    if !DESIGN_FORMS.contains(&form.as_str()) {
        return vec![Violation::error("compact_form", format!("form '{form}' not in §5"))];
    }
    let text = entry.text.as_deref().unwrap_or("");
    if form == "compact" && !text.lines().any(|l| heading_is(l, "## Phase 1")) {
        return vec![Violation::error(
            "compact_form",
            "a compact design keeps its phases as `## Phase N` (§5)".to_string(),
        )];
    }
    Vec::new()
}

/// `line` starts with `prefix` followed by a word boundary.
fn heading_is(line: &str, prefix: &str) -> bool {
    match line.strip_prefix(prefix) {
        Some(rest) => !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'),
        None => false,
    }
}

/// A design as the readiness rules see it.
pub(crate) struct DesignRow<'a> {
    pub rel: String,
    pub slug: String,
    pub entry: &'a Entry,
}

/// §5.1.1 readiness and autofix, over every design of a collection. `issue` finds a source by
/// id. Returns `(rel, violation)` pairs in the order the script reports them.
pub(crate) fn readiness(
    designs: &[DesignRow<'_>],
    issue: &dyn Fn(&str) -> Option<Entry>,
) -> Vec<(String, Violation)> {
    let mut out = Vec::new();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    let error = |rel: &str, message: String| (rel.to_string(), Violation::error("readiness", message));
    for design in designs {
        let Some(doc) = design.entry.doc() else { continue };
        if !doc.has_front_matter() {
            continue;
        }
        let rel = design.rel.as_str();
        let readiness = design.entry.field("readiness");
        if !readiness.is_empty() {
            let sources = super::front_matter::value_items(doc.get("issues").as_ref());
            let unique: BTreeSet<&String> = sources.iter().collect();
            if sources.is_empty() {
                out.push(error(
                    rel,
                    "readiness-labeled design must name at least one source issue or feature (§5.1.1)"
                        .to_string(),
                ));
            } else if unique.len() != sources.len() {
                out.push(error(rel, "a source is listed twice in issues (§5.1.1)".to_string()));
            } else if sources.len() > 1 && !is_date(&design.entry.field("merged")) {
                out.push(error(
                    rel,
                    "a readiness-labeled design with several sources must record the merge as \
                     `merged: YYYY-MM-DD` (§5.1.1)"
                        .to_string(),
                ));
            }
            let mut seen = BTreeSet::new();
            for source in &sources {
                if !seen.insert(source.clone()) {
                    continue;
                }
                match issue(source) {
                    None => out.push(error(
                        rel,
                        format!("source issue or feature '{source}' does not exist"),
                    )),
                    Some(found) => {
                        let linked = found.field("design");
                        if linked != design.slug {
                            out.push(error(
                                rel,
                                format!(
                                    "source '{source}' links to design '{linked}', expected '{}' (§5.1.1)",
                                    design.slug
                                ),
                            ));
                        }
                        match owner.get(source) {
                            Some(first) => out.push(error(
                                rel,
                                format!(
                                    "source '{source}' is already owned by readiness-labeled design '{first}' (§5.1.1)"
                                ),
                            )),
                            None => {
                                owner.insert(source.clone(), design.slug.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    let by_slug: BTreeMap<&str, &DesignRow<'_>> = designs.iter().map(|d| (d.slug.as_str(), d)).collect();
    for design in designs {
        let successor = design.entry.field("superseded_by");
        let Some(next) = by_slug.get(successor.as_str()) else { continue };
        if design.entry.field("status") != "superseded" {
            out.push(error(
                &design.rel,
                format!("superseded_by '{successor}' requires status 'superseded' (§5.1.1)"),
            ));
        }
        if !design.entry.field("readiness").is_empty() && !next.entry.field("merged").is_empty() {
            out.push(error(
                &design.rel,
                format!("merged into '{successor}', so it must drop readiness (§5.1.1)"),
            ));
        }
    }
    for design in designs {
        let autofix = design.entry.field("autofix");
        if autofix.is_empty() {
            continue;
        }
        let rel = design.rel.as_str();
        if !DESIGN_AUTOFIX.contains(&autofix.as_str()) {
            out.push(error(rel, format!("autofix '{autofix}' not in §5.1.1")));
            continue;
        }
        let readiness = design.entry.field("readiness");
        if readiness.is_empty() {
            out.push(error(rel, "autofix requires a readiness value (§5.1.1)".to_string()));
            continue;
        }
        if autofix != "eligible" {
            continue;
        }
        if readiness != "ready" {
            out.push(error(
                rel,
                format!("autofix 'eligible' requires readiness 'ready', not '{readiness}' (§5.1.1)"),
            ));
        }
        let sources = design
            .entry
            .doc()
            .map(|d| super::front_matter::value_items(d.get("issues").as_ref()))
            .unwrap_or_default();
        if let Some(leading) = sources.first().and_then(|s| issue(s)) {
            let complexity = leading.field("complexity");
            if complexity != "S" && complexity != "M" {
                out.push(error(
                    rel,
                    format!(
                        "autofix 'eligible' requires complexity S or M, but leading source '{}' is \
                         '{complexity}' (§5.1.1)",
                        sources[0]
                    ),
                ));
            }
        }
    }
    out
}

/// `AC-<n>` ids defined in Phase 1, each with the text of its list item, in order of definition
/// (an id defined twice appears twice).
fn scenario_items(phase1: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = phase1.lines().collect();
    let mut starts = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(id) = scenario_id(line) {
            starts.push((i, id));
        }
    }
    let mut out = Vec::new();
    for (n, (start, id)) in starts.iter().enumerate() {
        let end = starts.get(n + 1).map(|(i, _)| *i).unwrap_or(lines.len());
        let mut block = vec![lines[*start]];
        for line in &lines[start + 1..end] {
            if line.starts_with('#') {
                break;
            }
            block.push(line);
        }
        out.push((id.clone(), block.join("\n")));
    }
    out
}

/// The id of a scenario definition line: `- **AC-<n>**` (or `*`), after optional indentation.
fn scenario_id(line: &str) -> Option<String> {
    let rest = line.trim_start_matches([' ', '\t']);
    let rest = rest.strip_prefix('-').or_else(|| rest.strip_prefix('*'))?;
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim_start_matches([' ', '\t']).strip_prefix("**AC-")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || !rest[digits.len()..].starts_with("**") {
        return None;
    }
    Some(format!("AC-{digits}"))
}

fn has_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    text.match_indices(word).any(|(i, _)| {
        let before = i == 0 || !is_word(bytes[i - 1]);
        let after = i + word.len() >= bytes.len() || !is_word(bytes[i + word.len()]);
        before && after
    })
}

/// Every `AC-<n>` cited in `text`, as whole words.
fn cited_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = BTreeSet::new();
    for (i, _) in text.match_indices("AC-") {
        if i > 0 && is_word(bytes[i - 1]) {
            continue;
        }
        let digits: String = text[i + 3..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        let end = i + 3 + digits.len();
        if end < bytes.len() && is_word(bytes[end]) {
            continue;
        }
        out.insert(format!("AC-{digits}"));
    }
    out
}

/// One phase of a compact design: its `## Phase N` section of DESIGN.md.
pub(crate) fn compact_phase(text: &str, phase: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| heading_is(l, &format!("## Phase {phase}")))?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| {
            l.strip_prefix("## Phase ")
                .map(|r| r.starts_with(|c: char| c.is_ascii_digit()))
                .unwrap_or(false)
        })
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());
    Some(lines[start..end].join("\n"))
}

/// §5.2.1: each Phase 1 scenario has WHEN and THEN, and a written Phase 3 cites every one and no
/// other. `phase1` and `phase3` are the phases' texts, when they exist.
pub(crate) fn scenarios(entry: &Entry, phase1: Option<&str>, phase3: Option<&str>) -> Vec<Violation> {
    let mut out = Vec::new();
    let Some(phase1) = phase1 else { return out };
    let items = scenario_items(phase1);
    if items.is_empty() {
        return out;
    }
    let error = |message: String| Violation::error("scenarios", message);
    let mut seen = BTreeSet::new();
    let mut reported = BTreeSet::new();
    for (id, _) in &items {
        if !seen.insert(id.clone()) && reported.insert(id.clone()) {
            out.push(error(format!("scenario {id} is defined more than once (§5.2.1)")));
        }
    }
    // As the script's dict, the last definition of an id is the one checked.
    let mut defined: BTreeMap<String, String> = BTreeMap::new();
    for (id, block) in items {
        defined.insert(id, block);
    }
    for (id, block) in &defined {
        let missing: Vec<&str> = ["WHEN", "THEN"].into_iter().filter(|w| !has_word(block, w)).collect();
        if !missing.is_empty() {
            out.push(error(format!("scenario {id} has no {} (§5.2.1)", missing.join(" or "))));
        }
    }
    let Some(phase3) = phase3 else { return out };
    let phase = entry.field("phase");
    let status = entry.field("status");
    let written = if phase_rank(&phase) > phase_rank("examples") {
        true
    } else if phase == "examples" {
        status == "in_review" || status == "approved"
    } else {
        status == "complete"
    };
    if !written {
        return out;
    }
    let cited = cited_ids(phase3);
    for id in defined.keys().filter(|id| !cited.contains(*id)) {
        out.push(error(format!("scenario {id} has no Phase 3 test citing it (§5.2.1)")));
    }
    for id in cited.iter().filter(|id| !defined.contains_key(*id)) {
        out.push(error(format!("Phase 3 cites {id}, which Phase 1 does not define (§5.2.1)")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_lines_and_citations_are_recognised() {
        assert_eq!(scenario_id("- **AC-3** WHEN x"), Some("AC-3".to_string()));
        assert_eq!(scenario_id("  * **AC-12**: y"), Some("AC-12".to_string()));
        assert_eq!(scenario_id("-**AC-3**"), None);
        assert_eq!(scenario_id("- **AC-x**"), None);
        let cited = cited_ids("covers AC-1, AC-2; not XAC-3 or AC-4b");
        assert_eq!(cited.into_iter().collect::<Vec<_>>(), vec!["AC-1", "AC-2"]);
        assert!(has_word("x WHEN y", "WHEN"));
        assert!(!has_word("WHENEVER", "WHEN"));
    }

    #[test]
    fn compact_phase_ends_at_the_next_phase() {
        let text = "# D\n## Phase 1\na\n### sub\n## Phase 2\nb\n";
        assert_eq!(compact_phase(text, 1).as_deref(), Some("## Phase 1\na\n### sub"));
        assert_eq!(compact_phase(text, 2).as_deref(), Some("## Phase 2\nb"));
        assert_eq!(compact_phase(text, 3), None);
    }
}
