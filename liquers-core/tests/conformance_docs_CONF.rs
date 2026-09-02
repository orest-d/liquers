//! `D1` — the rule IDs in the code, in the contract, and in the guide are one set.
//!
//! The spine of this design is the rule ID: it names a function in
//! [`liquers_core::store_conformance::rules`], a citation in
//! `specs/reference/STORE_SEMANTICS.md`, and a row in
//! `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`. Nothing keeps three documents in step except a
//! test that fails when they diverge — which is the same reason
//! `liquers-lib/tests/registry_export.rs` exists for the command registry.
//!
//! **The relation is not equality in both directions.** Every rule in the code must be cited by
//! both documents, or a check exists that nobody wrote down. The converse is weaker: a document may
//! *mention* an ID in prose that is not a rule — a historical name, or one of the `keyabs`,
//! `diridx` or `pathmap` families, which are unit tests of their own components rather than
//! conformance rules. So this asserts **containment**, and separately that no cited ID looks like a
//! conformance rule the code does not have.

#![cfg(feature = "store-conformance")]

use liquers_core::store_conformance::rules;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Locate `specs/` by walking up from the crate directory.
///
/// Returns `None` for a packaged crate, which has no `specs/`. Skipping with a warning there is
/// deliberate: this test is about the repository's own consistency, and a published crate cannot
/// be inconsistent with documents it does not ship.
fn specs_dir() -> Option<PathBuf> {
    let mut dir: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("specs");
        if candidate.join("reference").is_dir() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// Every backticked token in `text` that has the shape of a rule ID: letters then two digits.
fn cited_rule_like(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for token in text.split('`') {
        let bytes = token.as_bytes();
        if bytes.len() >= 3
            && token.chars().all(|c| c.is_ascii_alphanumeric())
            && bytes[..bytes.len() - 2].iter().all(u8::is_ascii_alphabetic)
            && bytes[bytes.len() - 2..].iter().all(u8::is_ascii_digit)
        {
            out.insert(token.to_owned());
        }
    }
    out
}

#[test]
fn d1_rule_ids_agree_across_code_contract_and_guide() {
    let Some(specs) = specs_dir() else {
        eprintln!("warning: no specs/ directory found; skipping the documentation cross-check");
        return;
    };

    let registered: BTreeSet<String> = rules().iter().map(|r| r.meta.id.to_owned()).collect();
    assert!(
        !registered.is_empty(),
        "an empty rule set would make this test pass vacuously"
    );

    let contract_path = specs.join("reference/STORE_SEMANTICS.md");
    let guide_path = specs.join("guides/STORE_IMPLEMENTATION_GUIDE.md");
    let contract = std::fs::read_to_string(&contract_path).expect("STORE_SEMANTICS.md");
    let guide = std::fs::read_to_string(&guide_path).expect("STORE_IMPLEMENTATION_GUIDE.md");

    let in_contract = cited_rule_like(&contract);
    let in_guide = cited_rule_like(&guide);

    // 1. Every registered rule is cited by both documents.
    let missing_from_contract: Vec<&String> =
        registered.difference(&in_contract).collect();
    assert!(
        missing_from_contract.is_empty(),
        "these rules are registered but not cited in {}: {missing_from_contract:?}\n\
         Every rule enforces a written claim; add it to the section's *Enforced by* line.",
        contract_path.display()
    );

    let missing_from_guide: Vec<&String> = registered.difference(&in_guide).collect();
    assert!(
        missing_from_guide.is_empty(),
        "these rules are registered but not listed in {}: {missing_from_guide:?}\n\
         See its \"Where each rule comes from\" table.",
        guide_path.display()
    );

    // 2. No document cites a rule the code does not have — but only for the families that *are*
    //    conformance rules. `keyabs`, `diridx`, `pathmap` and `memdir` are component unit tests
    //    that both documents legitimately reference.
    let rule_families: BTreeSet<&str> = registered
        .iter()
        .map(|id| id.trim_end_matches(|c: char| c.is_ascii_digit()))
        .collect();
    for (label, cited) in [("contract", &in_contract), ("guide", &in_guide)] {
        let ghosts: Vec<&String> = cited
            .difference(&registered)
            .filter(|id| {
                rule_families.contains(id.trim_end_matches(|c: char| c.is_ascii_digit()))
            })
            .collect();
        assert!(
            ghosts.is_empty(),
            "the {label} cites {ghosts:?}, which look like conformance rules but are not \
             registered — either the rule was renamed or the citation is stale"
        );
    }
}
