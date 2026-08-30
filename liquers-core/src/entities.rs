//! Named entities: `~n<name>~`.
//!
//! This module owns the **named** half of the query grammar's escape mechanism. The general
//! escaping algorithm — character classes, the fixed-length mnemonics, the numeric entities and
//! [`crate::escape::encode_token`] — lives in [`crate::escape`].
//!
//! # Two tables, and why they are separate
//!
//! | Table | Compiled | Role |
//! |---|---|---|
//! | curated (~203 names) | always | what a default build decodes, **and the only names the encoder emits** |
//! | the rest of HTML5 (~1922 names) | with `entities-html5` | decode-only |
//!
//! The split is not an optimisation. Query text is identity in Liquers — asset keys, cache keys
//! and stored links are query strings — so the set of names the *encoder* may use is a frozen
//! compatibility surface: adding one changes the canonical text of that character and invalidates
//! everything derived from it. Keeping the encoder's view in its own table
//! ([`curated_name`], which cannot see the feature-gated one) makes that guarantee structural
//! rather than a rule someone has to remember.
//!
//! It also means **everything any build encodes, every build decodes**: the curated table is
//! present unconditionally, so a `liquers-web` bundle without `entities-html5` still reads any
//! query a native encoder produced. Only hand-written exotic names differ.
//!
//! # Names are case-sensitive
//!
//! As in HTML5: `~namp~` is `&` and `~nAMP~` is not a curated name. Two pairs in the curated set
//! differ only by case and mean different things — `~nprime~` is `′` and `~nPrime~` is `″`.
//!
//! # Two names that do not mean what they look like
//!
//! HTML5 has **no** name for ASCII `~` (U+007E) or ASCII `-` (U+002D). The names that look like
//! they should are different characters:
//!
//! | Written | Decodes to | Not |
//! |---|---|---|
//! | `~ntilde~` | `˜` U+02DC | `~` — write `~~` |
//! | `~nTilde~` | `∼` U+223C | `~` — write `~~` |
//! | `~nhyphen~`, `~ndash~` | `‐` U+2010 | `-` — write `~_` |
//!
//! Both would produce a *valid query meaning something subtly different*, which is the worst
//! failure mode available here. Neither name is curated, so a default build rejects them outright;
//! with `entities-html5` they decode to the characters above. Also in
//! `specs/guides/QUERY_ESCAPING_GUIDE.md`.

mod entities_data {
    include!("entities_data.rs");
}

use entities_data::*;

/// A set of named entities stored as two concatenated blobs and their offset indexes.
///
/// `&[(&str, &str)]` was measured against this and rejected: it spends 32 bytes per entry on fat
/// pointers before any character data — 86.2 KB for the full table against 28.1 KB here.
struct EntityTable {
    /// All names, concatenated, in ascending lexicographic order.
    names: &'static str,
    /// `count + 1` byte offsets into `names`; entry `i` is `names[o[i]..o[i + 1]]`.
    name_offsets: &'static [u16],
    /// All decoded values, concatenated, in the same order as `names`.
    values: &'static str,
    /// `count + 1` byte offsets into `values`.
    value_offsets: &'static [u16],
}

impl EntityTable {
    fn len(&self) -> usize {
        self.name_offsets.len().saturating_sub(1)
    }

    fn slice(blob: &'static str, offsets: &'static [u16], i: usize) -> Option<&'static str> {
        let start = *offsets.get(i)? as usize;
        let end = *offsets.get(i + 1)? as usize;
        blob.get(start..end)
    }

    fn name(&self, i: usize) -> Option<&'static str> {
        Self::slice(self.names, self.name_offsets, i)
    }

    fn value(&self, i: usize) -> Option<&'static str> {
        Self::slice(self.values, self.value_offsets, i)
    }

    /// Binary search over the offset index. Names are generated in sorted order.
    fn lookup(&self, name: &str) -> Option<&'static str> {
        let mut lo = 0usize;
        let mut hi = self.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.name(mid)?.cmp(name) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return self.value(mid),
            }
        }
        None
    }
}

