//! A `Text` value is stored under a `.md` key and read back as markdown text.
//!
//! Before `md` was declared on `Text`, `set_state` refused the write in hard metadata validation.
//! Design: `specs/design/text-value-markdown-format/`.

use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    state::State,
    store::AsyncMemoryStore,
    value::Value,
};

fn simple() -> EnvRef<SimpleEnvironment<Value>> {
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

#[tokio::test]
async fn text_value_is_stored_and_read_as_markdown() -> Result<(), Error> {
    let envref = simple();
    let key = parse_key("notes/today.md")?;
    let text = "# Notes\n- one\n";

    let mut record = MetadataRecord::new();
    record.with_type_identifier("Text".to_owned());
    record.with_type_name("text".to_owned());
    record.set_filename("today.md");
    record.with_status(Status::Ready);
    let state = State::from_value_and_metadata(
        Value::from(text),
        Arc::new(Metadata::MetadataRecord(record)),
    );
    envref.get_asset_manager().set_state(&key, state).await?;

    let (bytes, metadata) = envref.get_async_store().get(&key).await?;
    assert_eq!(bytes, text.as_bytes());
    assert_eq!(metadata.get_data_format(), "md");
    assert_eq!(metadata.get_media_type(), "text/markdown");

    let back = envref.get_asset_manager().get(&key).await?.get().await?;
    assert_eq!(back.try_into_string()?, text);
    assert_eq!(back.metadata.get_data_format(), "md");
    Ok(())
}
