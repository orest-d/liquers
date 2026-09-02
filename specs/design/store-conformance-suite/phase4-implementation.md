# Phase 4: Implementation Plan — `AsyncStore` conformance

## Overview

Sixteen steps in four movements: **specify** (1), **build the harness and prove it** (2–4),
**write the rules and run them** (5–12), **document, adopt and close** (13–16).

Two ordering rules are load-bearing and not negotiable:

- **The harness is tested before any real rule exists** (step 4). A rule that fails because the
  gating is wrong is indistinguishable from a store that diverges, and this project's whole output
  is a claim about which stores diverge.
- **No existing test is deleted until its replacement passes *and* fails when the behaviour is
  broken** (step 15, gated on steps 9–12). This is the review decision, expressed as step order.

Everything through step 12 is additive behind a non-default feature. Exactly two steps change
behaviour a consumer could notice — step 10 (`AsyncMemoryStore::keys`) and step 15 (deletions) —
and both have explicit rollbacks.

**Effort shape.** Steps 5–8 are the bulk (31 rules); steps 2–4 are the risk. A step that "feels
uncertain" here is step 12 (`liquers-web` under two harnesses) and it is deliberately last of the
suites so nothing depends on it.

## Implementation Steps

### Step 1 — Complete the contract

**Files:** `specs/reference/STORE_SEMANTICS.md`

Resolve the three ⚠ rows using the Phase 1 decisions: §5 the `removedir` postcondition (`Ok` means
the directory is gone; recursion follows; the trait default's `Err(KeyNotSupported)` stays valid
for a store declaring no `RemoveDirectories`), §9 `keys()` returns data keys plus directories plus
the prefix with **every returned key starting with the prefix**, §6 keep or clear the
`is_supported` ⚠ depending on whether `async-memory-store-prefix-support` has merged. Restate rules
trait-neutrally where they hold for both traits, noting only `AsyncStore` must satisfy them today
(`CORE-SYNC-STORE-TRAIT-OBSOLETE`). Replace each *Enforced by* line with the Phase 3 rule IDs. Add a
`## History` row and bump `reviewed:` in the same commit (§9.2).

**Validation:** `python3 scripts/docs_index.py --check`
**Agent:** sonnet · skills: none · knowledge: Phase 1 decisions 1–4, Phase 3 rule inventory, the
current `STORE_SEMANTICS.md`.

### Step 2 — Feature and module skeleton

**Files:** `liquers-core/Cargo.toml`, `liquers-core/src/lib.rs`,
`liquers-core/src/store_conformance/{mod,fixture,report}.rs`

Add `store-conformance = []`, **not** in `default`. Define `Capability`, `StoreCapabilities` (no
`Default`, with `has()` matching `Capability` exhaustively), `SafetyLevel` (three variants, `Ord`
order documented as load-bearing), `KeyRequest`, `Unavailable`, `Fixture`, `RuleOutcome`,
`From<Error> for RuleOutcome`, `ReportEntry`, `ConformanceReport` (with `created` and `residue`),
`AllowedFailure`, `RuleMeta`, `Rule`, `RuleFn`, `OutcomeCounts`, an empty `rules()`, `rule(id)`,
`run_all`, `run_rule`, and the `rule!` macro emitting the wrapper described in Phase 2. The
`ConformanceReport` methods land here too, not later: `counts() -> OutcomeCounts`, `failures()`,
`not_run_by_level()`, `assert_conformant(&[AllowedFailure])`, and `Display`. `assert_conformant`
must fail in **both** directions from the start — a disallowed failure, and an `allowed` rule that
passed — because step 4's `H5` tests exactly that and steps 9–12 depend on it to keep their
allowed-failure lists from going stale.

**Validation:**
```bash
cargo check -p liquers-core --features store-conformance
cargo check -p liquers-core                      # feature off must still compile
cargo check -p liquers-core --target wasm32-unknown-unknown --features store-conformance
```
**Agent:** sonnet · skills: `rust-best-practices` · knowledge: Phase 2 Data Structures and Trait
Implementations verbatim, `maybe_send.rs`, the `#[cfg_attr(..., async_trait)]` pattern at
`store.rs:327`.

### Step 3 — `StoreFactory::create_fixture`

**Files:** `liquers-core/src/store_factory.rs`

Additive, defaulted to `Ok(None)`, `#[cfg(feature = "store-conformance")]` on the method because its
return type only exists under the feature.