/// Annex B. Compiled unconditionally, and the encoder's whole repertoire.
static CURATED: EntityTable = EntityTable {
    names: CURATED_NAMES,
    name_offsets: CURATED_NAME_OFFSETS,
    values: CURATED_VALUES,
    value_offsets: CURATED_VALUE_OFFSETS,
};

/// The remaining HTML5 names. Decode-only, and free to grow without breaking canonical text.
#[cfg(feature = "entities-html5")]
static HTML5_EXTRA: EntityTable = EntityTable {
    names: HTML5_EXTRA_NAMES,
    name_offsets: HTML5_EXTRA_NAME_OFFSETS,
    values: HTML5_EXTRA_VALUES,
    value_offsets: HTML5_EXTRA_VALUE_OFFSETS,
};

/// Decoded text for a named entity, or `None` when the name is in no compiled table.
///
/// Searches the curated table first, then the HTML5 remainder when `entities-html5` is enabled.
///
/// ```
/// use liquers_core::entities::lookup;
/// assert_eq!(lookup("amp"), Some("&"));
/// assert_eq!(lookup("colon"), Some(":"));
/// assert_eq!(lookup("no_such_entity"), None);
/// ```
pub fn lookup(name: &str) -> Option<&'static str> {
    if let Some(value) = CURATED.lookup(name) {
        return Some(value);
    }
    #[cfg(feature = "entities-html5")]
    {
        return HTML5_EXTRA.lookup(name);
    }
    #[cfg(not(feature = "entities-html5"))]
    {
        None
    }
}

/// The curated name for a character, or `None` when it has none.
///
/// The encoder's only entry point into this module. It deliberately cannot see `HTML5_EXTRA`:
/// canonical query text must not depend on a cargo feature.
///
/// ```
/// use liquers_core::entities::curated_name;
/// assert_eq!(curated_name(':'), Some("colon"));
/// assert_eq!(curated_name('a'), None);
/// ```
pub fn curated_name(c: char) -> Option<&'static str> {
    let i = CURATED_BY_CHAR
        .binary_search_by_key(&c, |(ch, _)| *ch)
        .ok()?;
    let (_, index) = CURATED_BY_CHAR.get(i)?;
    CURATED.name(*index as usize)
}

/// Whether `name` is in the curated set.
///
/// Distinguishes "no such entity" from "that entity needs the `entities-html5` feature", which is
/// the difference between the two diagnostics.
pub fn is_curated(name: &str) -> bool {
    CURATED.lookup(name).is_some()
}

/// Number of names this build can decode. Differs between the two feature states.
pub fn compiled_count() -> usize {
    #[cfg(feature = "entities-html5")]
    {
        CURATED.len() + HTML5_EXTRA.len()
    }
    #[cfg(not(feature = "entities-html5"))]
    {
        CURATED.len()
    }
}

