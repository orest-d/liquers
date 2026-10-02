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

/// The ID families the conformance rules own: every registered rule ID with its digits removed.
///
/// Derived from the registry rather than listed, so a family is owned from its first rule on.
fn rule_families() -> BTreeSet<String> {
    rules()
        .iter()
        .map(|r| {
            r.meta
                .id
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .to_owned()
        })
        .collect()
}

/// The family a function name claims, if it begins `<family><digits>_`.
///
/// A rule function itself is `fn dir07(` — digits then `(` — so it does not match; only a name
/// that uses a rule ID as a *prefix of a longer name* does, which is the shape of a unit test.
fn claimed_family<'a>(name: &str, families: &'a BTreeSet<String>) -> Option<&'a str> {
    families.iter().map(String::as_str).find(|family| {
        name.strip_prefix(*family).is_some_and(|rest| {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            digits > 0 && rest[digits..].starts_with('_')
        })
    })
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn owned_rule_families_come_from_the_registry() {
    let families = rule_families();
    for family in ["dir", "sidecar", "sibling", "prefix", "nomakedir", "absence"] {
        assert!(families.contains(family), "{family} should be owned: {families:?}");
    }
    assert!(families.iter().all(|f| !f.is_empty()));
    assert_eq!(claimed_family("dir03_detection_is_not_a_count", &families), Some("dir"));
    assert_eq!(claimed_family("refute_dir07_fails", &families), None);
    assert_eq!(claimed_family("dir07", &families), None);
    assert_eq!(claimed_family("directory_metadata", &families), None);
}

/// **No unit test is named after a conformance rule.** The rule families belong to the rules,
/// which the contract and the guide cite; a test reusing `dir03_…` for a different claim made one
/// ID mean two things (`STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS`). A test about one store
/// takes a descriptive name; a rule's own refutation test is `refute_<rule id>_…`.
#[test]
fn no_unit_test_uses_an_owned_rule_id() {
    let Some(workspace) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("warning: no workspace directory; skipping the test-name scan");
        return;
    };
    let families = rule_families();
    let mut offenders = Vec::new();
    for krate in ["liquers-core", "liquers-store", "liquers-web"] {
        let root = workspace.join(krate);
        if !root.is_dir() {
            eprintln!("warning: {} not found; skipping it in the test-name scan", root.display());
            continue;
        }
        let mut files = Vec::new();
        for sub in ["src", "tests"] {
            rust_files(&root.join(sub), &mut files);
        }
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for (number, line) in text.lines().enumerate() {
                let Some(start) = line.find("fn ") else {
                    continue;
                };
                let name: String = line[start + 3..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if let Some(family) = claimed_family(&name, &families) {
                    offenders.push(format!(
                        "{}:{}: fn {name} (family `{family}`)",
                        file.display(),
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these functions are named after conformance rules; name them by subject, or \
         `refute_<rule id>_…` for a rule's own refutation test:\n{}",
        offenders.join("\n")
    );
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
    let rule_families = rule_families();
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