**Record the known limitation in the method's doc comment**, not only in this design: it is
synchronous, matching `create`, so a factory needing async setup — provisioning a scratch bucket,
opening a connection — cannot supply a fixture. A store author hits this at the moment they
implement the method, which is where the sentence has to be. If step 11 or a later store actually
needs async construction, file it rather than widening `create`.

**Validation:** `cargo check -p liquers-core --features store-conformance` and
`cargo check -p liquers-py` (the implementor most likely to break).
**Agent:** haiku · skills: `rust-best-practices` · knowledge: `store_factory.rs:267–292`, the seven
implementors Reviewer B enumerated.

### Step 4 — Harness unit tests (H1–H8) — **before any real rule**

**Files:** `liquers-core/src/store_conformance/mod.rs` (`#[cfg(test)] mod tests`)

A stub store and stub fixture, then H1–H8 from Phase 3. `H5` (an `allowed` rule that passed is an
error) and `H7` (every rule records every key it creates) are the two that keep the suite honest;
neither is optional.

**Validation:** `cargo test -p liquers-core --features store-conformance store_conformance`
**Agent:** sonnet · skills: `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 H-table.

### Steps 5–8 — The rules

Each step: write the rule bodies as `async fn(&dyn Fixture) -> Result<(), RuleOutcome>`, register
them through `rule!`, and for **every** rule answer *what implementation change would make this
fail?* — a rule with no plausible answer is rewritten, not registered.

| Step | Rules | Count | Contract § | File under `store_conformance/rules/` |
|---|---|---|---|---|
| 5 | `sibling01`–`sibling05` | 5 | §1 | `sibling.rs` |
| 6 | `dir01`–`dir07`, `data01` | 8 | §2 | `directories.rs` |
| 7 | `explicit01`–`explicit03`, `absence01`–`absence03`, `remove01`–`remove03`, `data02` | 10 | §3–§5 | `removal.rs`, `absence.rs` |
| 8 | `prefix01`–`prefix03`, `keyabs01`, `sidecar01`–`sidecar02`, `keys01`–`keys02` | 8 | §6–§9 | `prefix.rs`, `keyshape.rs`, `sidecar.rs`, `enumerate.rs` |

**5 + 8 + 10 + 8 = 31**, matching the Phase 3 inventory exactly — verified by diffing the ID sets
rather than by counting rows, since a range like `` `dir01`–`dir07` `` is easy to miscount by hand.

**Constraints (Phase 2, restated because they are violated by habit):** assert on `ErrorType`,
never message text; `contains`/`is_dir` before the first mutation at `Scratch`; `record_created`
immediately after every successful create; `?` for a store error, `Err(failed(..))` for a contract
disagreement, never conflated.

**Validation per step:** `cargo test -p liquers-core --features store-conformance`
**Agent:** sonnet · skills: `rust-best-practices`, `liquers-unittest` · knowledge: the relevant
`STORE_SEMANTICS.md` section, Phase 3 rule inventory row, Phase 3 Scenario 3's vacuous/correct pair.

### Step 9 — Fixtures and suites C1–C4 (`liquers-core`)

**Files:** `liquers-core/tests/store_conformance_CONF.rs`

`AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter` (memory + file), and a local minimal store
exercising the trait defaults — defined **in the test file**, not exported from the library, so the
defaults are tested without adding production surface. Reuse the `unique_temp_dir` helper pattern
from `tests/store_key_absolute.rs`.

**This step produces the divergence census** — the first honest answer to "do the implementations
agree?". Record every failure verbatim; it is Phase 5's most valuable artefact.

**Validation:** `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`
**Agent:** sonnet · skills: `liquers-unittest` · knowledge: Phase 3 Scenario 1 and the C-table.

### Step 10 — Fix what step 9 found ⚠ *behaviour change*

**Files:** `liquers-core/src/store.rs`, plus tests it breaks

Known in advance: **`AsyncMemoryStore::keys` must return data keys plus directories plus the
prefix** (decision 1). `keys()` has **no library consumers** — every call site in the tree is a
test — so the blast radius is bounded and predictable:

| Call site | Today | After |
|---|---|---|
| `store.rs:2141` | asserts `is_empty()` on a fresh store | returns the prefix — assert `== [prefix]` |
| `store.rs:2146`, `:2148` | asserts `len() == 1` | key + prefix (+ any directory) |

Also correct the `removedir` doc comments to the postcondition.

**Any *other* divergence step 9 surfaces**, under decision 5. The `S`/`M` boundary needs a
criterion rather than a feeling, so: **`S` — the fix touches only that store's own module and the
tests of it. `M` or larger — it changes a shared type, the `AsyncStore` trait, another store, or a
public signature.** Anything genuinely ambiguous is treated as `M`, because filing an issue is
cheap and a half-finished shared-type change inside a test-suite PR is not.

For an `M`+ divergence: file the issue, add the rule to that store's `allowed_failures` citing it,
and — the half decision 5 spells out — **the store's row in the guide's status matrix reads
`BLOCKED`, not `PARTIAL`**. `BLOCKED` is the state for "finished, and something it depends on is
not", which is exactly this; `PARTIAL` would say the store is unfinished, which is a different
claim and different work. `H5` forces the `allowed_failures` entry out the moment the issue is
fixed, and step 14's matrix is regenerated from the report, so the row cannot outlive the entry.

**Validation:** `cargo test -p liquers-core --features store-conformance` **and**
`cargo test -p liquers-lib --lib --tests` (the default loop, to catch anything downstream).
**Agent:** sonnet · skills: `rust-best-practices` · knowledge: step 9's census, decision 5's M+ rule.

### Step 11 — `liquers-store`: plumbing, C5, and a scratch factory

**Files:** `liquers-store/Cargo.toml`, `src/store_factory.rs`, `tests/store_conformance_CONF.rs`

New `store-conformance` feature forwarding to core; new `cli` feature with `clap` as a **new**
optional dependency (the crate has neither today). `create_fixture` for the OpenDAL `fs` type,
building a temp-directory backend. C5 runs `AsyncOpenDALStore` at `Scratch`.

**Validation:** `cargo test -p liquers-store --features store-conformance` and
`cargo check -p liquers-store --no-default-features`
**Agent:** sonnet · skills: `rust-best-practices` · knowledge: `liquers-store/src/store_factory.rs`,
`OPENDAL_STORE_TYPES`, Phase 2 Integration Points.

### Step 12 — `liquers-web`: C6, C7, C8

**Files:** `liquers-web/Cargo.toml`, `liquers-web/tests/store_conformance_CONF.rs`

`FetchStore` at `ReadOnly` with a stub `fetch` installed on the global object (it reads `fetch` from
`js_sys::global()` at call time, `fetch.rs:217`) and its known-key set as the `Existing` source;
`JsStore` at `Scratch` with a stub implementing **every** forwarded method — a partial stub reports
capability gaps belonging to the stub (`js_store.rs:211–256`); `LocalStorageStore` at `Scratch`
behind `browser-tests`, clearing its namespace first as `store_local_STORE.rs` already does.

**The riskiest step, deliberately last of the suites.** If `FetchStore` or `JsStore` turns out to
qualify on its own as `M` or larger, file it and fall back to `ReadOnly` over a hand-placed corpus,
reporting the unreachable rules as `SkippedPrecondition`.

**Validation:**
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown          # C6, C8, under Node
CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
  --target wasm32-unknown-unknown --features browser-tests         # C7
```
**Agent:** sonnet · skills: `rust-best-practices`, `liquers-unittest` · knowledge:
`liquers-web/src/store/*`, `tests/store_local_STORE.rs`, `liquers-web/README.md`, the wasm notes in
`CLAUDE.md`.

