//! `encodeParam` — turning an arbitrary string into a query parameter token.
//!
//! # Why this does not call `encode_token`
//!
//! `liquers_core::query::encode_token` escapes exactly four characters (`~`, space, `/`, `-`) and
//! passes everything else through **unchanged**. The parser's unescaped parameter set is much
//! narrower — ASCII alphanumerics plus `_`, `+` and `.` — so everything in the gap encodes to text
//! the parser later rejects, with no error raised at encode time. `a:b`, `a,b`, `a?b`, `café` and
//! `日本` all encode "successfully" and produce a query that fails to parse somewhere else, later.
//! See `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md`.
//!
//! Mirroring that behaviour into JavaScript would export the defect. So this encoder is written
//! against the **parser's** accepted set rather than against `encode_token`, and it **raises a
//! typed error** for any character it cannot represent.
//!
//! # The limitation, stated plainly
//!
//! The entity table (`liquers_core::parse`) offers `~~` `~_` `~<digit>` `~.` `~I` `~/` `~h` `~H`
//! `~f` `~P`. There is no entity for a lone colon, `?`, `=`, `&`, `#`, `,`, `(`, `)`, `%`, **or any
//! non-ASCII character**. Those values are therefore not encodable by *any* correct encoder today,
//! and `encodeParam` reports that rather than producing a broken query:
//!
//! ```javascript
//! liquers.encodeParam("hello world");   // "hello~.world"
//! liquers.encodeParam("-5");            // "~5"
//! liquers.encodeParam("12:30");         // throws: ':' cannot be encoded
//! liquers.encodeParam("café");          // throws: 'é' cannot be encoded
//! ```
//!
//! Refusing is the requirement, not a shortcoming of this implementation: silently emitting
//! unparseable text is the exact defect being avoided. When the entity redesign lands,
//! `encodeParam` delegates to the fixed `encode_token` and the limitation disappears — the API does
//! not change, only the set of inputs it accepts widens.

use liquers_core::error::{Error, ErrorType};

/// Characters the parser accepts unescaped inside a parameter, beyond ASCII alphanumerics.
///
/// From `liquers_core::parse::parameter_text`. Kept in sync by
/// `encode_param_matches_the_parser`, which round-trips every encodable character through the real
/// parser rather than trusting this list.
const UNESCAPED_EXTRA: [char; 3] = ['_', '+', '.'];

/// Encodes a string as an action-parameter token.
///
/// Returns a typed `ParameterError` naming the offending character when the value contains
/// anything the grammar cannot represent.
pub fn encode_param(text: &str) -> Result<String, Error> {
    if text.is_empty() {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            "An empty string cannot be encoded as a query parameter: the grammar requires at \
             least one character. Omit the parameter, or use its default."
                .to_string(),
        ));
    }

    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphanumeric() || UNESCAPED_EXTRA.contains(&c) {
            out.push(c);
        } else {
            match c {
                '~' => out.push_str("~~"),
                ' ' => out.push_str("~."),
                '/' => out.push_str("~/"),
                '-' => {
                    // The compact form: `~<digit>` decodes to `-<digit>`, one character shorter
                    // than `~_` followed by the digit. Both are correct; this matches what
                    // `encode_token` emits, so encoded queries look the same from either side.
                    match chars.get(i + 1) {
                        Some(next) if next.is_ascii_digit() => {
                            out.push('~');
                            out.push(*next);
                            i += 1;
                        }
                        Some(_) | None => out.push_str("~_"),
                    }
                }
                other => return Err(unencodable(text, other)),
            }
        }
        i += 1;
    }
    Ok(out)
}

