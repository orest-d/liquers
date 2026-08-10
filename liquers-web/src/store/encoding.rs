//! The `localStorage` value envelope.
//!
//! `localStorage` holds UTF-16 strings, not bytes, so arbitrary data needs an encoding. Three
//! rules govern this module, in order of importance
//! (`specs/design/liquers-web-store/phase1-high-level-design.md`, Q3):
//!
//! 1. **The encoding is recorded, never inferred on read.** Every stored value carries an explicit
//!    tag written at `set` time, so `get` decodes what was actually written rather than guessing
//!    from the bytes it finds.
//! 2. **The encoding is chosen by a check, not by a hint.** [`std::str::from_utf8`] succeeding is a
//!    *proof* that the direct path is lossless. A `Metadata` `type_name` or media type is a
//!    *guess*, and a wrong guess corrupts data silently — a `text/plain` asset may hold invalid
//!    UTF-8, and an `application/octet-stream` one may be pure ASCII. The check is a linear scan
//!    over bytes that are about to be copied anyway, so trusting metadata would not even be
//!    faster. Metadata is deliberately not consulted here.
//! 3. **Losslessness is tested, not argued.** See `tests/store_encoding_STORE.rs`.
//!
//! # Format
//!
//! One version digit, one encoding letter, then the payload:
//!
//! ```text
//! 1u<text>       the bytes are valid UTF-8 and are stored as text
//! 1b<base64>     anything else
//! ```
//!
//! Not JSON. A JSON envelope would escape the whole payload a second time — larger *and* a parse
//! on every read — for two fields, neither of which will grow. The leading digit is what allows a
//! third encoding to be added later, and [`decode_envelope`] refuses a version it does not know
//! rather than misreading it.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use liquers_core::error::Error;
use liquers_core::query::Key;

/// The current envelope format version.
const VERSION: u8 = b'1';

/// How a value's bytes are represented inside a `localStorage` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteEncoding {
    /// The bytes are valid UTF-8 and are stored as text.
    Utf8,
    /// Anything else, base64-encoded.
    Base64,
}

impl ByteEncoding {
    /// The tag character written into the envelope.
    ///
    /// Exhaustive with no `_ =>` arm, so adding a variant is a compile error here rather than a
    /// value that reaches storage untagged.
    pub fn tag(self) -> u8 {
        match self {
            ByteEncoding::Utf8 => b'u',
            ByteEncoding::Base64 => b'b',
        }
    }

    /// Parses a tag written by [`ByteEncoding::tag`]. `None` for anything else.
    pub fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            b'u' => Some(ByteEncoding::Utf8),
            b'b' => Some(ByteEncoding::Base64),
            _ => None,
        }
    }

    /// The encoding that can represent `data` losslessly, preferring the cheaper one.
    pub fn for_data(data: &[u8]) -> Self {
        match std::str::from_utf8(data) {
            Ok(_) => ByteEncoding::Utf8,
            Err(_) => ByteEncoding::Base64,
        }
    }
}

/// Wraps `data` in an envelope suitable for `localStorage`.
pub fn encode_envelope(data: &[u8]) -> String {
    let encoding = ByteEncoding::for_data(data);
    let mut out = String::new();
    out.push(VERSION as char);
    out.push(encoding.tag() as char);
    match encoding {
        // `for_data` returned `Utf8` only because `from_utf8` succeeded, so this cannot fail.
        // `from_utf8_lossy` rather than an `unwrap`: library code does not panic, and the
        // unreachable branch would produce replacement characters rather than a crash.
        ByteEncoding::Utf8 => out.push_str(&String::from_utf8_lossy(data)),
        ByteEncoding::Base64 => out.push_str(&BASE64.encode(data)),
    }
    out
}

/// Recovers the bytes from an envelope written by [`encode_envelope`].
///
/// Every failure is [`liquers_core::error::ErrorType::KeyReadError`] and never `KeyNotFound`: the
/// entry exists and cannot be read. Reporting it as absent would invite a caller to overwrite data
/// it merely failed to interpret.
pub fn decode_envelope(text: &str, key: &Key, store_name: &str) -> Result<Vec<u8>, Error> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return Err(Error::key_read_error(
            key,
            store_name,
            &format!(
                "stored value is too short to be an envelope ({} bytes)",
                bytes.len()
            ),
        ));
    }
    if bytes[0] != VERSION {
        return Err(Error::key_read_error(
            key,
            store_name,
            &format!(
                "unsupported envelope version {:?}; this build understands version {:?}",
                bytes[0] as char, VERSION as char
            ),
        ));
    }
    let encoding = ByteEncoding::from_tag(bytes[1]).ok_or_else(|| {
        Error::key_read_error(
            key,
            store_name,
            &format!("unknown envelope encoding tag {:?}", bytes[1] as char),
        )
    })?;

    // The first two bytes are ASCII, so this is always a character boundary.
    let payload = &text[2..];
    match encoding {
        ByteEncoding::Utf8 => Ok(payload.as_bytes().to_vec()),
        ByteEncoding::Base64 => BASE64.decode(payload).map_err(|e| {
            Error::key_read_error(key, store_name, &format!("corrupt base64 payload: {e}"))
        }),
    }
}
