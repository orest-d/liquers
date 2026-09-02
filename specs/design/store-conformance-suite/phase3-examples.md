# Phase 3: Examples & Use-cases — `AsyncStore` conformance

## Introduction

Phase 1 asked for a contract that is *executable* and a guide that is *operational*. This phase
fixes what that means concretely: the rule inventory, three worked scenarios that carry a store
author from "I have a struct implementing `AsyncStore`" to "I know which parts of the contract I
satisfy", the corner cases that will otherwise be discovered by breaking something, and the
mapping table that decision 2 requires before any existing test is deleted.

The progression is deliberate. **Scenario 1** is the ordinary path — a fixture and a suite for a
store that behaves normally. **Scenario 2** adds the validation tool, the safety levels and residue,
which is where a store meets a real backend. **Scenario 3** is the awkward one: the store that
cannot satisfy most of the suite and is nonetheless correct, plus the two ways a rule can look
green while checking nothing.

**Examples are conceptual, not runnable prototypes.** Nothing in `store_conformance` exists yet, so
no example here compiles; they are written at the precision of real signatures so Phase 4 can
implement them without re-deciding anything. The *tests* below are the runnable artefacts.

**No queries appear anywhere in this design.** Rules call `AsyncStore` directly — a conformance rule
routed through query evaluation would be testing the interpreter as well as the store. There is
therefore nothing for `liquers-validate` to check, and no `-R/` resource query, recipe or command
namespace is involved.

## Overview Table

| # | Item | Kind | Demonstrates / checks |
|---|---|---|---|
| S1 | Fixture + suite for a well-behaved store | Example | The ordinary path: `StoreCapabilities`, `keys_for`, `run_all`, `assert_conformant` |
| S2 | `liquers-store-check` against a document | Example | Safety levels, provenance defaults, residue at `create-only`, the report |
| S3 | A restricted store, and two vacuous rules | Example | `SkippedPrecondition`, argued `NA`, and how a rule passes while checking nothing |
| R1–R28 | The rule inventory | Rules | The nine sections of `STORE_SEMANTICS.md`, one ID per contract claim |
| H1–H8 | Harness unit tests | Unit | The report machinery itself: level gating, capability gating, `assert_conformant` in both directions, residue accounting |
| C1–C7 | Suites | Integration | Seven in-tree implementations plus trait defaults, natively and under wasm |
| D1 | Synchronization test | Integration | Rule IDs in code = IDs cited in the contract = IDs cited in the guide |
| M1 | Adoption mapping | Table | Which existing test each adopted ID replaces, and what is *not* replaced |

## The rule inventory

One ID per contract claim, grouped by `STORE_SEMANTICS.md` section. `Min` is the lowest
`SafetyLevel` the rule runs at; `Requires` is its `Capability` list.

