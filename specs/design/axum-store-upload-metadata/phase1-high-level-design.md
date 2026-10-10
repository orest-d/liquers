# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in `liquers-axum/src/store/handlers.rs` restoring the
  intended Store API behaviour; no new structure, no `pub` change in core/store/lib/records, no
  route or command change, one crate
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-10): an upload declares only what the
  extension cannot tell. For each multipart part: let `derived = file_extension_to_media_type(ext)`.
  If the part's `Content-Type` is present, not `application/octet-stream`, and differs from
  `derived`, declare it (`with_media_type(content_type)`); otherwise declare nothing, and the
  effective type comes from the filename/data format. Always set `filename`. The legacy-metadata
  half serves the document as stored, as the Assets API's `metadata_json` does.
- **Open questions:** None.

## Problem

`liquers-axum/src/store/handlers.rs`:

- `upload_handler` computes a media type from the extension and discards it (`Metadata::new()` in
  both branches; a comment says the setter is not reachable on the enum). It never reads the
  part's `Content-Type`.
- `get_metadata_handler` and `get_entry_handler` serve `Metadata::LegacyMetadata` as `{}`.

## Expected behaviour and acceptance

1. Upload `report.csv` with `Content-Type: text/csv`: stored metadata has `filename:
   "report.csv"`, no declared `media_type`, and the effective media type is `text/csv`.
2. Upload `data.bin` with `Content-Type: image/png`: declared `media_type: "image/png"`.
3. Upload `notes.txt` with `application/octet-stream` (a browser default for unknown types): no
   declaration.
4. `GET {store}/metadata/{key}` for a legacy document returns the document unchanged (not `{}`).
   The same holds for `entry`'s `metadata`.

## Scope

Store API only. The Assets API already serves legacy metadata correctly.

## Design Dependencies

- `axum-assets-endpoints` — **overlaps** (audit origin; owns `metadata_json`).
- `js-store-effective-media-type` — **overlaps** (same level-model reasoning on the browser side).

## Documentation assessment

- Reference: `WEB_API_SPECIFICATION.md`, Store API rows for `metadata`, `entry`, `upload`.

## Consolidated Findings

- `Metadata` is an enum. Build a `MetadataRecord`, call `with_filename`/`with_media_type`, and
  wrap it in `Metadata::MetadataRecord`. The setters are
  `MetadataRecord::with_filename(String)` and `with_media_type(String)` (`liquers-core/src/metadata.rs`).
- Reuse `crate::assets::common::metadata_json` in both metadata-serving handlers instead of the
  local `metadata_record()` match. That also removes duplicated code.
- `put_data_handler`'s `get_metadata().unwrap_or_else(Metadata::new)` is not in scope.
