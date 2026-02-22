use liquers_core::value::DefaultValueSerializer;
use liquers_lib::value::{ExtValueInterface, Value};
use polars::df;

#[test]
fn polars_value_csv_roundtrip() {
    let df = df!["a" => &[1i32, 2i32], "b" => &["x", "y"]].unwrap();
    let value = Value::from_polars_dataframe(df);

    let bytes = value.as_bytes("csv").unwrap();
    let decoded = Value::deserialize_from_bytes(&bytes, "polars_dataframe", "csv").unwrap();
    let decoded_df = decoded.as_polars_dataframe().unwrap();

    assert_eq!(decoded_df.height(), 2);
    assert_eq!(decoded_df.width(), 2);
}

#[test]
fn polars_value_parquet_roundtrip() {
    let df = df!["id" => &[1i64, 2i64, 3i64]].unwrap();
    let value = Value::from_polars_dataframe(df);

    let bytes = value.as_bytes("parquet").unwrap();
    let decoded = Value::deserialize_from_bytes(&bytes, "polars_dataframe", "parquet").unwrap();
    let decoded_df = decoded.as_polars_dataframe().unwrap();

    assert_eq!(decoded_df.height(), 3);
    assert_eq!(decoded_df.width(), 1);
}

#[test]
fn polars_value_xlsx_returns_error() {
    let df = df!["id" => &[1i32]].unwrap();
    let value = Value::from_polars_dataframe(df);
    let err = value.as_bytes("xlsx").unwrap_err();
    assert!(err.to_string().contains("XLSX serialization"));
}
