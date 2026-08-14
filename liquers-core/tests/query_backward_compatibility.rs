//! T8 — every query the workspace already writes must keep parsing and keep meaning the same.
//!
//! The entity redesign (`PARAMETER-ESCAPING-INCOMPLETE`) changes two character classes and adds
//! five entity openers. Both are meant to be safe — the openers are disjoint from the legacy set,
//! and the parameter class only widens — but "meant to be safe" is what the `c as u8` truncation
//! was, so this asserts it over the corpus rather than arguing it.
//!
//! The corpus is collected mechanically from every `parse_query("…")` literal in the workspace
//! and **checked in**, so it is reviewable and a change to it shows up in a diff instead of
//! silently varying per run. It is split in two, because a scrape of test files picks up negative
//! cases as well as positive ones:
//!
//! | Fixture | Count | Claim |
//! |---|---|---|
//! | `query_corpus.txt` | 147 | parsed before the change, must still parse |
//! | `query_corpus_rejected.txt` | 9 | was rejected before, must still be rejected |
//!
//! The split was produced by running each literal through a `liquers-validate` built from the
//! pre-change commit, not by reading them — which is the only way to make "still" mean anything.
//! Keeping the negative half matters as much as the positive: an entity change that made a
//! malformed link *start* parsing would be as bad as one that broke a valid query.

use liquers_core::parse::parse_query;

fn lines_of(fixture: &str) -> Vec<String> {
    fixture
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Queries that parsed before the change.
fn corpus() -> Vec<String> {
    lines_of(include_str!("fixtures/query_corpus.txt"))
}

/// Queries that were rejected before the change, and must stay rejected.
fn rejected_corpus() -> Vec<String> {
    lines_of(include_str!("fixtures/query_corpus_rejected.txt"))
}

/// The negative half. An entity change that made one of these *start* parsing would be as much a
/// regression as one that broke a valid query, and is the easier mistake to make: every one of
/// them contains a tilde.
#[test]
fn the_rejected_corpus_is_still_rejected() {
    let mut unexpected = Vec::new();
    for q in rejected_corpus() {
        if parse_query(&q).is_ok() {
            unexpected.push(q);
        }
    }
    assert!(
        unexpected.is_empty(),
        "these were rejected before and must still be:\n  {}",
        unexpected.join("\n  ")
    );
}

/// Every corpus query still parses.
#[test]
fn the_corpus_still_parses() {
    let mut failures = Vec::new();
    for q in corpus() {
        if let Err(e) = parse_query(&q) {
            failures.push(format!("{q:?}: {}", e.message));
        }
    }
    assert!(
        failures.is_empty(),
        "queries that parsed before must still parse:\n  {}",
        failures.join("\n  ")
    );
}

/// Canonicalisation is idempotent over the corpus: `E(E(t)) == E(t)` where `E = encode ∘ parse`.
///
/// This is the property that makes "canonical form" mean anything, and it is the whole-query
/// counterpart of the token-level property test in `escape::tests`. Note `E(t) == t` is false in
/// general — the resource/transform shorthand canonicalises to the explicit `-R/` form — which is
/// exactly why idempotence rather than identity is asserted.
#[test]
fn canonicalisation_is_idempotent_over_the_corpus() -> Result<(), Box<dyn std::error::Error>> {
    for q in corpus() {
        let once = parse_query(&q)?.encode();
        let twice = parse_query(&once)?.encode();
        assert_eq!(once, twice, "E is not idempotent for {q:?}");
    }
    Ok(())
}

/// **Canonical** text round-trips to an equal query.
///
/// Stated from canonical form deliberately. `parse(encode(t)) == parse(t)` is *false* in general,
/// and not because of anything this change did: the resource/transform shorthand `a/b/-/c/d`
/// re-encodes to the explicit `-R/a/b/-/c/d`, and `ResourceQuerySegment`'s equality includes the
/// segment header, so the two compare unequal despite denoting the same thing. `parse.rs` already
/// says the explicit form "is what `Query::encode` emits and what round-trips unchanged".
///
/// So the property every consumer actually relies on is this one: once a query has been through
/// `encode` once, its text and its value agree from then on. Together with idempotence above, that
/// is what makes canonical form a fixed point rather than a moving target.
#[test]
fn canonical_text_round_trips_to_an_equal_query() -> Result<(), Box<dyn std::error::Error>> {
    for q in corpus() {
        let canonical = parse_query(&q)?.encode();
        let parsed = parse_query(&canonical)?;
        let reparsed = parse_query(&parsed.encode())?;
        assert_eq!(parsed, reparsed, "round trip changed the meaning of {q:?}");
    }
    Ok(())
}

/// The shorthand exception, pinned so the asymmetry above is documented by a test rather than only
/// by a comment.
#[test]
fn the_shorthand_canonicalises_to_the_explicit_form() -> Result<(), Box<dyn std::error::Error>> {
    let shorthand = parse_query("a/b/-/c/d")?;
    assert_eq!(shorthand.encode(), "-R/a/b/-/c/d");
    // Not equal to itself re-parsed, because the header differs...
    assert_ne!(shorthand, parse_query(&shorthand.encode())?);
    // ...but a fixed point from there on.
    let canonical = parse_query(&shorthand.encode())?;
    assert_eq!(canonical.encode(), "-R/a/b/-/c/d");
    assert_eq!(canonical, parse_query(&canonical.encode())?);
    Ok(())
}

/// The one intended regression, pinned so it cannot happen by accident a second time.
///
/// `resource_name` narrowed to ASCII (D10). Names made entirely of characters whose low byte
/// happened to land in `[0-9A-Za-z]` used to parse — `Ł` is U+0141, low byte `0x41` — which nobody
/// could predict. `RESOURCE-NAME-ASCII-ONLY` records the limitation that remains.
#[test]
fn non_ascii_resource_names_are_rejected_coherently() {
    for q in [
        "-R/data/\u{141}\u{141}.csv", // parsed before, by accident
        "-R/data/caf\u{e9}.csv",      // did not parse before either
        "-R/data/\u{141}\u{f3}d\u{17a}.csv",
    ] {
        assert!(
            parse_query(q).is_err(),
            "{q:?} must be rejected: resource names are ASCII-only"
        );
    }
    // The ASCII neighbours still work, so the narrowing is precise rather than broad.
    assert!(parse_query("-R/data/report.csv").is_ok());
    assert!(parse_query("-R/data/a_b-c.2.csv").is_ok());
}

/// The widening half, in the same test file so the asymmetry is visible in one place.
///
/// Parameters accept Unicode alphanumerics, so a query can be *written* with them even though the
/// encoder never emits them.
#[test]
fn parameters_accept_unicode_alphanumerics() -> Result<(), Box<dyn std::error::Error>> {
    for (query, expected) in [
        ("f-caf\u{e9}", "caf\u{e9}"),
        ("f-\u{65e5}\u{672c}", "\u{65e5}\u{672c}"),
        ("f-\u{141}", "\u{141}"),
    ] {
        let parsed = parse_query(query)?;
        let action = parsed.action().ok_or("no action")?;
        assert_eq!(
            action.parameters[0].string_value(),
            Some(expected.to_owned()),
            "parsing {query:?}"
        );
        // ...but the canonical spelling is ASCII.
        assert!(
            parsed.encode().is_ascii(),
            "encoding {query:?} must be ASCII, got {:?}",
            parsed.encode()
        );
    }
    Ok(())
}
