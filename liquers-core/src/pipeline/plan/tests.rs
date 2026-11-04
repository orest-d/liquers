//! The dual-mode planning test (AC-10).

use super::*;
use crate::pipeline::convert::tests::converted_corpus;

/// A plan, serialized: the comparison covers every step, parameter, position, prefix query,
/// predecessor, flag and diagnostic.
fn plan_json(plan: &Plan) -> Json {
    serde_json::to_value(plan).unwrap_or_else(|e| Json::String(format!("<{e}>")))
}

/// AC-10 — every corpus query, in canonical form, plans the same in `Delegate` and `Native` mode.
#[test]
fn plans_are_equal_in_both_modes() {
    let corpus = converted_corpus();
    let cmr = &corpus.cmr;
    let delegate = PipelinePlanBuilder::new(cmr).with_mode(PlanningMode::Delegate);
    let native = PipelinePlanBuilder::new(cmr).with_mode(PlanningMode::Native);
    let mut failures = Vec::new();
    let mut compared = 0;
    for (text, _, pipeline) in &corpus.entries {
        let Ok(canonical) = pipeline.to_query_with(cmr) else {
            continue;
        };
        // The parsed query too: its plan carries the positions the text gave it.
        let parsed = crate::parse::parse_query(text).ok();
        for canonical in std::iter::once(canonical).chain(parsed) {
            let d = delegate.plan_query(&canonical);
            let n = native.plan_query(&canonical);
            compared += 1;
            match (d, n) {
                (Ok(d), Ok(n)) => {
                    let (d, n) = (plan_json(&d), plan_json(&n));
                    if d != n {
                        failures.push(format!(
                            "{text} ({}):\n    delegate {d}\n    native   {n}",
                            canonical.encode()
                        ));
                    }
                }
                (Err(d), Err(n)) => {
                    if d.error_type != n.error_type {
                        failures.push(format!(
                            "{text}: errors differ: {} / {}",
                            d.message, n.message
                        ));
                    }
                }
                (Ok(_), Err(n)) => failures.push(format!("{text}: native fails: {}", n.message)),
                (Err(d), Ok(_)) => {
                    failures.push(format!("{text}: only delegate fails: {}", d.message))
                }
            }
        }
    }
    assert!(compared > 100, "only {compared} corpus queries compared");
    assert!(
        failures.is_empty(),
        "{} of {compared} differ:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