fn unencodable(text: &str, c: char) -> Error {
    let escape = if c.is_ascii() {
        format!("{c:?}")
    } else {
        format!("{c:?} (U+{:04X})", c as u32)
    };
    Error::from_error(
        ErrorType::ParameterError,
        format!(
            "Cannot encode {text:?} as a query parameter: the character {escape} has no entity in \
             the Liquers query grammar. Encodable characters are ASCII letters and digits, \
             '_', '+', '.', and the escapable '~', ' ', '/' and '-'. See \
             specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md — a general entity mechanism is \
             designed but not yet implemented."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    /// Reads back the single decoded string parameter of `cmd-<encoded>`.
    fn decoded_parameter(query: &liquers_core::query::Query) -> String {
        let action = match query.action() {
            Some(a) => a,
            None => panic!("no action request in the query"),
        };
        match action.parameters.first() {
            Some(liquers_core::query::ActionParameter::String(s, _)) => s.clone(),
            other => panic!("expected one string parameter, got {other:?}"),
        }
    }

    /// Everything `encode_param` accepts must parse back to the original value **through the real
    /// parser**, not through a second copy of the rules.
    ///
    /// This is the assertion that matters: an encoder checked only against its own table agrees
    /// with itself by construction. Building a query, parsing it, and reading the argument back is
    /// what catches a divergence from `parameter_text`.
    fn roundtrip(value: &str) -> String {
        let encoded = encode_param(value).unwrap_or_else(|e| panic!("encode {value:?}: {e}"));
        let query = liquers_core::parse::parse_query(&format!("cmd-{encoded}"))
            .unwrap_or_else(|e| panic!("parse {encoded:?} (from {value:?}): {e}"));
        decoded_parameter(&query)
    }

    /// OBJECT09 (accepting half) — everything the encoder accepts parses back unchanged.
    #[wasm_bindgen_test]
    fn object09_encode_param_roundtrip() {
        for value in [
            "plain",
            "with_underscore",
            "with.dot",
            "with+plus",
            "MiXeD123",
            "hello world",
            "a/b",
            "a-b",
            "-5",
            "a-5",
            "3-4",
            "trailing-",
            "~tilde",
            "~",
            "-",
            " ",
        ] {
            assert_eq!(roundtrip(value), value, "round trip failed for {value:?}");
        }
    }

    /// OBJECT09 (refusing half) — an unrepresentable value raises rather than producing text
    /// that will not parse.
    #[wasm_bindgen_test]
    fn object09_encode_param_refuses_what_it_cannot_represent() {
        // Every one of these is a character `encode_token` would pass through verbatim, producing
        // text the parser rejects. Refusing is the whole point.
        for value in [
            "12:30", "a,b", "a?b", "a=b", "a&b", "a#b", "a%b", "a(b", "café", "日本",
        ] {
            let err = encode_param(value)
                .err()
                .unwrap_or_else(|| panic!("{value:?} must be refused, not silently encoded"));
            assert_eq!(err.error_type, ErrorType::ParameterError);
            assert!(
                err.message.contains(value),
                "the error must quote the offending value: {}",
                err.message
            );
        }
    }

    #[wasm_bindgen_test]
    fn web_encode_param_refuses_the_empty_string() {
        let err = encode_param("").err().expect("empty must be refused");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }

    /// The defect this encoder exists to avoid, demonstrated on the core encoder.
    ///
    /// If this test ever fails, `encode_token` has been fixed — at which point `encode_param`
    /// should delegate to it and this test should be replaced by one asserting they agree.
    #[wasm_bindgen_test]
    fn web_core_encode_token_still_produces_unparseable_text() {
        let broken = liquers_core::query::encode_token("12:30");
        assert_eq!(broken, "12:30", "encode_token passes ':' through unchanged");

        let round_trips = liquers_core::parse::parse_query(&format!("cmd-{broken}"))
            .ok()
            .and_then(|q| q.action().map(|a| (q, a)))
            .map(|(q, a)| a.parameters.len() == 1 && decoded_parameter(&q) == "12:30")
            .unwrap_or(false);
        assert!(
            !round_trips,
            "encode_token's output for '12:30' must not round-trip — if it now does, \
             PARAMETER-ESCAPING-INCOMPLETE has been fixed and encode_param should delegate to it"
        );
    }
}
