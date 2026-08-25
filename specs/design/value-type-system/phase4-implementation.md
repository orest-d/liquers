# Phase 4: Implementation Plan - Liquers value type system

## Overview

Eleven steps in three arcs. **Steps 0–1 are preparatory**: two filed defects that sit directly in
the paths this design depends on, plus hoisting a check that currently exists in four copies.
**Steps 2–7 build the mechanism** bottom-up — registry, trait methods, environment access, metadata
resolution, state seeding, then enforcement — each compiling and testable before the next.
**Steps 8–10 propagate and verify** across the binding crates and the build matrix.

The ordering is chosen so that no step leaves the workspace uncompilable and each has a validation
command that fails before it and passes after. Step 6 is the one that closes the P0; everything
before it is groundwork and everything after is reach.

## Implementation Steps

### Step 0 — Preparatory defect fixes

Both are filed issues, both are small, and both would silently corrupt the behaviour this design
specifies if left in place. They go first so that later steps are not debugging them.

**0a. `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`**

- File: `liquers-lib/src/value/extended.rs:150`
- Change: replace `_ => "ext".into()` with an explicit `CombinedValue::Extended(ext) => ext.default_extension()` arm, matching the four sibling methods.
- Why first: level-1 seeding reads `default_extension`; leaving it returns `"ext"`, a format no serializer implements, for every extended value.
- Validation: `cargo test -p liquers-lib --lib --tests`

**0b. `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON`**

- Files: `liquers-core/src/metadata.rs` — `Metadata::get_media_type` (`:1570`), `get_data_format` (`:1779`), `type_identifier` (`:1664`), `type_name` (`:1693`), and any sibling `LegacyMetadata` accessor found by the sweep.
- Change: extract with `as_str()` and fall back to `to_string()` only for values that genuinely are not strings.
- Why first: the resolution rule reads `get_data_format`; a quoted `"\"json\""` would be refused by Step 6's format check, turning a cosmetic bug into a refusal of every legacy or partial document.
- Validation: `cargo test -p liquers-core --lib` plus the new tests 5.5.

### Step 1 — Hoist the duplicated consistency check

- File: `liquers-core/src/assets.rs`
- Current state: `add_soft_consistency_warnings` is a *nested local function* declared four times — `:3173`, `:3280`, `:4767`, `:4906` — in two different signatures (`&mut MetadataRecord` and `&mut Metadata -> Result<(), Error>`). `validate_required_metadata_fields` is duplicated alongside it.
- Change: move both to module level with one signature each, add a `check_metadata(&mut Metadata, ..)` adapter for the two `Metadata`-holding sites, and call them from all four.
- **No behaviour change in this step.** It is a pure refactor whose only purpose is that Steps 6a–6b are written once rather than four times.
- Validation: `cargo test -p liquers-core --lib --tests` — the existing suite must pass unchanged.

### Step 2 — `liquers-core/src/type_system.rs` (new module)

- Add: `TypeInfo` (+ builder), `TypeKey`, `TypeRegistry`, `TypeIdentifiedIn<V>`, `to_type_identifier<V, T>`, and the `Arc<T>` / `&T` blanket impls.
- Register `error` as a bare type (Phase 2, "Error states").
- Declare the module in `lib.rs`.
- Nothing consumes it yet; this step is additive and cannot break anything.
- Tests: 4.1–4.6.
- Validation: `cargo test -p liquers-core --lib type_system`

### Step 3 — `ValueInterface` gains three methods

- File: `liquers-core/src/value.rs`
- Add `type_descriptions()` (default: empty `Vec`), `supports_data_format(&self, ..)`, `type_info(&self)`.
- Implement all three for `Value`, with one `TypeInfo` per variant identifier. **This is where `Value::I32.identifier()` changes from `"generic"` to `"I32"`**, and where `default_data_format` and `supported_data_formats` are stated per type.
- Names `identifier` and `type_name` are **not** renamed — that belongs to `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT`.
- Tests: 7.1–7.4. Test 7.2 fails at HEAD and passes here; it is the unit-level proof of the P0.
- Validation: `cargo test -p liquers-core --lib value`

### Step 4 — `Environment::get_type_registry`

- File: `liquers-core/src/context.rs`, plus three implementors.
- Add the required trait method; build the registry once at construction with `TypeRegistry::from_value_type::<Self::Value>()`.
- Implementors: `SimpleEnvironment` (`context.rs:1021`), `ImmediateEnvironment` (`:1141`), `DefaultEnvironment` (`liquers-lib/src/environment.rs:94`), `liquers-py` (`context.rs:82`).
- Breaking for external implementors; there are four in-tree and they are all updated in this step.
- Validation: `cargo check -p liquers-core -p liquers-lib -p liquers-py`