### Step 13 — `liquers-store-check`

**Files:** `liquers-store/src/bin/liquers_store_check.rs`, `liquers-store/Cargo.toml` (`[[bin]]`
with `required-features = ["cli", "store-conformance"]` — an auto-discovered binary cannot carry
one)

The full surface Phase 2 specifies: `--config <store.yaml>` (defaults to `read-only`) with
`--store <prefix>` to pick one store out of a multi-store document; `--scratch <store-type>`
(defaults to `scratch`) with repeatable `--arg k=v` passed through to `create_fixture`; plus
`--level`, repeatable `--rule <id>`, and `--format text|yaml|json`. Prints the resolved `StoreConfig` before running, and the not-run counts per level always.

**Listing the residue is a requirement of this step, not a formatting preference.** At
`create-only` the tool cannot remove what it made, so the operator now owns keys they did not have;
Phase 1 decision 9 is explicit that a run which does not list them is a slow leak with no record.
It prints **before** the summary, and a `create-only` run that reports no residue while `created` is
non-empty is a bug in this step, not a tidy result. Exit **0**
conformant · **1** non-conformant · **2** invocation or setup failure.

**Validation:** run it against a temp `fs` store at each of the three levels; assert exit codes and
that `create-only` lists residue.
**Agent:** sonnet · skills: `rust-best-practices` · knowledge: Phase 3 Scenario 2 output verbatim,
`liquers-core/src/bin/liquers_validate.rs` as the CLI precedent.

