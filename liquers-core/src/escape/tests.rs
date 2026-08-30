//! Tests for the escaping codec.
//!
//! The expected values here were produced by the design prototype
//! (`specs/design/parameter-entity-escaping/prototype.py`) and cross-checked against the parser,
//! rather than derived by hand — a hand-written octal constant in the design document was wrong,
//! which is why nothing in this file is typed from memory.

use super::*;

/// T1 — canonical spellings, one per priority step plus the motivating cases.
#[test]
fn encode_token_canonical_spellings() {
    for (value, expected) in [
        // The issue's motivating cases: none of these could be encoded at all before.
        ("12:30", "12~ncolon~30"),
        ("a,b", "a~ncomma~b"),
        ("café", "caf~UE9~"),
        ("日本", "~U65E5~~U672C~"),
        ("😀", "~U1F600~"),
        // Step 1: mnemonics, longest first.
        ("hello world", "hello~.world"),
        ("https://api.example.com/data", "~Hapi.example.com~/data"),
        ("http://x", "~hx"),
        ("file:///tmp/a", "~f~/tmp~/a"),
        ("://", "~P"),
        ("/", "~/"),
        ("-", "~_"),
        ("~", "~~"),
        ("-5", "~5"),
        ("-42", "~42"),
        // Step 2 outranks step 3: these have curated names but stay literal.
        ("+", "+"),
        (".", "."),
        ("_", "_"),
        // Step 3.
        ("&", "~namp~"),
        ("<", "~nlt~"),
        ("°", "~ndeg~"),
        ("π", "~npi~"),
        // A link marker in a value can no longer re-parse as a link.
        ("~X~", "~~X~~"),
        ("", ""),
    ] {
        assert_eq!(encode_token(value), expected, "encoding {value:?}");
    }
}

/// The encoder emits ASCII even where the parser would accept the character literally.
#[test]
fn encode_token_output_is_always_ascii() {
    for value in ["café", "日本", "😀", "Ł", "π", "°", "\u{10FFFF}", "\u{0}"] {
        let encoded = encode_token(value);
        assert!(
            encoded.is_ascii(),
            "{value:?} encoded to non-ASCII {encoded:?}"
        );
    }
}

/// Canonical numeric form: uppercase digits, no leading zeros. A compatibility surface.
#[test]
fn numeric_entities_are_uppercase_and_unpadded() {
    assert_eq!(encode_token("\u{1F600}"), "~U1F600~");
    assert_eq!(encode_token("\u{E9}"), "~UE9~");
    assert_eq!(encode_token("\u{1}"), "~U1~");
}

/// T2 — every spelling the decoder accepts, including the ones the encoder never emits.
#[test]
fn decode_token_accepts_the_full_superset() -> Result<(), Error> {
    for (text, expected) in [
        // Canonical forms.
        ("12~ncolon~30", "12:30"),
        ("~U1F600~", "😀"),
        ("~~", "~"),
        ("~5", "-5"),
        // Decode-only radixes: all four spell the same code point.
        ("~U41~", "A"),
        ("~D65~", "A"),
        ("~O101~", "A"),
        ("~B1000001~", "A"),
        // Leading zeros, accepted but never emitted.
        ("~U0041~", "A"),
        // D13: the optional separator tilde.
        ("~U~41~", "A"),
        ("~n~amp~", "&"),
        // Decode-only mnemonic and synonyms.
        ("~I", "/"),
        ("~nlbrack~", "["),
        ("~nsol~", "/"),
        ("~nplus~", "+"),
        // Adjacent entities: the `~~` in the middle is two terminators, not an escaped tilde.
        ("~U65E5~~U672C~", "日本"),
        ("", ""),
    ] {
        assert_eq!(decode_token(text)?, expected, "decoding {text:?}");
    }
    Ok(())
}

/// The four radixes of one astral code point, generated rather than typed.
#[test]
fn every_radix_spells_the_same_code_point() -> Result<(), Error> {
    let c = '\u{1F600}';
    let n = c as u32;
    for text in [
        format!("~U{n:X}~"),
        format!("~D{n}~"),
        format!("~O{n:o}~"),
        format!("~B{n:b}~"),
    ] {
        assert_eq!(decode_token(&text)?, c.to_string(), "decoding {text:?}");
    }
    Ok(())
}

