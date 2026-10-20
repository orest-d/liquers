---
id: SERIALIZED-BINARY-DISPOSAL-POLICY
kind: design
title: A serialized binary is released once the store holds it
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY]
created: 2026-10-19
---
# A serialized binary is released once the store holds it

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

State and implement a policy for the serialized copy a keyed asset caches (`AssetData::binary`):
bytes produced for persistence are released as soon as the store has written them, so a cached
keyed asset no longer holds its value and a full encoding of it for as long as it lives.

### Problem Example

A keyed recipe `big/table.csv` evaluates to a 500 MB DataFrame:

1. `AssetRef::prepare_version` (`liquers-core/src/assets.rs`) serializes it to compute the content
   hash and returns the bytes in `PreparedVersion.binary`;
2. `finalize_status_with_version` stores them in `lock.binary` so that `save_to_store` writes the
   exact bytes the version describes;
3. `save_to_store` writes them; `record_persistence_result` records `Persisted`.

Today: the asset keeps the DataFrame **and** the 500 MB of CSV until the asset is replaced,
invalidated or evicted (`binary` is cleared only by `reset`, the evaluation error branch, the
cancellation path, `set_value` and `set_state`). Nothing ever reads those bytes again unless a
client asks for the binary.
Should be: after step 3 `poll_binary()` is `None` and only the DataFrame stays resident; a later
`get_binary()` serializes again, as it does for any asset without cached bytes.

### Scope and Acceptance Criteria

- **AC-1** Persisted bytes are released
  WHEN a keyed computed asset is evaluated and its persistence status becomes `Persisted`
  THEN `poll_binary()` returns `None`, `get()` still returns the value, and `get_binary()` returns
  bytes equal to the stored bytes
- **AC-2** Bytes the store does not hold are kept
  WHEN the write is skipped (`stored: false`) or fails
  THEN the cached binary is kept, exactly as at HEAD
- **AC-3** Loaded bytes are kept
  WHEN a keyed asset is loaded by the fast track, and then made `Override` with
  `AssetManager::to_override`, which persists it
  THEN its cached binary is still present
- **AC-4** Only the written bytes are released
  WHEN the asset's cached binary was replaced by other bytes between the write and the release
  THEN the newer bytes are kept
- **AC-5** Readers still get bytes
  WHEN `get_binary` or `get_binary_any_status` is called after the release
  THEN it serializes from the value and returns, as for any asset with no cached binary
- **AC-6** Both managers
  WHEN AC-1 runs with `DefaultAssetManager` (background persistence) and in `EvalMode::Inline`
  THEN the outcome is the same

Non-goals: a byte budget or eviction order; a per-recipe or per-type retention declaration;
releasing bytes a reader cached (a served asset keeps its bytes, as today); bytes of non-keyed
query assets; re-reading released bytes from the store instead of re-serializing.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new memory policy, not a fix restoring documented
  behaviour (rule 2), with an open policy decision (rule 5).
- **Leading issue:** **Open design - policy:** which of the issue's three policies to adopt.
- **Explanation:** the recommended policy is a local change in `assets.rs` with no public API
  change; every set and clear site of `AssetData::binary` was inventoried at HEAD. Phases 3-4
  assume the recommended answers.
- **Open questions:**
  1. **Open design - policy:** drop after a successful persist; keep under a per-manager byte
     budget; or keep by a policy declared on the recipe or type. Recommended: drop after a
     successful persist, with no configuration. It is correct whenever the store is the cache, needs
     no size accounting, and costs only a re-serialization for a reader who later wants bytes. A
     budget can be added later without changing this rule.
  2. **Proposed resolution - reader caching stays:** `get_binary` and `get_binary_any_status` keep
     caching the bytes they serialize, so a repeatedly served asset keeps them (memory for CPU, as
     today). Releasing those too would re-serialize on every HTTP request.
  3. **Proposed resolution - loaded bytes stay (AC-3):** bytes loaded from the store by the fast
     track are not reproducible in general: a hand-placed CSV read as `Bytes` cannot be serialized
     back to `csv` (`SimpleValue::as_bytes` refuses), and a non-deterministic encoder would differ.
     Only bytes this process derived from the value are released.

### Design Dependencies

- `keyed-expiry-cascade-fix` — origin (complete, E4): made the persisted bytes and the version come
  from one serialization, and scoped this policy out.
- `serialization-lossy-write-warnings` — **overlaps**, weak: both edit `serialize_to_binary` and
  the finalize path; releasing makes its re-serialization deduplication (its AC-4) matter. Either
  order.
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue) — **overlaps**, weak: a streamed
  encoding is never cached (its question 6); this policy covers the whole-value case. No order.
- `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` (issue) — **overlaps**, weak: the Query API copies
  the cached `Arc` on every response; unrelated to retention. No order.

### Consolidated Findings

- Release must be tied to *those* bytes: background persistence runs in a spawned task, and the
  asset may have been re-evaluated meanwhile, so the release compares `Arc::ptr_eq` under the
  write lock (AC-4).
- The version stays valid: it describes the bytes the store holds. A non-deterministic encoder may
  serve re-serialized bytes that hash differently; nothing compares a served binary to the version.
- `to_override` can persist a fast-track-loaded asset, which is why provenance is tracked (AC-3)
  rather than releasing whatever was written.
- A served asset re-caches its bytes on the first `get_binary`, by decision (Q2); the gain is for
  the common case of keyed intermediates that are consumed as values, never as bytes.

## Phase 2: Architecture

### Solution

