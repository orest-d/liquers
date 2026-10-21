# Phase 1: High-Level Design — value-summary

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` struct and serialized `pub` fields to
  `MetadataRecord` and `AssetInfo`, and a trait method in `liquers-core` (rules 1, 3, 4, 6).
- **Leading issue:** **Open design - model.** The feature (written before `value-type-system`)
  proposes `kind`, `summary`, `properties`, `schema_version`; at HEAD `kind` duplicates
  `type_identifier` and `schema_version` duplicates serde's tolerant defaults.
- **Explanation:** a summary computed where metadata is already synced with the value
  (`State::sync_metadata_with_value`) cannot go stale; Phases 3-4 assume the answers below.
- **Open questions:**
  1. **Open design - model:** `ValueSummary { summary: String, properties: BTreeMap<String,
     serde_json::Value> }`; drop `kind` (use `type_identifier`) and `schema_version`.
     Recommended: yes.
  2. **Open design - names:** `ValueSummary` and the field `value_summary`, because `liquers-axum`
     already has a `ValueDescription` (the client-settable metadata subset,
     `liquers-axum/src/assets/value_description.rs`) and `MetadataRecord::description` is a
     human-written text. Recommended: as stated.
  3. **Open design - placement:** a typed field on `MetadataRecord` and `AssetInfo`, not an entry
     in the application attribute map of `metadata-application-attributes`. Recommended: typed —
     Liquers produces and reads it; the attribute map is for facts Liquers does not understand.
  4. **Proposed resolution - cost rule:** a summary may read sizes and schemas but never scan data:
     text gives `bytes` (not `chars`/`lines`, which are O(n) on every state construction), a record
     source gives none (it would need `materialize`).
  5. **Proposed resolution - bindings:** no Python or JavaScript accessor in this design; the field
     reaches both through metadata JSON already.

## Purpose

Let a UI, an API client or an agent show what a value is — "1 200 × 800 RGBA image", "3 rows ×
2 columns: region, amount" — from its metadata, without fetching and decoding the value
(VISION: "Trust the result"; agent workbench — an agent inspecting assets cheaply).

## Problem Example

`GET /liquer/api/assets/metadata/data/sales.csv/-/convert-RecordView` (or `store.get_metadata`)
returns a `MetadataRecord` saying `type_identifier: RecordView`, `type_name: record_view` and a
filename — nothing about its size or columns. To learn that it has 2 rows and the fields
`region, amount`, a client must download and parse the CSV. Should be:

```json
"value_summary": { "summary": "2 rows × 2 columns",
                   "properties": { "rows": 2, "columns": 2, "fields": ["region", "amount"] } }
```

in the same metadata, and in the `AssetInfo` of a directory listing.

## Scope and Acceptance Criteria

- **AC-1** Images carry their size
  WHEN a state holding an `Image` is created
  THEN its metadata's `value_summary.properties` has `width`, `height` and `color_type`
- **AC-2** Tables carry their shape
  WHEN a state holds a `polars.DataFrame` or a `RecordView`
  THEN `properties` has `rows`, `columns` and the ordered `fields`, and the frame also `dtypes`
- **AC-3** Minimal kinds
  WHEN a state holds `Text`, `Bytes`, `Array` or `Object`
  THEN `properties` has `bytes`, `bytes`, `len` and `len` respectively; scalars and `None` have none
- **AC-4** Never stale
  WHEN a command replaces a state's value (`State::with_data`, `from_value_and_metadata`)
  THEN the summary describes the new value, and a value without a summary clears it
- **AC-5** Listings and persistence
  WHEN the value is stored and its key listed with `listdir_asset_info`, or its metadata read back
  THEN the `AssetInfo` and the stored record carry the same summary
- **AC-6** Absent is fine
  WHEN a record without `value_summary` (written before this change) is read
  THEN it loads as a `MetadataRecord` with `value_summary: None`, and a record with none serializes
  without the key
- **AC-7** Extensible
  WHEN a downstream `ValueExtension` or `ForeignValue` overrides `value_summary`
  THEN its properties appear unchanged in the metadata JSON

Non-goals: computed or scanned statistics (min, max, nulls); summaries for record sources; Python
and JavaScript accessors; using the summary for any computation.

## Core Interactions

Value types (a summary hook); State (sync); Metadata (`MetadataRecord`, `AssetInfo`); Web/API and
stores (carry the field through JSON).

## Crate Placement

`ValueSummary`, the fields and the hook in `liquers-core` (metadata and value contract); summaries
of images, frames and records in `liquers-lib` where those types live.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` (new §Value summary).
- Guide: `specs/guides/TYPE_SYSTEM_GUIDE.md` — an optional step "summarize it".
- Other documents: none.
- Documents to update: none further.

## Design Dependencies

- `value-interface-capability-split` — **requires** (soft): the hook joins the same traits; landing
  after it avoids editing `ForeignValue` twice.
- `metadata-application-attributes` — **overlaps**: both add an optional serialized field to
  `MetadataRecord` and `AssetInfo`; independent contracts (question 3).
- `value-type-definition-macro` — **overlaps**: a `summary:` hook key, added by whichever lands second.

## Consolidated Findings

- `sync_metadata_with_value` (`liquers-core/src/state.rs`) is the single place both type fields are
  set from the value; the summary is set beside them, so AC-4 needs no new call site.
- `MetadataRecord` is `deny_unknown_fields`: a record written by this build read by an older build
  falls back to `LegacyMetadata`. Accepted (no backward compatibility); stated in the reference.
- The O(1) rule keeps the sync cheap; text `chars`/`lines` are deliberately out (question 4).

## References

- `specs/issues/VALUE-DESCRIPTION.md`; `liquers-axum/src/assets/value_description.rs` (name clash).
