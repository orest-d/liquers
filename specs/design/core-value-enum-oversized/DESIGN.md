---
id: CORE-VALUE-ENUM-OVERSIZED-DESIGN
kind: design
title: Put the large Value variants behind Arc
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value]
issues: [CORE-VALUE-ENUM-OVERSIZED]
created: 2026-10-12
---
# Put the large Value variants behind Arc

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`liquers_core::value::Value` (`liquers-core/src/value.rs`) and its mirror `SimpleValue`
(`liquers-lib/src/value/simple.rs`) store `MetadataRecord`, `Recipe` and `CommandMetadata` inline,
so every value, including `Value::None`, is 752 bytes. Holding those three payloads as `Arc<T>`
brings both enums to 64 bytes and makes cloning one of them a reference-count bump, matching the
rule in `specs/reference/VALUE_TYPE_SYSTEM.md` §"What a value variant may hold".

### Problem Example

```rust
let v: Vec<Value> = (0..10_000).map(Value::from_i32).collect();
```

Today `size_of::<Value>()` is **752**, so `v` allocates 7,520,000 bytes to hold 40 KB of integers,
and every `Result<Value, Error>` a command returns moves 752 bytes. It should be **64**: 640,000
bytes for the same vector. Measured at HEAD on 2026-10-12 (x86_64, a `size_of` probe built against
`liquers-core` by path, outside the repository):

| Type | Bytes | | Type | Bytes |
|---|---|---|---|---|
| `Value` | **752** | | `Query` | 64 |
| `CommandMetadata` | **736** | | `Key`, `String`, `Vec<_>` | 24 |
| `MetadataRecord` | 624 | | `Error` | 8 (boxed, `core-error-payload-size`) |
| `AssetInfo` | 576 (already behind `Vec`) | | `State<Value>` | 16 (holds `Arc<Value>`) |
| `Recipe` | 256 | | `Arc<T>`, `Box<T>` | 8 |

The issue's 704 figure is stale: `CommandMetadata` has grown past `MetadataRecord` and now sets the
size. Arc-ing only `Metadata` leaves 736. Mirror enums of the same shape give `SimpleValue` 752 and
`CombinedValue<SimpleValue, ExtValue>` (the lib `Value`) 752; after the change the core mirror is
64, the `SimpleValue` mirror 64 and the combined mirror 64 (`ExtValue` is 24). Also putting `Query`
behind `Arc` would give 32.

### Scope and Acceptance Criteria

- **AC-1** Core value size
  - WHEN `std::mem::size_of::<liquers_core::value::Value>()` is evaluated
  - THEN it is at most 64 bytes
- **AC-2** Library value sizes
  - WHEN `size_of::<SimpleValue>()` and `size_of::<liquers_lib::value::Value>()` are evaluated
  - THEN each is at most 64 bytes
- **AC-3** Wire format unchanged
  - WHEN a `Metadata`, `Recipe` or `CommandMetadata` value of either enum is written as `json`
    (`DefaultValueSerializer::serialize_to_bytes`, and core's `try_into_json_value`) and read back
  - THEN the bytes equal `serde_json` of the bare payload, as today, and the value read back equals
    the original
- **AC-4** Clone shares the payload
  - WHEN a `Metadata`, `Recipe` or `CommandMetadata` value is cloned
  - THEN both copies point at the same allocation (`Arc::ptr_eq`)
- **AC-5** Trait contract unchanged
  - WHEN a caller uses `ValueInterface::from_metadata`, `from_recipe`, `from_command_metadata` or
    `try_into_command_metadata`
  - THEN they take and return owned `T` exactly as today, and `liquers-py` builds unmodified

Non-goals: `liquers-py`'s own `Value` enum (`liquers-py/src/value.rs`, pyclass wrappers of the same
payloads, same problem, not reachable through this change); `Query`; new accessors returning `Arc`;
`ExtValue`, which already complies.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes the payload types of variants of the `pub` enums
  `Value` and `SimpleValue` and enables serde's `rc` feature (rule 4); spans `liquers-core` and
  `liquers-lib` (rule 6).
- **Leading issue:** Proposed resolution - variant set and size bound.
- **Explanation:** The change is mechanical and its blast radius is measured (Phase 2); every choice
  has a recommended answer, so Phases 3-4 are complete under it. The choices change public enum
  shapes, so they stay visible for review.