/// Regenerates `entities_data.rs`. Available to the `generate-entities` binary and to the
/// freshness test, so a stale checked-in table fails the suite rather than drifting silently.
#[cfg(any(feature = "cli", test))]
pub mod codegen;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_lookup_covers_the_ascii_punctuation_the_grammar_rejects() {
        // Exactly the printable ASCII characters that are neither accepted unescaped nor covered
        // by a mnemonic. Annex B tier B-1 claims every one of them has an HTML5 name.
        for (name, expected) in [
            ("excl", "!"),
            ("quot", "\""),
            ("num", "#"),
            ("dollar", "$"),
            ("percnt", "%"),
            ("amp", "&"),
            ("apos", "'"),
            ("lpar", "("),
            ("rpar", ")"),
            ("ast", "*"),
            ("comma", ","),
            ("colon", ":"),
            ("semi", ";"),
            ("lt", "<"),
            ("equals", "="),
            ("gt", ">"),
            ("quest", "?"),
            ("commat", "@"),
            ("lsqb", "["),
            ("bsol", "\\"),
            ("rsqb", "]"),
            ("Hat", "^"),
            ("grave", "`"),
            ("lcub", "{"),
            ("verbar", "|"),
            ("rcub", "}"),
        ] {
            assert_eq!(lookup(name), Some(expected), "lookup({name:?})");
            assert!(is_curated(name), "{name:?} must be curated");
        }
    }

    #[test]
    fn tilde_and_hyphen_names_are_not_the_ascii_characters() {
        // The trap: HTML5 has no name for ASCII `~` (U+007E) or `-` (U+002D). `tilde` is U+02DC
        // and `hyphen` is U+2010, so neither is a way to write the ASCII character.
        assert_eq!(
            curated_name('~'),
            None,
            "'~' has no name; the escape is `~~`"
        );
        assert_eq!(
            curated_name('-'),
            None,
            "'-' has no name; the escape is `~_`"
        );
        // Both are deliberately left out of the curated set, so a default build rejects them
        // rather than silently decoding them to the wrong character.
        assert!(!is_curated("tilde"));
        assert!(!is_curated("hyphen"));
    }

    #[cfg(feature = "entities-html5")]
    #[test]
    fn the_trap_names_decode_to_the_wrong_looking_characters() {
        assert_eq!(lookup("tilde"), Some("\u{2DC}"), "not ASCII '~'");
        assert_eq!(lookup("Tilde"), Some("\u{223C}"), "not ASCII '~'");
        assert_eq!(lookup("hyphen"), Some("\u{2010}"), "not ASCII '-'");
        assert_eq!(lookup("dash"), Some("\u{2010}"), "not ASCII '-'");
    }

    #[test]
    fn names_are_case_sensitive() {
        assert_eq!(lookup("prime"), Some("\u{2032}"));
        assert_eq!(lookup("Prime"), Some("\u{2033}"));
    }

    #[test]
    fn curated_name_round_trips_through_lookup() {
        // Every character the encoder may emit a name for decodes back to that character.
        for (c, _) in CURATED_BY_CHAR {
            let Some(name) = curated_name(*c) else {
                panic!("curated_name({c:?}) returned None despite being in CURATED_BY_CHAR");
            };
            assert_eq!(
                lookup(name).and_then(|v| v.chars().next()),
                Some(*c),
                "~n{name}~ must decode back to {c:?}"
            );
        }
    }

    #[test]
    fn curated_by_char_is_sorted_and_unique() {
        // Binary search depends on the first; canonical encoding depends on the second.
        for w in CURATED_BY_CHAR.windows(2) {
            assert!(w[0].0 < w[1].0, "CURATED_BY_CHAR must be sorted and unique");
        }
    }

    #[test]
    fn table_names_are_sorted() {
        for i in 1..CURATED.len() {
            let (Some(a), Some(b)) = (CURATED.name(i - 1), CURATED.name(i)) else {
                panic!("CURATED offsets do not cover index {i}");
            };
            assert!(a < b, "curated names must be sorted: {a:?} then {b:?}");
        }
    }

    #[cfg(not(feature = "entities-html5"))]
    #[test]
    fn default_build_decodes_only_the_curated_set() {
        assert_eq!(compiled_count(), 203);
        // `check` (U+2713) is outside Annex B, so a default build cannot decode `~ncheck~`.
        // `hellip` deliberately *is* curated (tier B-2), which is why it is not the example.
        assert_eq!(
            lookup("check"),
            None,
            "check needs the entities-html5 feature"
        );
        assert_eq!(lookup("hellip"), Some("\u{2026}"), "hellip is curated");
    }

    #[cfg(feature = "entities-html5")]
    #[test]
    fn full_build_decodes_the_whole_html5_table() {
        assert_eq!(compiled_count(), 2125);
        assert_eq!(lookup("check"), Some("\u{2713}"));
        assert_eq!(lookup("CounterClockwiseContourIntegral"), Some("\u{2233}"));
    }

    /// The checked-in generated table must match what the generator produces from the vendored
    /// JSON. Mirrors what `registry_export` enforces for `specs/command_registry.yaml`.
    #[test]
    fn entity_table_is_current() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let json = std::fs::read_to_string(root.join("data/entities.json"))?;
        let table: std::collections::BTreeMap<String, String> = serde_json::from_str(&json)?;
        let expected = codegen::generate(&table)?;
        let actual = std::fs::read_to_string(root.join("src/entities_data.rs"))?;
        assert_eq!(
            actual, expected,
            "src/entities_data.rs is stale — run \
             `cargo run -p liquers-core --features cli --bin generate-entities`"
        );
        Ok(())
    }
}
