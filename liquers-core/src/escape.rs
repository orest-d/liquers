//! Escaping and unescaping string action parameters.
//!
//! This module owns the **general** escape mechanism: the character classes, the fixed-length
//! mnemonics, the numeric entities, and [`encode_token`]. The named entities `~n<name>~` live in
//! [`crate::entities`]. Both directions derive from the tables here and there, so the encoder and
//! the parser cannot drift — [`crate::parse`] contains no entity knowledge of its own, only the
//! `nom` plumbing that calls [`match_entity`].
//!
//! # The invariant
//!
//! **[`crate::query::ActionParameter::String`] holds the decoded parameter value, unconstrained.**
//! It is the value itself, never query text: any sequence of Unicode scalar values, including `~`,
//! `/`, `-`, spaces, and text that looks like an entity. A caller builds a query with a raw string
//! and escaping happens only when `encode()` is called.
//!
//! # Entity forms
//!
//! | Form | Meaning | Emitted by the encoder |
//! |---|---|---|
//! | `~~` `~_` `~.` `~/` | `~` `-` space `/` | yes |
//! | `~h` `~H` `~f` `~P` | `http://` `https://` `file://` `://` | yes |
//! | `~I` | `/` | no — `~/` is canonical |
//! | `~<digits>` | `-<digits>` | yes |
//! | `~U<hex>~` | code point, hexadecimal | **yes — canonical for anything else** |
//! | `~D<dec>~` `~O<oct>~` `~B<bin>~` | the same code point in another radix | no |
//! | `~n<name>~` | a named entity | only for [curated](crate::entities) names |
//! | `~<opener>~<body>~` | any long form, with a separator tilde | no |
//!
//! `~X~` and `~E` are **not** entities: they delimit a link parameter. See [`crate::parse`].
//!
//! # What the encoder emits
//!
//! At each position, in this fixed order — the priority is what makes the output stable, and
//! stability is a compatibility guarantee, because query text is identity in Liquers:
//!
//! 1. the longest matching **mnemonic** (`https://` → `~H`, `-4` → `~4`, `/` → `~/`);
//! 2. the character **literally**, if ASCII-accepted (`[A-Za-z0-9_+.]`);
//! 3. the **curated named entity** (`:` → `~ncolon~`), even when the numeric form is shorter;
//! 4. `~U<hex>~`, uppercase digits, no leading zeros.
//!
//! Step 1 outranks 3, so `/` stays `~/` and never becomes `~nsol~`. Step 2 outranks 3, so `+`,
//! `.` and `_` stay literal despite having the names `plus`, `period` and `lowbar`.
//!
//! Output is **pure ASCII** (a non-ASCII character is escaped even though the parser would accept
//! it literally), so a query survives transport through ASCII-only systems.
//!
//! # Properties
//!
//! - `decode_token(&encode_token(s)) == s` for every `s`.
//! - `encode_token` is **infallible**: `&str` guarantees every `char` is a Unicode scalar value,
//!   and every scalar value has a `~U<hex>~` spelling. Fallibility belongs to the decoder.
//! - Canonicalisation is **idempotent**. With `E = encode ∘ parse`, `E(E(t)) == E(t)`, so `E` is a
//!   projection onto canonical text. `E(t) == t` is false in general, which is why idempotence and
//!   not identity is the property that holds.

use std::borrow::Cow;

use crate::entities;
use crate::error::{Error, ErrorType};

/// The radix an entity opener selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radix {
    /// `~U…~` — the only radix the encoder emits.
    Hexadecimal,
    /// `~D…~`
    Decimal,
    /// `~O…~`
    Octal,
    /// `~B…~`
    Binary,
}

impl Radix {
    /// The opener letter that selects this radix.
    pub fn opener(self) -> char {
        match self {
            Radix::Hexadecimal => 'U',
            Radix::Decimal => 'D',
            Radix::Octal => 'O',
            Radix::Binary => 'B',
        }
    }

    /// The numeric base.
    pub fn base(self) -> u32 {
        match self {
            Radix::Hexadecimal => 16,
            Radix::Decimal => 10,
            Radix::Octal => 8,
            Radix::Binary => 2,
        }
    }

    fn from_opener(c: char) -> Option<Radix> {
        match c {
            'U' => Some(Radix::Hexadecimal),
            'D' => Some(Radix::Decimal),
            'O' => Some(Radix::Octal),
            'B' => Some(Radix::Binary),
            _ => None,
        }
    }
}

/// A fixed-length entity: exact encoded text, exact decoded text.
struct Mnemonic {
    encoded: &'static str,
    decoded: &'static str,
    /// Whether [`encode_token`] may emit this spelling. `~I` decodes `/` but is never emitted,
    /// because `~/` occupies the same slot and only one spelling can be canonical.
    emitted: bool,
}

