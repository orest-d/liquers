//! Schema-less type inference for text-based table cells (CSV/TSV and Markdown; NDJSON reuses
//! its date and timestamp tests).
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Schema-less inference rules". The
//! rule is applied **per column**: the first of `Bool`, `Int`, `Float`, `Date`, `Timestamp`,
//! `Text` that fits **every** non-null cell wins. A column of nulls only is nullable `Text`, and
//! the `Id` role is never guessed — an inferred field always carries `KeyRole::None` (the default
//! [`crate::schema::FieldSchema::new`] already gives it).
//!
//! **`Float` is a spelling rule, not a round-trip rule.** Phase 2 states the canonical round-trip
//! check ("formatting the parsed number gives the cell back") only for `Int`. Applied to `Float`
//! it made ordinary decimals — `1.0`, `0.10`, `2.50`, `1e-7` — text. So a `Float` cell is a plain
//! decimal literal, `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?` with a point or an exponent (see
//! [`is_float_literal`]), or a canonical `Int` in a column that also holds decimals. `+5` and a
//! leading zero stay text, which keeps Phase 3's
//! `schema_less_inference_canonical_int_rule_keeps_leading_zeros_as_text` (`+5`, `1e3` → text:
//! `+5` alone keeps that column text). **A column of exponent forms such as `1e3` alone is
//! `Float`**: Phase 2's "`1e3` stays text" is said of the `Int` rule, and an exponent is how the
//! writers spell very large and very small floats.

use chrono::{DateTime, NaiveDate};

use crate::schema::FieldType;

fn is_bool(text: &str) -> bool {
    text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false")
}

/// `true` only when parsing `text` as `i64` and formatting it back gives exactly `text` — the
/// canonical check that keeps `01234` (a leading zero), `+5` (an explicit sign) and `1e3` (parses
/// as a number but not as this one, canonically) as text rather than a numeric type under a
/// changed spelling.
fn is_canonical_int(text: &str) -> bool {
    match text.parse::<i64>() {
        Ok(value) => value.to_string() == text,
        Err(_) => false,
    }
}

/// A decimal literal `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?` that is finite as an `f64`, or one of
/// the writer's non-finite spellings. Not a round-trip check — `1.0`, `0.10` and `1e-7` are
/// ordinary decimals — but a spelling check, so what stays text is what a number's spelling
/// cannot be trusted for: a sign (`+5`), a leading zero (`01.5`), a bare point (`.5`, `5.`), and a
/// value that overflows to infinity. A plain integer too large for `i64` has neither a point nor
/// an exponent and is not accepted here either, so it is never silently turned into a `Float`
/// (Phase 2's `Int` rule). A column of integers alone is `Int`, tried first.
///
/// **Non-finite values**: exactly `NaN`, `inf` and `-inf` — the spellings this crate's writers
/// produce for them (`csv::format_value`, Rust's `{:?}`) — so a `Float` column holding them
/// round-trips; other spellings (`nan`, `Infinity`) stay text rather than guessing.
fn is_float_literal(text: &str) -> bool {
    if matches!(text, "NaN" | "inf" | "-inf") {
        return true;
    }
    let bytes = text.as_bytes();
    let mut i = usize::from(bytes.first() == Some(&b'-'));
    let digits = |from: usize| bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();

    let int_len = digits(i);
    if int_len == 0 || (int_len > 1 && bytes[i] == b'0') {
        return false;
    }
    i += int_len;
    let mut has_fraction_or_exponent = false;
    if bytes.get(i) == Some(&b'.') {
        let frac_len = digits(i + 1);
        if frac_len == 0 {
            return false;
        }
        i += 1 + frac_len;
        has_fraction_or_exponent = true;
    }
    if matches!(bytes.get(i), Some(b'e') | Some(b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+') | Some(b'-')) {
            i += 1;
        }
        let exp_len = digits(i);
        if exp_len == 0 {
            return false;
        }
        i += exp_len;
        has_fraction_or_exponent = true;
    }
    has_fraction_or_exponent
        && i == bytes.len()
        && text.parse::<f64>().is_ok_and(f64::is_finite)
}

/// A `Float` cell: a float literal, or a canonical integer (a column mixing `1` and `2.5` is
/// `Float`).
fn is_float(text: &str) -> bool {
    is_float_literal(text) || is_canonical_int(text)
}

/// `pub(super)`: [`super::ndjson`]'s schema-less reader applies the same two checks to a JSON
/// *string* cell (phase2-architecture.md §"Schema-less inference rules": "Only strings go through
/// the date and timestamp tests").
pub(super) fn is_iso_date(text: &str) -> bool {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
}

