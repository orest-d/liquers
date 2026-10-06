# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — workspace-wide `serde_json/preserve_order`, or an
  order-capturing parse local to `liquers-records`.** The first is a one-line change with
  workspace-wide effects. The second is contained.
- **Explanation:** A risk found while designing makes the choice consequential:
  `CommandMetadataRegistry::calculate_metadata_version` hashes `serde_json::to_vec` of command
  metadata, which contains `serde_json::Map` fields (`ArgumentInfo::hints`). With
  `preserve_order`, those maps serialize in insertion order instead of sorted order. Any command
  with two or more hints could change its `metadata_version`, and other hashed or compared JSON
  could change too. The design specifies the local alternative.
- **Open questions:**
  1. **Proposed resolution — local ordered parse.** `liquers-records` parses JSON tables through a
     small `Deserialize` impl that records object key order (`OrderedObject(Vec<(String,
     serde_json::Value)>)`) for the levels that carry column names, and leaves `serde_json` as is
     for the rest of the workspace.

## Problem

`liquers-records`' JSON readers take a `serde_json::Value`, whose `Map` is a `BTreeMap` without
`preserve_order`. A table read from NDJSON, `json`, `records`, `columns` or `index` shapes without
a declared schema gets alphabetical columns (`{"name", "age"}` → `age, name`). Row order is
unaffected. A declared schema already fixes the order.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. NDJSON `{"name":"a","age":1}` read without a schema has columns `name, age`.
2. `records` shape: column order is the order of first appearance across rows (keys only in a
   later row are appended).
3. `columns` shape (`{"name": {…}, "age": {…}}`): top-level key order.
4. `index` shape: inner key order of the first row, then first-appearance.
5. With a declared schema: unchanged (schema order).
6. `serde_json` features unchanged: `Cargo.lock` shows no new `indexmap` feature for
   `serde_json`, and `metadata_version`s are unchanged (registry export test passes).

## Scope

JSON table readers in `liquers-records/src/formats/` (`ndjson.rs`, `shapes.rs`, the `json`
reader). Writers already emit schema order.

## Design Dependencies

None.

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, JSON shapes: "without a schema, columns follow
  the file's key order (first appearance)".

## Consolidated Findings

- The readers' public signatures take bytes at the format boundary (`read_table`). Only internal
  helpers take `serde_json::Value`. Changing those helpers to the ordered representation is
  internal. Phase 4 step 1 confirms no public `fn(…: serde_json::Value)` in `liquers-records`
  (else keep it and add a bytes variant).
- An ordered object only needs the key order. Cell values can stay `serde_json::Value`, because
  nested objects in cells are not columns.