| ID | Claim | §  | Requires | Min |
|---|---|---|---|---|
| `sibling01` | `removedir("sub")` leaves `subway/` untouched | 1 | RemoveDirectories | Scratch |
| `sibling02` | `listdir("sub")` reports nothing from `subway/` | 1 | Directories | CreateOnly |
| `sibling03` | `remove("data")` leaves `database/x` readable | 1 | Remove | Scratch |
| `sibling04` | `is_dir`/`contains` on `sub` are unaffected by `subway` | 1 | Directories | CreateOnly |
| `dir01` | A directory holding children is addressable by `is_dir` and `contains` | 2 | Directories | CreateOnly |
| `dir02` | `is_dir` on an absent key is `Ok(false)`, never `Err` | 2 | Directories | ReadOnly |
| `dir03` | Every entry `listdir` calls a directory answers `is_dir == true` | 2 | Directories | CreateOnly |
| `dir04` | A directory's metadata has `is_dir == true` and carries its key | 2 | Directories | CreateOnly |
| `dir05` | `contains` falls back to `is_dir` | 2 | Directories | CreateOnly |
| `explicit01` | `makedir` creates a childless directory that persists | 3 | ExplicitDirectories | CreateOnly |
| `explicit02` | A derived directory retires when its last child goes | 3 | Directories, Remove | Scratch |
| `explicit03` | Recursive `removedir` takes explicit descendants with it | 3 | ExplicitDirectories, RemoveDirectories | Scratch |
| `absence01` | `get`/`get_bytes`/`get_metadata` on an absent key give `KeyNotFound` | 4 | — | ReadOnly |
| `absence02` | `contains` on an absent key is `Ok(false)` | 4 | — | ReadOnly |
| `absence03` | `removedir` on an absent directory does not claim to have removed one | 4 | RemoveDirectories | CreateOnly |
| `remove01` | **Postcondition:** after `removedir` returns `Ok`, `is_dir` is false | 5 | RemoveDirectories | Scratch |
| `remove02` | `removedir` is recursive — no child survives it | 5 | RemoveDirectories | Scratch |
| `remove03` | `remove` deletes data and metadata together | 5 | Remove | Scratch |
| `prefix01` | `key_prefix()` reports the configured prefix | 6 | — | ReadOnly |
| `prefix02` | `is_supported` is false for a key outside the prefix | 6 | — | ReadOnly |
| `prefix03` | `is_supported` is false for a key the store cannot address | 6 | — | ReadOnly |
| `keyabs01` | Every fallible key-taking method refuses a relative key with `KeyNotAbsolute` | 7 | — | ReadOnly |
| `sidecar01` | A key colliding with the `.__metadata__` form is refused, not silently aliased | 8 | — | ReadOnly |
| `sidecar02` | Metadata written with `set_metadata` reads back | 8 | StoredMetadata | CreateOnly |
| `keys01` | **Every key `keys()` returns starts with the store's prefix** | 9 | EnumerateKeys | ReadOnly |
| `keys02` | `keys()` returns data keys, directories, and the prefix itself | 9 | EnumerateKeys | CreateOnly |
| `data01` | `set` then `get` returns the same bytes | 2 | Write | CreateOnly |
| `data02` | Writing an existing key replaces its content | 5 | Write | Scratch |

**What each level buys** — the count Phase 1's open question asked for, before the interface is
fixed:

| Level | Rules runnable | Cumulative |
|---|---|---|
| `ReadOnly` | 9 | 9 |
| `CreateOnly` | +11 | 20 |
| `Scratch` | +8 | 28 |
| `Unrestricted` | +0 | 28 |

Two findings fall out, and both belong in the tool's output rather than in this document alone:

- **`ReadOnly` is a third of the suite and misses every rule this project was created for.** The
  sibling rule, the `removedir` postcondition and the derived-directory lifecycle all need
  `Scratch`. A clean `read-only` report is genuinely weak evidence, which is why the tool prints
  the not-run counts rather than a bare "conformant".
- **No rule requires `Unrestricted`.** Level 4 exists for a *fixture* that cannot honour scratch
  bookkeeping — a store whose creations cannot be enumerated, say — not because any rule needs it.
  Stating that in the guide stops it being chosen out of vagueness.

## Example

### Scenario 1 — a fixture and a suite for a well-behaved store

**What it demonstrates:** the ordinary path, end to end, and the Phase 1 promise that a store author
answers questions rather than writes assertions.

The sequence: the author writes a fixture that owns a temporary location and answers three
questions — what the store can do, how much the test may do to it, and what keys satisfy a given
precondition. The harness does the rest: it gates each rule on capability and level, runs what
applies, collects a report, cleans up, and the test asserts on the report.