### Step 14 — Documentation

**Files:** `specs/reference/CONFORMANCE_TERMS.md` (new),
`specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (new),
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (§3 lines 81–100 → link; §STORE → cross-link),
`specs/guides/STORE_FACTORY_GUIDE.md`, `specs/reference/STORE_CONFIG_FSD.md`, `CLAUDE.md`,
`specs/README.md`

**Not** `specs/guides/UNITTEST_GUIDE.md`: its `AsyncStoreWrapper` example belongs to
`DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS`, which also covers `STORE_CONFIG_FSD.md`'s mention and
the `liquers-unittest` skill's import block. If that issue has landed by this step, nothing to do;
if not, leave it — fixing half of an issue's surface in passing makes the issue look done when it
is not. The two `CLAUDE.md` passages are the exception, because this step edits that file anyway.

The guide follows the nine-part outline in Phase 2, including the worked restricted store and the
statement that level 3 is rule discipline rather than a guarantee, and that a `create-only` run
leaves everything it created behind. `CLAUDE.md` §"Adding a Store Backend" gains a step and loses
its two `AsyncStoreWrapper` mentions.

**The per-store status matrix is generated, not written.** It comes from the reports steps 9, 11
and 12 produce — `ConformanceReport` derives serde precisely so this is a transformation rather
than a transcription — merged with the `allowed_failures` lists to mark `BLOCKED` rows. Hand-
maintaining it would guarantee it goes stale, which is the failure this whole project exists to
prevent; a matrix nobody can regenerate is a claim about the past.

**Validation:** `python3 scripts/docs_index.py --check`; every rule ID cited must exist (step 16
enforces).
**Agent:** sonnet · skills: none · knowledge: `LANGUAGE-INTEGRATION_GUIDE.md` structure, Phase 1's
guide-question list, Phase 3 Scenario 3, the step-9 census.

### Step 15 — Adoption deletions ⚠ *deletes tests* — **gated on steps 9–12 passing**

**Files:** `liquers-store/src/opendal_store.rs`, `liquers-core/src/store.rs`

Execute the Phase 3 mapping table row by row. For each row marked *Yes*: confirm the shared rule
passes against that store, **and confirm it fails when the behaviour is deliberately broken**
(revert the assertion locally, see red, restore) — then delete the old test. A rule that stays green
under a broken store replaces nothing and the row is not executed.

Rows marked *Kept* are not touched: the whole `diridx` and `memdir` families, `router01`,
`pathmap02`–`pathmap07`, `keyabs12`–`keyabs14`.

**Validation:** `cargo test -p liquers-core -p liquers-store --features store-conformance`, and the
test count before/after recorded in the commit message.
**Agent:** sonnet · skills: `liquers-unittest` · knowledge: Phase 3 mapping table, the exact test
locations Reviewer 3 confirmed.

### Step 16 — Synchronization, matrix, and closure

**Files:** `liquers-core/tests/conformance_docs_CONF.rs`, `scripts/check-build-matrix.sh`,
`specs/issues/*`, `specs/index.csv`

`D1` asserts the ID sets from `rules()`, `STORE_SEMANTICS.md` and `STORE_IMPLEMENTATION_GUIDE.md`
are equal, printing the difference in both directions; it locates `specs/` by walking up from
`CARGO_MANIFEST_DIR` and skips with a warning if absent, as `registry_export` does. Add a
`store-conformance` row to the build matrix. Close
`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` and
`CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` with resolution notes (§4.3).

**Validation:** `bash scripts/check-build-matrix.sh`; `python3 scripts/docs_index.py --check`
**Agent:** sonnet · skills: `liquers-unittest` · knowledge:
`liquers-lib/tests/registry_export.rs`, `DOCS_STRUCTURE_GUIDE.md` §4.3.

## Testing Plan

| When | Command |
|---|---|
| After every step | `cargo check -p liquers-core --features store-conformance` and with the feature **off** |
| Steps 4–10 | `cargo test -p liquers-core --features store-conformance` |
| Step 10 | `cargo test -p liquers-lib --lib --tests` — the default loop |
| Step 11 | `cargo test -p liquers-store --features store-conformance` |
| Step 12 | wasm loop under Node; `browser-tests` separately, after `cargo clean` |
| Step 13 | the tool at all three levels against a temp `fs` store |
| Step 16 | `bash scripts/check-build-matrix.sh`, `python3 scripts/docs_index.py --check` |

Run the wasm and browser loops **after `cargo clean`**, separately from the native loop: they build
a different target, and `CLAUDE.md` records that a combined run exhausts the 30 GB session
allowance.

Manual check, once, at step 13: point `liquers-store-check` at a store containing a pre-existing
key and confirm at `create-only` that the key is untouched and the residue list names only what the
run made.

## Agent Assignment

| Step | Model | Skills | Why |
|---|---|---|---|
| 1 | sonnet | — | Contract prose; the three ⚠ resolutions are subtle |
| 2 | sonnet | rust-best-practices | The type design is the whole architecture |
| 3 | haiku | rust-best-practices | One defaulted method |
| 4 | sonnet | liquers-unittest, rust-best-practices | H5/H7 are easy to write vacuously |
| 5–8 | sonnet | rust-best-practices, liquers-unittest | Each rule needs the "what would make this fail?" test |
| 9 | sonnet | liquers-unittest | Fixtures plus the census |
| 10 | sonnet | rust-best-practices | A behaviour change with downstream tests |
| 11 | sonnet | rust-best-practices | Feature plumbing is where cfg bugs hide |
| 12 | sonnet | rust-best-practices, liquers-unittest | Two harnesses, one wasm-only crate |
| 13 | sonnet | rust-best-practices | CLI plus safety behaviour |
| 14 | sonnet | — | Long-form documentation |
| 15 | sonnet | liquers-unittest | Deleting tests demands judgement, not speed |
| 16 | sonnet | liquers-unittest | Cross-document assertion |

No step is assigned opus: none is architecturally open after Phases 2–3. No step is assigned haiku
except step 3, because every other step either designs types, deletes tests, or writes an
assertion that could be vacuous.

## Rollback Plan

| Step | Risk | Rollback |
|---|---|---|
| 1 | Contract wrong | Revert the file; nothing depends on it until step 5 |
| 2–9, 11–13 | Additive behind a non-default feature | `git revert`; the default build never compiled it |
| 3 | Trait method breaks an implementor | Revert; the method is defaulted, so no implementor *had* to change |
| **10** | **Behaviour change** — `AsyncMemoryStore::keys` | Revert the `keys` change and re-add `keys01`/`keys02` to that store's `allowed_failures` citing `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS`, which stays open. The suite still ships; the divergence is recorded instead of fixed |
| **15** | **Deletes tests** | `git revert` restores them exactly; this is why deletion is one late step and not spread across steps 5–12 |
| 14, 16 | Documentation and assertions | Revert; `D1` failing is a signal, not a breakage |

The project degrades gracefully: if everything after step 9 were abandoned, the repository would
still hold a written contract, a working harness, 31 rules and a census of which stores diverge —
which is most of what the issue asked for.

## Phase 5 Entry Criteria

Phase 5 begins when **all** of these hold:

1. Steps 1–16 complete, or an incomplete step is filed as an issue and named here.
2. `cargo test -p liquers-core -p liquers-store --features store-conformance` green; the wasm loop
   green under Node; the browser loop green or its absence explained.
3. `bash scripts/check-build-matrix.sh` green, including the feature-off row.
4. `python3 scripts/docs_index.py --check` reports 0 errors.
5. Every rule a store is allowed to fail cites an open issue, and `H5` confirms no stale entry.
6. `D1` green — the rule IDs in code, contract and guide are one set.
7. Every review comment on the PR is answered or incorporated.
8. The Phase 5 learning log has its material: the step-9 census, the per-level counts, the
   `Unavailable` reasons and the residue (Phase 1 decisions 9 and 11), plus any rule found vacuous
   and the real adoption ratio (added by Phase 3 — the first because five such tests had to be
   retro-fitted to `LANGUAGE-INTEGRATION_GUIDE.md` after the same mistake, the second because
   Phase 3's own estimate moved from nine-of-fifteen to twelve-replaced-twenty-kept once the
   families were counted properly).
