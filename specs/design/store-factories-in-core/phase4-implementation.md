---
title: "Phase 4: Implementation plan — Store configuration and factories in liquers-core"
kind: design
audience: internal
area: [core/store, store/config, store/backends, web, docs]
---
# Phase 4: Implementation Plan — Store Configuration and Factories in `liquers-core`

## Overview

Eleven steps across three crates, ordered so the workspace compiles at the end of every step except
5 and 6, which are one atomic move (deleting `liquers-store`'s two modules and adding their
replacement cannot be split without a broken intermediate). Steps 1–4 are additive to
`liquers-core` and touch nothing else; the disruption is concentrated in 5–7.

**Sequencing decision made here:** `STORE-OPENDAL-SERVICES-NOT-ENABLED` (P0) is **not** fixed by
this plan, and the two S3 tests it blocks are deferred to Step 11 rather than dropped. See
§"Dependency on a P0 outside this design".

`rust-best-practices` was applied to this plan; its findings are recorded inline per step and
summarized in §Review.

## Dependency on a P0 outside this design

Phase 3 specified `s3_01` and `s3_02`, which cannot compile until `services-s3` is enabled —
[`STORE-OPENDAL-SERVICES-NOT-ENABLED`](../../issues/STORE-OPENDAL-SERVICES-NOT-ENABLED.md).

**Recommendation: fix that P0 first, as separate work, before Step 11.** It is P0 on its own merits,
it is a manifest change of a few lines, and folding it into this refactor would bury a user-facing
defect fix inside a code move — where a reviewer looking at "did the types move correctly?" is not
looking at "which backends does the product ship with?".

**If it is not fixed first**, Step 11 is skipped and the two tests are carried forward on that
issue instead. The rest of the plan is unaffected: no other step needs an OpenDAL service compiled
in, because `OpendalStoreFactory` is compiled whether or not `opendal` is on and reports
`Unavailable` when it is not.

## Implementation Steps

### Step 1 — `liquers-core`: the `toml` feature

**Files:** `liquers-core/Cargo.toml`

```toml
toml = { version = "0.8", optional = true }

[features]
toml = ["dep:toml"]
```

Not in `default`. Same version `liquers-store` pins today.

**Validation:** `cargo check -p liquers-core && cargo check -p liquers-core --features toml`

**Agent:** haiku · skills: none · knowledge: `liquers-core/Cargo.toml`, `liquers-store/Cargo.toml`

---

### Step 2 — `liquers-core::error`: `Error::parse_error`

**Files:** `liquers-core/src/error.rs`

```rust
/// A document failed to parse, with no position to report.
///
/// `key_parse_error` and `query_parse_error` both require a `Position`; this is the
/// constructor for a whole-document parse failure (YAML, JSON, TOML).
pub fn parse_error(message: String) -> Self {
    Error::new(ErrorType::ParseError, message)
}
```

**Why it is here and not skipped.** The code moving in Step 3 builds parse errors with
`Error::new(ErrorType::ParseError, …)`, which `CLAUDE.md` forbids. Moving a known rule violation
*into* the crate that enforces the rule most strictly is worse than adding one constructor.

**Outside the Phase 1 boundary** — flagged at the Phase 2 gate and still awaiting an explicit
decision. If declined, Step 3 moves the `Error::new` calls verbatim and files the gap.

**Validation:** `cargo check -p liquers-core`

**Agent:** haiku · skills: rust-best-practices · knowledge: `liquers-core/src/error.rs` constructor
conventions

---

### Step 3 — `liquers-core/src/store_config.rs`: move the data

**Files:** new `liquers-core/src/store_config.rs`; `liquers-core/src/lib.rs` (`pub mod store_config;`)

Move verbatim from `liquers-store/src/config.rs`: `StoreRouterConfig`, `StoreConfig`, all their
methods, `expand_env_vars`, and the **11 unit tests**. Changes to the moved code, and only these:

- `use liquers_core::…` becomes `use crate::…`
- `store_type` gains `#[serde(default)]` (Phase 2 §Data Structures — inert today)
- `uri` is **not** added; that is `STORE-CONFIG-FROM-URI`, deferred
- `Error::new(ErrorType::ParseError, …)` becomes `Error::parse_error(…)` if Step 2 landed
- the `expand_env_vars` doc-test's `use` line changes crate

**Left behind** in `liquers-store`: `OPENDAL_STORE_TYPES`, `is_opendal_store_type`,
`get_opendal_scheme` and their 2 tests.

