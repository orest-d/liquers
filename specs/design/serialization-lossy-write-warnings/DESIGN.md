---
id: SERIALIZATION-LOSSY-WRITE-WARNINGS
kind: design
title: A lossy serialization is recorded as a warning in the asset's metadata log
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, core/assets]
issues: [SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE]
created: 2026-10-19
---
# A lossy serialization is recorded as a warning in the asset's metadata log

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

A write that succeeds but changes the value must leave a trace. Serialization gets a side channel
that says what a given format would lose, and every asset path that serializes records it as a
`Warning` log entry, so the loss is visible in the stored metadata, the asset info and the UI.

### Problem Example

Core `Value` (`liquers-core/src/value.rs`, `impl DefaultValueSerializer for Value`, the `"json"`
arm) writes a non-finite float nested in an untagged `Array` / `Object` as `null`, by maintainer
decision (`design/value-to-json-non-finite-float/`):

```rust
// recipe lossy/x.json  <-  query `nan_array/x.json`, where nan_array returns
let v = Value::Array(vec![Value::F64(1.0), Value::F64(f64::NAN)]);
v.as_bytes("json")?;                  // b"[1.0,null]" — Ok, nothing else
```

Today: the keyed asset is persisted as `[1.0,null]`; `store.get_metadata("lossy/x.json")` has no
log entry about it; after a restart the fast track reads `[1.0, None]`. Whether a user sees NaN or
`None` depends on whether the asset came from the cache, and nothing says why.
Should be: the stored and the live metadata both carry one `Warning` such as
`Lossy serialization to json: 1 non-finite float nested in an array or object was written as null
and reads back as none`.

Only core `Value` is lossy this way at HEAD: `SimpleValue`, and so `liquers_lib::value::Value`,
writes tagged JSON, lossless at any depth (`SERIALIZATION_GUIDE.md` §Non-finite floats). Core
`Value` serves every environment built on `liquers-core` alone.

### Scope and Acceptance Criteria

- **AC-1** Lossy keyed evaluation is logged and stored
  WHEN a keyed recipe whose value is a core `Value::Array` holding a NaN is evaluated and persisted
  as `json`
  THEN the asset's metadata and the stored metadata each hold exactly one `Warning` entry naming
  `json` and the non-finite float
- **AC-2** Lossless writes add nothing
  WHEN a value without nested non-finite floats, or a top-level `Value::F64(NaN)` (written as the
  lossless `"NaN"` token), is persisted as `json`
  THEN no serialization warning is logged
- **AC-3** `set_state` reports on both managers
  WHEN `AssetManager::set_state` installs a lossy core `Value` on `DefaultAssetManager` and on
  `ImmediateAssetManager`
  THEN the stored and the live metadata carry the warning, and the version is the content hash of
  the bytes that were stored
- **AC-4** Re-serialization does not duplicate
  WHEN the same asset is serialized again (`get_binary` after its cached binary is cleared, or a
  second persist)
  THEN its log still holds one copy of the warning
- **AC-5** The value type decides, and combined values forward
  WHEN `serialization_warnings(format)` is asked of a type that does not override it, of a
  `CombinedValue` whose half overrides it, and of core `Value` for a format other than `json`
  THEN they return empty, that half's messages, and empty respectively
- **AC-6** Serving reports too
  WHEN `get_binary` or `get_binary_any_status` serializes a lossy value that has no cached binary
  THEN the warning is added to the asset's metadata and to the metadata returned with the bytes