```rust
struct FileFixture { store: AsyncFileStore, root: PathBuf, created: Mutex<Vec<Key>> }

#[async_trait]
impl Fixture for FileFixture {
    fn store(&self) -> &dyn AsyncStore { &self.store }
    fn label(&self) -> String { "AsyncFileStore (temp dir)".to_owned() }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Scratch }

    // Every field named: adding a Capability is a compile error here, not a silent skip.
    fn capabilities(&self) -> StoreCapabilities {
        StoreCapabilities {
            write: true, remove: true, directories: true,
            explicit_directories: true, remove_directories: true,
            stored_metadata: true, enumerate_keys: true,
        }
    }

    async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
        let base = self.store.key_prefix();
        match request {
            KeyRequest::Fresh => Ok(vec![base.join(unique("f"))]),
            KeyRequest::FreshSiblings { count } =>
                Ok((0..*count).map(|i| base.join(format!("{}-{i}", unique("s")))).collect()),
            // The sibling rule's subject: one name is a proper prefix of the other.
            KeyRequest::FreshPrefixPair => {
                let stem = unique("sub");
                Ok(vec![base.join(&stem), base.join(format!("{stem}way"))])
            }
            KeyRequest::FreshNested { depth } => Ok(vec![nested(&base, *depth)]),
            KeyRequest::Existing => Ok(vec![self.seeded.clone()]),
            KeyRequest::ExistingDirectory => Ok(vec![self.seeded.parent()]),
            KeyRequest::Unsupported => Ok(vec![parse_key("elsewhere/x.txt")
                .map_err(|e| Unavailable { reason: e.message })?]),
        }
    }

    fn record_created(&self, key: &Key) { /* push under the mutex — sync, no await held */ }
    fn created_keys(&self) -> Vec<Key> { /* snapshot */ }
    async fn cleanup(&self) { /* remove created keys, best effort */ }
}

#[tokio::test]
async fn conformance_async_file_store() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = FileFixture::new()?;
    let report = run_all(&fixture).await;
    report.assert_conformant(&[])?;   // no allowed failures: this store is expected to be clean
    Ok(())
}
```

The `match` on `KeyRequest` is exhaustive by design (Phase 2): when a precondition is added, this
fixture stops compiling rather than silently declining a rule that was meant to run.

### Scenario 2 — the validation tool, safety levels and residue

**What it adds to Scenario 1:** the same rules against a store nobody wrote a fixture for, described
by a configuration document, with the safety machinery that makes that survivable.

```bash
# Default: read-only, because this document describes somebody's data.
$ liquers-store-check --config deploy/stores.yaml
resolved: store_type=opendal_fs prefix=data root=/srv/liquers/data
AsyncOpenDALStore (fs) · read-only · 9/28 rules run
  passed 8 · failed 1 · not run 19
  FAILED dir02  is_dir on an absent key is Ok(false), never Err  [§2]
    expected Ok(false), got Err(KeyNotFound)
  not run: 11 need create-only, 8 need scratch
exit 1
```

The report names the rules it could **not** run and the level that would run them, so a clean
default run cannot be mistaken for conformance. Raising the level on a `--config` store is always
explicit:

```bash
$ liquers-store-check --config deploy/stores.yaml --level create-only
...
LEFT BEHIND — this level cannot remove what it created:
  data/lqcheck-3f9a1c/f-01.txt
  data/lqcheck-3f9a1c/sub
  data/lqcheck-3f9a1c/subway
  20/28 rules run · passed 20 · failed 0 · not run 8 (need scratch)
```

Residue is printed **before** the summary, not in a trailer: at `create-only` the operator now owns
three keys they did not have, and that is the first thing they need to know.

A factory-built fixture inverts the default, because it is expendable by construction:

```bash
$ liquers-store-check --scratch opendal_fs        # defaults to --level scratch
AsyncOpenDALStore (fs, scratch) · scratch · 28/28 rules run · passed 28 · residue: none
exit 0
```

### Scenario 3 — a restricted store, and two rules that check nothing

**What it demonstrates:** the case Phase 1 said would be the challenge — a general suite meeting a
store that is deliberately narrow — and the failure mode that makes a green report worthless.

