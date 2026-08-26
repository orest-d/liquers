//! `vts8` — the write-path tiers, through a real asset manager.
//!
//! The hard tier refuses what would make a stored value unreadable; the soft tier records what is
//! merely worth knowing. Splitting them is what lets a deliberate media-type override survive a
//! rule that otherwise rejects divergence.
//!
//! See `specs/design/value-type-system/` and `specs/issues/CORE-METADATA-FORMAT-TYPE-CONSISTENCY.md`.

use liquers_core::{
    assets::AssetManager,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    context::{Environment, SimpleEnvironment},
    error::ErrorType,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    value::{Value, ValueInterface},
};

fn environment() -> SimpleEnvironment<Value> {
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env
}

fn record(key: &Key, type_identifier: &str) -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier(type_identifier.to_owned())
        .with_type_name(type_identifier.to_lowercase())
        .with_status(Status::Source);
    record
}

/// `vts8.1` — consistent metadata is stored, and the format it names is the one used.
#[tokio::test]
async fn set_accepts_a_consistent_value() -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let key = parse_key("test/notes.txt")?;
    let mut metadata = record(&key, "Text");
    metadata.data_format = Some("txt".to_owned());

    let envref = env.to_ref();
    envref
        .get_asset_manager()
        .set_binary(&key, b"hello", metadata)
        .await?;
    Ok(())
}

/// `vts8.2` — the P0 itself: a format the type cannot be written in is refused, and the message
/// names the type, the format and what the type does support.
#[tokio::test]
async fn set_rejects_an_unsupported_format() -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let key = parse_key("test/notes.txt")?;
    let mut metadata = record(&key, "Text");
    metadata.data_format = Some("parquet".to_owned());

    let envref = env.to_ref();
    let error = envref
        .get_asset_manager()
        .set_binary(&key, b"hello", metadata)
        .await
        .expect_err("an unsupported format must be refused");

    assert_eq!(error.error_type, ErrorType::SerializationError);
    assert!(error.message.contains("Text"), "names the type: {error}");
    assert!(error.message.contains("parquet"), "names the format: {error}");
    assert!(
        error.message.contains("txt"),
        "names what the type supports: {error}"
    );
    Ok(())
}

/// `vts8.3` — an identifier this build does not know is refused by name.
#[tokio::test]
async fn set_rejects_an_unregistered_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let key = parse_key("test/frame.csv")?;
    let metadata = record(&key, "polars.DataFrame");

    let envref = env.to_ref();
    let error = envref
        .get_asset_manager()
        .set_binary(&key, b"a,b", metadata)
        .await
        .expect_err("an unregistered identifier must be refused");

    assert_eq!(error.error_type, ErrorType::General);
    assert!(
        error.message.contains("polars.DataFrame"),
        "names the identifier: {error}"
    );
    Ok(())
}

/// `vts8.4` — a declared media-type override survives the reject rule and is stored verbatim.
///
/// This is the case `liquers-web`'s remote fetch depends on: an origin server's `Content-Type`
/// that the extension does not imply. Promoting the soft warning to an error would break it.
#[tokio::test]
async fn a_declared_media_type_override_survives() -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let key = parse_key("test/notes.txt")?;
    let mut metadata = record(&key, "Text");
    metadata.data_format = Some("txt".to_owned());
    metadata.with_media_type("application/x-custom".to_owned());

    let envref = env.to_ref();
    envref
        .get_asset_manager()
        .set_binary(&key, b"hello", metadata)
        .await?;

    let (_binary, stored) = envref.get_async_store().get(&key).await?;
    assert_eq!(stored.get_media_type(), "application/x-custom");
    Ok(())
}

/// `vts8.6` — an error state is storable even though its format contradicts its identifier.
///
/// An errored asset keeps the intended output's filename, so its effective format is `csv` while
/// its value — and therefore its type — is none. Its bytes are not a serialization of that type,
/// so the format check does not apply; the identifier check still does, and the none type is
/// registered like any other.
///
/// There is no `error` identifier. The type axis reports what is *available*, not what was
/// intended, and a failure is recorded in `is_error`/`Status::Error` instead.
#[tokio::test]
async fn error_state_with_a_mismatched_filename_is_storable(
) -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let key = parse_key("test/report.csv")?;
    let mut metadata = record(&key, "None");
    metadata.with_status(Status::Error);
    metadata.with_error_message("the recipe failed".to_owned());
    metadata.data_format = Some("csv".to_owned());

    let envref = env.to_ref();
    envref
        .get_asset_manager()
        .set_binary(&key, b"", metadata)
        .await?;
    Ok(())
}

/// `vts8.7` — a media-type override that could inject a header is refused before the store.
#[tokio::test]
async fn malformed_media_type_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let key = parse_key("test/notes.txt")?;
    let envref = env.to_ref();

    for bad in ["text/plain\r\nX-Injected: 1", "notamediatype", "text/"] {
        let mut metadata = record(&key, "Text");
        metadata.data_format = Some("txt".to_owned());
        metadata.with_media_type(bad.to_owned());

        let error = envref
            .get_asset_manager()
            .set_binary(&key, b"hello", metadata)
            .await
            .expect_err(&format!("{bad:?} must be refused"));
        assert_eq!(error.error_type, ErrorType::General);
    }
    Ok(())
}

/// An ordinary state that declares no data format is storable.
///
/// Regression test for a defect found in review of PR #37: hard validation resolved the format
/// through `Metadata::get_data_format()`, whose level-1 slot is the constant `bin` because metadata
/// cannot see a value — so `State::new().with_data(Value::I32(1))` was rejected as "cannot be
/// serialized as 'bin'" even though serialization would correctly have used `json`. Only a
/// *declared* format can be inconsistent; an absent one means the value's own default applies, and
/// a type always supports its own default.
#[tokio::test]
async fn state_with_no_declared_format_is_storable() -> Result<(), Box<dyn std::error::Error>> {
    let env = environment();
    let envref = env.to_ref();

    for value in [
        Value::I32(1),
        Value::Text("hello".to_string()),
        Value::Bool(true),
        Value::None,
    ] {
        let identifier = value.identifier().to_string();
        let key = parse_key(&format!("test/{identifier}"))?;
        let state = State::<Value>::new().with_data(value);
        assert_eq!(
            state.metadata.declared_data_format(),
            None,
            "the premise: nothing declared a format"
        );
        envref
            .get_asset_manager()
            .set_state(&key, state)
            .await
            .unwrap_or_else(|e| panic!("{identifier} with no declared format must store: {e}"));
    }
    Ok(())
}
