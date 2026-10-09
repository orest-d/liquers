# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — needs-decision (rule 5): the upload half sets the stored
  `media_type` contract. Once decided it is a bug fix in `liquers-axum` and would be eligible
- **Leading issue:** **Open design question — what an upload declares as `media_type`.** Under the
  metadata level model, `MetadataRecord.media_type` holds only a *declared override*. The effective
  media type is derived from the data format (`MetadataRecord::get_media_type`). Writing the
  extension's media type would make every upload look like an override.
- **Explanation:** The legacy-metadata half is unambiguous (serve the document as stored, as the
  Assets API's `metadata_json` does). The upload half is specified around the recommended rule.
- **Open questions:**
  1. **Proposed resolution — declare only what the extension cannot tell.** For each multipart
     part: let `derived = file_extension_to_media_type(ext)`. If the part's `Content-Type` is
     present, not `application/octet-stream`, and differs from `derived`, declare it
     (`with_media_type(content_type)`). Otherwise declare nothing, and the effective type comes
     from the filename/data format. Always set `filename` (the part's file name), so the data
     format is derivable on every store.

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
  wrap it in `Metadata::MetadataRecord`. Check the `filename` setter name on `MetadataRecord`
  (`with_filename` or `set_filename`).
- Reuse `crate::assets::common::metadata_json` in both metadata-serving handlers instead of the
  local `metadata_record()` match. That also removes duplicated code.
- `put_data_handler`'s `get_metadata().unwrap_or_else(Metadata::new)` is not in scope.