A store presenting one database table: each "file" is a serialized row, the key is a numeric row
ID, there are no directories, and an arbitrary key cannot be created because IDs are assigned.

```rust
fn capabilities(&self) -> StoreCapabilities {
    StoreCapabilities {
        write: true,               // a row can be inserted
        remove: true,
        directories: false,        // there is no hierarchy to have
        explicit_directories: false,
        remove_directories: false,
        stored_metadata: false,    // derived from the column types
        enumerate_keys: true,      // SELECT id FROM t
    }
}

async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
    match request {
        KeyRequest::Fresh => Ok(vec![self.prefix.join(self.next_id()?.to_string())]),
        KeyRequest::FreshSiblings { count } => Ok(self.next_ids(*count)?),
        KeyRequest::FreshPrefixPair => Err(Unavailable {
            reason: "row IDs are numeric; no ID is a proper prefix of another".to_owned() }),
        KeyRequest::FreshNested { .. } => Err(Unavailable {
            reason: "the key space is one level deep: a row ID".to_owned() }),
        KeyRequest::Existing => Ok(vec![self.any_existing_row()?]),
        KeyRequest::ExistingDirectory => Err(Unavailable {
            reason: "no directories".to_owned() }),
        KeyRequest::Unsupported => Ok(vec![self.prefix.join("not-a-number")]),
    }
}
```

The resulting report is a *subset*, and every gap has a stated reason:

| Outcome | Count | Why |
|---|---|---|
| Passed | 12 | `absence01`–`absence02`, `prefix01`–`prefix03`, `keyabs01`, `sidecar01`, `keys01`–`keys02`, `data01`–`data02`, `remove03` |
| SkippedCapability | 15 | every `dir*` and `explicit*`, `sibling01`, `sibling02`, `sibling04`, `absence03`, `remove01`–`remove02`, `sidecar02` |
| SkippedPrecondition | 1 | `sibling03` — it needs a `FreshPrefixPair`, and numeric IDs have none |

**Twelve of twenty-eight, and that is the correct answer for this store.**

Note which mechanism did the work: **capability gating accounts for 15 of the 16 gaps, and the
`KeyRequest` decline for one.** Capability gating fires first, so a store with no directories never
reaches `keys_for` for a directory rule at all. That is worth knowing before Phase 4 — the
`Capability` vocabulary carries most of the weight for a restricted store, and `KeyRequest` is the
long tail that catches what capabilities cannot express (a store that *has* directories but whose
names can never form a prefix pair). Both are needed; they are not redundant. The guide's counterpart
to `LANGUAGE-INTEGRATION_GUIDE.md` §3 says so explicitly: for a deliberately restricted store many
`NA`s are expected, and what keeps that from becoming an excuse is that each one is *argued* — the
`reason` string is in the report, reviewable, and wrong reasons are visible.

**And the trap.** Two rules that would pass whatever the store does:

```rust
// VACUOUS — the two-branch match. Passes if removedir works, and equally if it is a no-op
// that reports success, which is the exact defect remove01 exists to catch.
match store.removedir(&dir).await {
    Ok(()) => RuleOutcome::Passed,
    Err(_) => RuleOutcome::Passed,   // "some stores refuse removedir"
}

// CORRECT — assert the postcondition, and let capability gating handle refusal.
store.removedir(&dir).await?;
if store.is_dir(&dir).await? { return failed("removedir returned Ok but the directory is still there"); }
```

```rust
// VACUOUS — an existence check on something never absent. `subway` was created by this rule,
// so of course it is there; nothing about `sub` was tested.
if store.contains(&subway).await? { RuleOutcome::Passed } else { failed(..) }

// CORRECT — read it, and compare the bytes. A removedir that truncated rather than
// unlinked would pass the check above and fail this one.
assert_bytes_eq(store.get_bytes(&subway).await?, original)
```

