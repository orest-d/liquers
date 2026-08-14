//! T3 — a malformed entity is diagnosed specifically, **at the right position**.
//!
//! Positions are the part worth testing rather than assuming. `nom` reports the span the failure
//! was raised with, exactly — measured before this was written: `a-x~X~q~E` already reported offset
//! 3, the `~X~` itself, not the end of input. So the entity parser captures the span at the opening
//! `~` before consuming anything and constructs the failure with it.
//!
//! `cut` alone would not do this. It converts `Err::Error` into `Err::Failure` without rewriting
//! the span, so it reports wherever the *inner* parser stopped — which for `~U110000~` would be
//! after the digits, not at the `~` that introduced them.

use liquers_core::parse::parse_query;

/// Each malformed entity, the substring its message must contain, and the offset it must report.
///
/// The offsets are into the whole query text, so they are what a caller would underline.
#[test]
fn malformed_entities_report_a_specific_message_at_the_offending_tilde() {
    // `f-` is two characters, so an entity starting the parameter is at offset 2.
    for (query, needle, offset) in [
        ("f-~U110000~", "beyond the maximum code point", 2),
        ("f-~UD800~", "surrogate", 2),
        ("f-~U~", "empty body", 2),
        ("f-~Uzz~", "not a valid base-16 number", 2),
        ("f-~nfoo~", "not a named entity available in this build", 2),
        ("f-~U41", "is not terminated", 2),
        // Not at the start of the parameter: the offset must follow the literal text.
        ("f-abc~U110000~", "beyond the maximum code point", 5),
        ("f-abc~nfoo~", "not a named entity", 5),
        // Not the first parameter either.
        ("f-ok-~nfoo~", "not a named entity", 5),
        // Inside a link. `link_query` parses on the original span rather than a slice, so the
        // position must still be absolute — this is the case that would break if it did not.
        ("f-~X~g-~nfoo~~E", "not a named entity", 7),
    ] {
        let Err(e) = parse_query(query) else {
            panic!("{query:?} must not parse");
        };
        assert!(
            e.message.contains(needle),
            "parsing {query:?} gave {:?}, expected it to mention {needle:?}",
            e.message
        );
        assert_eq!(
            e.position.offset, offset,
            "parsing {query:?}: message was {:?}",
            e.message
        );
    }
}

/// The diagnostic for a name that exists in HTML5 but is not compiled must name the feature,
/// because the fix is a build-configuration change rather than a typo. Without this, a query that
/// works on the server reports a generic parse failure in the browser.
#[cfg(not(feature = "entities-html5"))]
#[test]
fn an_uncurated_name_points_at_the_feature_and_at_the_numeric_form() {
    let Err(e) = parse_query("f-~ncheck~") else {
        panic!("`check` is not curated, so it must not parse in a default build");
    };
    assert!(e.message.contains("entities-html5"), "{}", e.message);
    assert!(e.message.contains("~U<hex>~"), "{}", e.message);
    assert_eq!(e.position.offset, 2);
}

#[cfg(feature = "entities-html5")]
#[test]
fn an_uncurated_name_parses_with_the_feature() -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_query("f-~ncheck~")?;
    let action = query.action().ok_or("no action")?;
    assert_eq!(action.parameters[0].string_value(), Some("\u{2713}".to_owned()));
    Ok(())
}

/// Well-formed entities still parse, so the error paths above are not simply rejecting everything.
#[test]
fn well_formed_entities_parse() -> Result<(), Box<dyn std::error::Error>> {
    for (query, expected) in [
        ("f-~U41~", "A"),
        ("f-~D65~", "A"),
        ("f-~O101~", "A"),
        ("f-~B1000001~", "A"),
        ("f-~U~41~", "A"),
        ("f-~namp~", "&"),
        ("f-~n~amp~", "&"),
        ("f-12~ncolon~30", "12:30"),
        ("f-~U1F600~", "\u{1F600}"),
        ("f-~U65E5~~U672C~", "\u{65E5}\u{672C}"),
    ] {
        let parsed = parse_query(query)?;
        let action = parsed.action().ok_or("no action")?;
        assert_eq!(
            action.parameters[0].string_value(),
            Some(expected.to_owned()),
            "parsing {query:?}"
        );
    }
    Ok(())
}

/// An entity error is a *committed* failure, so it is not swallowed by an outer alternative and
/// re-reported as something vaguer. Without the commit, `alt` would fall through and the user
/// would get "Can't parse query completely" pointing somewhere unhelpful.
#[test]
fn an_entity_error_is_not_masked_by_a_later_alternative() {
    let Err(e) = parse_query("f-~nfoo~") else {
        panic!("must not parse");
    };
    assert!(
        !e.message.contains("parse query completely"),
        "the specific entity message must survive, got {:?}",
        e.message
    );
}
