//! I1 and I3 — the value/encoding invariant, and full-query round trips.
//!
//! # The invariant
//!
//! > In every `ActionParameter::String(s, _)`, `s` is the **decoded** parameter value,
//! > unconstrained.
//!
//! It is a property of stored state. Everything else follows from it: constructors and setters do
//! not encode, the parser stores the decoded result, and `encode` / `render` / `styled_tokens` are
//! the only three sites that escape — computing it on the way out and never storing it.
//!
//! What it buys is `parse(encode(p)) == p` for any programmatically built parameter, which is what
//! `PARAMETER-ESCAPING-INCOMPLETE` asks for. What it deliberately does *not* claim is
//! `encode(parse(t)) == t`: decoding is many-to-one, so re-encoding normalises.

use liquers_core::error::Error;
use liquers_core::parse::parse_query;
use liquers_core::query::{ActionParameter, ActionRequest, Query};

/// Values chosen to hit every branch of the encoder plus the historical failure cases.
fn values() -> Vec<&'static str> {
    vec![
        "plain",
        "12:30",
        "a,b",
        "hello world",
        "-5",
        "-42",
        "a-4",
        "/",
        "~",
        "~X~",
        "~E",
        "https://api.example.com/data",
        "://",
        "café",
        "日本",
        "😀",
        "°C",
        "π",
        "a&b=c?d#e",
        "\u{0}",
        "\u{10FFFF}",
    ]
}

/// I1 — every path into and out of the variant agrees on what is stored.
#[test]
fn stored_value_is_always_the_decoded_value() {
    for v in values() {
        // Constructor.
        let built = ActionParameter::new_string(v.to_owned());
        assert_eq!(built.string_value(), Some(v.to_owned()), "new_string({v:?})");

        // Setter. This is `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`: it used to store
        // `encode_token(v)`, so `string_value` returned the escaped text and `encode` escaped it a
        // second time.
        let mut set = ActionParameter::new_string(String::new());
        set.set_value(v);
        assert_eq!(set.string_value(), Some(v.to_owned()), "set_value({v:?})");

        // The two paths are indistinguishable, which is the point.
        assert_eq!(set, built, "set_value and new_string must agree for {v:?}");
        assert_eq!(set.encode(), built.encode(), "encoding differs for {v:?}");
    }
}

/// The escaping happens once, and only in `encode`.
#[test]
fn encoding_is_a_boundary_not_a_stored_form() {
    let mut p = ActionParameter::new_string(String::new());
    p.set_value("a b");
    assert_eq!(p.string_value(), Some("a b".to_owned()), "storage is not escaped");
    assert_eq!(p.encode(), "a~.b", "escaping happens on the way out");
    // Encoding twice would give `a~~.b`, which decodes to `a~.b` rather than `a b`.
    assert_ne!(p.encode(), "a~~.b");
}

/// I3 — a whole query built programmatically survives a round trip through its own text.
#[test]
fn programmatically_built_queries_round_trip() -> Result<(), Error> {
    for v in values() {
        let action = ActionRequest::new("filter".to_owned())
            .with_parameters(vec![ActionParameter::new_string(v.to_owned())]);
        let text = action.encode();
        assert!(text.is_ascii(), "encoded query must be ASCII, got {text:?} for {v:?}");

        let parsed = parse_query(&text)?;
        let Some(reparsed) = parsed.action() else {
            panic!("{text:?} did not parse back to an action");
        };
        assert_eq!(
            reparsed.parameters[0].string_value(),
            Some(v.to_owned()),
            "round trip failed for {v:?} (encoded {text:?})"
        );
    }
    Ok(())
}

/// Multiple parameters, including ones that previously could not coexist with the `-` separator.
#[test]
fn several_parameters_round_trip_together() -> Result<(), Error> {
    let action = ActionRequest::new("filter".to_owned()).with_parameters(vec![
        ActionParameter::new_string("12:30".to_owned()),
        ActionParameter::new_string("a-b".to_owned()),
        ActionParameter::new_string("café".to_owned()),
    ]);
    let text = action.encode();
    assert_eq!(text, "filter-12~ncolon~30-a~_b-caf~UE9~");

    let parsed = parse_query(&text)?;
    let Some(reparsed) = parsed.action() else {
        panic!("no action");
    };
    let got: Vec<Option<String>> = reparsed
        .parameters
        .iter()
        .map(ActionParameter::string_value)
        .collect();
    assert_eq!(
        got,
        vec![
            Some("12:30".to_owned()),
            Some("a-b".to_owned()),
            Some("café".to_owned())
        ]
    );
    Ok(())
}

/// The documented example from the issue, end to end.
///
/// `-R/data/report.csv/-/filter-12:30` was unwritable: the colon could not be encoded by any
/// correct encoder, so a query builder had to refuse the value.
#[test]
fn the_motivating_case_from_the_issue() -> Result<(), Error> {
    let value = "12:30";
    let action = ActionRequest::new("filter".to_owned())
        .with_parameters(vec![ActionParameter::new_string(value.to_owned())]);
    assert_eq!(action.encode(), "filter-12~ncolon~30");

    // In a full resource-plus-transform query. Neither `Query::action` nor `Query::transform_query`
    // applies here: the first wants a query that *is* a single action, the second a query that is
    // *purely* a transform. This one is both segments, so the segment itself is the way in.
    let query = parse_query("-R/data/report.csv/-/filter-12~ncolon~30")?;
    let Some(last) = query.segments.last() else {
        panic!("no segments");
    };
    let Some(a) = last.action() else {
        panic!("no action in the final segment");
    };
    assert_eq!(a.parameters[0].string_value(), Some(value.to_owned()));

    // And the whole thing is canonical already: it is what `encode` produces.
    assert_eq!(query.encode(), "-R/data/report.csv/-/filter-12~ncolon~30");
    Ok(())
}

/// A link parameter still round-trips, and a string parameter that *looks* like a link no longer
/// re-parses as one — the hole documented in `parse.rs` under "Round-trip limits of programmatic
/// construction".
#[test]
fn a_value_that_looks_like_a_link_is_not_one() -> Result<(), Error> {
    let inner = parse_query("greeting")?;
    let action = ActionRequest::new("greet".to_owned()).with_parameters(vec![
        ActionParameter::new_link(inner.clone()),
        ActionParameter::new_string("~X~greeting~E".to_owned()),
    ]);
    let text = action.encode();
    let parsed = parse_query(&text)?;
    let Some(a) = parsed.action() else {
        panic!("no action");
    };

    assert!(a.parameters[0].is_link(), "the real link stays a link");
    assert!(
        a.parameters[1].is_string(),
        "text that looks like a link must stay a string"
    );
    assert_eq!(
        a.parameters[1].string_value(),
        Some("~X~greeting~E".to_owned())
    );
    Ok(())
}

/// `Query`-level equality ignores `Position`, which is what makes `parse(encode(q)) == q` a
/// well-formed claim rather than an accident.
#[test]
fn equality_ignores_position() -> Result<(), Error> {
    let built = Query::default();
    assert_eq!(built, parse_query(&built.encode())?);

    let parsed = parse_query("filter-12~ncolon~30")?;
    let reparsed = parse_query(&parsed.encode())?;
    assert_eq!(parsed, reparsed);
    Ok(())
}