The test for every rule is the question `LANGUAGE-INTEGRATION_GUIDE.md` §3 poses: *what
implementation change would make this fail?* If nothing plausible would, the rule is decoration —
and a decorative rule in a conformance suite is worse than a missing one, because it reports safety
it never checked.

## Corner Cases

| # | Case | Why it bites | Handling |
|---|---|---|---|
| 1 | `LocalStorageStore` persists across tests in one browser session | A suite that assumes a fresh profile passes locally and fails the second run | Fixture namespaces every key and clears its namespace first, as `store_local_STORE.rs` already does |
| 2 | Two temp directories collide | Nanosecond-stamped names collide under parallel `cargo test` | Reuse the `unique_temp_dir` pattern from `store_key_absolute.rs`, plus the rule's own unique stem |
| 3 | Check-then-write is not atomic at `Scratch` | Phase 1 accepted this; a concurrent writer in the gap is overwritten | Documented as a limit, not a guarantee; unit test H6 asserts the *check* happens, not that it is atomic |
| 4 | A rule creates and does not record | Residue is under-reported and cleanup misses it — the one failure the levels exist to prevent | H7 runs every rule against a recording stub store and asserts `created_keys()` covers every key the stub saw written |
| 5 | `AsyncStoreRouter` mixes implementations | A rule's key may land in a different store than its precondition assumed | The router fixture supplies keys from **one** member store per request and says which; a cross-store rule is out of scope for this project |
| 6 | Report must serialize on wasm | `serde_json` is present, but `Key` and `ErrorType` must round-trip | H8 asserts a report round-trips through JSON on both targets |
| 7 | The feature-off build | `store-conformance` off must compile everywhere, including the `create_fixture` `#[cfg]` | Added as a row to `scripts/check-build-matrix.sh` |

## Adoption mapping (decision 2)

The table Phase 4 executes row by row. **No row is deleted until its replacement passes, and fails
when the behaviour is broken.**

| Adopted ID | Existing test | Location | Replaced? |
|---|---|---|---|
| `sibling01`–`sibling04` | `sibling01`–`sibling04` | `liquers-store/src/opendal_store.rs` | Yes, once the rule runs against `AsyncOpenDALStore` |
| `dir01`–`dir04` | `dir01`–`dir04` | `liquers-store/src/opendal_store.rs` | Yes |
| `remove01`–`remove02` | `remove01`–`remove02` | `liquers-store/src/opendal_store.rs` | Yes |
| `prefix01`, `router01` | same | `liquers-store/src/opendal_store.rs` | `prefix01` yes; `router01` **kept** — router selection is not a store rule |
| `sidecar01` | `pathmap01`–`pathmap07` | `liquers-store/src/opendal_store.rs` | **Partly.** The refusal generalizes; the path-mapping internals do not. Keep `pathmap02`–`pathmap07` |
| `explicit01`–`explicit03` | `diridx04`, `diridx05`, `diridx09`, `memdir04` | `store_dir_index.rs`, `store.rs` | **Kept.** `DirectoryIndex` is a component with its own tests; the rules test stores, not it |
| `keyabs01` | `keyabs07`–`keyabs11`, `keyabs16`–`keyabs17` | `store.rs`, `opendal_store.rs` | Yes, per store |
| — | `keyabs12`–`keyabs14` | `tests/store_key_absolute.rs` | **Kept.** They test refusal through evaluation and recipe CWD, which is not a store rule |
| `absence01`–`absence03` | `traitdef01` | `liquers-core/src/store.rs` | Yes |

Roughly nine of the ~15 duplicated functions are replaced; six are kept for a stated reason. That
ratio is itself worth recording — the suite generalizes less than a count of matching IDs suggests.

## Test Plan

### H — harness unit tests (`liquers-core/src/store_conformance/`, `#[cfg(test)] mod tests`)

