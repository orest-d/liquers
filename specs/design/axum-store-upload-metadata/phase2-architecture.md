# Phase 2: Solution and Architecture

## Legacy metadata

`get_metadata_handler`: replace the `metadata_record()` branch pair with
`ApiResponse::ok(metadata_json(&metadata), "Metadata retrieved successfully")`.
`get_entry_handler`: if it builds the entry's metadata JSON locally, use `metadata_json` (or call
`entry_response` directly, as the Assets API does).

## Upload

In `upload_handler`, per part (the multipart field gives `file_name()` and `content_type()`):

```rust
let mut record = MetadataRecord::new();
record.with_filename(file_name.clone());
if let Some(declared) = declared_media_type(&file_name, field_content_type.as_deref()) {
    record.with_media_type(declared);
}
let metadata = Metadata::MetadataRecord(record);

/// The media type an upload declares: the part's `Content-Type` when the extension does not
/// already imply it (level model: `media_type` is an override, not a cache of the derived value).
fn declared_media_type(file_name: &str, content_type: Option<&str>) -> Option<String>
```

`declared_media_type` returns `None` for an absent, empty, or `application/octet-stream` type, or
for one equal to `file_extension_to_media_type(ext)`. Otherwise it returns `Some`. Capture the
part's content type before consuming the field's bytes.

## Alternatives

Always declare the part's content type. Rejected: it makes every upload an override and
contradicts the level model.

## Known-issue preflight

None blocking.

## Relevant commands

None.

## Documentation architecture

Spec rows (three), History, `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `store/handlers.rs`; `tests/store_api_routes.rs`; spec |
| Existing tests | The upload test may assert default metadata. Update it. |
| Data | Uploads gain `filename` and sometimes a declared `media_type` |
| Security | Client-declared content types are stored as declared metadata, which is fine for metadata. They are served back as `Content-Type` by the binary routes, which already trust stored metadata. Note it in the spec. |
| Recovery | Revert |
| Certainty | High once decided |
