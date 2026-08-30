//! `encodeParam` — turning an arbitrary string into a query parameter token.
//!
//! # This delegates to the core encoder
//!
//! `liquers_core::escape::encode_token` (re-exported as `liquers_core::query::encode_token`)
//! escapes every character the grammar cannot carry literally, so **any** string is representable:
//! `12:30`, `a,b`, `café`, `日本`, `😀`. This module is a thin wrapper that adds one thing the core
//! encoder deliberately does not have — a check for the empty string.
//!
//! ```javascript
//! liquers.encodeParam("hello world");   // "hello~.world"
//! liquers.encodeParam("-5");            // "~5"
//! liquers.encodeParam("12:30");         // "12~ncolon~30"
//! liquers.encodeParam("café");          // "caf~UE9~"
//! ```
//!
//! # Why there is no second implementation any more
//!
//! This module used to contain its own encoder, written against the parser's accepted set, which
//! raised a typed error for any character it could not represent — the entity table offered no
//! escape for a lone colon, `?`, `=`, `&`, `#`, `,`, `(`, `)`, `%`, or any non-ASCII character, so
//! no correct encoder could exist. Refusing was better than emitting text that would fail to parse
//! somewhere else, later.
//!
//! `PARAMETER-ESCAPING-INCOMPLETE` removed that limitation, and with it the reason for a second
//! copy of the escaping rules. Two implementations of one grammar is exactly how the original
//! defect went unnoticed, so this one is gone rather than merely corrected.
//!
//! # The one build-configuration caveat
//!
//! `liquers-web` does not enable `entities-html5`, so it decodes the **curated** ~203 names rather
//! than the full HTML5 table. This cannot affect anything encoded here: the core encoder emits
//! only curated names, and the curated table is compiled unconditionally, so everything any build
//! encodes, every build decodes. Only a *hand-written* query using an exotic name — `~ncheck~` —
//! differs, and the parser says so by name when it does.

use liquers_core::error::{Error, ErrorType};
use liquers_core::escape::encode_token;

/// Encodes a string as an action-parameter token.
///
/// Infallible except for the empty string: every other value is representable, because
/// `encode_token` escapes anything the grammar cannot carry.
pub fn encode_param(text: &str) -> Result<String, Error> {
    if text.is_empty() {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            "An empty string cannot be encoded as a query parameter: the grammar requires at \
             least one character. Omit the parameter, or use its default."
                .to_string(),
        ));
    }
    Ok(encode_token(text))
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

    /// OBJECT09 — everything the encoder accepts parses back unchanged.
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

    /// The values this module used to refuse. Each one is a case from
    /// `PARAMETER-ESCAPING-INCOMPLETE`; all of them now round-trip through the real parser.
    ///
    /// This test replaces `object09_encode_param_refuses_what_it_cannot_represent`, which asserted
    /// the opposite, and `web_core_encode_token_still_produces_unparseable_text`, which asserted
    /// that the core encoder was broken.
    #[wasm_bindgen_test]
    fn web_previously_unrepresentable_values_now_round_trip() {
        for value in [
            "12:30", "a,b", "a?b", "a=b", "a&b", "a#b", "a%b", "a(b", "café", "日本", "😀", "a;b",
            "a[b", "a@b", "a!b", "a*b", "a:b/c",
        ] {
            assert_eq!(roundtrip(value), value, "round trip failed for {value:?}");
        }
    }

    /// A curated-only build decodes everything a full build encodes. This is the claim that bounds
    /// the `entities-html5` asymmetry, and `liquers-web` is the build where it matters.
    #[wasm_bindgen_test]
    fn web_encoder_output_is_decodable_without_the_full_entity_table() {
        assert_eq!(encode_param("12:30").ok(), Some("12~ncolon~30".to_owned()));
        assert_eq!(encode_param("café").ok(), Some("caf~UE9~".to_owned()));
        // `hellip` is curated, so the ellipsis has a name even here; `check` is not, so its
        // character encodes numerically rather than to a name this build could not read back.
        assert_eq!(roundtrip("…"), "…");
        assert_eq!(roundtrip("✓"), "✓");
    }

    #[wasm_bindgen_test]
    fn web_encode_param_refuses_the_empty_string() {
        let err = encode_param("").err().expect("empty must be refused");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }
}
