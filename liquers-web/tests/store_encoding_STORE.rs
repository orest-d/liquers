//! Envelope-codec tests for the `localStorage` store.
//!
//! Test IDs and names follow `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3. The inventory these
//! implement is in `specs/design/liquers-web-store/phase3-examples.md`, test group 4.
//!
//! These are pure functions over byte slices — no DOM, no `localStorage`, no `fetch` — so they run
//! under Node with no WebDriver, which is deliberate: this is where silent data corruption would
//! live, so it belongs in the fast loop rather than behind a browser.
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --test store_encoding_STORE
//! ```

#![cfg(target_arch = "wasm32")]

use liquers_core::error::ErrorType;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_web::store::{decode_envelope, encode_envelope, ByteEncoding};
use wasm_bindgen_test::*;

const STORE: &str = "test store";

fn key() -> Key {
    parse_key("d/f.bin").expect("fixture key must parse")
}

/// The corpus, fixed by Phase 3. Each entry declares the path it *must* take.
///
/// The entries that matter are the last three. `ED A0 80` is the UTF-8 encoding of a lone
/// surrogate — the one byte sequence where "looks like text" and "is valid UTF-8" diverge, and the
/// one most likely to survive a naive implementation and corrupt on the way back.
fn corpus() -> Vec<(&'static str, Vec<u8>, ByteEncoding)> {
    vec![
        ("empty", b"".to_vec(), ByteEncoding::Utf8),
        ("ascii", b"hello".to_vec(), ByteEncoding::Utf8),
        (
            "utf8 multibyte",
            "héllo — ok".as_bytes().to_vec(),
            ByteEncoding::Utf8,
        ),
        ("emoji", "🦀".as_bytes().to_vec(), ByteEncoding::Utf8),
        ("embedded nul", b"a\0b".to_vec(), ByteEncoding::Utf8),
        ("invalid utf8", vec![0xFF, 0xFE], ByteEncoding::Base64),
        (
            "lone surrogate",
            vec![0xED, 0xA0, 0x80],
            ByteEncoding::Base64,
        ),
        (
            "png header",
            vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            ByteEncoding::Base64,
        ),
    ]
}

/// encoding01 — every corpus entry round-trips byte-identical.
#[wasm_bindgen_test]
fn encoding01_corpus_round_trips_byte_identical() {
    for (name, data, _) in corpus() {
        let encoded = encode_envelope(&data);
        let decoded = decode_envelope(&encoded, &key(), STORE)
            .unwrap_or_else(|e| panic!("{name}: decode failed: {}", e.message));
        assert_eq!(decoded, data, "{name}: round trip changed the bytes");
    }
}

/// encoding02 — the envelope tag records the path actually taken.
///
/// Without this, `encoding01` would still pass if the selector inverted and base64 were used for
/// everything: the bytes would survive, and the recorded-encoding contract would be silently gone.
#[wasm_bindgen_test]
fn encoding02_selector_picks_the_declared_path() {
    for (name, data, expected) in corpus() {
        assert_eq!(
            ByteEncoding::for_data(&data),
            expected,
            "{name}: wrong encoding chosen"
        );

        let encoded = encode_envelope(&data);
        let tag = encoded.as_bytes()[1];
        assert_eq!(
            ByteEncoding::from_tag(tag),
            Some(expected),
            "{name}: envelope tag {:?} does not record the encoding used",
            tag as char
        );
        assert_eq!(
            encoded.as_bytes()[0],
            b'1',
            "{name}: envelope must carry the format version"
        );
    }
}

/// encoding03 — an envelope that cannot be read is a read error, never silently misread.
///
/// The `"2u…"` case is a *future* format version arriving at today's decoder: it must be refused,
/// because reading its payload as though it were version 1 is exactly the corruption the version
/// digit exists to prevent.
#[wasm_bindgen_test]
fn encoding03_corrupt_envelope_is_key_read_error() {
    let cases: &[(&str, &str)] = &[
        ("empty string", ""),
        ("one character", "1"),
        ("unknown version", "2uhello"),
        ("unknown tag", "1zhello"),
        ("corrupt base64", "1b!!!!"),
        ("not an envelope", "hello"),
    ];

    for (name, text) in cases {
        match decode_envelope(text, &key(), STORE) {
            Ok(v) => panic!("{name}: {text:?} decoded to {v:?} instead of failing"),
            Err(e) => assert_eq!(
                e.error_type,
                ErrorType::KeyReadError,
                "{name}: wrong error type ({})",
                e.message
            ),
        }
    }
}

/// encoding04 — a large binary payload round-trips.
///
/// Deterministic pseudorandom bytes rather than a fixed pattern: a repeating pattern can survive a
/// codec that mishandles block boundaries.
#[wasm_bindgen_test]
fn encoding04_large_binary_round_trips() {
    let mut data = Vec::with_capacity(1 << 20);
    let mut state: u32 = 0x1234_5678;
    for _ in 0..(1 << 20) {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        data.push((state >> 24) as u8);
    }

    assert_eq!(
        ByteEncoding::for_data(&data),
        ByteEncoding::Base64,
        "1 MiB of pseudorandom bytes should not be valid UTF-8"
    );

    let encoded = encode_envelope(&data);
    let decoded = decode_envelope(&encoded, &key(), STORE)
        .unwrap_or_else(|e| panic!("decode failed: {}", e.message));
    assert_eq!(decoded, data);
}