**Assertions must not change.** If any moved test needs a changed assertion, the move was not
behaviour-preserving — stop and report rather than adjusting the test.

**Validation:** `cargo test -p liquers-core --lib store_config`

**Agent:** sonnet · skills: rust-best-practices · knowledge: `liquers-store/src/config.rs` in full,
`liquers-core/src/lib.rs` module ordering, Phase 2 §Data Structures

---

### Step 4 — `liquers-core/src/store_factory.rs`: the factory machinery

**Files:** new `liquers-core/src/store_factory.rs`; `liquers-core/src/lib.rs`

All new code. In dependency order within the file:

```rust
pub enum StoreArgumentType { String, Number, Boolean, Array, Object, Any }
pub struct StoreArgumentInfo { name, label, doc, argument_type, required, default }
    // ::new, ::derived, .with_label, .with_doc, .required, .with_default
pub enum StoreTypeAvailability { Available, Unavailable(String) }
pub enum ArgumentCoverage { Complete, Partial { authority: String } }
pub struct StoreTypeInfo { store_type, label, doc, arguments, availability, coverage }
    // ::new, .with_label, .with_doc, .with_argument, .unavailable, .partial

pub trait StoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo>;
    fn resolve(&self, config: &StoreConfig) -> Option<String>;  // default: exact match
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}

pub type StoreConstructor = Box<dyn Fn(&StoreConfig) -> Result<Box<dyn AsyncStore>, Error>>;
pub struct StoreTypeMap { entries: BTreeMap<String, (StoreTypeInfo, StoreConstructor)> }
pub struct ChainedStoreFactory { factories: Vec<Box<dyn StoreFactory>> }
pub fn core_store_factory() -> StoreTypeMap;
pub fn default_store_factory() -> ChainedStoreFactory;
pub struct StoreRouterBuilder { config, factory }
pub fn unknown_store_type_error(store_type: &str, known: &[StoreTypeInfo]) -> Error;
```

**Five things to get right, each a `rust-best-practices` finding:**

1. **No `Send`/`Sync` bound** on `StoreFactory` or `StoreConstructor`. Load-bearing: `WebStoreFactory`
   holds `js_sys::Object` and is `!Send`. Adding one later is breaking for the browser.
2. **`BTreeMap`, not `HashMap`**, in `StoreTypeMap` — `store_types()` feeds error text, and
   `HashMap` order varies per run.
3. **`StoreTypeMap` overrides `resolve`** with a map lookup rather than inheriting the scanning
   default, so dispatch does not rebuild every `StoreTypeInfo` per entry.
4. **`ChainedStoreFactory::create` writes the resolved name** onto a cloned `StoreConfig` before
   calling the member's `create`. This is the invariant `resolve04` tests.
5. **No `_ =>` arm** on `StoreTypeAvailability`, `ArgumentCoverage` or `StoreArgumentType`.

`core_store_factory` registers `memory` always and `filesystem` always — the latter marked
`Unavailable("not available on wasm32: needs tokio::fs")` under `#[cfg(target_arch = "wasm32")]`, so
the type is listed and explained rather than absent.

**Validation:** `cargo check -p liquers-core && cargo check -p liquers-core --target wasm32-unknown-unknown`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 in full,
`liquers-core/src/store.rs` (`AsyncStore`, `AsyncStoreRouter`, `AsyncMemoryStore`, `AsyncFileStore`),
`liquers-store/src/store_builder.rs` for the behaviour being replaced

---

### Step 5 — `liquers-core` tests

**Files:** `liquers-core/src/store_factory.rs` (`#[cfg(test)] mod tests`), new
`liquers-core/tests/store_router_STORE.rs`

18 unit tests and 4 integration tests, named in Phase 3 §Test Plan. Two carry specific traps:

- **`chain04_store_types_is_the_union_first_wins`** — two factories both claiming `memory` with
  different `doc`; the union must contain it **once**, with the first factory's description.
  Otherwise `store_types()` advertises a description belonging to a factory that will never run, and
  the error message lies.
- **`chain05_unclaimed_type_lists_supported_types`** — assert on message *content*, not `is_err()`.
  The existing `test_unknown_store_type` asserts only `is_err()` and would pass against an empty
  message.

`core_router01_builds_from_yaml_without_liquers_store` must live in `liquers-core/tests/` — a test
there structurally cannot reach `liquers-store`, so it cannot pass by accident.