- **Provenance.** A private field `AssetData::binary_derived: bool`, true when `binary` was
  serialized from the value in this process, false when it was loaded. Set beside every write of
  `binary`: `finalize_status_with_version` (true), `AssetRef::serialize_to_binary` (true),
  `get_binary_any_status` (true), the fast-track install in `AssetData` (false), the test-only
  setters (false). Every site that clears `binary` also clears the flag (a private
  `clear_binary()` helper replaces the ~10 direct `binary = None` writes).
- **Release.** A private `AssetData::release_written_binary(&mut self, written: &Arc<Vec<u8>>)`:
  clears `binary` iff `binary_derived` and `Arc::ptr_eq(binary, written)`. `save_to_store` calls it
  under the write lock right after `store.set` succeeds, with the `Arc` it wrote (it already holds
  it as `data`). It does not apply on `Skipped` or on an error.
- Background (`tokio::spawn` in `persist_with_status_tracking`) and inline paths both go through
  `save_to_store`, so AC-6 needs no second site. The manager `set_state` / `set_binary` paths
  install no cached binary at HEAD and are unaffected.

Rejected: releasing in `record_persistence_result` (has no `Arc` to compare); a byte budget (needs
size accounting the asset lacks; Q1); a recipe/type declaration (new vocabulary for no current
user; Q1); weak references to the bytes (nothing else holds them, so they would drop at once);
re-reading from the store on demand (an async store read inside `get_binary`, and the store may
have changed).

Known-issue preflight: `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` is closed; the open overlaps above
do not block. `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` is closed (the persistence path stays
gate-free). Nothing open blocks.

### Changes

- `liquers-core/src/assets.rs`: `AssetData` (`binary_derived`, `clear_binary`,
  `release_written_binary`), the fast-track install, `finalize_status_with_version`,
  `serialize_to_binary`, `get_binary_any_status`, `save_to_store`, and the clear sites (`reset`,
  the evaluation error branch, cancellation, `set_value`, `set_state`, the two manager installs).
- No public signature, command, store or binding change.
- Documents: `specs/reference/ASSETS.md` — the binary cache (§Assets `binary`, §Status and reads):
  when cached bytes are released, that loaded and reader-cached bytes are kept; History row,
  `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A clear site misses the flag, so stale `binary_derived` releases loaded bytes later | single `clear_binary` helper; grep for `binary = ` in review; AC-3 | high |
| Released bytes cannot be reproduced (a value whose `as_bytes` now fails) | only derived bytes are released, and they were produced by `as_bytes` | high |
| Background persist releases bytes of a newer evaluation | `Arc::ptr_eq` under the write lock; AC-4 | high |
| Extra CPU for a reader after release | Q2: reader-cached bytes stay; only the first read re-serializes | high |
| Existing tests assert `poll_binary().is_some()` after a persist | full `liquers-core` suite; adjust to `get_binary` where the test meant "bytes available" | medium |

Recovery: one commit per step; removing the `release_written_binary` call restores HEAD behaviour.

## Phase 3: Examples and Tests

### Examples

```rust
let asset = envref.evaluate(&parse_query("-R/big/table.csv")?).await?;   // keyed recipe
asset.get().await?;                                    // value resident
wait_until(|| asset.persistence_status() == Persisted).await;
assert!(asset.poll_binary().await.is_none());           // bytes released
let (bytes, _) = asset.get_binary().await?;             // serialized again, cached for readers
```

### Tests

Unit, `liquers-core/src/assets.rs` `mod tests` (the `cancelled_keyed_asset` fixture):
- `release_written_binary_keeps_newer_bytes` — `ptr_eq` mismatch keeps them: AC-4
- `release_written_binary_keeps_loaded_bytes` — `binary_derived == false`: AC-3
- `save_to_store_releases_derived_binary_on_written` — persists, `poll_binary` is `None`: AC-1

New `liquers-core/tests/binary_disposal.rs` (recipes in an `AsyncMemoryStore`, as in
`keyed_version_cascade.rs`; `table/x.txt` ← `hello`):
- `binary01_persisted_keyed_asset_releases_bytes` — default manager, background persistence: AC-1, AC-6
- `binary02_inline_manager_releases_bytes` — `EvalMode::Inline`: AC-6
- `binary03_stored_false_keeps_bytes` — recipe with `stored: false`: AC-2
- `binary04_failed_write_keeps_bytes` — a `FailingWriteStore` wrapper modelled on
  `fixtures::CountingStore`, whose `set` errs: AC-2
- `binary05_fast_track_loaded_bytes_survive_to_override`: AC-3
- `binary06_get_binary_after_release_returns_stored_bytes` — equal to `store.get_bytes`; both read
  methods: AC-5

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test binary_disposal` and
`cargo test -p liquers-core --lib assets::`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/binary_disposal.rs` — the six tests; binary01, 02 fail at HEAD — `cargo test -p liquers-core --test binary_disposal`. Rollback: delete the file.
- [ ] 2. `assets.rs` `AssetData` — `binary_derived`, `clear_binary`; replace every `binary = None` with `clear_binary()`; set the flag at the four set sites — `cargo test -p liquers-core --lib --tests` (no behaviour change yet). Rollback: revert.
- [ ] 3. `assets.rs` `AssetData::release_written_binary`, called in `save_to_store` after a successful `store.set`; the three unit tests — binary01-06 pass. Rollback: drop the call.
- [ ] 4. Existing tests that read `poll_binary` after a persist — switch to `get_binary` where they meant "bytes are available" — `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`.
- [ ] 5. Docs: `ASSETS.md` (History row, `reviewed:`); close `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (wasm rows use the inline path);
`python3 scripts/docs_index.py --check`; diff review for debug output and edits outside
`assets.rs`, the new test file and `ASSETS.md`.