### Step 5 — Metadata resolution

- File: `liquers-core/src/metadata.rs`
- **5a.** `MetadataRecord.media_type: String` → `Option<String>`. 19 direct field accesses workspace-wide; each becomes explicit about whether it wants the override or the resolved value.
- **5b.** Add `declared_data_format`, `effective_data_format(value_default)`, `effective_media_type(value_default_format)`. Remove the extension-and-`"bin"` fallback chain from `get_data_format`/`get_media_type`; the level-1 answer now arrives from the value.
- **5c.** `#[serde(default)]` on every `MetadataRecord` field with a sensible default, so `{"media_type":"text/plain"}` deserializes into a record rather than dropping to `LegacyMetadata`. This is the root-cause half of `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON`, handed to this project by that issue.
- **`AssetInfo.media_type` stays a `String`.** It is a resolved projection for clients, not a place to record an override, so it carries the effective value. This keeps the UI (`egui/widgets.rs:854`) and the Python bindings unchanged.
- Tests: 5.1–5.7.
- Validation: `cargo test -p liquers-core --lib metadata`

### Step 6 — Enforcement (the P0)

- File: `liquers-core/src/assets.rs` — the functions hoisted in Step 1.
- **6a. Hard tier.** Add to `validate_metadata_hard`: identifier registered; effective `data_format` in `supported_data_formats`; `Some(media_type)` well-formed (no CR/LF, `type/subtype`). **Skip the format check when the status is `Error`** — an error state's bytes are not a serialization of its declared type, and it often retains the intended output's filename.
- **6b. Soft tier.** Compare the extension against the **base** format (split at the first `:`), so `csv` vs `csv:comma` no longer warns; keep the media-type divergence advisory so a declared override survives; add the `Info` provenance entry naming which seeding level supplied the format.
- **6c. Read path.** `deserialize_stored_value` gains a `registry: &TypeRegistry` parameter and returns `DeserializedValue<E::Value>`; an unregistered identifier degrades rather than failing. Both call sites (`AssetData::try_fast_track` at `:654`, and the manager's store-fallback path) hold `envref` and supply it.
- Tests: 8.1–8.9, 9.1–9.4.
- Validation: `cargo test -p liquers-core --lib --tests`

### Step 7 — `State` seeding

- File: `liquers-core/src/state.rs`
- Extend the existing private `sync_metadata_with_value` (`:25`) to seed `data_format` / extension / `media_type` from the value's `TypeInfo`, **only where not already set**.
- Fix the ordering hazard: `State::from_error` must not have its `error` identifier overwritten by the sync.
- Every constructor already calls the helper, so all of them gain seeding without further change; `from_parts` deliberately does not.
- Tests: 6.1–6.3.
- Validation: `cargo test -p liquers-core --lib state`

### Step 8 — `liquers-lib`

- `value/mod.rs`, `value/extended.rs`, `value/simple.rs`: implement the three `ValueInterface` / `ValueExtension` additions for existing variants. No new variant, no new feature, no new dependency.
- `value/foreign.rs`: `ForeignValue` gains `type_info`, with a default derived from its existing `identifier`/`default_*` methods so no integration breaks.
- `environment.rs`: registry construction.
- Identifiers adopt the Phase 2 naming: `polars.DataFrame`, `Image` (bare, canonical), `ui.Element`, etc.
- Tests: 10.1–10.4.
- Validation: `cargo test -p liquers-lib --lib --tests`

### Step 9 — Bindings and the HTTP surface

- `liquers-axum/src/axum_integration.rs:52`: read `effective_media_type` rather than `get_media_type`.
- `liquers-web/src/store/fetch.rs:96-101`: set the level-3 override explicitly (`Some(..)`) instead of detecting an empty string.
- `liquers-py`: implement the three `ValueInterface` methods and `get_type_registry`; `metadata.rs:358/362/596/600` follow the `Option<String>` media type.
- `liquers-web/src/value.rs`, `tests/second_value_type.rs`: same three methods.
- Validation: `cargo check -p liquers-axum -p liquers-py` and `cargo check -p liquers-web --target wasm32-unknown-unknown`.
  **The `wasm32-unknown-unknown` target is not installed in the reference environment** (`rustup target list --installed` shows only `x86_64-unknown-linux-gnu`), so the step must either install it or record `liquers-web` as compile-unverified. Do not report the step complete on the strength of the native crates alone.

### Step 10 — Build matrix and registry export

- Run the full matrix from Phase 3.
- Regenerate `specs/command_registry.yaml` **only if** a command signature changed — none should have. `cargo test -p liquers-lib --test registry_export` is the guard (test 10.4).

## Testing Plan

| When | Command |
|---|---|
| After each of steps 0–7 | `cargo test -p liquers-core --lib --tests` |
| After step 8 | `cargo test -p liquers-lib --lib --tests` |
| After step 9 | `cargo check -p liquers-axum -p liquers-py`; `cargo check -p liquers-web --target wasm32-unknown-unknown` |
| Step 10 — feature matrix | `cargo test -p liquers-lib --no-default-features --lib --tests`<br>`cargo test -p liquers-lib --no-default-features --features polars --lib --tests`<br>`cargo test -p liquers-lib --lib --tests` |
| Step 10 — wasm suites | Requires the wasm32 target (see step 9). `cargo clean` first, then `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` |
| Documentation | `python3 scripts/docs_index.py --check` |

`CARGO_INCREMENTAL=0` throughout, and `cargo clean` before the wasm loop — the disk allowance is
30 GB and the two target sets do not coexist comfortably (`CLAUDE.md`, "Building and testing").

**Manual check:** none required. Every behaviour in this design is reachable from a test; there is
no UI or HTTP behaviour that a human has to eyeball.

## Agent Assignment

| Step | Model | Skills | Knowledge the agent must read |
|---|---|---|---|
| 0a, 0b | haiku | rust-best-practices | The two issue documents; `extended.rs:130-175`; `metadata.rs:1560-1800` |
| 1 | sonnet | rust-best-practices | `assets.rs:3150-3300`, `:4760-4930`; Phase 2 "Where the invariants are enforced" |
| 2 | sonnet | rust-best-practices | Phase 2 "Data Structures", "Type identifier naming", "`TypeIdentifiedIn`" |
| 3 | sonnet | rust-best-practices, liquers-unittest | Phase 2 "Trait Implementations"; `value.rs` in full — the variant/identifier/format table is the substance |
| 4 | haiku | rust-best-practices | Phase 2 "Trait Implementations"; the four `impl Environment` sites |
| 5 | sonnet | rust-best-practices, liquers-unittest | Phase 2 "Metadata Changes"; `metadata.rs`; the 19 `.media_type` sites |
| 6 | **opus** | rust-best-practices, liquers-unittest | Phase 2 "Where the invariants are enforced" + "Error Handling"; Phase 3 examples 1 and 3; `assets.rs` |
| 7 | sonnet | rust-best-practices | Phase 2 "Level-1 seeding"; `state.rs`; Phase 3 example 2 |
| 8 | sonnet | rust-best-practices, liquers-unittest | Phase 2 "Integration Points"; `liquers-lib/src/value/` in full |
| 9 | haiku | rust-best-practices | The four named call sites |
| 10 | haiku | — | `CLAUDE.md` "Building and testing" |

Step 6 is the only one assigned opus: it is where the P0 is actually closed, it carries the
error-state exception, and a mistake there is the silent-corruption class of bug this whole project
exists to remove.

## Rollback Plan

Each step is a separate commit, and the arcs give three natural rollback points.

| Rollback to | Recovers | Consequence |
|---|---|---|
| Before step 6 | Everything is additive: a new module, three trait methods with a default, an `Environment` method, metadata accessors nobody calls yet | The P0 is unfixed but nothing is worse than HEAD |
| Before step 5 | No metadata field change | The 19 `.media_type` sites revert; the largest single source of churn is undone |
| Before step 3 | No identifier change | The `"generic"` → `"I32"` rename is undone. **After step 3, stored identifiers written by a newer build are not readable by an older one** — accepted by the user, no migration provided |
| Before step 0 | HEAD | — |

The irreversible boundary is **step 3**, and only for data already written by a build that includes
it. Steps 0–2 are safe to keep under any rollback; 0a and 0b are independently useful bug fixes and
step 1 is a pure refactor.

## Phase 5 Entry Criteria

Phase 5 begins when all of the following hold:

1. Steps 0–10 are complete and every command in the Testing Plan passes.
2. The 37 test specifications in Phase 3 exist and pass, including the four that fail at HEAD
   (7.2, 5.5, 8.5, 10.2).
3. `python3 scripts/docs_index.py --check` reports no errors.
4. All review comments on the pull request are answered or incorporated.
5. Any scope not delivered is filed as an issue rather than described only in the PR.

At that point Phase 5 sets `phase: documentation`, writes the summary, creates
`specs/reference/VALUE_TYPE_SYSTEM.md` and `specs/guides/TYPE_SYSTEM_GUIDE.md`, reviews the
`affects_docs` set, updates `specs/README.md` and `CLAUDE.md`, and closes
`CORE-METADATA-FORMAT-TYPE-CONSISTENCY`, `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` and
`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`.