Non-goals: changing what is written (the `null` decision stands); formats lossy by nature and
chosen knowingly (JPEG); `ExtValue` overrides (none is silently lossy at HEAD: polars JSON is
unimplemented, the records JSON writer refuses non-finite floats); the streaming redesign.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` default method to the `DefaultValueSerializer`
  trait (rule 4) and touches `liquers-core` and `liquers-lib` (rule 6).
- **Leading issue:** **Open design - interim or redesign:** the maintainer's preferred home for
  this is the serialization redesign (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, `L`,
  no design yet), with this method shape as the interim fallback "only if that redesign is far off".
- **Explanation:** additive (a defaulted trait method, logging at five `assets.rs` sites, each
  re-read at HEAD with the metadata in hand). Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - interim or redesign:** implement this now, or wait for the redesign?
     Recommended: now. The redesign has no design, and the method survives it: the sketched
     `SerializedBytes` would take its warnings from `serialization_warnings` for every in-memory
     value, and only a lazy value would add warnings found mid-stream.
  2. **Proposed resolution - where the loss is recorded:** as `LogEntry::warning` in the metadata
     log, not as a new structured `MetadataRecord` field. The log is already stored, shown in the
     asset info and in the UI; a structured flag can follow if a consumer needs to filter on it.
  3. **Proposed resolution - which serializations report:** every serialization whose bytes the
     asset keeps or stores (persistence, `set_state`, and the serving reads that cache bytes), not
     only store writes, so a served lossy byte form is explained too. Deduplicated (AC-4).

### Design Dependencies

- `value-to-json-non-finite-float` — **requires** (complete): made the `null` decision and named
  this issue as its companion.
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue, no design) — **overlaps** (T2, the
  serialization contract): its streamed form would reuse this method as the warning source for
  in-memory values. No order; recorded in that issue's 2026-10-13 update.
- `value-serialization-writer` — **overlaps**, weak: no T1-T4 holds, because the warnings are a
  separate method, not part of the writer's result. No order.
- `serialized-binary-disposal-policy` — **overlaps**, weak: both edit `serialize_to_binary` and the
  `finalize_status_with_version` region; disposal makes AC-4's re-serialization common. Either
  order; whichever lands second rebases.

### Consolidated Findings

- The loss exists only for core `Value`; liquers-lib values are lossless. The trait method still
  belongs on `DefaultValueSerializer`, because `CombinedValue` and extensions must be able to report.
- `serde_json` writes `null` silently, so any design must scan the value; a separate scan method
  costs the same and leaves `as_bytes` unchanged.
- Both managers' `set_state` serialize the value twice (once for the version, once for the store
  write). This design serializes once and reuses the bytes for version, warnings and write (same
  change site, so folded in here rather than filed).
- `serialize_to_binary` returns the *state's* metadata: the warning goes into both (AC-6).

## Phase 2: Architecture

### Solution

A defaulted method on `DefaultValueSerializer`:

```rust
/// What `as_bytes(data_format)` would lose, one message per kind of loss; empty when the
/// write is lossless or the format is not supported. Must not fail and must not serialize.
fn serialization_warnings(&self, data_format: &str) -> Vec<String> {
    let _ = data_format;
    Vec::new()
}
```

- **Core `Value`** overrides it: for `"json"`, a private recursive count of non-finite `F64` inside
  `Array` / `Object` (a top-level `F64` is exempt: written as a token); one message when the count
  is non-zero. Every other format returns empty.
- **`CombinedValue<B, E>`** (`liquers-lib/src/value/extended.rs`) forwards to the half it holds,
  as its `as_bytes` does. Without this an extension's override would be invisible.
- **Asset layer** (`liquers-core/src/assets.rs`): a private helper
  `fn add_serialization_warnings(metadata: &mut Metadata, data_format: &str, warnings: &[String])`
  appends `LogEntry::warning(format!("Lossy serialization to {data_format}: {w}"))` for each
  message not already present in the log (AC-4; the log is short, a linear scan suffices). Called at:
  1. `AssetRef::prepare_version` → new field `PreparedVersion.warnings: Vec<String>`, filled when
     `as_bytes` succeeded; `finalize_status_with_version` applies it under the lock it already holds,
     before `save_to_store` snapshots the metadata (AC-1).
  2. `AssetRef::serialize_to_binary` (persistence fallback and `get_binary`) — to `lock.metadata`
     under the write lock it already takes, and to the metadata it returns (AC-1, AC-6).
  3. `AssetRef::get_binary_any_status` serialization branch — the same (AC-6).
  4. `DefaultAssetManager::set_state` and 5. `ImmediateAssetManager::set_state` — serialize once
     before step 4 (version), keep `Result<Vec<u8>, Error>`, add the warnings to `metadata`, and reuse
     the bytes for the store write in step 8 instead of calling `state.as_bytes()` again (AC-3).
- The data format at every site is `State::effective_data_format()`, the one `State::as_bytes` uses.

Rejected: `as_bytes` returning `(bytes, warnings)` (breaks every implementor for one case); a new
`serialize` method returning a `Serialized` struct (the only lossy case needs a scan anyway);
refusing the write (reverses the maintainer decision); a structured `lossy` field (Q2).

Known-issue preflight: `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` overlaps (not a
blocker); `PY-VALUE-SERIALIZER-IS-A-STUB` inherits the default. Nothing open blocks.

### Changes

- `liquers-core/src/value.rs`: `DefaultValueSerializer::serialization_warnings` (default); override
  in `impl DefaultValueSerializer for Value`; private `count_nested_non_finite(&Value) -> usize`;
  doc comment of `as_bytes` points at the method.
- `liquers-core/src/assets.rs`: `PreparedVersion` (+`warnings`), `prepare_version`,
  `finalize_status_with_version`, `serialize_to_binary`, `get_binary_any_status`, both `set_state`
  bodies, new private `add_serialization_warnings`.
- `liquers-lib/src/value/extended.rs`: `impl DefaultValueSerializer for CombinedValue<B, E>` forwards.
- No command, registry, store, HTTP or binding change. Sync, no locks added (each site already
  holds the asset write lock or owns the metadata).
- Documents: `specs/guides/SERIALIZATION_GUIDE.md` (§"Why nested core floats become null": the loss
  is now logged; History row, `reviewed:`), `specs/reference/ASSETS.md` (binary/serialization: the
  warning entry; History row, `reviewed:`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A warning added after `save_to_store` snapshots metadata is not stored | added in `finalize_status_with_version`, which runs before persistence; AC-1 reads the store | high |
| Duplicate entries on repeated serialization | dedupe by message (AC-4) | high |
| `CombinedValue` drops an extension's warnings | explicit forwarding; AC-5 | high |
| One serialization in `set_state` changes behaviour | same bytes for a deterministic encoder; otherwise the version now matches the stored bytes | high |

Recovery: one commit per step.

## Phase 3: Examples and Tests

### Examples

```rust
let v = Value::Array(vec![Value::F64(1.0), Value::F64(f64::NAN)]);
assert_eq!(v.serialization_warnings("json").len(), 1);
assert!(Value::F64(f64::NAN).serialization_warnings("json").is_empty());   // token, lossless
assert!(v.serialization_warnings("txt").is_empty());                       // not writable: as_bytes errs
```

### Tests

Unit, `liquers-core/src/value.rs` `mod tests`:
- `value_serialization_warnings_reports_nested_non_finite_json` — Array, Object, nested two deep, ±∞: AC-5
- `value_serialization_warnings_empty_when_lossless` — top-level NaN, finite floats, `txt`/`md`/`bin`: AC-2, AC-5

New file `liquers-core/tests/lossy_serialization_warning.rs` (`SimpleEnvironment<Value>`, an
`AsyncMemoryStore` seeded with `recipes.yaml` declaring `lossy/x.json` ← `nan_array/x.json` and
`clean/x.json` ← `finite_array/x.json`; both commands registered with `register_command!`):
- `lossy01_keyed_evaluation_logs_and_stores_warning` — `get()`, then live metadata and `store.get_metadata`: AC-1
- `lossy02_lossless_value_logs_no_warning` — `clean/x.json`, and a top-level NaN via `set_state`: AC-2
- `lossy03_set_state_logs_on_default_manager` — version equals `Version::from_content(stored bytes)`: AC-3
- `lossy04_set_state_logs_on_immediate_manager` — `EnvironmentBuilder` inline mode: AC-3
- `lossy05_reserialization_does_not_duplicate_warning` — `set_state` logs the warning and installs the asset with no cached binary; `get_binary` then serializes again; the log holds one copy: AC-4
- `lossy06_get_binary_returns_metadata_with_warning` — non-keyed query `nan_array`, `get_binary` metadata log: AC-6

Unit, `liquers-lib/src/value/extended.rs` `mod tests` (beside `ScalarExtension`):
- `combined_value_forwards_serialization_warnings` — a test extension reporting one message; base `SimpleValue` reports none: AC-5

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test lossy_serialization_warning`,
`cargo test -p liquers-core --lib value::`, `cargo test -p liquers-lib --lib value::extended`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/value.rs` `DefaultValueSerializer` — add `serialization_warnings` (default) and the `Value` override with `count_nested_non_finite`; the two unit tests — `cargo test -p liquers-core --lib value::`. Rollback: revert the commit.
- [ ] 2. `liquers-lib/src/value/extended.rs` `CombinedValue` — forward; `combined_value_forwards_serialization_warnings` — `cargo test -p liquers-lib --lib value::extended`. Rollback: revert.
- [ ] 3. `liquers-core/tests/lossy_serialization_warning.rs` — add the six tests; lossy01, 03-06 fail at HEAD — `cargo test -p liquers-core --test lossy_serialization_warning`. Rollback: delete the file.
- [ ] 4. `liquers-core/src/assets.rs` — `add_serialization_warnings`; `PreparedVersion.warnings`, `prepare_version`, `finalize_status_with_version` — lossy01, lossy02. Rollback: revert.
- [ ] 5. `assets.rs` `serialize_to_binary`, `get_binary_any_status` — log to `lock.metadata` and the returned metadata — lossy05, lossy06.
- [ ] 6. `assets.rs` `DefaultAssetManager::set_state`, `ImmediateAssetManager::set_state` — serialize once; warnings into `metadata`; reuse bytes for the write — lossy03, lossy04; `cargo test -p liquers-core --lib --tests`.
- [ ] 7. Docs: `SERIALIZATION_GUIDE.md`, `ASSETS.md` (History rows, `reviewed:`); close `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE` with a resolution note; add a line to `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` naming the method as its in-memory warning source — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (the trait gains a method every implementor inherits, wasm rows
included); `python3 scripts/docs_index.py --check`; diff review for debug output and edits outside
`value.rs`, `assets.rs`, `extended.rs` and the named documents.
