---
id: RECORD-STREAMS-PHASE4-IMPLEMENTATION
kind: plan
title: Phase 4 implementation plan — record streams
workflow: liquers-project
status: draft
area: [lib/value, core/assets, core/recipes]
created: 2026-09-21
---
# Phase 4: Implementation Plan — Record streams

## Overview

**Feature:** record streams — a chunked, Arrow-compatible tabular value for Liquers
([Phase 1](./phase1-high-level-design.md)).

**Architecture** ([Phase 2](./phase2-architecture.md)): a new crate, `liquers-records`, that
depends on `liquers-core` only and holds three traits: `RecordSource` (async, asked repeatedly),
`RecordStream` (one traversal) and `RecordView` (a finite, synchronous table). It also holds
`RecordBatch` as the materialized view, the table formats and the manifest machinery.
`liquers-lib` adds glue behind a `records` feature: two `ExtValue` variants, the `ns-rec`
commands, and `to_record` / `to_record_source`. `liquers-core` gains two general features: a
recipe provider chain, and `stored` / `cached` flags that the asset manager honours.

**Tests:** [Phase 3](./phase3-tests.md) — 189 tests in 28 files, plus 2 wasm tests. **This plan
specifies 13 more** (§"Tests this plan adds"), covering behaviour that Phase 3 left untested, plus
step-local unit tests whose number the implementing step decides (Milestone 0's conversion tests,
Step 4.3's provider tests).

**Estimated complexity:** High. It adds a new crate of roughly 8–10 k lines including tests,
touches core in two places, adds a feature-gated value variant pair, and adds a wasm handle.

**Estimated time:** 70–100 hours for an experienced Rust developer. With agents, about 25 steps,
each sized to one agent session.

**Prerequisites:**
- Phases 1–3 approved (2026-09-20 / 2026-09-25 / 2026-09-25). Phase 3's additions are absorbed
  into Phase 2 (commit `c2aa049`).
- Open questions: none block implementation. Phase 2 §"Open Questions" 1, 5–9, 12 and 14–17 are
  future extensions, each with a stated default for this version. Question 12 (override
  equivalence) is covered by Phase 3's `assert_reads_agree` tests.
- Dependencies: `bytemuck` 1.25 (already in `Cargo.lock` at 1.25.2), `chrono` 0.4.45, `futures`
  0.3.34, `serde_json`, and `flate2` 1.1 (already in the lockfile). **`flatbuffers` is pinned here
  at 25.12.19**; Phase 2 left its version to this phase.
- Two `liquers-lib` issues must be fixed first (Milestone 0).

## Decisions made in this phase

1. **`RecipeProviderChoice` is not extended.** Phase 2 says it "gains the chain as a choice", but a
   choice is data in a configuration document and cannot name a provider that lives in another
   crate, such as `ManifestRecipeProvider` in `liquers-records`. The chain is built in code instead:
   - `EnvironmentBuilder::with_appended_recipe_provider(provider)` records the provider in a
     builder-held `Vec`; `build()` composes `RecipeProviderChain::new([base, appended…])`, where
     `base` is the configured provider or, when none is, `K::default_recipe_provider()`. A
     `Vec` rather than "push onto the configured chain": the builder holds an
     `Arc<dyn AsyncRecipeProvider>`, which cannot be recognized as a chain without a downcast
     the trait does not offer, nor pushed onto once shared. A later `with_recipe_provider` /
     `with_config` replaces the base and keeps the appended providers.
   - `LibKind::default_recipe_provider()` returns a chain of `[DefaultRecipeProvider,
     ManifestRecipeProvider]` when `records` is on.
   - **Consequence, stated:** the kind's default applies only when *no* provider is configured.
     An environment built with `with_config(…)` or `with_recipe_provider_choice(…)` gets exactly
     the chosen provider and **does not serve keyed chunks** unless the application also calls
     `with_appended_recipe_provider(Arc::new(ManifestRecipeProvider::new()))`. Phase 5's guide
     says so; see §"Open questions from the final review", 3.

   Phase 2's rows for `src/recipes.rs` were corrected at the Phase 4 review (Step 1.4).

2. **Arrow IPC is written without generated code.** `flatbuffers` 25.12.19 is a small runtime with
   no code generator. `ipc.rs` builds and reads Arrow's `Schema`, `Message` and `Footer` tables
   directly, through `FlatBufferBuilder`'s `start_table` / `push_slot` / `end_table` and
   `flatbuffers::Table::get`, with slot offsets taken from Arrow's `Schema.fbs` / `Message.fbs` /
   `File.fbs` and recorded as named constants.
   - This avoids a `flatc` build step, which this environment does not have.
   - It also avoids vendoring about 5 k lines of generated code for a subset that uses perhaps
     twenty fields.
   - Correctness is checked against an independent reader: polars' IPC reader. `ipc` is added
     to the feature list of `liquers-lib`'s **existing optional** `polars` dependency, and the
     cross-check tests are gated on `all(feature = "polars", feature = "records-ipc")`.
     **Not** a `polars` entry in `[dev-dependencies]`: dev-dependencies cannot be optional, so it
     would compile polars into *every* `--tests` build of `liquers-lib` — including the
     `--no-default-features` matrix rows and reduced-feature test runs that exist precisely to
     avoid it — which is the largest avoidable disk and time cost in this plan.

3. **Execution is sequential, in one working tree.** The 30 GB disk allowance holds one `target/`
   directory, not several, so worktree-isolated parallel agents are not used. Steps within a
   milestone are ordered so that each leaves the workspace compiling.

4. **`file_records` joins the `rec` namespace.** It is Phase 3's scenario command: it lists a
   directory key as a table. It is not in Phase 2's commands table, but Phase 3's queries and
   tests use it, and it is the one producer the command set otherwise lacks. It is added to the
   table in Step 5.5.

5. **HTTP streaming of a source is not in this plan.** Phase 2's `liquers-axum` row makes it depend on
   the core-level hook of `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, which is designed
   separately. Until then a source reaches HTTP as a `materialize`d table through the existing
   `BinaryResponse` path, and `liquers-axum` is only compile-checked.

6. **`liquers-py` needs no code change.** Its `Recipe` / `AssetInfo` wrappers hold the Rust values
   (`liquers-py/src/value.rs:59, 71`); there are no struct literals to extend. Step 1.1's
   `cargo check -p liquers-py` is the check. The Arrow C Data Interface export is Phase 2's "later
   milestone" and not part of this plan.

7. **Async commands take `State` and `Context` by value**, as `register_command!` requires. Phase 3
   has already been corrected to match (see its §"Corrections and unexpected learning").

## Tests this plan adds

Phase 3 tests the *defaults* of `stored` / `cached` and every `ManifestSource` data structure.
Three behaviours had no test: that the asset manager honours the flags, that the manifest
provider serves a keyed chunk through `-R/`, and that a whole query evaluates through the
commands. A test-first plan should not leave them to be discovered, so they are specified here and
written in the step that implements them.

| File | Test | Asserts |
|---|---|---|
| `liquers-core/tests/stored_cached_flags.rs` | `stored_false_value_is_not_written` | After evaluating a key whose recipe has `stored: Some(false)`, `store.contains(key)` is `false` — no data **and no metadata-only entry** (the metadata saver must not write either). Assert only after waiting past the `MetadataSaver` interval (100 ms, `assets.rs:946`): natively its writes are debounced in a spawned task, so an immediate assertion would pass before the very write it guards against |
| | `stored_false_still_reads_an_existing_copy` | With data already stored under that key, a request returns it and the recipe's counter command does not run |
| | `cached_false_asset_is_not_reused` | With `stored: Some(false), cached: Some(false)`, two requests run the counter command twice. (`stored` must be false too: a stored copy is preferred to recomputation, so with `stored` absent the second request would be served from the store and prove nothing about the cache) |
| | `cached_true_asset_is_reused` | The control case, with `stored: Some(false)` so reuse can only come from the cache: two requests run it once |
| | `both_false_is_not_volatile` | The resulting metadata has `is_volatile() == false` |
| | `flags_are_recorded_in_metadata_and_asset_info` | `MetadataRecord::stored()` / `cached()` and `AssetInfo`'s equivalents carry the recipe's values |
| `liquers-core/src/recipes.rs` | `appended_provider_is_consulted_after_the_configured_one` | `with_appended_recipe_provider` keeps the configured provider first |
| `liquers-records/src/manifest.rs` | `manifest_spec_serializes_its_discriminator` | A `ManifestSource` serialized to YAML begins with `manifest: record-stream`, and the text reads back through `to_record_source`'s recognition |
| `liquers-records/src/provider.rs` | `shared_arguments_reach_every_template_chunk_recipe` | A template chunk's recipe carries the manifest's `arguments` / `links`, `volatile` and `expires`; an explicit chunk's own `arguments` win over the shared ones |
| `liquers-lib/tests/records_end_to_end.rs` | `template_chunk_key_is_served_by_the_manifest` | With `data/sales/daily.manifest.yaml` (a template over a fixture command `fixture_rows-<offset>-<batch>`) in a memory store, `-R/data/sales/daily_0010.csv` evaluates to the rows of offset `first_offset + 10 × step` |
| | `stored_template_chunk_is_written_under_its_key` | With `stored` absent, the store holds `data/sales/daily_0010.csv` afterwards; with `stored: false`, it does not |
| | `materialize_query_yields_csv_bytes` | `-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv` (an explicit-chunk manifest) returns CSV whose rows are every chunk's rows, in order |
| | `rowid_evaluates_only_its_chunk` | `…/ns-rec/rowid-2-0` runs the fixture command once, for chunk 2 |

**Test file count:** Phase 3's 28 files and 189 tests, plus these 13 tests (2 new files, and one test each
added to `recipes.rs`, `manifest.rs` and `provider.rs`), plus the 2 wasm tests.

**Counting commands without cross-talk.** Tests in one file run in parallel threads, so a single
`static AtomicUsize` shared by the file's tests races. The counting fixture command takes a
`tag: String` argument and counts in a `static Mutex<HashMap<String, usize>>` keyed by it; each test
uses its own tag in its recipe query (`count-t1`, `count-t2`, …) and reads only its own entry. The
same applies to `fixture_rows` in `records_end_to_end.rs`.

## Milestones

| M | What | Crates | Leaves working |
|---|---|---|---|
| 0 | Prerequisite fixes in `liquers-lib`'s value layer | lib | the default lib loop |
| 1 | `stored` / `cached`; the recipe provider chain | core | core tests; lib and py compile |
| 2 | `liquers-records`: buffers, schema, columns, batch, traits, views, mutable | records | `cargo test -p liquers-records` |
| 3 | Table formats: CSV/TSV, NDJSON/JSON, JSON orients, Markdown, HTML, inference | records | the same |
| 4 | Manifest, sources, resolvers, manifest provider | records | the same |
| 5 | `liquers-lib` glue: features, `ExtValue`, conversions, `ns-rec`, the provider chain, registry | lib | the default lib loop and the registry test |
| 6 | Optional formats: Arrow IPC, the Parquet writer, the polars bridge | records, lib | feature rows |
| 7 | `liquers-web`: the `RecordBatch` handle and its tests; wasm size | web | the wasm loops |
| 8 | Build matrix, `CLAUDE.md`, issue statuses, full validation | all | the matrix script |

## Implementation Steps

Every step names its Phase 2 section for signatures and its Phase 3 section for the tests that land
with it. **Tests are copied from `phase3-tests.md` verbatim, then made to pass.** When a test and the
implementation disagree, first decide which one Phase 2 supports:

- if the implementation is wrong, fix the implementation;
- if the test is wrong, fix the test and record the correction for Phase 5 (§"Phase 5 Evidence
  Capture").

Do not weaken a test to get green. Never use `#[ignore]` except for the six tests Phase 3 already
marks, each with its stated reason.

Common rules for every step, from `CLAUDE.md`:
- no `unwrap` / `expect` / `println!` in library code;
- only typed `Error` constructors;
- explicit match arms on enums;
- `eprintln!` for diagnostics;
- run with `CARGO_INCREMENTAL=0` for one-shot builds.

### Milestone 0 — prerequisites in `liquers-lib`

#### Step 0.1 — Scalar hooks for extended values (`EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS`)

**File:** `liquers-lib/src/value/extended.rs`

**Change:**
- `ValueExtension` gains default-refusing hooks: `try_into_i32`, `try_into_i64`, `try_into_f64`,
  `try_into_bool` and the `_option` forms that `ValueInterface` has.
- `CombinedValue`'s `ValueInterface` impl delegates to the hooks for the `Extended` case.
- `TryFrom<CombinedValue<B, E>>` for `i32`, `i64`, `f64`, `bool` **and `String`** delegates to the
  same hooks instead of refusing — and so do the other scalar impls in the same file (`f32`,
  `u32`, `u8`, `extended.rs:332–471`), through the `i64` / `f64` hooks, so no scalar path is left
  refusing.
- The default bodies return the same `conversion_error` as today, so no existing extension changes.

**Tests (new, in the same file):** one per scalar type, each asserting that the `ValueInterface`
path and the `TryFrom` path agree. Cover both a refusing extension and one that implements the
hook; the second is a test-local `ValueExtension`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib value::extended
```

**Issue:** set `status: closed` in `specs/issues/EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS.md`
(`DOCS_STRUCTURE_GUIDE.md` §4.3), with `design: record-streams`.

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: the issue file,
`extended.rs`, `liquers-core/src/value.rs` (`ValueInterface`), `liquers-core/src/commands.rs`
(`CommandArguments::get`, which uses `T::try_from`).

**Rollback:** revert the file; nothing else depends on it until Step 5.3.

#### Step 0.2 — `SimpleValue` reads JSON and YAML (`SIMPLE-VALUE-CANNOT-READ-JSON`)

**File:** `liquers-lib/src/value/simple.rs`

**Change:** `deserialize_from_bytes` accepts `json` and `yaml` and builds the value through the
existing `try_from_json_value` (`simple.rs:284`); YAML goes through `serde_yaml::Value` →
`serde_json::Value`.

**Tests:** a round trip per data format that `SimpleValue`'s `TypeInfo` declares. The loop is
driven by the `TypeInfo`, so a format added later is covered automatically.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib value::simple
```

**Issue:** set `SIMPLE-VALUE-CANNOT-READ-JSON` to `status: closed`.

**Agent:** haiku · liquers-unittest · context: the issue file, `simple.rs`.

**Rollback:** revert the file.

### Milestone 1 — `liquers-core`: flags and the provider chain

#### Step 1.1 — `stored` and `cached` on `Recipe`, `MetadataRecord`, `AssetInfo`

**Files:** `liquers-core/src/recipes.rs`, `liquers-core/src/metadata.rs`

**Change:** Phase 2 §"C. `stored` and `cached` in the asset manager":
- `Option<bool>` fields with `#[serde(default, skip_serializing_if = "Option::is_none")]`;
- `stored()` / `cached()` accessors defaulting to `true`, on the three structs **and on the
  `Metadata` enum** (delegating to the record; `LegacyMetadata` answers `true`), because the
  asset-manager sites of Step 1.2 hold a `Metadata`, not a `MetadataRecord`;
- `MetadataRecord::get_asset_info` (`metadata.rs:1097`) and `Recipe::get_asset_info`
  (`recipes.rs:364`) copy both fields;
- struct literals gain the fields — the compiler finds them: `metadata.rs:758, 1081, 1098`,
  `recipes.rs:141, 398–449`, `validate/mod.rs:267`, and each `default_metadata` in `store.rs`,
  where `..Default::default()` suffices.

**Tests:** Phase 3 §4.2 (10 tests in `recipes.rs` / `metadata.rs`).

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib
cargo check -p liquers-py -p liquers-store -p liquers-axum
```

**Agent:** haiku · rust-best-practices, liquers-unittest · context: Phase 2 §C, Phase 3 §4.2.

**Rollback:** revert both files. No serialized data changes shape, because the fields are absent
when `None`.

#### Step 1.2 — The asset manager honours the flags

**File:** `liquers-core/src/assets.rs`

**Change:** Phase 2 §C's table. Line numbers are as of the Phase 4 review.

- **Where the flags come from — decided once, when the keyed asset is created.** Both managers'
  `get_resource_asset` (`DefaultAssetManager` `:4928`, `ImmediateAssetManager` `:6493`) already
  resolve the key's recipe (through `is_volatile` → `recipe_opt`). Resolve it **once** there,
  and write `stored` / `cached` into the new asset's `MetadataRecord` before anything can
  persist it. **Not** in `resolve_volatility_before_evaluation` (`:1890`): that runs as step 1 of
  `evaluate`, while `lock.recipe` is still the ad-hoc key recipe — the provider's recipe replaces
  it only in step 2 (`:2703–2710`) — so the flags read there are always `None`. Also copy them
  where the recipe is replaced (`:2705`), so the metadata agrees with the authoritative recipe.
- **`stored: false`** — every store write for the asset's key reads `metadata.stored()`:
  - `AssetData::save_metadata_to_store` (`:971`), the `MetadataSaver` path the service loop calls
    on every status, log and progress change (`:2337–2383`). **Easy to miss, and the most
    frequent writer:** left alone it leaves a metadata-only entry for a key that was never
    stored, which then reloads through the corrupted-data branch
    (`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED`);
  - `save_to_store` (`:2902`);
  - `set_state` in both managers (`:5696–5712`, `:6806–6812`) and `set_binary` (`:5570–5575`,
    `:6746–6751`). These are explicit writes of a caller-supplied state, whose recipe is the
    ad-hoc `key.into()`; they read the **supplied** metadata's `stored()`, which is `true` unless
    the caller set it. An `Override` must stay writable — it is why a stored copy is preferred.

  Reading an existing stored copy is unchanged. `mark_expired_status` (`:3280`) already writes
  only when the key is persisted.
- **`cached: false`** — the asset is not registered for reuse. The registering sites are
  `get_nonvolatile_resource_asset` (`:4884`, `entry_async … or_insert_with`) and
  `ImmediateAssetManager::get_resource_asset` (`:6503–6510`, `map.insert`) — **not**
  `try_insert_key_asset` (`:4550`, `:6424`), which only `set_state` calls. Model it on the
  volatile path (`get_volatile_resource_asset`, `:4908`; `make_volatile`): a fresh, unregistered
  asset per request, **without** marking it volatile.
- **`save_to_store`'s ownership check** (`:2946–2960`) records a warning when a non-volatile keyed
  asset is not the registered owner. An uncached asset is never registered by design, so skip the
  warning when `!cached()`, as it is skipped for volatile assets — otherwise every
  stored-but-uncached chunk carries a spurious `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` warning.
- Neither flag makes the asset volatile.

**Tests:** the six `stored_cached_flags.rs` tests of §"Tests this plan adds". They use an
`AsyncMemoryStore`, the tag-keyed counting command (§"Tests this plan adds"), and a test recipe
provider serving one key. Both managers change, so each test runs against **both** kinds —
`SimpleEnvironment<Value>` (`Queued`) and `GenericEnvironment<Value, (), Inline>` — through one
generic helper.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
```
The full core suite must pass, not only the new tests: `assets.rs` carries the lifecycle
invariants of `ASSET_LIFECYCLE.md`.

**Issue:** `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT` is set to `status: closed`.

**Agent:** sonnet · rust-best-practices, liquers-unittest · context:
- Phase 2 §C;
- `specs/reference/ASSETS.md` and `specs/reference/ASSET_LIFECYCLE.md`;
- `assets.rs` around each named site;
- `liquers-core/tests/async_hellow_world.rs` for the environment pattern.

**Rollback:** revert `assets.rs`; Step 1.1's fields remain harmless.

#### Step 1.3 — `RecipeProviderChain` and appending a provider

**Files:** `liquers-core/src/recipes.rs`, `liquers-core/src/environment_builder.rs`,
`liquers-core/src/context.rs`

**Change:**
- `RecipeProviderChain<E>` and its `AsyncRecipeProvider` impl follow Phase 2 §"B" and §"Construction
  helpers, options and the provider chain", with the per-target `async_trait` attributes the trait
  uses (`recipes.rs:464–465`).
- `EnvironmentBuilder::with_appended_recipe_provider(Arc<dyn AsyncRecipeProvider<…>>)`: pushes
  onto a builder-held `Vec`; `build()` (`environment_builder.rs:343–345`) composes
  `RecipeProviderChain::new([base, appended…])` when the `Vec` is non-empty, with `base` the
  configured provider or `K::default_recipe_provider()` (decision 1).
- `GenericEnvironment::with_appended_recipe_provider(&mut self, Box<dyn …>)` beside
  `with_recipe_provider` (`context.rs:1170`): replaces `self.recipe_provider` with
  `RecipeProviderChain::new([current, new])`. A chain nested in a chain is correct, merely one
  indirection deeper.
- `get_asset_info` goes to the provider that has the recipe.

**Tests:** Phase 3 §4.1 (5 tests) and `appended_provider_is_consulted_after_the_configured_one`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib recipes
```

**Issue:** `NO-RECIPE-PROVIDER-CHAIN` is set to `status: closed`.

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: Phase 2 §B, Phase 3 §4.1,
`environment_builder.rs:85–110, 190–350`, `context.rs:1170–1210`.

**Rollback:** revert the three files.

#### Step 1.4 — Correct Phase 2's row for `recipes.rs` — **done at the Phase 4 review**

The three places in `phase2-architecture.md` that described `RecipeProviderChoice` gaining the
chain (the Integration Points rows for `src/recipes.rs` and for `src/context.rs` /
`src/environment_builder.rs`, and §B) now name `with_appended_recipe_provider` and say
`RecipeProviderChoice` is unchanged; Phase 2's changelog records it (2026-09-25, Phase 4 review).
Nothing is left to do here; the step is kept so the numbering of later steps is stable.

### Milestone 2 — `liquers-records`: the data model

#### Step 2.1 — Create the crate; `buffer.rs`

**Files:**
- `liquers-records/Cargo.toml` — new:
  ```toml
  [dependencies]
  liquers-core = { path = "../liquers-core" }
  serde = { version = "1.0.229", features = ["derive", "rc"] }
  serde_json = "1.0.151"
  serde_yaml = "0.9.34"      # the manifest provider parses *.manifest.yaml and recipes.yaml
  futures = "0.3.34"
  chrono = "0.4.45"
  bytemuck = "1.25"
  async-trait = "0.1.92"     # ManifestRecipeProvider implements core's #[async_trait] trait
  scc = "3.8.8"              # the provider's manifest cache (Phase 3 §1.2 sketch)
  flatbuffers = { version = "25.12.19", optional = true }
  flate2 = { version = "1", optional = true }

  [dev-dependencies]
  tokio = { version = "1.53.1", features = ["macros", "rt"] }   # #[tokio::test], tokio::spawn

  [features]
  default = []
  ipc = ["dep:flatbuffers"]
  parquet = ["dep:flate2"]
  ```
  Phase 2's Cargo block lists fewer: `derive` (core uses `serde_derive` separately, so nothing
  guarantees it by unification), `serde_yaml`, `async-trait`, `scc` and the `tokio`
  dev-dependency are what the later steps and Phase 3's tests need. All are already in the
  lockfile at these versions, so nothing new is downloaded.
- `liquers-records/src/lib.rs` — module declarations and re-exports only, for now, **plus
  `extern crate self as liquers_records;`**. Phase 3's in-crate unit tests (§3.1–§3.8) name the
  crate by its external path (`use liquers_records::{…}`), which does not resolve inside the crate
  without that line; with it, they are copied verbatim;
- `liquers-records/src/buffer.rs`;
- the workspace `Cargo.toml` — add `liquers-records` to `members` and `default-members`.

**Change:**
- `AlignedBuffer` (64-byte aligned), `Buffer<T: bytemuck::Pod>`, and `Bitmap` with the full method
  list (Phase 2 §"Function Signatures"; §"Bitmap"; §"64-byte alignment").
- `Bitmap::set` panics on an out-of-range index, as slice indexing does. That is the only
  deliberate panic in the crate, and it is documented.

**Tests:** Phase 3 §2.2 (14).

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-records --lib
cargo check -p liquers-records --target wasm32-unknown-unknown
```

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: Phase 2 §"Columns, not rows",
§"Bitmap", §"64-byte alignment", §"The features"; Phase 3 §2.2.

**Rollback:** remove the crate directory and the workspace entries.

#### Step 2.2 — `schema.rs`

**File:** `liquers-records/src/schema.rs`

**Change:**
- `RecordSchema`, `FieldSchema`, `FieldType`, `FieldRole`, `KeyRole` and `VectorMetric`, with
  their serde defaults (Phase 2 §"RecordSchema — three axes"; §"The identity field";
  §"Field resolution").
- The construction helpers (§"Construction helpers…").
- `RecordSchema::new`'s validation: at most one `Id`, at most one `Source`, and an `Id`'s role
  consistent with it being indexed exactly and stored.
- Qualified-name resolution, where an ambiguous name is an error naming every candidate.

**Tests:** Phase 3 §2.1 (13).

**Validation:** `cargo test -p liquers-records --lib schema`

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: the Phase 2 sections named,
Phase 3 §2.1.

**Rollback:** remove the module.

#### Step 2.3 — `lib.rs`: values, columns, the batch, ids, origins, the traits

**File:** `liquers-records/src/lib.rs`

**Change:**
- `FieldValue`;
- `Column` and its kernels (`len`, `get`, `slice`, `take`, `filter`, `compare`, `null_mask`,
  `concat`) and `CompareOp`;
- `RecordBatch` with `new` / `concat`;
- `RowId`, `RowRun`, `ChunkId`, `ChunkOrigin`, `LocatorRule`, `ChunkList`, `ChunkDescriptor`;
- the traits `RecordView` (with its provided methods) and `RecordStream`, with
  `BoxRecordStream`, `record_stream` and `RecordStreamExt`.
- **Not `RecordSource`** — it names `ChunkResolver` (Step 2.6) and `ManifestSpec`, so it lands in
  Step 2.6 with them.

Phase 2 sources: §"The types", §"Every row has an implicit id", §"Columns, not rows",
§"FieldValue", §"ChunkOrigin", §"Provenance and validity", §"Why `liquers-core` needs no stream
alias".

`RecordView::materialize` and `as_batch` are provided methods here; a `RecordBatch` overrides both.

**Tests:** Phase 3 §2.3 (11).

**Validation:**
```bash
cargo test -p liquers-records --lib
cargo check -p liquers-records --target wasm32-unknown-unknown
```
The wasm check proves that the `MaybeSend` / `MaybeSync` supertraits hold on both targets.

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: the sections named, Phase 3
§2.3, `liquers-core/src/maybe_send.rs`.

**Rollback:** revert to Step 2.2.

#### Step 2.4 — `views.rs`

**File:** `liquers-records/src/views.rs`

**Change:**
- The built-in views: `ColumnsView`, `RowRangeView`, `RowIndexView`, `DerivedColumnView`,
  `AppendedColumnsView`, `RowFnView`.
- Their `impl dyn RecordView` constructors (`select_columns` keeps the `Id` and `Source` columns;
  `filter`, `slice`, `with_column`, `concat`).
- Implicit row ids and row numbers.
- Scalar reading: one row × one payload column reads as its cell would; anything larger refuses,
  naming its shape.

Phase 2 sources: §"Views", §"Building views", §"Writing a view", §"A view as a value".

**Tests:** Phase 3 §2.4 (11) and §2.5 (4).

**Validation:** `cargo test -p liquers-records --lib views`

**Agent:** sonnet · rust-best-practices, liquers-unittest.

**Rollback:** remove the module.

#### Step 2.5 — `mutable.rs`

**File:** `liquers-records/src/mutable.rs`

**Change:** `RecordViewMut` (`append_row`, `reserve`, `with_capacity`), `RecordBatchMut` and
`ColumnMut`, using the `BytesMut` → freeze idiom; `RecordBatch::into_mut` takes over buffers the
batch holds alone (Phase 2 §"Writing a view").

**Tests:** Phase 3 §2.6 (8).

**Validation:** `cargo test -p liquers-records --lib mutable`

**Agent:** sonnet · rust-best-practices, liquers-unittest.

**Rollback:** remove the module.

#### Step 2.6 — `value.rs`: `RecordValue` and `ChunkValue`; the resolver trait

**Files:** `liquers-records/src/value.rs`, `liquers-records/src/lib.rs`,
`liquers-records/src/manifest.rs` (new, declarations only)

**Change:**
- `RecordValue` (a supertrait of `ValueInterface`), `ChunkValue`, and the object-safe
  `ChunkResolver` trait (Phase 2 §"`ChunkResolver`", §"Value extension").
- The `RecordSource` trait, with its provided `materialize` (moved here from Step 2.3).
- `ManifestSpec` and `ChunkTemplate` as **plain serde declarations**, because
  `RecordSource::manifest` returns `Option<&ManifestSpec>`. Their behaviour — `ChunkNaming`,
  `query_at`, validation — is Step 4.1's.

The concrete resolvers wait for Step 4.2.

**Tests:** compile-only at this step. The records crate cannot test `RecordValue` against a real
value: core's `Value` has no variant that can hold a view, so a test-only impl could not honestly
implement `from_record_view`. Its tests use `ChunkResolver` fixtures that return `ChunkValue`
directly. `RecordValue` and the two resolvers are exercised in `liquers-lib`, whose `Value`
implements the trait (Steps 5.2 and 5.6).

**Validation:**
```bash
cargo test -p liquers-records --lib
cargo check -p liquers-records --target wasm32-unknown-unknown
```

**Agent:** sonnet · rust-best-practices — raised from haiku now that the step carries
`RecordSource`, whose `self: Arc<Self>` receivers, `'static` `BoxFuture`s and provided
`materialize` are the object-safety-sensitive part of the crate.

**Rollback:** remove the module.

### Milestone 3 — table formats

#### Step 3.1 — `formats/mod.rs`, `infer.rs`, `csv.rs`

**Files:** `liquers-records/src/formats/{mod,infer,csv}.rs`

**Change:**
- `TableFormat` with `from_data_format`, `ReadSchema::{Declared, Infer}`, `ReadOptions` /
  `WriteOptions` with hand-written `Default`, and `read_table` / `write_table`.
- Schema-less inference: the canonical-`Int` rule, so `01234` stays text; ISO dates and timestamps.
- CSV and TSV: hand-written, with the PostgreSQL null convention, where an empty unquoted field
  is null and `""` is the empty string.
- The schema-aware reader: declared types, strict, one pass.

Phase 2 sources: §"Table formats", §"Two readers", §"Inference".

**Tests:** Phase 3 §3.1 (13) and §3.6 (3).

**Validation:** `cargo test -p liquers-records --lib formats`

**Agent:** sonnet · rust-best-practices, liquers-unittest.

**Rollback:** remove the `formats` module.

#### Step 3.2 — `ndjson.rs` and `shapes.rs`

**Files:** `liquers-records/src/formats/{ndjson,shapes}.rs`

**Change:** NDJSON, and the `json` format as a records array. `JsonOrient` with `to_json` /
`from_json` for records, list, split, values, columns, index, table and auto; `auto` refuses the
ambiguous shape. See Phase 2 §"JSON shapes are conversions".

**Tests:** Phase 3 §3.2 (3) and §3.3 (12).

**Agent:** sonnet · rust-best-practices, liquers-unittest.

#### Step 3.3 — `markdown.rs` and `html.rs`; round trips

**Files:** `liquers-records/src/formats/{markdown,html}.rs`,
`liquers-records/tests/format_round_trip.rs`

**Change:** both formats are write-only and use field labels. HTML escapes every cell and header
(Phase 2 §"Markdown and HTML").

**Tests:** Phase 3 §3.4 (3), §3.5 (4) and §7 (8).

**Validation:**
```bash
cargo test -p liquers-records --lib --tests
cargo check -p liquers-records --target wasm32-unknown-unknown
```

**Agent:** haiku · liquers-unittest.

### Milestone 4 — manifests and sources

#### Step 4.1 — `manifest.rs`

**File:** `liquers-records/src/manifest.rs`

**Change:**
- `ManifestSpec` (declared in Step 2.6): the version envelope — the `manifest: record-stream`
  discriminator and `version` — is **tolerated, not modelled** (no `deny_unknown_fields`), so an
  unknown `version` reads as the latest (Phase 3 §2.7, §9).
- `ChunkTemplate` and `ChunkNaming`: `<prefix>_{n:04}.<extension>`, `index_of`, and collision
  checks.

Sources: Phase 2 §"A. Chunk keys" and §"Construction helpers…"; `manifest-format.md`.

**Tests:** the Phase 3 §2.7 and §9 tests that do not name `ManifestSource` — §2.7's eight
`ChunkNaming` / `ChunkTemplate` / `ManifestSpec` tests and §9's four others. The five that
construct a `ManifestSource` (§2.7's `manifest_source_*` ×4, §9's
`explicit_chunk_name_collisions_are_refused_at_load`) need Step 4.2's type and land there; a
test module is compiled whole, so pasting them here would stop this step compiling.

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: add `manifest-format.md` to
the sections named.

#### Step 4.2 — `sources.rs`

**File:** `liquers-records/src/sources.rs`

**Change:**
- **`ManifestSource`**: `new`, `with_key`, `spec`; `stream` resolves **one chunk at a time, in
  order**, through `futures::stream::unfold`, following Phase 3 §1.2's corrected sketch; `chunks`,
  `describe_chunk`, `schema`, `truncated`, `manifest` and `materialize` (default limit
  1 000 000 rows; refuses a non-uniform source, naming the first differing field).
- **Reading a chunk:**
  - A stored keyed chunk is read by `read_resource` and parsed with `uniform_schema`.
  - A keyed chunk that is not stored is evaluated and then checked against the schema.
  - An unkeyed chunk is evaluated.
- **`InMemorySource`** and the wrapping sources.
- **`ContextResolver<E>`**: records each chunk as a dependency of the context's asset.
- **`EnvResolver<E>`**: records nothing.
- Both resolvers have the bound `E::Value: RecordValue`.

Sources: Phase 2 §"The types", §"`ChunkResolver`", §"Why a *source* makes this work", §"A source
serializes only as its manifest".

**Tests:**
- the five `ManifestSource` tests deferred from Step 4.1 (§2.7 ×4 into `manifest.rs`'s test
  module, with `use crate::ManifestSource;`, and §9 ×1);
- Phase 3 §5.6 (3) and §5.7 (1);
- all of §6's native tests (9: `RECORDS01` ×2, `02`, `04`, `05`, `06`, `07`, `08`, `11`). They
  form one file, `liquers-records/tests/records_guide_counterparts.rs`, and need the data model
  and sources only. §6's two wasm tests land in Step 7.1, and `RECORDS03` / `RECORDS10` belong to
  §3.7 / §5.5.
- `ContextResolver` and `EnvResolver` are **compile-checked** here. §5.8's behaviour tests need
  the lib `Value` (see Step 2.6), so they move to `liquers-lib` and land in Step 5.6.

**Validation:** `cargo test -p liquers-records --lib --tests`

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: add
`liquers-core/src/context.rs` (dependency recording) to the sections named.

#### Step 4.3 — `provider.rs`

**File:** `liquers-records/src/provider.rs`

**Change:** `ManifestRecipeProvider`, implementing `AsyncRecipeProvider<E>` for any `E`:
- It reads the folder's `*.manifest.yaml` and caches parsed manifests by key and stored version.
- `recipe_opt` serves explicit and template chunks, with `cwd` set and the manifest's `stored`
  and `cached` copied onto the recipe. Phase 2 §B also names the manifest's `expires` and
  `volatile`, but `ManifestSpec` as specified has neither field (nor `manifest-format.md`'s shared
  `arguments` / `links`) — see §"Open questions from the final review", 1; copy
  them only if that adds them.
- `contains` matches without enumerating.
- `assets_with_recipes` lists only explicit chunks.
- A chunk name that a sibling `recipes.yaml` also defines is refused. This is Phase 3 §9's open
  note; its test is written here against a memory store holding both files. The provider reads
  the sibling list with core's public `DefaultRecipeProvider::get_recipes` (`recipes.rs:627`).
- **Lookup cost — the provider sits on the hot path.** In the lib chain it is consulted for every
  key `recipes.yaml` does not define, i.e. every plain `-R/…` file, and each `get(key)` resolves
  the recipe more than once (`is_volatile`, then evaluation). So:
  - a name matching `<prefix>_<digits>.<ext>` checks only `<folder>/<prefix>.manifest.yaml`
    (one `contains`), never a listing;
  - an explicit-chunk lookup needs the folder's `*.manifest.yaml` names; cache that listing per
    folder beside the parsed manifests, keyed by the stored version, rather than listing on every
    call;
  - a store whose `listdir` is the default empty answer (`store.rs:486`) simply has no manifests.

  Performance-sensitive per `CLAUDE.md` ("Asset lookups in `AssetManager`"); Step 5.6's tests
  catch correctness, not cost, so the step's review checks this explicitly.

Source: Phase 2 §"B".

**Tests:** the §9 collision test, and the provider's own unit tests: serving an explicit chunk, a
template chunk, `contains` without enumeration, and `assets_with_recipes` listing only explicit
chunks, over an `AsyncMemoryStore`. Phase 3 §1.2's `ManifestRecipeProvider` code is the
implementation sketch this step fills in. §1.2's *tests* are in a `liquers-lib` file and land in
Step 5.4.

**Validation:** `cargo test -p liquers-records --lib --tests`

**Agent:** sonnet · rust-best-practices, liquers-unittest.

**Issue:** set `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY` to `status: closed` if the
override resolves it for this provider; otherwise note the partial resolution in the issue.

### Milestone 5 — `liquers-lib` glue

#### Step 5.1 — Features and the glue module

**Files:** `liquers-lib/Cargo.toml`, `liquers-lib/src/lib.rs`, `liquers-lib/src/records/mod.rs`

**Change:**
- The features, forwarding to the records crate:
  ```toml
  default = ["egui", "image-support", "polars", "records"]
  records = ["dep:liquers-records"]
  records-ipc = ["records", "liquers-records/ipc"]
  records-parquet = ["records", "liquers-records/parquet"]
  # [dependencies]
  liquers-records = { path = "../liquers-records", optional = true }
  ```
  `records-ipc` and `records-parquet` join `default` in Steps 6.1 and 6.2, when their formats
  exist — so that "after M5: records usable from queries, without IPC or Parquet"
  (§"Partial completion") is true, and no build advertises a format it cannot write. **No
  `polars` dev-dependency** (decision 2); Step 6.1 adds `ipc` to the optional `polars` entry.
- `pub mod records` gated on `records`, containing `pub use liquers_records::*;` — a re-export
  inside the module, **not** at the crate root (E0255).
- **Not** `impl RecordValue for Value` yet: its methods build and match the `ExtValue` variants,
  which Step 5.2 adds. It lands there.

**Validation:**
```bash
cargo check -p liquers-lib
cargo check -p liquers-lib --no-default-features
```

**Agent:** haiku · rust-best-practices.

#### Step 5.2 — The `ExtValue` variants

**File:** `liquers-lib/src/value/mod.rs`

**Change:**
- `ExtValue::RecordView { value: Arc<dyn RecordView> }` and
  `ExtValue::RecordSource { value: Arc<dyn RecordSource> }`, both cfg-gated — **struct variants**,
  as every existing `ExtValue` variant is and as Phase 2 §"Value extension" writes them.
- `impl RecordValue for Value` in `records/mod.rs` (moved from Step 5.1).
- A gated arm in every exhaustive match: `type_name`, `type_identifier`, `as_bytes`,
  `type_descriptions` and the `ExtValueInterface` conversions in `value/mod.rs`, **and** the
  matches outside it — `ui/web/html.rs::ext_to_html` (compiled only with `webui`, so the default
  build does not see it) and `egui/mod.rs`'s `show`. `liquers-web/src/default_value.rs` is
  Step 7.1's. See Phase 2 §"Feature-gating discipline".
- **Add the `liquers-lib` records rows to `scripts/check-build-matrix.sh` now** (Phase 2
  §"Feature-gating discipline": `records`, `records,polars`, `webui,records`, and the two wasm32
  `webui,records…` rows), not in Step 8.1: the existing rows all have `records` off, so without
  them the script below cannot see the `webui` + `records` arm this step adds.
- Both `TypeInfo`s, with identifiers `RecordView` and `RecordSource`:
  - `RecordView` writes every format its features provide.
  - `RecordSource` writes only `yaml` / `json` (the manifest).
- The `DefaultValueSerializer` arms.
- Deserialization: `type_identifier: RecordSource` reads back as a `ManifestSource`.

**Tests:** Phase 3 §5.1 (2) and §5.2 (3).

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
```
Run the matrix script here, not only at the end: this is the step that breaks reduced builds.

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: add
`specs/guides/TYPE_SYSTEM_GUIDE.md` (the four steps) to the Phase 2 sections named.

#### Step 5.3 — Scalar reading through the hooks

**File:** `liquers-lib/src/value/mod.rs`

**Change:** `ExtValue`'s implementation of Step 0.1's hooks. A single-cell `RecordView` answers
`try_into_i64`, `try_into_f64` and the others as its cell would, and refuses with its shape
otherwise.

**Tests:** Phase 3 §5.5 (3).
- The linked-`f64` test's `#[ignore]` **is removed** here, because Step 0.1 fixed its blocker —
  and its `todo!()` body is **written**: a recipe whose `links:` binds an `f64` argument of a
  fixture command to a one-cell view query, evaluated end to end. Removing the `#[ignore]` alone
  would turn a sketch into a panic.
- `multi_row_view_refuses_scalar_read_naming_its_shape` as drafted builds its own error and
  checks that; strengthen it to call `Value::try_into_f64` (and the `TryFrom` path) on the
  two-row view and assert the refusal names the shape. Strengthening is not weakening.

**Agent:** haiku · liquers-unittest.

#### Step 5.4 — `convert.rs`

**File:** `liquers-lib/src/records/convert.rs`

**Change:** `to_record` and `to_record_source` with `ToRecordOptions`, per Phase 2 §"What a record
command accepts":
- **Accepted inputs:** views, sources, bytes, text, JSON values and keys.
- **Keys:** fetched through the asset manager.
- **Manifests:** recognized by the discriminator, then `ManifestSource::with_key(metadata key)`.
  Phase 3 §1.2's sketch handles **text** only. Add the **JSON-value** branch: after Step 0.2 a
  stored `*.manifest.yaml` with no type metadata loads as a structured `SimpleValue`, not text,
  so `materialize_query_yields_csv_bytes` (Step 5.6) goes through `try_into_json_value` →
  `serde_json::from_value::<ManifestSpec>`.
- **A `RecordSource` value that is a keyless manifest** — what `deserialize_from_bytes` returns
  for a stored `type_identifier: RecordSource`, since it receives no metadata — is **re-keyed**,
  not taken as-is: when `source.manifest()` is `Some(spec)`, rebuild it as
  `ManifestSource::new(spec.clone(), metadata.key()?)`. Otherwise a
  manifest written by Liquers and read back would silently treat its keyed chunks as unkeyed.
- **Format:** taken from the options, else from the metadata; never sniffed.

**Tests:** Phase 3 §5.3 (3), §5.4 (1), and all 9 of §1.2 (`liquers-lib/tests/record_manifest_keyed.rs`:
6 `ManifestSource` tests and 3 `to_record_source` tests — one file, landed together).

**Agent:** sonnet · rust-best-practices, liquers-unittest.

#### Step 5.5 — The `ns-rec` commands

**Files:** `liquers-lib/src/records/commands.rs`, `liquers-lib/src/commands.rs`,
`liquers-lib/src/bin/export_command_registry.rs`, `specs/command_registry.yaml`

**Change:**
- Every command in Phase 2 §"Relevant Commands" — `rec_id`, `row`, `select_columns`, `head`,
  `slice`, `rowid`, `to_record_source`, `materialize`, `records_schema`, `to_json`, `from_json`,
  `to_record` — plus `file_records` (decision 4), 13 in all, in namespace `rec`.
- The command functions share names with the `convert.rs` helpers (`to_record`,
  `to_record_source`), and `register_command!` has no renaming. Keep the commands in
  `records/commands.rs` and call the helpers by path (`super::convert::to_record_source(…)`);
  never glob-import `convert` there.
- A `#[macro_export] register_records_commands!` in `records/commands.rs`, following
  `polars/mod.rs`. `register_all_commands!` gains a call to it, which — like its polars call
  (see the comment in `tests/registry_export.rs:40`) — compiles only when the feature is on.
- A `Group::Records` in the exporter: an **ungated** variant, with its arm body and
  `is_compiled_in` gated on `records`, as `Group::Polars` is (`export_command_registry.rs:45–83`);
  `Group::all()` grows to six.
- Regenerate the registry and add a changelog line between the markers.
- `tests/registry_export.rs`: `full_registry()` gains
  `#[cfg(feature = "records")] liquers_lib::register_records_commands!(cr)?;`, the non-empty test
  gains a `rec` anchor command, and the freshness gate becomes `all(egui, image-support, polars,
  records)`.
- **`schema`'s spelling, left to this step by Phase 2** (§"What a record command accepts": "how
  the macro spells an optional value-typed argument is a Phase 4 detail"). `from_json` and
  `to_record` need an argument that is absent by default and linkable. Settle it **before**
  regenerating the registry, because it fixes the signature `liquers-validate` checks: try
  `schema: Option<Value>` against `register_command!`; if the macro cannot bind it, use
  `schema: String = ""` (empty meaning none; a linked schema document arrives as its text). See
  §"Open questions from the final review", 2.
- Phase 2's command table gains the `file_records` row.

**Tests:**
- Phase 3 §1.1 (8).
- Run `liquers-validate` over every query in `phase3-examples.md` §"Queries used" **with no
  `--command` flags**. All must pass against the regenerated registry.

**Validation:**
```bash
cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml
cargo test -p liquers-lib --test registry_export
cargo run -p liquers-core --features cli --bin liquers-validate -- --query-file <queries.txt>
```

**Agent:** sonnet · rust-best-practices, liquers-unittest, liquers-validate · context: add
`specs/reference/REGISTER_COMMAND_FSD.md` and `liquers-lib/src/polars/mod.rs` (the macro pattern)
to the Phase 2 sections named.

#### Step 5.6 — The manifest provider in the library environment; end to end

**Files:** `liquers-lib/src/environment.rs`, `liquers-lib/tests/records_end_to_end.rs`

**Change:** with `records` on, `LibKind::default_recipe_provider()` returns the chain
`[DefaultRecipeProvider, ManifestRecipeProvider]`.

The end-to-end tests must build their environment with `DefaultEnvironment<Value>` or
`default_environment_builder()` **without** configuring a recipe provider: only `LibKind`'s
default carries the manifest provider (decision 1). `SimpleEnvironment` resolves no recipes at
all. The `fixture_rows` command counts per test tag (§"Tests this plan adds").

**Tests:**
- the four `records_end_to_end.rs` tests of §"Tests this plan adds";
- Phase 3 §5.8, **relocated** to `liquers-lib/tests/resolver_dependency_recording.rs` and written
  out as real tests. `ContextResolver::evaluate(q)` adds `q` to the calling asset's
  dependencies; `EnvResolver` adds nothing. Both run through `evaluate` and a probe command that
  opens a stream. Their `#[ignore]`s are removed, and they must pass: once the resolvers exist, a
  failure is a bug, not a missing fixture.

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · rust-best-practices, liquers-unittest.

**Rollback (Milestone 5):** turning `records` off in `default` removes all of it from the default
build. Reverting the milestone's commits removes it entirely.

### Milestone 6 — optional formats

#### Step 6.1 — Arrow IPC (`ipc` feature)

**Files:** `liquers-records/src/formats/ipc.rs`, `liquers-lib/Cargo.toml`

**Cargo:** `records-ipc` joins `liquers-lib`'s `default`; `"ipc"` joins the feature list of the
existing optional `polars` dependency (decision 2). Declare the module as
`#[cfg(feature = "ipc")] mod ipc;` in `formats/mod.rs` — Phase 3 §3.7's file-level
`#![cfg(feature = "ipc")]` then either stays as the file's first line or is dropped as redundant;
an inner attribute below any item does not compile.

**Change:** the Arrow IPC file format, Feather v2, following decision 2:
- It writes the subset Phase 2 §"Still no claim of full Arrow support" allows.
- It reads the same subset, and refuses a dictionary-encoded or compressed batch, naming what it
  found.
- The schema and roles survive through Arrow's `custom_metadata` under a `liquers.` prefix.

**Tests:**
- Phase 3 §3.7 (4, 2 of them fixture-dependent — see below);

**The fixture tests:** polars 0.55.2 has an `ipc` feature and `IpcWriter::with_compression`
(verified in `polars-io-0.55.2/src/ipc/write.rs`).
- Generate the compressed-body fixture with it in a `liquers-lib` test helper gated on
  `all(feature = "polars", feature = "records-ipc")` and itself `#[ignore]`d (run once with
  `-- --ignored`; a routine test must not write into the source tree), commit the file under
  `liquers-records/tests/fixtures/`, and remove the reading test's `#[ignore]`; it reads the file
  with `include_bytes!`.
- For the dictionary-encoded fixture, cast a column to polars' `Categorical`, which IPC writes
  dictionary-encoded.
- **If either fixture cannot be produced**, the test stays ignored with its reason, and an issue
  is filed: `IPC-READER-UNTESTED-ON-DICTIONARY-OR-COMPRESSED-INPUT`, with `priority: P3`. The
  refusal code path is still exercised by a unit test that builds a minimal flatbuffer message
  claiming dictionary encoding.

**Validation:**
```bash
cargo test -p liquers-records --features ipc --lib --tests
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · rust-best-practices, liquers-unittest · context: Phase 2 §"Arrow
interoperability"; the Arrow columnar spec and `.fbs` files (cite them in comments by field name).

#### Step 6.2 — Parquet writer (`parquet` feature); the polars bridge

**Files:** `liquers-records/src/formats/{parquet,thrift}.rs`, `liquers-lib/src/records/polars.rs`

**Change:**
- A minimal Parquet writer: one row group, PLAIN encoding, gzip through `flate2`, and a
  hand-written Thrift compact encoder for the footer. A `Vector` column is refused.
- `RecordBatch → DataFrame` and `DataFrame → RecordBatch`; Parquet is read through the latter.

**Cargo:** `records-parquet` joins `liquers-lib`'s `default`.

**Tests:** Phase 3 §3.8 (2).
- `parquet_writer_refuses_vector_columns` lands in `liquers-records/src/formats/parquet.rs` as
  drafted.
- `parquet_round_trip_through_polars_preserves_data` **cannot stay where Phase 3 put it**: it is
  gated `#[cfg(feature = "polars")]` inside `liquers-records`, which has no `polars` feature and
  must never depend on polars, so it would never compile. It **moves** to
  `liquers-lib/tests/records_parquet_polars.rs`, gated
  `#![cfg(all(feature = "records-parquet", feature = "polars"))]`, is written out against the
  bridge, and its `#[ignore]` is removed.

**Validation:**
```bash
cargo test -p liquers-records --features parquet --lib --tests
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · rust-best-practices, liquers-unittest.

**Rollback (Milestone 6):** each format is behind its own feature. Dropping `records-ipc` /
`records-parquet` from lib's `default` removes it from routine builds.

### Milestone 7 — `liquers-web`

**Run after `cargo clean`**, separately from the native loop (`CLAUDE.md` §"liquers-web").

#### Step 7.1 — The `RecordBatch` handle

**Files:** `liquers-web/Cargo.toml`, `liquers-web/src/records.rs`, `liquers-web/src/lib.rs`,
`liquers-web/src/records_column.js` (new), `liquers-web/src/typescript.rs`

`liquers-web` has no JS companion files today. `records_column.js` is the first. It is bound with
`#[wasm_bindgen(module = "/src/records_column.js")]`, so wasm-bindgen copies it into the package's
`snippets/`. Its TypeScript declaration goes where the crate's other hand-written types go:
`typescript.rs`'s `typescript_custom_section`.

**Check first, before writing the handle:** that a crate importing a local `module = "/…"`
snippet still runs in the **Node** test loop (`wasm-bindgen-test-runner` under Node) and in the
quickstart's Trunk build. If either refuses local snippets, ship `RecordColumn` from the
quickstart's own JS (and the `.d.ts`) instead of binding it from Rust — the Rust tests do not use
the companion (Phase 3 §6 re-creates the view by hand), so nothing else changes. Record which
route was taken in the evidence log.

**Change:**
- A `records = ["liquers-lib/records"]` feature in `liquers-web`, **forwarded the way `polars` and
  `egui` already are** (`liquers-web/Cargo.toml:57–64`, whose comment explains why: a
  `#[cfg(feature = …)]` in this crate is evaluated against *this* crate's features), and listed
  in `liquers-web`'s `default`. Not a bare `"records"` in the `liquers-lib` dependency's feature
  list: that would give `liquers-web` no feature of its own to gate on, which the rollback below
  and Step 7.2's before/after measurement both need.
- `mod records` gated on it, and gated `ExtValue::RecordView { .. }` / `RecordSource { .. }` arms
  in the exhaustive match in `liquers-web/src/default_value.rs` (`as_js_opaque`, `:45–64`) — the
  match that otherwise stops compiling the moment `records` reaches this crate.
- `LiquersRecordBatch` with `numRows`, `numColumns`, `schemaJson`, `column` and `columnCopy`.
- `From<Arc<RecordBatch>>` and `Drop`, with the live-handle count behind `debug-handles`.
- The `RecordColumn` JS class, with the refresh-on-access getter (Phase 2 §"The wasm route";
  §"Construction helpers…").
- The `RecordBatch` / `RecordColumn` TypeScript declarations, and a usage line for each in
  `liquers-web/tests/stubs/valid_usage.ts` (existing), so `check-stubs.sh` checks them.

**Tests:** `liquers-web/tests/records_RECORDS.rs` (2), from Phase 3 §6.

**Validation:**
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
./liquers-web/scripts/check-stubs.sh
./liquers-web/examples-web/quickstart/build.sh
```

**Agent:** sonnet · rust-best-practices · context:
- `liquers-web/README.md`;
- `liquers-web/src/bridge.rs` (the handle convention);
- `liquers-web/tests/runtime_RUNTIME.rs` (`RUNTIME05`);
- the Phase 2 sections named.

#### Step 7.2 — Measure the wasm size

**Change:** build the quickstart's `.wasm` with `liquers-web`'s `records` feature off and on, and
record both sizes in Phase 5's evidence.
- **Over 10% growth** is not a failure, but it triggers a decision. Put it to the user: keep
  `records` in `liquers-web`'s default feature list, or make it opt-in.

**Agent:** done by the orchestrator.

**Rollback (Milestone 7):** remove `"records"` from `liquers-web`'s `default`. The handle module
and the `default_value.rs` arms are gated on the crate's own `records` feature.

### Milestone 8 — matrix, documentation hooks, validation

#### Step 8.1 — Build-matrix rows

**File:** `scripts/check-build-matrix.sh`

**Change:** the rest of Phase 2 §"Feature-gating discipline"'s eleven rows — the `liquers-lib`
ones were added in Step 5.2 — in the script's existing row style (`--tests` on native rows,
library-only on wasm32): a new `RECORDS_CONFIGS` array checked with `-p liquers-records` (no
features; `ipc,parquet`; wasm32), and the `records-ipc`, `records-parquet`,
`records-parquet,polars` and wasm32 `webui,records,records-ipc` `liquers-lib` rows. Phase 2's
rows are the superset: Phase 3 §8's table omits the wasm32 `liquers-lib` rows, which are the ones
`liquers-web` depends on. Phase 3 §8's `cargo test` commands are how to *run* a row, not a
separate list.

**Validation:**
```bash
bash scripts/check-build-matrix.sh
```

**Agent:** haiku.

#### Step 8.2 — `CLAUDE.md`

The crate exists now, so `CLAUDE.md` can describe it. Apply Phase 2 §"Existing documents to
update"'s `CLAUDE.md` row:
- project structure and the dependency-flow line;
- "Where Code Goes";
- `cargo test -p liquers-records` beside the default command;
- the feature-matrix rows.

**Agent:** done by the orchestrator.

#### Step 8.3 — Issue statuses and full validation

- **Update the issues** this project resolves or advances:
  - `RECORD-SELECTION-IS-EAGER-NOT-A-VIEW` — resolved by views;
  - `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` — unchanged, and noted as still open;
  - `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` — unchanged.
- **Leave these open**, with a line saying why:
  - `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED`;
  - `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS`;
  - `MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS`;
  - `VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`;
  - `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`;
  - `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT`.
- **Run the full set:**
  ```bash
  CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
  CARGO_INCREMENTAL=0 cargo test -p liquers-records --all-features --lib --tests
  CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests
  bash scripts/check-build-matrix.sh
  cargo check -p liquers-axum -p liquers-py -p liquers-store
  cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
  ```

**Agent:** done by the orchestrator.

## Testing Plan

### Unit tests

Each lands in the step that implements its module (above). The records crate's loop,
`cargo test -p liquers-records --lib --tests`, builds `liquers-core` and nothing above it. It is the
loop to run after every records step.

### Integration tests

The integration tests land in Milestone 5, which is the first point at which the lib `Value` can
carry records:
- Phase 3 §5.x;
- the four end-to-end tests;
- the lib half of §6.

The core flag tests (Step 1.2) are integration tests of the asset manager and run in core's loop.

### Feature matrix and wasm

- After Step 5.2 and at the end, run `bash scripts/check-build-matrix.sh`.
- Run `cargo check -p liquers-records --target wasm32-unknown-unknown` after every Milestone 2–4
  step.
- The `liquers-web` loops run once, in Milestone 7, after `cargo clean`.

### Manual validation

```bash
# Every query in phase3-examples.md parses, plans and names registered commands:
cargo run -p liquers-core --features cli --bin liquers-validate -- --query-file queries.txt
# Expected: status Ok for each, exit code 0.

# The registry records the rec namespace:
grep -c "namespace: rec" specs/command_registry.yaml
# Expected: the number of ns-rec commands (13).
```

## Task Splitting (Agent Assignments)

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 0.1 | sonnet | rust-best-practices, liquers-unittest | Two conversion paths must agree; touches a generic trait |
| 0.2 | haiku | liquers-unittest | One match arm and a round-trip loop |
| 1.1 | haiku | rust-best-practices, liquers-unittest | Fields, accessors, struct literals the compiler lists |
| 1.2 | sonnet | rust-best-practices, liquers-unittest | Asset-lifecycle invariants; several write sites |
| 1.3 | sonnet | rust-best-practices, liquers-unittest | New provider composition and builder API |
| 2.1–2.5 | sonnet | rust-best-practices, liquers-unittest | New data model, unsafe-free alignment, trait design |
| 2.6 | sonnet | rust-best-practices | `RecordSource`, `ChunkResolver`, `RecordValue`: object safety and `'static` futures on both targets |
| 3.1, 3.2 | sonnet | rust-best-practices, liquers-unittest | Parsers with strict edge-case contracts |
| 3.3 | haiku | liquers-unittest | Write-only formats and round-trip tests |
| 4.1–4.3 | sonnet | rust-best-practices, liquers-unittest | Async streaming, dependency recording, provider semantics |
| 5.1 | haiku | rust-best-practices | Features and re-exports |
| 5.2, 5.4–5.6 | sonnet | rust-best-practices, liquers-unittest (+ liquers-validate for 5.5) | Cross-crate glue, gated variants, command registration |
| 5.3 | haiku | liquers-unittest | Hook implementations following Step 0.1 |
| 6.1, 6.2 | sonnet | rust-best-practices, liquers-unittest | Binary formats written against external specs |
| 7.1 | sonnet | rust-best-practices | wasm-bindgen handle and JS companion |
| 8.1 | haiku | — | Script rows following the existing pattern |
| 1.4, 7.2, 8.2, 8.3 | orchestrator | — | Doc edits, measurement, final validation |

Each agent is given this plan's step, the Phase 2 sections it names, the Phase 3 tests it lands,
and `CLAUDE.md`. **After every step, the orchestrator checks three things before moving on:** the
step's validation commands, `git diff --stat` (that the step stayed inside its files), and that no
test was weakened.

### Model selection

The data-model and format steps are sonnet steps. They are not hard in the abstract, but each carries
a precise contract that Phase 3's tests pin down, and a haiku agent is more likely to bend a test
than read the contract. Opus is not assigned to any step. It is the Phase 4 final reviewer, and it
reviews the implemented diff before Phase 5.

## Rollback Plan

### Per-step

Each step is **one commit**, and the milestone boundaries are tags on the branch
(`record-streams-m0` … `record-streams-m8`). To roll back a step, revert its commit; each step
lists what depends on it.

### Full feature

- **Remove records:**
  - revert Milestones 2–7;
  - drop the `liquers-records` workspace entries;
  - remove the features from `liquers-lib/Cargo.toml` and `"records"` from `liquers-web`;
  - regenerate `specs/command_registry.yaml`.
- **Milestones 0 and 1 stand alone.** They fix general issues and add general core features, so
  they are worth keeping even if records are withdrawn.
- **Dependencies removed with records:** `flatbuffers`, plus `bytemuck` as a direct dependency. It
  stays in the lockfile through `egui`.

### Partial completion

The milestones are ordered so that stopping after any one leaves a consistent tree:
- **after M1:** core features, no records;
- **after M4:** a records crate that no one uses yet;
- **after M5:** records usable from queries, without IPC or Parquet;
- **after M6:** complete for native builds;
- **after M7:** complete.

If work stops mid-milestone, revert to the last milestone tag.

## Documentation Updates

### New reference and guide documents (Phase 5)

`specs/reference/RECORD_STREAMS.md` and `specs/guides/RECORD_STREAM_GUIDE.md`, per Phase 2
§"Documentation Architecture". They are written in Phase 5 from the implemented code, and every
snippet is taken from a passing test.

### Existing documents and `affects_docs`

Phase 2's `affects_docs` set: `VALUE_TYPE_SYSTEM.md`, `TYPE_SYSTEM_GUIDE.md`,
`COMMAND_REGISTRATION_GUIDE.md`, `LANGUAGE-INTEGRATION_GUIDE.md`, `ASSETS.md`,
`ASSET_LIFECYCLE.md`, `ENVIRONMENT_CONFIG.md` and `PROJECT_OVERVIEW.md`.
- **`ENVIRONMENT_CONFIG.md`** documents `with_appended_recipe_provider` (decision 1), not a
  `RecipeProviderChoice` variant.
- **`REGISTER_COMMAND_FSD.md` is added to the set**, beyond Phase 2's list. Its async example should state that a
  `context` parameter needs the `CommandEnvironment` alias. That is how Phase 3's drafters went
  wrong, and a one-line addition prevents it.

Beyond `affects_docs`, and outside §9.2's History rule:
- **`specs/README.md`**: the capability-map entry (below).
- **`CLAUDE.md`**: Step 8.2.

Each document changed gets a `## History` row and a `reviewed:` bump in the same commit (§9.2).

### Design, capability and cross-links

`specs/README.md`: `record-streams` moves `designing` → `built` in Phase 5. The design folder's
entry points at the new reference and guide.

### Phase 5 evidence capture

During implementation the orchestrator keeps `specs/design/record-streams/phase5-evidence.md`, a
running log. For each step it records:
- requested versus implemented scope;
- any test corrected, and why;
- issues filed;
- surprises;
- Step 7.2's wasm sizes.

Phase 5 synthesizes from this log rather than rediscovering it.

### CLAUDE.md

Step 8.2, when the crate exists.

### PROJECT_OVERVIEW.md

In Phase 5: recipe providers as a chain; `liquers-records` in the architecture; records as a value
family.

## Phase 5 Entry Criteria

- [ ] Every step's validation passes. The build matrix and the three `liquers-web` loops pass.
- [ ] Every query in `phase3-examples.md` validates against the regenerated registry with no
      `--command` flags.
- [ ] The implemented diff has been reviewed, and the review comments are resolved or answered.
- [ ] Every issue named in Step 8.3 has an updated status or an explanatory line.
- [ ] `phase5-evidence.md` covers every step.

## Questions settled after the final review (2026-09-25)

1. **`ManifestSpec` models the manifest-level fields.** Settled as option (a), because it follows
   the user's earlier decision that a template's arguments and links are shared by its chunks.
   - The new fields are `manifest` (`ManifestKind`), `version`, `title`, `description`, shared
     `arguments` / `links`, `volatile` and `expires`.
   - `ManifestSpec` gets a hand-written `Default`.
   - Phase 3's literals end in `..ManifestSpec::default()`.
   - The discriminator is written on serialization.

   Shared arguments and links need keyed chunks, as per-chunk ones do. Phase 2 has a changelog
   row for this, and two tests are added above.
2. **`schema` on `from_json` / `to_record`** (Step 5.5). The default is option (a),
   `schema: Option<Value>`, if `register_command!` binds it. Otherwise it is option (b),
   `schema: String = ""`, with a linked schema arriving as its YAML/JSON text. Whichever is used
   is recorded in the evidence log.
3. **Keyed chunks in a configured environment** (decision 1). Settled as option (b):
   `liquers-lib` adds `with_records_recipe_provider()` to its builder and calls it on its own
   configured construction paths, so a server built from a configuration document serves keyed
   chunks too. It is written in Step 5.6.

## Execution Options

After approval:
1. **Execute now** — Milestone 0 first, one commit per step, pushing at each milestone tag.
2. **Create a task list** — one task per step, for later execution.
3. **Revise the plan.**
4. **Exit** — implement manually from this plan.
