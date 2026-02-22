use std::io::{Cursor, Write};

use liquers_core::error::{Error, ErrorType};
use polars::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolarsDataFormat {
    Csv { separator: u8 },
    Parquet,
    Ipc,
    Json,
    NdJson,
    Avro,
    Xlsx,
}

fn serialization_error(message: impl Into<String>) -> Error {
    Error::new(ErrorType::SerializationError, message.into())
}

pub fn parse_polars_data_format(data_format: &str) -> Result<PolarsDataFormat, Error> {
    let normalized = data_format.trim().to_ascii_lowercase();
    let format = normalized.as_str();
    let parsed = match format {
        "csv" | "csv_comma" | "csv:comma" => PolarsDataFormat::Csv { separator: b',' },
        "tsv" | "csv_tab" | "csv:tab" => PolarsDataFormat::Csv { separator: b'\t' },
        "csv_semicolon" | "csv:semicolon" => PolarsDataFormat::Csv { separator: b';' },
        "csv_pipe" | "csv:pipe" => PolarsDataFormat::Csv { separator: b'|' },
        "parquet" => PolarsDataFormat::Parquet,
        "ipc" | "feather" | "arrow_ipc" => PolarsDataFormat::Ipc,
        "json" => PolarsDataFormat::Json,
        "ndjson" | "jsonl" => PolarsDataFormat::NdJson,
        "avro" => PolarsDataFormat::Avro,
        "xlsx" => PolarsDataFormat::Xlsx,
        _ => {
            if let Some(rest) = format.strip_prefix("csv:") {
                if rest.len() == 1 {
                    PolarsDataFormat::Csv {
                        separator: rest.as_bytes()[0],
                    }
                } else {
                    return Err(serialization_error(format!(
                        "Unsupported csv separator '{}'. Use comma/tab/semicolon/pipe or a single character",
                        rest
                    )));
                }
            } else {
                return Err(serialization_error(format!(
                    "Unsupported polars data_format '{}'",
                    data_format
                )));
            }
        }
    };
    Ok(parsed)
}

pub fn deserialize_dataframe_from_reader<T: AsRef<[u8]> + Send + Sync>(
    reader: Cursor<T>,
    data_format: &str,
) -> Result<DataFrame, Error> {
    match parse_polars_data_format(data_format)? {
        PolarsDataFormat::Csv { separator } => CsvReadOptions::default()
            .map_parse_options(|parse_options| parse_options.with_separator(separator))
            .into_reader_with_file_handle(reader)
            .finish()
            .map_err(|e| {
                serialization_error(format!("Failed to deserialize CSV DataFrame: {}", e))
            }),
        PolarsDataFormat::Parquet => ParquetReader::new(reader).finish().map_err(|e| {
            serialization_error(format!("Failed to deserialize Parquet DataFrame: {}", e))
        }),
        PolarsDataFormat::Xlsx => Err(serialization_error(
            "XLSX deserialization is not supported by current Polars integration",
        )),
        PolarsDataFormat::Ipc
        | PolarsDataFormat::Json
        | PolarsDataFormat::NdJson
        | PolarsDataFormat::Avro => Err(serialization_error(format!(
            "Polars deserialization for data_format '{}' is not implemented yet",
            data_format
        ))),
    }
}

pub fn deserialize_lazyframe_from_reader<T: AsRef<[u8]> + Send + Sync>(
    reader: Cursor<T>,
    data_format: &str,
) -> Result<LazyFrame, Error> {
    let df = deserialize_dataframe_from_reader(reader, data_format)?;
    Ok(df.lazy())
}