**Validation:** `cargo test -p liquers-core --lib && cargo test -p liquers-core --test store_router_STORE`

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: Phase 3 §Test Plan,
Step 4's output

---

### Step 6 — `liquers-store`: replace `config.rs` and `store_builder.rs` (atomic with Step 7)

**Files:** delete `liquers-store/src/config.rs` and `liquers-store/src/store_builder.rs`; new
`liquers-store/src/store_factory.rs`; `liquers-store/src/lib.rs`; `liquers-store/Cargo.toml`

**No re-export shims** — the gate decision was "reuse core structures, don't shadow them; no
backwards compatibility required".

`store_factory.rs` receives: `OPENDAL_STORE_TYPES`, `is_opendal_store_type`, `get_opendal_scheme`
and their 2 tests; `OpendalStoreFactory`; `default_store_factory()` = `core` then OpenDAL;
`create_router_from_yaml` / `_json` rebuilt over `default_store_factory()`.

`create_store` is **deleted**, not relocated: its `memory`/`filesystem` arms are
`core_store_factory()`, its OpenDAL arm is `OpendalStoreFactory`, its unknown arm is the chain's
error. It has no caller outside its own tests.

`OpendalStoreFactory` is compiled **whether or not** `opendal` is on; with it off, every OpenDAL type
is listed `Unavailable("requires the 'opendal' feature")`. This is what preserves conformance item
`STORE13`.

`Cargo.toml`: `toml = ["liquers-core/toml"]`, and the crate's own `toml` dependency is dropped —
nothing here parses TOML any more. The `opendal` feature stays; **its comment must be rewritten**,
since the wasm-consumer justification it gives is exactly what this change removes.

**Validation:** `cargo check -p liquers-store && cargo check -p liquers-store --no-default-features --features async_store`

**Agent:** sonnet · skills: rust-best-practices · knowledge: both files being deleted in full,
Step 4's output, Phase 2 §Integration Points

---

### Step 7 — `liquers-store` tests

**Files:** `liquers-store/src/store_factory.rs` (`#[cfg(test)] mod tests`)

12 tests. **Three are rewrites of tests that assert what this design inverts** — treat each as a
deliberate behaviour change, not a mechanical port:

| Old | New | What changed |
|---|---|---|
| `factory02_factory_precedes_builtin` | `chain03_earlier_factory_wins` | Asserted factories beat built-ins; now asserts chain order. Its doc comment argues the old rule at length — rewrite it, do not delete it |
| `factory03_unclaimed_type_falls_through` | `chain05_…` (Step 5) | **Deleted.** There are no built-ins to fall through to |
| `factory04_gated_type_names_the_feature` | preserved, retargeted | Same guarantee via `StoreTypeAvailability`; keep both assertions including "a gated-off type is not an unknown type" |

`default02_core_types_are_not_shadowed_by_opendal` guards a real near-miss: OpenDAL claims `fs`, core
claims `filesystem`. They do not collide today, and a rename of either would silently reroute
documents.

**Validation:**
```bash
cargo test -p liquers-store
cargo test -p liquers-store --no-default-features --features async_store   # the only run of factory04
```

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: the existing
`factory01`–`factory04` suite in full, Phase 3 §Test Plan

---

### Step 8 — `liquers-web`: drop the dependency

**Files:** `liquers-web/Cargo.toml`, `src/store/builder.rs`, `src/environment.rs`,
`tests/store_js_STORE.rs`, `tests/eval_EVAL.rs`

1. Delete the `liquers-store` line from `Cargo.toml`.
2. Imports move to `liquers_core::store_config` / `liquers_core::store_factory` (4 files).
3. `WebStoreFactory::store_types` returns `Vec<StoreTypeInfo>` describing `localstorage`
   (`namespace`, `quota_bytes`), `js` (`object`), `http`/`https` (`url_prefix`, `keys`) — arguments
   currently documented only in a module doc-comment. `coverage: Complete`; Liquers owns these.
4. `build_router` becomes
   `ChainedStoreFactory::new().chain(core_store_factory()).chain(factory)`.
5. **The module doc's "factories are consulted before the built-in types" paragraph is now false** —
   replace it with the first-wins rule and the reason the browser's `http` still wins (nothing else
   in this chain claims it).

**Test assertions must not change** — only import paths. `STORE11`, `c12` and the environment-rebuild
tests exercise the configuration path end to end.

