# Phase 4: Implementation Plan — resource-name-unicode

## Overview

Prerequisite: the Phase 1 decision (option C, contract 2). Recommended predecessor:
`store-backend-absolute-key` (then Step 5's name check goes into `AbsoluteKey::new` once instead of
into each backend). The order builds the grammar and the two key forms first. The store audit
follows. The whole change is one PR: a grammar that accepts `café.csv` while stores still use
`encode()` as the physical name would read `caf~UE9~.csv` from disk (contract 1 by accident).

## Progress

- [ ] Step 1: name predicates and encoder in `escape.rs`
- [ ] Step 2: `ResourceName::encode` → `Cow`, `try_new`, `Key::try_join`, `Key::path`
- [ ] Step 3: parser `resource_name` with literals and entities
- [ ] Step 4: audit `Key`/`ResourceName` string uses; physical names switch to `path()`
- [ ] Step 5: backend component check (file store, OpenDAL fs)
- [ ] Step 6: web stores
- [ ] Step 7: conformance rule `keyshape03`
- [ ] Step 8: followers (template, builder, styled tokens)
- [ ] Step 9: documents and issue

## Implementation Steps

### Step 1: predicates and encoder
- Files / symbols: `liquers-core/src/escape.rs` — `is_valid_resource_name`,
  `is_literal_resource_name_char`, `encode_resource_name`
- Change: per Phase 2; the encoder reuses `entities::curated_name` and the `~U<hex>~` writer
- Depends on: none
- Proof: `cargo test -p liquers-core --lib escape` (`encode_resource_name_is_ascii_and_round_trips` using `decode_token` plus the Step 3 parser once present)
- Rollback: delete the functions

### Step 2: key forms
- Files / symbols: `liquers-core/src/query.rs` `ResourceName::encode` (→ `Cow<'_, str>`),
  `ResourceName::try_new`, `Key::try_join`, `Key::path`, `Key::encode` (joins encoded names);
  `ResourceName::render` / `styled_tokens` use the encoded form
- Change: fix compile errors at `encode()` callers that bound `&str` (mechanical `.as_ref()` /
  `into_owned()`)
- Depends on: Step 1
- Also: `liquers-py/src/query.rs` `ResourceName::encode` returns `String`
- Proof: `cargo check -p liquers-core -p liquers-store -p liquers-lib -p liquers-axum -p liquers-py -p liquers-records -p liquers-template`; `cargo test -p liquers-core --lib query`
- Rollback: revert; the signature change is the only API break

### Step 3: parser
- Files / symbols: `liquers-core/src/parse.rs` `resource_name`, new `resource_entity`;
  `describe_query_failure` message for a forbidden decoded character
- Depends on: Steps 1-2
- Proof: `cargo test -p liquers-core --lib parse` (Tests 3, 4); `cargo test -p liquers-core --test query_backward_compatibility` with the replaced and new tests (AC-3)
- Rollback: revert `resource_name`

### Step 4: audit of string uses
- Files / symbols: every `Key::encode`, `Display for Key`, `ResourceName::encode` call (≈150; list
  with `grep -rn "encode()\|to_string()"` over the crates). Classify each as **query text** (keep)
  or **physical / display name** (→ `path()` or `.name`). Known physical sites:
  `AsyncFileStore::key_to_path*` (`key.to_string()` → per-segment push),
  `liquers-store/src/opendal_store.rs` `key_to_path*`, `liquers-lib/src/mem/layout.rs`, the metadata
  `set_filename` in `store.rs`
- Change: record the classification in the PR description
- Depends on: Step 2
- Proof: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-store`,
  `cargo test -p liquers-lib --lib --tests`
- Rollback: per file

### Step 5: backend component check
- Files / symbols: `AsyncFileStore::key_to_path`, `key_to_path_metadata`, `key_to_lock_path`;
  OpenDAL `fs` service path builder. If `store-backend-absolute-key` has landed, these builders
  already take `AbsoluteKey<'_>`, and the check is added beside `reject_reserved`
- Depends on: Step 4
- Proof: `cargo test -p liquers-core --lib store::tests::file_store_refuses_non_component_name` (AC-7)
- Rollback: remove the check (the parser still refuses `/`)

### Step 6: web stores
- Files / symbols: `liquers-web/src/store/local_storage.rs` (storage key from `path()`),
  `js_store.rs` (callbacks receive `path()`; document it), `fetch.rs` `key_to_url` (percent-encode
  each decoded name)
- Depends on: Step 4
- Proof: after `cargo clean`, the `liquers-web` Node loop (with and without `records`)
- Rollback: per file

### Step 7: conformance
- Files / symbols: `liquers-core/src/store_conformance/rules/keyshape.rs` `keyshape03`, registered
  for every suite in `liquers-core/tests/store_conformance_CONF.rs` and `liquers-store`'s suites
- Depends on: Steps 4-5
- Proof: `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`;
  `cargo test -p liquers-store` (AC-6)
- Rollback: drop the rule

### Step 8: followers
- Files / symbols: `liquers-template/src/query/instantiate.rs` `is_key_segment` →
  `is_valid_resource_name`; `query_builder` key-segment check if landed; `ResourceName::styled_tokens`
  entity split if `styled-query-entity-tokens` landed
- Depends on: Step 1
- Proof: `cargo test -p liquers-template`; `cargo test -p liquers-core --lib query_builder` (AC-8)
- Rollback: per file

### Step 9: documents and issue
- Files: the six `affects_docs` (Phase 2 table), History rows and `reviewed:` bumps; remove the
  RESOURCE-NAME pointer in `liquers-core/src/escape.rs` docs of `is_resource_name_char`; close
  `RESOURCE-NAME-ASCII-ONLY` with the decision recorded
- Proof: `python3 scripts/docs_index.py --check`; `cargo test -p liquers-core --doc`

## Testing Plan

Per-step proofs, then `cargo test -p liquers-core --lib --tests`, the conformance command,
`cargo test -p liquers-store`, `cargo test -p liquers-lib --lib --tests`,
`cargo test -p liquers-template`, `bash scripts/check-build-matrix.sh` (wasm32 rows: `liquers-web`
store changes), the `liquers-web` loops after `cargo clean`, and the `liquers-py` script
(`bash scripts/test-liquers-py.sh`) because `ResourceName.encode` is exposed there
(`liquers-py/src/query.rs` returns `&str` → `String`).

## Rollback Plan

Revert the PR. For keys that parse at HEAD, nothing was written under a new physical name. A store
that received non-ASCII keys while the change was live keeps files named with the decoded names. A
reverted build cannot address them again, which is the HEAD situation.

## Documentation Updates

`DOC_02`, `PROJECT_OVERVIEW.md` (required by `CLAUDE.md` for a Key encoding change),
`STORE_SEMANTICS.md` §7, `QUERY_ESCAPING_GUIDE.md`, `STORE_IMPLEMENTATION_GUIDE.md`,
`TEMPLATE_LANGUAGE.md`. No `command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.