/// `decode_token` is the exact inverse of what the parser accepts, so a literal the parser would
/// reject must not decode either. Reported by review: the scan used to advance over any non-`~`
/// character, so `":"` and `"a/b"` decoded "successfully" into tokens that mean something else —
/// or nothing — once placed in a query.
#[test]
fn decode_token_rejects_literals_the_parser_would_not_accept() {
    for (text, offending) in [
        (":", ':'),
        ("a-b", '-'),
        ("a/b", '/'),
        ("a b", ' '),
        ("a,b", ','),
    ] {
        let Err(e) = decode_token(text) else {
            panic!("{text:?} must not decode: {offending:?} is not valid unescaped");
        };
        assert!(
            e.message.contains("cannot appear unescaped"),
            "decoding {text:?} gave {:?}",
            e.message
        );
    }
    // The widened class still passes: these are alphanumeric, so the parser accepts them.
    for text in ["café", "日本", "Ł", "a_b+c.d"] {
        assert!(decode_token(text).is_ok(), "{text:?} must decode");
    }
}

/// T3 — each malformed entity produces its own message.
#[test]
fn malformed_entities_are_diagnosed_specifically() {
    for (text, needle) in [
        ("~U110000~", "beyond the maximum code point"),
        ("~UD800~", "surrogate"),
        ("~U~", "empty body"),
        ("~Uzz~", "not a valid base-16 number"),
        // Feature-independent substring: the rest of the sentence differs by build, and
        // the two cfg-gated tests below cover each wording.
        ("~nfoo~", "not a named entity"),
        ("~U41", "is not terminated"),
        ("~", "escapes nothing"),
    ] {
        let Err(e) = decode_token(text) else {
            panic!("{text:?} should not decode");
        };
        assert!(
            e.message.contains(needle),
            "decoding {text:?} gave {:?}, expected it to mention {needle:?}",
            e.message
        );
        assert_eq!(e.error_type, ErrorType::ParseError, "for {text:?}");
    }
}

/// The unavailable-name diagnostic names the feature, so a wasm failure is actionable.
#[cfg(not(feature = "entities-html5"))]
#[test]
fn an_uncurated_name_points_at_the_feature() {
    let Err(e) = decode_token("~ncheck~") else {
        panic!("`check` is not curated, so it must not decode in a default build");
    };
    assert!(e.message.contains("entities-html5"), "{}", e.message);
    assert!(e.message.contains("~U<hex>~"), "{}", e.message);
}

#[cfg(feature = "entities-html5")]
#[test]
fn an_uncurated_name_decodes_with_the_feature() -> Result<(), Error> {
    assert_eq!(decode_token("~ncheck~")?, "\u{2713}");
    Ok(())
}

/// T4 — the character-class change, in both directions.
#[test]
fn character_classes_widen_for_parameters_and_narrow_for_resource_names() {
    // Widened: the previous test truncated to the low byte, so `Ł` passed and `é` did not.
    assert!(is_unescaped_parameter_char('é'));
    assert!(is_unescaped_parameter_char('Ł'));
    assert!(is_unescaped_parameter_char('日'));
    assert!(is_unescaped_parameter_char('π'));
    assert!(
        !is_unescaped_parameter_char('°'),
        "a symbol is not alphanumeric"
    );
    assert!(!is_unescaped_parameter_char(':'));

    // Narrowed, deliberately: resource names become store keys.
    assert!(is_resource_name_char('a'));
    assert!(is_resource_name_char('_'));
    assert!(
        !is_resource_name_char('Ł'),
        "narrowing is the point; see RESOURCE-NAME-ASCII-ONLY"
    );
    assert!(!is_resource_name_char('é'));
}

/// Segments carry the spelling, which is what entity-aware tooling needs.
#[test]
fn segments_expose_spelling_and_decoded_text() -> Result<(), Error> {
    let s = segments("a~ncolon~b~U41~")?;
    assert_eq!(
        s,
        vec![
            TokenSegment::Text("a"),
            TokenSegment::Entity {
                spelling: "~ncolon~",
                decoded: Cow::Borrowed(":")
            },
            TokenSegment::Text("b"),
            TokenSegment::Entity {
                spelling: "~U41~",
                decoded: Cow::Owned("A".to_owned())
            },
        ]
    );
    Ok(())
}

// ---------------------------------------------------------------- property tests