**Validation:**
```bash
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: all five files,
`design/liquers-web-store/phase2-architecture.md` for the rationale being superseded

---

### Step 9 — Derived OpenDAL arguments

**Files:** `liquers-store/src/store_factory.rs`

```rust
#[cfg(feature = "opendal")]
fn derived_arguments<C: opendal::Configurator + Default>() -> Vec<StoreArgumentInfo>;
```

Plus the `store_type -> config type` match (~20 entries) it needs. Sound because `Configurator`
bounds `Serialize`, all 62 service configs derive `Default`, and none uses `skip_serializing_if` —
so `serde_json::to_value(C::default())` yields every field name and default from the linked version.

OpenDAL types get `coverage: Partial { authority: "<OpenDAL docs URL>" }`, plus hand-written `doc`
text for only the two or three arguments per type where guidance helps (`bucket`, `root`,
`endpoint`, the `${VAR}` convention).

**`derive01` must not assert an exhaustive field list.** Assert a few long-stable names are present
(`bucket`, `region`, `root` for `s3`) and the list is non-empty. An exhaustive assertion
reintroduces through the test suite exactly the maintenance burden derivation removes.

**Deferrable.** If Step 9 is dropped, OpenDAL types keep `Partial` with a short hand-written list and
the authority URL — already honest and useful. Nothing else depends on it.

**Validation:** `cargo test -p liquers-store derive`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 3 §"Derive the field names",
`opendal-0.55.0/src/types/builder.rs` (`Configurator`), Phase 2 §Function Signatures

---

### Step 10 — Build matrix

**Files:** `scripts/check-build-matrix.sh`

1. **Add `liquers-core` rows — the crate has none today**, and it is about to gain its first optional
   feature and target-conditional store availability:
   `""`, `"--no-default-features"`, `"--features toml"`, `"--target wasm32-unknown-unknown"`.
2. Rewrite the header comment justifying the `liquers-store` wasm32 row: it claims to prove "the
   dependency edge liquers-web relies on", and that edge is deleted. The row's remaining purpose —
   the `opendal`-off feature split — is still real.

**Validation:** `bash scripts/check-build-matrix.sh`

**Agent:** haiku · skills: none · knowledge: `scripts/check-build-matrix.sh` header and arrays

---

### Step 11 — Offline S3 tests *(blocked; see §Dependency above)*

**Files:** `liquers-store/src/store_factory.rs`

`s3_01_arguments_and_uri_agree`, `s3_02_missing_region_fails_at_construction`. Verified to pass
against OpenDAL 0.55 in a Phase 3 probe. **Requires `services-s3`**, i.e.
`STORE-OPENDAL-SERVICES-NOT-ENABLED` fixed first. Skip this step if it is not; the tests carry
forward on that issue.

**Validation:** `cargo test -p liquers-store s3_`

**Agent:** haiku · skills: liquers-unittest · knowledge: Phase 3 §"The offline S3 test"

## Testing Plan

Run per step as listed. Full gate before Phase 5:

```bash
cargo test -p liquers-core --lib
cargo test -p liquers-core --test store_router_STORE
cargo test -p liquers-store
cargo test -p liquers-store --no-default-features --features async_store   # factory04's only run
cargo test -p liquers-lib --lib --tests                                    # incl. registry_export
cargo test -p liquers-axum
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

**The `--no-default-features` run is not optional.** `factory04` is `#[cfg(not(feature = "opendal"))]`
and never executes in the default configuration; it is the only coverage of the message
`StoreTypeAvailability` exists to preserve.

**`registry_export` must stay green untouched** — proof this design did not leak into the command
surface.

Disk: `cargo clean` before the wasm run, per `CLAUDE.md` §"Building and testing".

## Agent Assignment

| Step | Model | Skills | Why |
|---|---|---|---|
| 1, 10, 11 | haiku | — / liquers-unittest | Mechanical; manifest and script edits with clear targets |
| 2 | haiku | rust-best-practices | One constructor, but it must match `error.rs` conventions |
| 3 | sonnet | rust-best-practices | A verbatim move where "verbatim" must be verified, not assumed |
| 4 | sonnet | rust-best-practices | All-new trait and type design; five specific traps |
| 5, 7 | sonnet | liquers-unittest, rust-best-practices | Three tests are behaviour changes, not ports |
| 6 | sonnet | rust-best-practices | Deletion plus reconstruction across a crate boundary |
| 8 | sonnet | rust-best-practices | Cross-crate, wasm target, prose that is now false |
| 9 | sonnet | rust-best-practices | Generic bound over an external trait |