/// Ordered **longest `decoded` first**, which is what makes the encoder's step 1 a greedy longest
/// match rather than a search.
///
/// `~<digits>` is absent because it is contextual — a `-` *followed by* a digit — and is handled
/// explicitly in both directions.
static MNEMONICS: &[Mnemonic] = &[
    Mnemonic { encoded: "~H", decoded: "https://", emitted: true },
    Mnemonic { encoded: "~h", decoded: "http://", emitted: true },
    Mnemonic { encoded: "~f", decoded: "file://", emitted: true },
    Mnemonic { encoded: "~P", decoded: "://", emitted: true },
    Mnemonic { encoded: "~~", decoded: "~", emitted: true },
    Mnemonic { encoded: "~.", decoded: " ", emitted: true },
    Mnemonic { encoded: "~/", decoded: "/", emitted: true },
    Mnemonic { encoded: "~_", decoded: "-", emitted: true },
    Mnemonic { encoded: "~I", decoded: "/", emitted: false },
];

/// One piece of a parameter token: literal text, or an entity and what it decoded to.
///
/// The decoder produces these; [`decode_token`] is their concatenation. Both exist because
/// tooling that wants to highlight or explain entities needs the pieces, while every current
/// caller needs the join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSegment<'a> {
    /// Literal characters, accepted unescaped.
    Text(&'a str),
    /// An entity: its exact spelling in the source, and its decoded text.
    Entity {
        /// The source text, `~` to `~` inclusive.
        spelling: &'a str,
        /// What it decodes to. Borrowed from a static table for mnemonics and named entities;
        /// owned only for a numeric entity, which materialises a `char`.
        decoded: Cow<'static, str>,
    },
}

/// Whether a character may appear unescaped in a string action parameter.
///
/// Unicode alphanumerics are accepted, so `f-café` and `f-日本` parse. This **widens** what the
/// parser took before, which is the backward-compatible direction: the previous test truncated the
/// code point to its low byte, so `Ł` (U+0141, low byte `0x41`) was accepted while `é` was not.
///
/// The encoder does not rely on this — it escapes every non-ASCII character regardless — so the
/// two are deliberately asymmetric: the decoder accepts a superset of what the encoder emits.
pub fn is_unescaped_parameter_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '+' || c == '.'
}

/// Whether a character may appear in a resource name, other than a leading `-`.
///
/// ASCII only, deliberately: resource names become store keys, and a key reaches a real
/// filesystem. This **narrows** what the parser took before — `-R/data/ŁŁ.csv` parses at older
/// revisions and does not now — because the previous behaviour accepted a character exactly when
/// its low byte happened to land in `[0-9A-Za-z]`, which nobody could predict.
///
/// Resource names have no entity production, so a name outside this class cannot be written at
/// all. See `specs/issues/RESOURCE-NAME-ASCII-ONLY.md`.
pub fn is_resource_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