/// A corpus mirroring the design prototype's: every printable ASCII character, Latin-1 and CJK
/// samples, astral code points, controls, adversarial strings, and pseudo-random strings over a
/// pool weighted toward the characters that interact with the grammar.
///
/// The seed is this generator's own — Python's Mersenne Twister and the generator below produce
/// different sequences, so the prototype's corpus is not reproduced element by element. What
/// transfers is the shape, and the ability to paste a failing input into the prototype.
fn corpus() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in ' '..='~' {
        out.push(c.to_string());
    }
    for s in [
        "é",
        "Ł",
        "°",
        "π",
        "…",
        "≠",
        "€",
        "😀",
        "\u{10FFFF}",
        "\u{0}",
        "\t",
        "\n",
        "\u{A0}",
        "12:30",
        "a,b",
        "café",
        "日本",
        "-5",
        "-42",
        "a-4",
        "~X~",
        "~E",
        "~~",
        "~n",
        "~U41~",
        "~namp~",
        "a~I b",
        "https://api.example.com/data",
        "http://x",
        "file:///tmp/a",
        "://",
        "a b c",
        "",
        "~",
        "-",
        "/",
        "+",
        ".",
        "_",
    ] {
        out.push(s.to_owned());
    }

    let pool: Vec<char> = (' '..='~')
        .chain(['~', '/', '-', ' ', 'é', 'π', '😀', ':'])
        .collect();
    // xorshift64*, so the corpus is deterministic without a dev-dependency.
    let mut state: u64 = 0x2026_0814_2026_0814;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..4000 {
        let len = (next() % 13) as usize;
        let s: String = (0..len)
            .filter_map(|_| pool.get((next() % pool.len() as u64) as usize).copied())
            .collect();
        out.push(s);
    }
    out
}

/// P1 — `decode_token(&encode_token(s)) == s` for every generated `s`.
/// P3 — and the encoding is always ASCII.
#[test]
fn round_trip_and_ascii_over_the_corpus() -> Result<(), Error> {
    for s in corpus() {
        let encoded = encode_token(&s);
        assert!(encoded.is_ascii(), "{s:?} encoded to non-ASCII {encoded:?}");
        let decoded = decode_token(&encoded)?;
        assert_eq!(
            decoded, s,
            "round trip failed for {s:?} (encoded {encoded:?})"
        );
    }
    Ok(())
}

/// P2 — canonicalisation is idempotent at token level: `encode(decode(encode(s))) == encode(s)`.
#[test]
fn canonicalisation_is_idempotent_over_the_corpus() -> Result<(), Error> {
    for s in corpus() {
        let once = encode_token(&s);
        let twice = encode_token(decode_token(&once)?);
        assert_eq!(once, twice, "not idempotent for {s:?}");
    }
    Ok(())
}

/// P2 again, entered from arbitrary valid token text rather than from encoder output — this is
/// what exercises the decode-only spellings collapsing to canonical.
#[test]
fn decode_only_spellings_collapse_to_canonical() -> Result<(), Error> {
    for text in [
        "~I",
        "~/",
        "~namp~",
        "~n~amp~",
        "~U0041~",
        "~U41~",
        "~D65~",
        "~O101~",
        "~B1000001~",
        "~nlbrack~",
        "~nlsqb~",
        "~nsol~",
        "~nplus~",
        "~nperiod~",
        "~nlowbar~",
        "~H",
        "~h",
        "~f",
        "~P",
        "a~Ib",
        "~5",
        "~42",
        "abc",
        "~~",
        "~.",
        "~_",
        "~U1F600~",
        "~D128512~",
    ] {
        let once = encode_token(decode_token(text)?);
        let twice = encode_token(decode_token(&once)?);
        assert_eq!(once, twice, "not idempotent entering from {text:?}");
    }
    Ok(())
}

/// T7 — the property backward compatibility rests on: a long-form opener can never be a legacy
/// short entity, so adding the long forms cannot change what an existing query means.
#[test]
fn long_form_openers_are_disjoint_from_the_legacy_set() {
    let long_form = ['U', 'D', 'O', 'B', 'n'];
    let mut legacy: Vec<char> = MNEMONICS
        .iter()
        .filter_map(|m| m.encoded.chars().nth(1))
        .collect();
    // `~X`/`~E` delimit links, and `~<digit>` is the compact negative number.
    legacy.extend(['X', 'E']);
    legacy.extend('0'..='9');

    for c in long_form {
        assert!(
            !legacy.contains(&c),
            "long-form opener '{c}' collides with a legacy entity; backward compatibility \
             depends on these two sets being disjoint"
        );
    }
}
