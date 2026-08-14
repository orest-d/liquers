//! Generates `src/entities_data.rs` from `data/entities.json`.
//!
//! Lives in the library rather than in the binary so that `entity_table_is_current` can call it
//! directly — a freshness check that has to spawn a process tends to stop being run.
//!
//! # Why the curated list is a literal
//!
//! The curated set is the encoder's repertoire, and therefore a frozen compatibility surface:
//! adding a name changes the canonical text of that character and invalidates derived keys and
//! stored links. One reviewable literal makes a change to it visible in a diff, which a predicate
//! over the table would not. See `specs/design/parameter-entity-escaping/`, Annex B.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Annex B tiers B-0 through B-3: every name a default build can decode.
pub const CURATED: &[&str] = &[
    // B-0 XML predefined
    "amp", "lt", "gt", "quot", "apos",
    // B-1 ASCII punctuation the grammar rejects, plus decode-only synonyms
    "excl", "num", "dollar", "percnt", "lpar", "rpar", "ast", "comma", "colon", "semi", "equals",
    "quest", "commat", "lsqb", "bsol", "rsqb", "Hat", "grave", "lcub", "verbar", "rcub", "lbrack",
    "rbrack", "lbrace", "rbrace", "vert", "midast", "sol", "lowbar", "plus", "period",
    // B-2 Latin-1 and typography. Accented letters are deliberately absent: the numeric form is
    // shorter and no less legible for a letter (`caf~UE9~`, not `caf~neacute~`).
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "times", "divide", "szlig", "aelig", "oslash", "eth", "thorn", "ndash", "mdash", "horbar",
    "lsquo", "rsquo", "sbquo", "ldquo", "rdquo", "bdquo", "dagger", "Dagger", "bull", "hellip",
    "permil", "prime", "Prime", "trade", "euro",
    // B-3 Greek
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "sigmaf", "tau", "upsilon",
    "phi", "chi", "psi", "omega", "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta",
    "Theta", "Iota", "Kappa", "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi", "Rho", "Sigma", "Tau",
    "Upsilon", "Phi", "Chi", "Psi", "Omega",
    // B-3 mathematics and arrows
    "minus", "lowast", "radic", "prop", "infin", "ang", "and", "or", "cap", "cup", "int",
    "there4", "sim", "cong", "asymp", "ne", "equiv", "le", "ge", "sub", "sup", "nsub", "sube",
    "supe", "oplus", "otimes", "perp", "sdot", "part", "exist", "forall", "empty", "nabla",
    "isin", "notin", "ni", "prod", "sum", "larr", "uarr", "rarr", "darr", "harr", "crarr",
    "lArr", "uArr", "rArr", "dArr", "hArr", "loz", "spades", "clubs", "hearts", "diams", "ensp",
    "emsp", "thinsp", "zwnj", "zwj", "lrm", "rlm",
];

/// Curated names the **encoder must never emit**.
///
/// Either another curated name already spells the character (`lsqb` beats `lbrack`), or the
/// character is not escaped at all: `/` has the mnemonic `~/`, and `_`, `+` and `.` are accepted
/// unescaped. They stay in the table so a mechanical HTML-to-Liquers translation never fails.
pub const NOT_EMITTED: &[&str] = &[
    "lbrack", "rbrack", "lbrace", "rbrace", "vert", "midast", "sol", "lowbar", "plus", "period",
];

/// Plain `//` comments, not `//!`: the generated file is pulled in with `include!`, and an inner
/// doc comment cannot appear in a macro expansion at that position.
const HEADER: &str = "\
// Named-entity tables. @generated - do not edit.
//
// Produced by `cargo run -p liquers-core --features cli --bin generate-entities` from
// `liquers-core/data/entities.json`, the WHATWG HTML5 named character references.
// `entity_table_is_current` fails if this file and the generator disagree.
//
// Names are concatenated into one blob with a `u16` offset index, rather than stored as
// `&[(&str, &str)]`: the slice of pairs spends 32 bytes per entry on fat pointers before any
// character data, which measured 86.2 KB against 28.1 KB for this representation.
";