Tests of the machinery, using a stub store and stub fixture — no real backend.

| ID | Checks |
|---|---|
| `H1` | A rule whose `requires` is unmet yields `SkippedCapability` and is **not called** |
| `H2` | A rule above the fixture's level yields `NotRunSafetyLevel` and is **not called** |
| `H3` | `assert_conformant(&[])` fails when any rule failed, naming it |
| `H4` | `assert_conformant` accepts a failure listed in `allowed` |
| `H5` | **`assert_conformant` fails when an `allowed` rule passed**, naming the stale entry |
| `H6` | A `Scratch` rule calls `contains`/`is_dir` before its first mutation |
| `H7` | Every rule records every key it creates (`created_keys()` covers the stub's writes) |
| `H8` | `ConformanceReport` round-trips through JSON and YAML |

`H5` and `H7` are the two that would be easiest to omit and hardest to re-learn: one keeps ignore
lists honest, the other keeps residue reporting truthful.

### C — conformance suites

| ID | Store | Crate | Harness | Level |
|---|---|---|---|---|
| `C1` | `AsyncMemoryStore` | liquers-core | `#[tokio::test]` | Scratch |
| `C2` | `AsyncFileStore` | liquers-core | `#[tokio::test]` | Scratch |
| `C3` | `AsyncStoreRouter` (memory + file) | liquers-core | `#[tokio::test]` | Scratch |
| `C4` | trait defaults (`MinimalStore`) | liquers-core | `#[tokio::test]` | ReadOnly |
| `C5` | `AsyncOpenDALStore` (fs, temp dir) | liquers-store | `#[tokio::test]` | Scratch |
| `C6` | `FetchStore` (stub global `fetch`) | liquers-web | `#[wasm_bindgen_test]`, Node | ReadOnly |
| `C7` | `LocalStorageStore` | liquers-web | `#[wasm_bindgen_test]`, `browser-tests` | Scratch |
| `C8` | `JsStore` (stub JS object) | liquers-web | `#[wasm_bindgen_test]`, Node | Scratch |

`C1` ships with `prefix02` in `allowed_failures` citing
`CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX` **only if** that design has not merged; `H5`
then forces the entry's removal the moment it does.

### D — synchronization

`D1` (`liquers-core/tests/conformance_docs_CONF.rs`): the ID sets from `rules()`, from
`STORE_SEMANTICS.md` and from `STORE_IMPLEMENTATION_GUIDE.md` are equal, with the difference
printed in both directions. Locates `specs/` by walking up from `CARGO_MANIFEST_DIR` and skips with
a warning if absent, as a packaged crate has no `specs/`. Modelled on
`liquers-lib/tests/registry_export.rs`.

### Commands to run

```bash
cargo test -p liquers-core --features store-conformance
cargo test -p liquers-store --features store-conformance
cargo test -p liquers-web --target wasm32-unknown-unknown                     # C6, C8
CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
  --target wasm32-unknown-unknown --features browser-tests                    # C7
bash scripts/check-build-matrix.sh          # + a store-conformance-off row
```

## Documentation and Learning Log

Collected during implementation, for Phase 5:

- **The divergence census** — which rules each in-tree store fails on first run. This is the
  evidence the issue predicted and the most valuable single artefact this project produces.
- **Every `Unavailable` reason** a fixture returns, and whether it was a real limit or a fixture
  written lazily.
- **Runnable-rule counts per level**, measured rather than estimated as above.
- **Residue** a `create-only` run actually leaves.
- **Rules that turned out vacuous** when the "what change would make this fail?" test was applied —
  each one becomes a line in the guide, as five such tests did in `LANGUAGE-INTEGRATION_GUIDE.md`.
- **The real adoption ratio**: how many existing tests the shared rules genuinely replaced against
  the nine predicted here.
- **Whether `create_fixture`'s synchronous signature blocked any factory**, which decides whether
  an async factory is worth filing.