No step is opus-tier: every one has a written specification and a validation command. The judgement
was spent in Phases 2 and 3.

## Rollback Plan

| Step | Rollback |
|---|---|
| 1, 2, 10 | Revert the file; nothing depends on them until Step 3 |
| 3, 4, 5 | Purely additive to `liquers-core` — delete the two modules and their `lib.rs` lines; no other crate has changed yet |
| 6, 7 | The only irreversible-feeling step, and it is not: `git revert` restores `config.rs` and `store_builder.rs` whole. **Do not hand-reconstruct them** |
| 8 | Revert; `liquers-web` returns to depending on `liquers-store` |
| 9, 11 | Independent; drop without touching anything else |

**Whole-change rollback** is a branch revert. There is no migration, no persisted state, no generated
file, and no document whose meaning changes — a `StoreRouterConfig` written today parses identically
after.

**Partial-value stopping points:** after Step 5, `liquers-core` owns configuration and factories and
`liquers-store` still works unchanged — that alone unblocks `EnvironmentConfig`, which is the
prerequisite `environment-builder` recorded. After Step 8 the `liquers-web` dependency is gone. Both
are coherent places to stop.

## Phase 5 Entry Criteria

Phase 5 starts when **all** hold:

1. Steps 1–10 complete; Step 11 complete **or** explicitly deferred to
   `STORE-OPENDAL-SERVICES-NOT-ENABLED`.
2. Every command in §Testing Plan run and green, including the `--no-default-features` run.
3. The three rewritten tests reviewed as deliberate behaviour changes — each with a doc comment
   saying what it asserts now and why the old assertion no longer holds.
4. The `Error::parse_error` question answered either way, and the answer recorded.
5. Every "prose that is now false" site fixed: `liquers-web/src/store/builder.rs` module doc,
   `liquers-store/Cargo.toml`'s `opendal` comment, `scripts/check-build-matrix.sh` header.
6. No new issue filed during implementation left unrecorded.

Phase 5 then delivers: the summary; `STORE_CONFIG_FSD.md` extended with the factory model (its
HEAD-true half already landed 2026-08-29); the new `STORE_FACTORY_GUIDE.md`; updates to
`DOC_01_ARCHITECTURE_REFERENCE.md`, `LANGUAGE-INTEGRATION_GUIDE.md` (including the option-2 reversal
and `STORE12`), root `README.md`, `CLAUDE.md`, `DOCS_STRUCTURE_GUIDE.md` §3; `STORE-CONFIG-IN-CORE`
closed with its corrected verification list; and the capability-map and index updates.

## Review

**Phase 1 conformity.** Every step traces to a Phase 1 decision. Two steps sit outside the boundary
Phase 1 drew and are marked as such: Step 2 (`error.rs`) and Step 10 (`check-build-matrix.sh`).
Neither is absorbed silently.

**Phase 2 conformity.** Signatures in Steps 3, 4, 6 and 9 are quoted from Phase 2 rather than
restated. The `resolve` change adopted after the URI audit is carried through Steps 4, 5 and 7.

**Phase 3 conformity.** All 47 tests are placed: 11 moved (Step 3), 2 moved (Step 6), 18+4 new
(Step 5), 12 (Step 7), 2 deferred (Step 11). The three rewrites are called out in Step 7 with what
changed.

**Codebase compatibility.** Call sites verified at HEAD, and one finding revises the blast radius:
**`liquers-axum` declares `liquers-store` but references nothing from it** —
`grep -rn liquers_store liquers-axum/src/` is empty. So Step 6 cannot break it, and the only in-tree
consumers of the moved code are `liquers-web` (Step 8) and `liquers-store`'s own tests.
`liquers-lib`'s example constructs `AsyncOpenDALStore` directly and never touches the builder.
Removing the unused `liquers-axum` dependency is a tempting one-line cleanup and is **not** in this
plan — it is unrelated scope.

**`rust-best-practices` findings**, all folded into Step 4: no `Send`/`Sync` bound (breaking for the
browser if added later); `BTreeMap` for deterministic error text; `resolve` overridden in
`StoreTypeMap` to avoid rebuilding descriptions per entry; exhaustive matches on all three new enums;
`Box<dyn StoreFactory>` rather than a generic parameter, which would propagate through every
signature holding a builder.

**Certainty:** high. Every signature is specified, every step has a validation command, and the one
external dependency (`services-s3`) is isolated to a step that can be skipped.