/// Build the generated module from the vendored table.
pub fn generate(table: &BTreeMap<String, String>) -> Result<String, String> {
    for name in CURATED {
        if !table.contains_key(*name) {
            return Err(format!("curated name {name:?} is not in the vendored table"));
        }
    }
    for name in NOT_EMITTED {
        if !CURATED.contains(name) {
            return Err(format!("not-emitted name {name:?} is not curated"));
        }
    }

    let mut curated: Vec<&str> = CURATED.to_vec();
    curated.sort_unstable();
    curated.dedup();
    if curated.len() != CURATED.len() {
        return Err("the curated list contains a duplicate name".to_owned());
    }
    let extra: Vec<&str> = table
        .keys()
        .map(String::as_str)
        .filter(|n| curated.binary_search(n).is_err())
        .collect();

    // The encoder's reverse index: one name per character, sorted by code point.
    let mut by_char: Vec<(char, u16)> = Vec::new();
    for (i, name) in curated.iter().enumerate() {
        if NOT_EMITTED.contains(name) {
            continue;
        }
        let Some(value) = table.get(*name) else {
            return Err(format!("curated name {name:?} vanished from the table"));
        };
        let mut chars = value.chars();
        // A per-`char` encoder cannot reach a multi-character value.
        let (Some(c), None) = (chars.next(), chars.next()) else {
            continue;
        };
        let index = u16::try_from(i).map_err(|_| "curated table exceeds u16 indexing".to_owned())?;
        by_char.push((c, index));
    }
    by_char.sort_unstable_by_key(|(c, _)| *c);
    if let Some(w) = by_char.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(format!(
            "character {:?} is claimed by two emitted curated names; exactly one must be canonical",
            w[0].0
        ));
    }

    let mut out = String::new();
    out.push_str(HEADER);
    emit_table(&mut out, "CURATED", &curated, table, "")?;
    out.push_str(
        "\n// Curated entities that decode to exactly one character, sorted by code point.\n\
         //\n\
         // The `u16` indexes `CURATED`, so each name is stored once. This is the encoder's only\n\
         // view of the table, and it deliberately cannot see the feature-gated one.\n\
         pub(crate) const CURATED_BY_CHAR: &[(char, u16)] = &[\n",
    );
    for (c, i) in &by_char {
        let _ = writeln!(out, "    ('{}', {}),", escape_char(*c), i);
    }
    out.push_str("];\n");
    emit_table(
        &mut out,
        "HTML5_EXTRA",
        &extra,
        table,
        "#[cfg(feature = \"entities-html5\")]\n",
    )?;
    Ok(out)
}

fn emit_table(
    out: &mut String,
    ident: &str,
    names: &[&str],
    table: &BTreeMap<String, String>,
    cfg: &str,
) -> Result<(), String> {
    let mut name_blob = String::new();
    let mut name_offsets: Vec<u16> = vec![0];
    let mut value_blob = String::new();
    let mut value_offsets: Vec<u16> = vec![0];
    for name in names {
        let Some(value) = table.get(*name) else {
            return Err(format!("name {name:?} is not in the table"));
        };
        name_blob.push_str(name);
        value_blob.push_str(value);
        name_offsets.push(u16::try_from(name_blob.len()).map_err(|_| {
            format!("{ident} name blob exceeds u16; widen the offset type in entities.rs")
        })?);
        value_offsets.push(u16::try_from(value_blob.len()).map_err(|_| {
            format!("{ident} value blob exceeds u16; widen the offset type in entities.rs")
        })?);
    }

    let _ = writeln!(
        out,
        "\n{cfg}pub(crate) const {ident}_NAMES: &str = \"{}\";",
        escape_str(&name_blob)
    );
    let _ = writeln!(
        out,
        "{cfg}pub(crate) const {ident}_NAME_OFFSETS: &[u16] = &{name_offsets:?};"
    );
    let _ = writeln!(
        out,
        "{cfg}pub(crate) const {ident}_VALUES: &str = \"{}\";",
        escape_str(&value_blob)
    );
    let _ = writeln!(
        out,
        "{cfg}pub(crate) const {ident}_VALUE_OFFSETS: &[u16] = &{value_offsets:?};"
    );
    Ok(())
}

/// Escape for a Rust string literal, forcing non-ASCII to `\u{...}` so the generated file is pure
/// ASCII and cannot be corrupted by an editor's encoding.
fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => {
                let _ = write!(out, "\\u{{{:X}}}", c as u32);
            }
        }
    }
    out
}

fn escape_char(c: char) -> String {
    match c {
        '\'' => "\\'".to_owned(),
        '\\' => "\\\\".to_owned(),
        c if c.is_ascii_graphic() || c == ' ' => c.to_string(),
        c => format!("\\u{{{:X}}}", c as u32),
    }
}