pub fn serialize_dataframe_to_writer<W: Write>(
    df: &DataFrame,
    data_format: &str,
    writer: W,
) -> Result<(), Error> {
    match parse_polars_data_format(data_format)? {
        PolarsDataFormat::Csv { separator } => {
            let mut df = df.clone();
            CsvWriter::new(writer)
                .with_separator(separator)
                .finish(&mut df)
                .map_err(|e| {
                    serialization_error(format!("Failed to serialize CSV DataFrame: {}", e))
                })?;
            Ok(())
        }
        PolarsDataFormat::Parquet => {
            let mut df = df.clone();
            ParquetWriter::new(writer).finish(&mut df).map_err(|e| {
                serialization_error(format!("Failed to serialize Parquet DataFrame: {}", e))
            })?;
            Ok(())
        }
        PolarsDataFormat::Xlsx => Err(serialization_error(
            "XLSX serialization is not supported by current Polars integration",
        )),
        PolarsDataFormat::Ipc
        | PolarsDataFormat::Json
        | PolarsDataFormat::NdJson
        | PolarsDataFormat::Avro => Err(serialization_error(format!(
            "Polars serialization for data_format '{}' is not implemented yet",
            data_format
        ))),
    }
}

pub fn serialize_lazyframe_to_writer<W: Write>(
    lf: LazyFrame,
    data_format: &str,
    writer: W,
) -> Result<(), Error> {
    let df = lf.collect().map_err(|e| {
        serialization_error(format!(
            "Failed to materialize LazyFrame for serialization: {}",
            e
        ))
    })?;
    serialize_dataframe_to_writer(&df, data_format, writer)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn test_parse_polars_data_format_aliases() {
        assert_eq!(
            parse_polars_data_format("csv").unwrap(),
            PolarsDataFormat::Csv { separator: b',' }
        );
        assert_eq!(
            parse_polars_data_format("tsv").unwrap(),
            PolarsDataFormat::Csv { separator: b'\t' }
        );
        assert_eq!(
            parse_polars_data_format("csv:semicolon").unwrap(),
            PolarsDataFormat::Csv { separator: b';' }
        );
        assert_eq!(
            parse_polars_data_format("csv:|").unwrap(),
            PolarsDataFormat::Csv { separator: b'|' }
        );
        assert_eq!(
            parse_polars_data_format("parquet").unwrap(),
            PolarsDataFormat::Parquet
        );
    }

    #[test]
    fn test_parse_polars_data_format_unsupported() {
        assert!(parse_polars_data_format("foo").is_err());
        assert!(parse_polars_data_format("csv:xx").is_err());
    }

    #[test]
    fn test_csv_roundtrip_comma() {
        let df = df!["a" => &[1, 2], "b" => &["x", "y"]].unwrap();
        let mut bytes = Vec::new();
        serialize_dataframe_to_writer(&df, "csv", &mut bytes).unwrap();
        let restored = deserialize_dataframe_from_reader(Cursor::new(bytes), "csv").unwrap();
        assert_eq!(restored.height(), 2);
        assert_eq!(restored.width(), 2);
        assert_eq!(
            restored.column("a").unwrap().get(0).unwrap().to_string(),
            "1"
        );
    }

    #[test]
    fn test_csv_roundtrip_pipe() {
        let df = df!["x" => &[10, 11]].unwrap();
        let mut bytes = Vec::new();
        serialize_dataframe_to_writer(&df, "csv_pipe", &mut bytes).unwrap();
        let restored = deserialize_dataframe_from_reader(Cursor::new(bytes), "csv_pipe").unwrap();
        assert_eq!(restored.height(), 2);
        assert_eq!(restored.width(), 1);
        assert_eq!(
            restored.column("x").unwrap().get(1).unwrap().to_string(),
            "11"
        );
    }

    #[test]
    fn test_parquet_roundtrip() {
        let df = df!["id" => &[7i64, 8i64], "ok" => &[true, false]].unwrap();
        let mut bytes = Vec::new();
        serialize_dataframe_to_writer(&df, "parquet", &mut bytes).unwrap();
        let restored = deserialize_dataframe_from_reader(Cursor::new(bytes), "parquet").unwrap();
        assert_eq!(restored.height(), 2);
        assert_eq!(restored.width(), 2);
        assert_eq!(
            restored.column("id").unwrap().get(0).unwrap().to_string(),
            "7"
        );
    }

    #[test]
    fn test_xlsx_reports_unsupported() {
        let df = df!["a" => &[1]].unwrap();
        let mut bytes = Vec::new();
        let err = serialize_dataframe_to_writer(&df, "xlsx", &mut bytes).unwrap_err();
        assert!(err.to_string().contains("XLSX serialization"));
    }
}
