---
id: AXUM-STORE-UPLOAD-AND-METADATA-DROP-INFORMATION
kind: issue
title: Store API uploads carry no media type, and legacy metadata is served as an empty object
status: closed
priority: P3
complexity: S
area: [axum]
design: axum-store-upload-metadata
created: 2026-09-28
github:
---
## Problem

In `liquers-axum/src/store/handlers.rs`:

- `upload_handler` (`POST {store}/upload/{*key}`) computes a media type from each file's extension
  and then discards it (`metadata = Metadata::new()` in both branches; a comment says the setter is
  not reachable on the `Metadata` enum). Every uploaded file is stored with default metadata, so its
  media type is whatever the store derives later, and the multipart part's own `Content-Type` is
  never consulted.
- `get_metadata_handler` and `get_entry_handler` serve a `Metadata::LegacyMetadata` document as
  `{}` instead of the document itself (the Assets API's `metadata_json` returns it as is).

## Expected behaviour

An upload records the media type it knows (the part's `Content-Type`, else the extension's), and
legacy metadata is returned as stored.

## Discovery

Found 2026-09-28 during the `WEB_API_SPECIFICATION.md` audit of `specs/design/axum-assets-endpoints/`
(Step 13); the specification now documents the current behaviour.

## Resolution (2026-10-10)

Maintainer decision: an upload declares only what the extension cannot tell. `upload_handler`
(`liquers-axum/src/store/handlers.rs`) stores a `MetadataRecord` with the part's `filename`, and
declares its `Content-Type` only when it is present, not `application/octet-stream`, and differs
from the type the filename implies (`declared_media_type`). `get_metadata_handler` and
`get_entry_handler` serve legacy metadata as stored via `assets::common::metadata_json`. Tests:
`store::handlers::tests` (3) and `sar17`–`sar20` in `tests/store_api_routes.rs`.
`WEB_API_SPECIFICATION.md` updated. Design: `design/axum-store-upload-metadata/`.