/// Characters a name or numeric body may contain, between the opener and the closing `~`.
fn is_body_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Encode a string as an action-parameter token.
///
/// Infallible, and the output is pure ASCII. See the module docs for the priority order.
///
/// ```
/// use liquers_core::escape::encode_token;
/// assert_eq!(encode_token("12:30"), "12~ncolon~30");
/// assert_eq!(encode_token("hello world"), "hello~.world");
/// assert_eq!(encode_token("https://api.example.com/data"), "~Hapi.example.com~/data");
/// assert_eq!(encode_token("-5"), "~5");
/// assert_eq!(encode_token("café"), "caf~UE9~");
/// assert_eq!(encode_token("😀"), "~U1F600~");
/// // `~X~` in a value can no longer re-parse as a link.
/// assert_eq!(encode_token("~X~"), "~~X~~");
/// ```
pub fn encode_token<S: AsRef<str>>(text: S) -> String {
    let text = text.as_ref();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    'outer: while let Some(c) = rest.chars().next() {
        // Step 1a: the contextual compact negative-number form, `-4` -> `~4`.
        if c == '-' {
            let mut following = rest[1..].chars();
            if let Some(d) = following.next() {
                if d.is_ascii_digit() {
                    out.push('~');
                    out.push(d);
                    rest = &rest[1 + d.len_utf8()..];
                    continue;
                }
            }
        }
        // Step 1b: the longest matching mnemonic.
        for m in MNEMONICS {
            if m.emitted && rest.starts_with(m.decoded) {
                out.push_str(m.encoded);
                rest = &rest[m.decoded.len()..];
                continue 'outer;
            }
        }
        // Step 2: literal, if ASCII-accepted. Note this is *not* is_unescaped_parameter_char:
        // the encoder emits ASCII only, while the parser also accepts Unicode alphanumerics.
        if c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '.' {
            out.push(c);
        } else if let Some(name) = entities::curated_name(c) {
            // Step 3: the curated name, even when `~U<hex>~` would be shorter. Readability wins,
            // and the curated set is frozen so this stays stable.
            out.push_str("~n");
            out.push_str(name);
            out.push('~');
        } else {
            // Step 4.
            out.push_str("~U");
            for ch in format!("{:X}", c as u32).chars() {
                out.push(ch);
            }
            out.push('~');
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Decode a parameter token: the exact inverse of what the parser accepts.
///
/// ```
/// use liquers_core::escape::decode_token;
/// assert_eq!(decode_token("12~ncolon~30")?, "12:30");
/// // Spellings the encoder never emits are still accepted.
/// assert_eq!(decode_token("~U0041~")?, "A");
/// assert_eq!(decode_token("~D65~")?, "A");
/// assert_eq!(decode_token("~U~41~")?, "A");
/// assert!(decode_token("~UD800~").is_err(), "surrogates are not scalar values");
/// # Ok::<(), liquers_core::error::Error>(())
/// ```
pub fn decode_token(text: &str) -> Result<String, Error> {
    let mut out = String::with_capacity(text.len());
    for segment in segments(text)? {
        match segment {
            TokenSegment::Text(t) => out.push_str(t),
            TokenSegment::Entity { decoded, .. } => out.push_str(&decoded),
        }
    }
    Ok(out)
}

/// The segments of a token, in order. Errors on the first malformed entity.
///
/// [`decode_token`] is this, concatenated. This form exists for tooling that needs to know which
/// spans were entities — syntax highlighting, or an explanation on hover.
///
/// ```
/// use liquers_core::escape::{segments, TokenSegment};
/// let s = segments("a~ncolon~b")?;
/// assert_eq!(s.len(), 3);
/// assert_eq!(s[0], TokenSegment::Text("a"));
/// assert!(matches!(&s[1], TokenSegment::Entity { spelling, .. } if *spelling == "~ncolon~"));
/// # Ok::<(), liquers_core::error::Error>(())
/// ```
pub fn segments(text: &str) -> Result<Vec<TokenSegment<'_>>, Error> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut text_start = 0usize;
    while i < text.len() {
        let Some(rest) = text.get(i..) else {
            break;
        };
        if !rest.starts_with('~') {
            let Some(c) = rest.chars().next() else { break };
            if !is_unescaped_parameter_char(c) {
                // Not merely unusual — the parser would not accept it here, and this
                // function is documented as the exact inverse of what the parser accepts.
                // Letting it through would let a caller "successfully" decode a token that
                // means something else, or nothing, once placed in a query.
                return Err(entity_error(format!(
                    "{c:?} at offset {i} cannot appear unescaped in a parameter; \
                     write it as an entity, for example ~U{:X}~",
                    c as u32
                )));
            }
            i += c.len_utf8();
            continue;
        }
        if text_start < i {
            if let Some(t) = text.get(text_start..i) {
                out.push(TokenSegment::Text(t));
            }
        }
        let Some((len, decoded)) = match_entity(rest)? else {
            return Err(entity_error(format!(
                "'~' at offset {i} does not start a known entity"
            )));
        };
        if let Some(spelling) = text.get(i..i + len) {
            out.push(TokenSegment::Entity { spelling, decoded });
        }
        i += len;
        text_start = i;
    }
    if text_start < text.len() {
        if let Some(t) = text.get(text_start..) {
            out.push(TokenSegment::Text(t));
        }
    }
    Ok(out)
}

/// Match one entity at the start of `text`.
///
/// The single decode implementation: [`crate::parse`]'s entity combinator is a thin wrapper over
/// this, so there is exactly one place that knows what an entity means.
///
/// `Ok(None)` means "no entity starts here". `Err` means "an entity starts here and is
/// malformed", which the parser turns into a committed failure rather than a backtrack.
///
/// This is also the only place the **optional separator tilde** is handled: after a long-form
/// opener, one `~` may precede the body. That is unambiguous because body characters exclude `~`,
/// so a tilde in that position cannot be anything else.
pub(crate) fn match_entity(text: &str) -> Result<Option<(usize, Cow<'static, str>)>, Error> {
    if !text.starts_with('~') {
        return Ok(None);
    }
    let Some(opener) = text[1..].chars().next() else {
        return Err(entity_error("a '~' at the end of a parameter escapes nothing".to_owned()));
    };

    // The compact negative-number form: `~42` is `-42`.
    if opener.is_ascii_digit() {
        let digits: String = text[1..].chars().take_while(char::is_ascii_digit).collect();
        let len = 1 + digits.len();
        return Ok(Some((len, Cow::Owned(format!("-{digits}")))));
    }

    // Long forms: `~U…~` `~D…~` `~O…~` `~B…~` `~n…~`, each with an optional separator tilde.
    let radix = Radix::from_opener(opener);
    if radix.is_some() || opener == 'n' {
        let after_opener = 1 + opener.len_utf8();
        // D13: skip one optional separator tilde.
        let body_start = match text.get(after_opener..) {
            Some(r) if r.starts_with('~') => after_opener + 1,
            Some(_) | None => after_opener,
        };
        let Some(rest) = text.get(body_start..) else {
            return Err(entity_error(format!("~{opener} is missing its body")));
        };
        let body_len = rest.find(|c: char| !is_body_char(c)).unwrap_or(rest.len());
        let Some(body) = rest.get(..body_len) else {
            return Err(entity_error(format!("~{opener} is missing its body")));
        };
        if body.is_empty() {
            return Err(entity_error(format!(
                "~{opener}…~ has an empty body; write the {} between the opener and the closing '~'",
                if opener == 'n' { "entity name" } else { "digits" }
            )));
        }
        if rest.get(body_len..).is_none_or(|r| !r.starts_with('~')) {
            return Err(entity_error(format!(
                "~{opener}{body} is not terminated; a '~' must close it"
            )));
        }
        let total = body_start + body_len + 1;
        let decoded = match radix {
            Some(radix) => Cow::Owned(decode_numeric(radix, body)?.to_string()),
            None => match entities::lookup(body) {
                Some(value) => Cow::Borrowed(value),
                None => return Err(unknown_name_error(body)),
            },
        };
        return Ok(Some((total, decoded)));
    }

    // Fixed-length mnemonics. `~X` and `~E` are deliberately absent: they delimit a link and are
    // handled by the parser, not here.
    for m in MNEMONICS {
        if text.starts_with(m.encoded) {
            return Ok(Some((m.encoded.len(), Cow::Borrowed(m.decoded))));
        }
    }
    Ok(None)
}

/// Decode the body of a numeric entity into a character.
fn decode_numeric(radix: Radix, body: &str) -> Result<char, Error> {
    let Ok(value) = u32::from_str_radix(body, radix.base()) else {
        return Err(entity_error(format!(
            "~{}{}~ is not a valid base-{} number",
            radix.opener(),
            body,
            radix.base()
        )));
    };
    match char::from_u32(value) {
        Some(c) => Ok(c),
        None if (0xD800..=0xDFFF).contains(&value) => Err(entity_error(format!(
            "~{}{}~ is U+{:04X}, a surrogate; surrogates are not Unicode scalar values and \
             cannot appear in a string",
            radix.opener(),
            body,
            value
        ))),
        None => Err(entity_error(format!(
            "~{}{}~ is U+{:X}, beyond the maximum code point U+10FFFF",
            radix.opener(),
            body,
            value
        ))),
    }
}

fn unknown_name_error(name: &str) -> Error {
    // The two diagnostics differ because the fix differs, and the discriminator is the *build*,
    // not the name: this is only reached after `lookup` returned `None`, so no test on `name`
    // can tell "missing from this build" from "does not exist". With the full table compiled,
    // an unknown name can only be a typo, and suggesting a feature that is already enabled
    // would send the reader in the wrong direction.
    #[cfg(feature = "entities-html5")]
    {
        entity_error(format!(
            "~n{name}~ is not a named entity. This build has the full HTML5 table \
             ({} names), so the name does not exist — check the spelling, which is \
             case-sensitive, or write the character numerically as ~U<hex>~.",
            entities::compiled_count()
        ))
    }
    #[cfg(not(feature = "entities-html5"))]
    {
        entity_error(format!(
            "~n{name}~ is not a named entity available in this build. This build decodes {} \
             names; enable the `entities-html5` feature for the full HTML5 table, or write the \
             character numerically as ~U<hex>~.",
            entities::compiled_count()
        ))
    }
}

fn entity_error(message: String) -> Error {
    Error::from_error(ErrorType::ParseError, message)
}

/// Explain why the entity at the start of `text` is malformed.
///
/// Returns `None` when `text` does not begin an entity, so the caller falls through to its generic
/// message. Used by [`crate::parse`] to turn a committed parse failure into a specific diagnostic
/// at the position of the offending `~`.
pub(crate) fn explain_entity_error(text: &str) -> Option<String> {
    match match_entity(text) {
        Ok(Some(_)) => None,
        Ok(None) => None,
        Err(e) => Some(e.message.clone()),
    }
}

#[cfg(test)]
mod tests;
