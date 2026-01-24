use liquers_core::{error::Error, state::State, value::ValueInterface};
use crate::value::ExtValueInterface;
use polars::prelude::*;
use std::sync::Arc;
use std::io::Cursor;
use chrono::{NaiveDate, NaiveDateTime};

/// Core utility function to extract DataFrame from State
///
/// This function tries multiple strategies to obtain a Polars DataFrame:
/// 1. Direct conversion if value is already a PolarsDataFrame
/// 2. Deserialization from Text or Bytes based on metadata data_format
///
/// Supported formats: csv, parquet
pub fn try_to_polars_dataframe<V: ValueInterface + ExtValueInterface>(
    state: &State<V>,
) -> Result<Arc<DataFrame>, Error> {
    // Try direct conversion first
    if let Ok(df) = state.data.as_polars_dataframe() {
        return Ok(df);
    }

    // Get data format from metadata
    let format = state.metadata.get_data_format();

    // Try deserialization based on value type and format
    match format.as_str() {
        "csv" => {
            // Try to get as text or bytes
            if let Ok(text) = state.data.try_into_string() {
                let df = CsvReader::new(Cursor::new(text.as_bytes()))
                    .finish()
                    .map_err(|e| {
                        Error::general_error(format!("Failed to parse CSV as DataFrame: {}", e))
                    })?;
                return Ok(Arc::new(df));
            }

            // Try bytes
            if let Ok(bytes) = state.as_bytes() {
                let df = CsvReader::new(Cursor::new(bytes.as_slice()))
                    .finish()
                    .map_err(|e| {
                        Error::general_error(format!("Failed to parse CSV as DataFrame: {}", e))
                    })?;
                return Ok(Arc::new(df));
            }
        }
        "parquet" => {
            // TODO: Enable parquet support by adding "parquet" feature to polars dependency
            return Err(Error::general_error(
                "Parquet format not yet supported. Add 'parquet' feature to polars dependency.".to_string()
            ));
        }
        _ => {
            return Err(Error::general_error(format!(
                "Unsupported data format '{}' for DataFrame conversion. Supported: csv, parquet",
                format
            )));
        }
    }

    Err(Error::conversion_error(
        state.data.identifier().as_ref(),
        "Polars DataFrame",
    ))
}

/// Parse date from string in multiple formats
///
/// Supported formats (tried in order):
/// 1. YYYYMMDD (compact, no separators): "20240115"
/// 2. YYYY-MM-DD (ISO 8601): "2024-01-15"
/// 3. YYYY_MM_DD (underscore separator): "2024_01_15"
pub fn parse_date(s: &str) -> Result<NaiveDate, Error> {
    let s = s.trim();

    // Try YYYYMMDD (compact format)
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        return NaiveDate::parse_from_str(s, "%Y%m%d").map_err(|_| {
            Error::general_error(format!("Invalid date format: {}", s))
        });
    }

    // Try YYYY-MM-DD (ISO format)
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date);
    }

    // Try YYYY_MM_DD (underscore format)
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y_%m_%d") {
        return Ok(date);
    }

    Err(Error::general_error(format!(
        "Cannot parse '{}' as Date (use YYYYMMDD, YYYY-MM-DD, or YYYY_MM_DD)",
        s
    )))
}

/// Parse datetime from string in multiple formats
///
/// Supported formats:
/// 1. YYYY-MM-DD HH:MM:SS (ISO with space): "2024-01-15 14:30:00"
/// 2. YYYY-MM-DDTHH:MM:SS (ISO with T): "2024-01-15T14:30:00"
/// 3. YYYYMMDDHHMMSS (compact): "20240115143000"
pub fn parse_datetime(s: &str) -> Result<NaiveDateTime, Error> {
    let s = s.trim();

    // Try YYYYMMDDHHMMSS (compact format)
    if s.len() == 14 && s.chars().all(|c| c.is_ascii_digit()) {
        return NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M%S").map_err(|_| {
            Error::general_error(format!("Invalid datetime format: {}", s))
        });
    }

    // Try YYYY-MM-DD HH:MM:SS (ISO with space)
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt);
    }

    // Try YYYY-MM-DDTHH:MM:SS (ISO with T)
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(dt);
    }

    // Try with microseconds - YYYY-MM-DD HH:MM:SS.SSS
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(dt);
    }

    // Try with microseconds - YYYY-MM-DDTHH:MM:SS.SSS
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(dt);
    }

    Err(Error::general_error(format!(
        "Cannot parse '{}' as DateTime (use ISO 8601 or YYYYMMDDHHMMSS)",
        s
    )))
}