- **Open questions:**
  1. **Proposed resolution - variant set and bound:** `Arc` exactly `Metadata`, `Recipe`,
     `CommandMetadata`; the rule is "a payload over 64 bytes goes behind `Arc`", and the guard is
     `<= 64`. `Query` (exactly 64) stays inline: `Arc<Query>` would reach 32 but touches the
     query paths (`from_query`, `try_into_query`, the `txt` reader), for a gain only `Vec<Value>`
     notices. Recommend: three variants, bound 64.
  2. **Proposed resolution - Arc versus Box:** `Arc`. Both are 8 bytes; `Box` deep-copies on every
     `Clone`, which is what `Value` does constantly, and `ExtValue` already uses `Arc`.
  3. **Proposed resolution - trait signatures:** keep `ValueInterface`'s owned `T` in and out
     (`Arc::new` inside `from_*`, `(*arc).clone()` in `try_into_command_metadata`). Returning
     `Arc<T>` would break the `liquers-py` implementor and every caller for no current consumer;
     an additive `Arc` accessor can come later.
  4. **Proposed resolution - scope across crates:** `SimpleValue` is in scope (it is the base of the
     lib `Value`, so AC-2 fails without it). `liquers-py`'s `Value` is out of scope and gets its own
     issue, because it is a separate enum of pyclass wrappers built only with PyO3.
  5. **Implementation detail - guard form:** a `#[test]` asserting the bound, following
     `liquers-core/src/error.rs` `error_is_one_pointer_wide`, not a `const` assertion; the bound
     holds on wasm32 too, where pointers are smaller.

### Design Dependencies

- `core-error-payload-size` (complete) - **overlaps**: precedent for boxing a hot type and guarding
  its size with a test; no ordering.
- `value-type-system` (complete) - **overlaps**: wrote the payload rule this design satisfies; the
  reference paragraph it produced is updated here (Phase 4 step 5). Its frozen folder is not edited.
- `simple-value-serializer-parity` (in_review, documentation) and
  `simple-value-untyped-and-scalar-reads` (PR 91) - **overlaps**: edit other arms of
  `SimpleValue::serialize_to_bytes` / `deserialize_from_bytes`. Weak overlap (T3 fails: different
  arms); whichever lands second rebases.
- Issues without a design: `WORKSPACE-SERDE-DERIVE-UNDECLARED` edits the same `serde =` line in
  `liquers-core/Cargo.toml` (merge to `features = ["derive", "rc"]`);
  `VALUE-TYPE-DEFINITION-MACRO` should generate `Arc` payloads; `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT`
  may reshape the accessors kept here. All weak; none orders this work.

### Consolidated Findings

- Core needs serde's `rc` feature: `liquers-core` alone does not get it (only `liquers-records`
  enables it, by unification in lib builds). With `rc`, `Arc<T>` serializes exactly as `T`, so no
  stored file or HTTP payload changes and the untagged deserialization order is untouched.
- Blast radius is two files of code: about 7 edits in `liquers-core/src/value.rs` and 10 in
  `liquers-lib/src/value/simple.rs`. Match arms that ignore the payload (`identifier`, `type_name`,
  `default_extension`, `default_filename`, `default_media_type`, `html.rs`), the egui display arms
  (auto-deref of `&Arc<T>`) and the serde serializer arms need no edit. `liquers-py`,
  `liquers-web`, `liquers-axum`, `liquers-store`, `liquers-records` and every `from_*` caller
  (`interpreter.rs`, lib `commands.rs`, `extended.rs`) need none.
- `specs/reference/VALUE_TYPE_SYSTEM.md` states the 704-byte figure as current; it must change in the
  same commit (History row, `reviewed:` bump).
- Follow-up to file: `liquers-py`'s `Value` has the same oversize.

## Phase 2: Architecture

### Solution

Change three variants of each enum to hold `Arc<T>`; keep every trait signature; enable serde `rc`
in `liquers-core` and `liquers-lib`; add size-guard tests. Rejected: **`Box<T>`** (same size, deep
copy on clone); **leaving as is** (every scalar keeps paying 752 bytes, and the reference rule stays
violated by the type it is modelled on); **boxing only `Metadata`** (the issue's framing; measured
736, since `CommandMetadata` is now the largest); **`Arc<Query>` too** (open question 1);
**`#[serde(with = …)]` adapters instead of `rc`** (more code for the same bytes).

Known-issue preflight: no open issue blocks. `WORKSPACE-SERDE-DERIVE-UNDECLARED` (P2, accepted)
touches the same dependency line and is merged by hand if it lands first. No command, namespace or
`specs/command_registry.yaml` change.

### Changes

- `liquers-core/Cargo.toml`, `liquers-lib/Cargo.toml`: `serde = { version = "1.0.229", features = ["rc"] }`
  (add `"derive"` too if `WORKSPACE-SERDE-DERIVE-UNDECLARED` has landed).
- `liquers-core/src/value.rs` `Value`:

  ```rust
  Metadata(Arc<MetadataRecord>),
  Recipe(Arc<Recipe>),
  CommandMetadata(Arc<CommandMetadata>),
  ```

  `ValueInterface for Value`: `from_metadata` / `from_recipe` / `from_command_metadata` wrap with
  `Arc::new`; `try_into_command_metadata` returns `command_metadata.as_ref().clone()`;
  `try_into_json_value` unchanged (`&Arc<T>: Serialize` under `rc`); `tests::sample_values`
  wraps its three payloads.
