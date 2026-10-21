# Phase 4: Implementation Plan — a type registry that describes every realm

## Overview

Core first (steps 1-3, one file), then the web entry (step 4), then documents (step 5). No
prerequisite issue. Steps 1-3 can land without step 4; step 4 needs step 1.

## Progress

- [ ] Step 1: `TransferPolicy` and `TypeInfo::transfer`
- [ ] Step 2: serialized `TypeRegistry`
- [ ] Step 3: `import_realm`, `realms`, `types_not_in`
- [ ] Step 4: `js.Value` declared untransferable
- [ ] Step 5: documentation and final checks

## Implementation Steps

### Step 1: `TransferPolicy` and `TypeInfo::transfer`
- Files / symbols: `liquers-core/src/type_system.rs` — new `TransferPolicy`; `TypeInfo` field
  `transfer` (`#[serde(default)]`), set to `TransferPolicy::Serialize` in `TypeInfo::new`;
  `TypeInfo::with_transfer`, `TypeInfo::transfer_mode` (exhaustive `match`, no `_ =>`).
- Change: signatures as Phase 2.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib type_system` — tests 7 and 9.
- Rollback: revert the file; no caller uses the field yet.

### Step 2: serialized `TypeRegistry`
- Files / symbols: `type_system.rs` — `#[derive(Serialize, Deserialize)]` with
  `#[serde(into = "Vec<TypeInfo>", try_from = "Vec<TypeInfo>")]` on `TypeRegistry`;
  `impl From<TypeRegistry> for Vec<TypeInfo>`; `impl TryFrom<Vec<TypeInfo>> for TypeRegistry`
  (loop over `register`, first error returned).
- Depends on: step 1 (the list carries the new field).
- Proof: tests 3 and 4.
- Rollback: remove the derive and the two impls.

### Step 3: `import_realm`, `realms`, `types_not_in`
- Files / symbols: `type_system.rs` `impl TypeRegistry`.
- Change: `import_realm` builds the relabelled entries (`info.clone().with_realm(realm)`) into a
  scratch list, checks every key against `self.types` first, then inserts — so a clash applies
  nothing; refuses `realm == DEFAULT_TYPE_REALM`. `realms` collects `TypeKey::realm`;
  `types_not_in` groups identifiers by realm and returns, once per identifier in key order, the
  first `TypeInfo` of each identifier absent from `realm`.
- Depends on: none (parallel with step 2).
- Proof: tests 5, 6, 8.
- Rollback: remove the three methods.

### Step 4: `js.Value` declared untransferable
- Files / symbols: `liquers-web/src/value.rs` `js_value_type_info` — append
  `.with_transfer(liquers_core::type_system::TransferPolicy::Untransferable)`; add test 10.
- Depends on: step 1.
- Proof: `cargo clean` then `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles`; `./liquers-web/scripts/check-stubs.sh --build` unchanged (no binding change).
- Rollback: drop the line; inference still reports `Untransferable`.

### Step 5: documentation and final checks
- Files: `specs/reference/VALUE_TYPE_SYSTEM.md` ("Realms" subsection: list form, `import_realm`,
  transfer modes and inference), `specs/reference/REALMS.md` (open question "Types per realm"),
  `specs/guides/TYPE_SYSTEM_GUIDE.md` (one paragraph); each with a `## History` row and
  `reviewed:` bump. Issue `TYPE-REGISTRY-NOT-REALM-AWARE`: `closed` with a resolution naming the
  follow-ups `REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS` and `CROSS-REALM-TRANSFER-NOT-PLANNED`.
- Checks: `cargo test -p liquers-core --lib`; `cargo test -p liquers-lib --lib --tests` (consumers
  of `TypeInfo` recompile); `bash scripts/check-build-matrix.sh` is not needed (no `cfg(feature)`
  touched) but the wasm32 row of the web crate is covered by step 4;
  `python3 scripts/docs_index.py` then `--check`; final diff review for scope and debug output.
- Depends on: steps 1-4.

## Testing Plan

Steps 1-3 run the core unit tests after each step; step 4 runs the web Node loop once; step 5 runs
the crate suites above. `specs/command_registry.yaml` does not change (no command touched).

## Rollback Plan

Each step is additive and revertible alone. If only steps 1-3 landed, the registry is complete in
core and `js.Value` still reports `Untransferable` by inference.

## Documentation Updates

As step 5. `specs/index.csv` regenerated; no generated registry file changes.

## Phase 5 Entry Criteria

None — four-phase design.