/// Parse boolean from string
///
/// Accepts (case-insensitive):
/// - true, t, 1 => true
/// - false, f, 0 => false
pub fn parse_boolean(s: &str) -> Result<bool, Error> {
    let s = s.trim().to_lowercase();

    match s.as_str() {
        "true" | "t" | "1" => Ok(true),
        "false" | "f" | "0" => Ok(false),
        _ => Err(Error::general_error(format!(
            "Cannot parse '{}' as Boolean (use true/false, t/f, or 1/0)",
            s
        ))),
    }
}

/// Parse CSV separator from string
///
/// Accepts:
/// - "comma" => b','
/// - "tab" => b'\t'
/// - "semicolon" => b';'
/// - "pipe" => b'|'
/// - Single character => that character as byte
pub fn parse_separator(s: &str) -> Result<u8, Error> {
    match s.trim().to_lowercase().as_str() {
        "comma" => Ok(b','),
        "tab" => Ok(b'\t'),
        "semicolon" => Ok(b';'),
        "pipe" => Ok(b'|'),
        single if single.len() == 1 => Ok(single.as_bytes()[0]),
        _ => Err(Error::general_error(format!(
            "Invalid separator '{}'. Use 'comma', 'tab', 'semicolon', 'pipe', or single character",
            s
        ))),
    }
}

/// Check if column exists in DataFrame, return error if not
pub fn check_column_exists(df: &DataFrame, column: &str) -> Result<(), Error> {
    let column_names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    if !column_names.contains(&column.to_string()) {
        return Err(Error::general_error(format!(
            "Column '{}' not found in DataFrame. Available columns: {:?}",
            column,
            column_names
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date_yyyymmdd() {
        let date = parse_date("20240115").unwrap();
        assert_eq!(date.to_string(), "2024-01-15");
    }

    #[test]
    fn test_parse_date_iso() {
        let date = parse_date("2024-01-15").unwrap();
        assert_eq!(date.to_string(), "2024-01-15");
    }

    #[test]
    fn test_parse_date_underscore() {
        let date = parse_date("2024_01_15").unwrap();
        assert_eq!(date.to_string(), "2024-01-15");
    }

    #[test]
    fn test_parse_date_invalid() {
        assert!(parse_date("2024/01/15").is_err());
        assert!(parse_date("invalid").is_err());
    }

    #[test]
    fn test_parse_datetime_iso_space() {
        let dt = parse_datetime("2024-01-15 14:30:00").unwrap();
        assert_eq!(dt.to_string(), "2024-01-15 14:30:00");
    }

    #[test]
    fn test_parse_datetime_iso_t() {
        let dt = parse_datetime("2024-01-15T14:30:00").unwrap();
        assert_eq!(dt.to_string(), "2024-01-15 14:30:00");
    }

    #[test]
    fn test_parse_datetime_compact() {
        let dt = parse_datetime("20240115143000").unwrap();
        assert_eq!(dt.to_string(), "2024-01-15 14:30:00");
    }

    #[test]
    fn test_parse_boolean() {
        assert_eq!(parse_boolean("true").unwrap(), true);
        assert_eq!(parse_boolean("TRUE").unwrap(), true);
        assert_eq!(parse_boolean("t").unwrap(), true);
        assert_eq!(parse_boolean("1").unwrap(), true);
        assert_eq!(parse_boolean("false").unwrap(), false);
        assert_eq!(parse_boolean("f").unwrap(), false);
        assert_eq!(parse_boolean("0").unwrap(), false);
        assert!(parse_boolean("yes").is_err());
    }

    #[test]
    fn test_parse_separator() {
        assert_eq!(parse_separator("comma").unwrap(), b',');
        assert_eq!(parse_separator("tab").unwrap(), b'\t');
        assert_eq!(parse_separator("semicolon").unwrap(), b';');
        assert_eq!(parse_separator("pipe").unwrap(), b'|');
        assert_eq!(parse_separator("|").unwrap(), b'|');
        assert_eq!(parse_separator(":").unwrap(), b':');
        assert!(parse_separator("invalid").is_err());
    }
}
