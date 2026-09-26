//! Schema-less type inference for text-based table cells (CSV/TSV today; Markdown reuses it in a
//! later step).
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Schema-less inference rules". The
//! rule is applied **per column**: the first of `Bool`, `Int`, `Float`, `Date`, `Timestamp`,
//! `Text` that fits **every** non-null cell wins. A column of nulls only is nullable `Text`, and
//! the `Id` role is never guessed — an inferred field always carries `KeyRole::None` (the default
//! [`crate::schema::FieldSchema::new`] already gives it).
//!
//! **Resolved ambiguity**: Phase 2's prose states the canonical, round-trip check ("formatting the
//! parsed number gives the cell back") only for `Int`. Phase 3's
//! `schema_less_inference_canonical_int_rule_keeps_leading_zeros_as_text` also requires it of
//! `Float`: `+5` and `1e3` both parse as a valid `f64` (5.0 and 1000.0), so without the same
//! round-trip check the `amount` column in that test would be inferred `Float`, not `Text`. This
//! implementation applies the round-trip check to both `Int` and `Float`, which is what the test
//! requires and is consistent with the rule's stated purpose (a cell whose spelling a numeric type
//! cannot reproduce is not safely that type).

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

/// The same canonical check as [`is_canonical_int`], applied to `f64`. See this module's doc
/// comment for why Phase 3's tests require it here too.
fn is_canonical_float(text: &str) -> bool {
    match text.parse::<f64>() {
        Ok(value) => value.to_string() == text,
        Err(_) => false,
    }
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
    if non_null.iter().all(|text| is_canonical_float(text)) {
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