pub(super) fn is_iso_timestamp(text: &str) -> bool {
    DateTime::parse_from_rfc3339(text).is_ok()
}

/// Infers a column's [`FieldType`] and nullability from its cell texts (`None` is a null cell).
/// Tries `Bool`, `Int`, `Float`, `Date`, `Timestamp` in that order and returns the first that fits
/// every non-null cell; falls back to `Text` when none does. An all-null column is nullable
/// `Text`; otherwise nullability is `true` exactly when a null was actually seen.
pub fn infer_column(cells: &[Option<&str>]) -> (FieldType, bool) {
    let nullable = cells.iter().any(|cell| cell.is_none());
    let non_null: Vec<&str> = cells.iter().filter_map(|cell| *cell).collect();

    if non_null.is_empty() {
        return (FieldType::Text, true);
    }
    if non_null.iter().all(|text| is_bool(text)) {
        return (FieldType::Bool, nullable);
    }
    if non_null.iter().all(|text| is_canonical_int(text)) {
        return (FieldType::Int, nullable);
    }
    if non_null.iter().all(|text| is_float(text)) {
        return (FieldType::Float, nullable);
    }
    if non_null.iter().all(|text| is_iso_date(text)) {
        return (FieldType::Date, nullable);
    }
    if non_null.iter().all(|text| is_iso_timestamp(text)) {
        return (FieldType::Timestamp, nullable);
    }
    (FieldType::Text, nullable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_bool_column_any_case() {
        let cells = [Some("true"), Some("FALSE"), Some("True")];
        assert_eq!(infer_column(&cells), (FieldType::Bool, false));
    }

    #[test]
    fn infers_int_column() {
        let cells = [Some("1"), Some("-2"), Some("3")];
        assert_eq!(infer_column(&cells), (FieldType::Int, false));
    }

    #[test]
    fn leading_zero_is_not_canonical_int_falls_back_to_text() {
        let cells = [Some("01234"), Some("9999")];
        assert_eq!(infer_column(&cells).0, FieldType::Text);
    }

    #[test]
    fn plus_sign_and_exponent_are_not_canonical_numbers() {
        let cells = [Some("+5"), Some("1e3")];
        assert_eq!(infer_column(&cells).0, FieldType::Text);
    }

    #[test]
    fn infers_float_column() {
        let cells = [Some("3.14"), Some("2.71")];
        assert_eq!(infer_column(&cells), (FieldType::Float, false));
    }

    #[test]
    fn infers_ordinary_decimals_and_exponents_as_float() {
        let cells = [Some("1.0"), Some("0.10"), Some("2.50"), Some("1e-7"), Some("-3.25E+2"), Some("7")];
        assert_eq!(infer_column(&cells), (FieldType::Float, false));
    }

    #[test]
    fn non_canonical_spellings_stay_text_not_float() {
        for cell in ["+5", "01.5", ".5", "5.", "1e", "1.5e400", "99999999999999999999", "nan", "Infinity"] {
            assert_eq!(infer_column(&[Some(cell)]).0, FieldType::Text, "{cell}");
        }
    }

    #[test]
    fn the_writers_non_finite_spellings_are_float() {
        let cells = [Some("NaN"), Some("inf"), Some("-inf"), Some("0.5")];
        assert_eq!(infer_column(&cells), (FieldType::Float, false));
    }

    #[test]
    fn infers_date_column() {
        let cells = [Some("2026-09-25"), Some("2026-09-26")];
        assert_eq!(infer_column(&cells), (FieldType::Date, false));
    }

    #[test]
    fn infers_timestamp_column() {
        let cells = [Some("2026-09-25T12:00:00Z"), Some("2026-09-26T13:00:00Z")];
        assert_eq!(infer_column(&cells), (FieldType::Timestamp, false));
    }

    #[test]
    fn all_null_column_is_nullable_text() {
        let cells: [Option<&str>; 2] = [None, None];
        assert_eq!(infer_column(&cells), (FieldType::Text, true));
    }

    #[test]
    fn nullable_is_true_only_when_a_null_was_seen() {
        let cells = [Some("1"), None, Some("3")];
        assert_eq!(infer_column(&cells), (FieldType::Int, true));
    }

    #[test]
    fn falls_back_to_text_when_nothing_else_fits() {
        let cells = [Some("hello"), Some("world")];
        assert_eq!(infer_column(&cells), (FieldType::Text, false));
    }

    #[test]
    fn empty_column_is_nullable_text() {
        let cells: [Option<&str>; 0] = [];
        assert_eq!(infer_column(&cells), (FieldType::Text, true));
    }
}