- `liquers-lib/src/value/simple.rs` `SimpleValue`: `Metadata { value: Arc<MetadataRecord> }`,
  `Recipe { value: Arc<Recipe> }`, `CommandMetadata { value: Arc<CommandMetadata> }`;
  `try_into_command_metadata`, the three `from_*`, the three typed `json` readers in
  `deserialize_from_bytes` (`.map(|value| SimpleValue::Metadata { value: Arc::new(value) })`), and
  `tests::sample`. `std::sync::Arc` is already imported.
- `specs/reference/VALUE_TYPE_SYSTEM.md` §"What a value variant may hold": `Value` now complies;
  cite the 64-byte guard.

Ownership: payloads become shared and immutable; nothing mutates a `Value::Metadata` in place today
(no `&mut` binding of these variants exists), so no `Arc::make_mut` is needed. No async, error or
`cfg(feature)` change; no `_ =>` arm added.

### Risks

| Area | Risk | Mitigation / certainty |
|---|---|---|
| Compile | A binding site was missed | `cargo check` finds it; counts above are from grep at HEAD. High |
| Serialization | `rc` changes bytes | It does not; AC-3 tests compare bytes. Deserialized `Arc`s are never shared with each other, which nothing relies on. High |
| Bindings | `liquers-py`/`liquers-web` break | Signatures kept (AC-5); both built in Validation. High |
| Concurrent PRs | Conflicts in `simple.rs` | Different arms; rebase. Medium |
| Performance | One extra allocation per metadata/recipe value | Those are created rarely and cloned often. High |
| Recovery | Revert one commit; no data migration | High |

## Phase 3: Examples and Tests

### Examples

The Problem Example: after the change `Vec<Value>` of 10,000 `I32` occupies 640,000 bytes, and
`Value::from_metadata(MetadataRecord::new()).clone()` copies 64 bytes and increments a count.
No Liquers query is involved.

### Tests

In `liquers-core/src/value.rs` `mod tests`:

- `value_is_at_most_64_bytes` - asserts `size_of::<Value>() <= 64` with the measured size in the
  message: AC-1
- `large_variants_serialize_as_their_payload` - for each of the three variants,
  `serialize_to_bytes("json")` equals `serde_json::to_vec` of the bare payload, `try_into_json_value`
  equals `serde_json::to_value` of it, and `deserialize_from_bytes` with the identifier returns an
  equal value: AC-3
- `cloning_a_large_variant_shares_the_payload` - `Arc::ptr_eq` after `clone()` for all three: AC-4
- `value_interface_takes_and_returns_owned_payloads` - `from_command_metadata(cm.clone())` then
  `try_into_command_metadata()` equals `cm`; `from_metadata`/`from_recipe` build the variants:
  AC-5

In `liquers-lib/src/value/simple.rs` `mod tests`:

- `simple_and_lib_values_are_at_most_64_bytes` - `SimpleValue` and `crate::value::Value`: AC-2
- `simple_large_variants_round_trip_through_json` - as the core test, for `SimpleValue`: AC-3, AC-4

AC-5's binding half is the `liquers-py` and `liquers-web` builds in Validation.

Run: `cargo test -p liquers-core --lib value::tests` and `cargo test -p liquers-lib --lib value::simple`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/Cargo.toml`, `liquers-lib/Cargo.toml` — enable serde `rc` — `cargo check -p liquers-core`. Rollback: revert the line.
- [ ] 2. `liquers-core/src/value.rs` `Value` and `impl ValueInterface for Value` — `Arc` the three variants, wrap in `from_*`, clone out in `try_into_command_metadata`, fix `tests::sample_values`; add the four core tests — `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`. Rollback: revert the file; nothing else depends on it yet.
- [ ] 3. `liquers-lib/src/value/simple.rs` `SimpleValue` — same change, plus the three `json` readers and `tests::sample`; add the two lib tests — `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests`. Depends on 1.
- [ ] 4. Bindings — confirm no edit is needed — `cargo check -p liquers-py` (prefix `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` on CPython 3.13+) and, after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. Rollback: if either needs an edit, AC-5 is broken; revisit open question 3 before patching.
- [ ] 5. `specs/reference/VALUE_TYPE_SYSTEM.md` §"What a value variant may hold" — replace the 704-byte sentence with the 64-byte guard; History row and `reviewed:` bump. File the `liquers-py` `Value` follow-up issue. Close `CORE-VALUE-ENUM-OVERSIZED` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`bash scripts/check-build-matrix.sh` (the enum is matched in every feature configuration and on
wasm32); `cargo test -p liquers-lib --test registry_export` (no command changes, must stay green);
`cargo fmt --check`; a diff review for stray probes, `println!`, and edits outside the files above.
No generated file changes.
