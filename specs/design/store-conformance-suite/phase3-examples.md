# Phase 3: Examples & Use-cases — `AsyncStore` conformance

## Introduction

Phase 1 asked for a contract that is *executable* and a guide that is *operational*. This phase
fixes what that means concretely: the rule inventory, three worked scenarios that carry a store
author from "I have a struct implementing `AsyncStore`" to "I know which parts of the contract I
satisfy", the corner cases that will otherwise be discovered by breaking something, and the
mapping table that decision 2 requires before any existing test is deleted.

The progression is deliberate. **Scenario 1** is the ordinary path — a fixture and a suite for a
store that behaves normally. **Scenario 2** covered the validation tool and moved out with it
at the Phase 4 gate. **Scenario 3** is the awkward one: the store that
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
| ~~S2~~ | ~~`liquers-store-check` against a document~~ | — | Deferred to `STORE-CONFORMANCE-VALIDATION-TOOL` |
| S3 | A restricted store, and two vacuous rules | Example | `SkippedPrecondition`, argued `NA`, and how a rule passes while checking nothing |
| R1–R32 | The rule inventory | Rules | The nine sections of `STORE_SEMANTICS.md`, one ID per contract claim |
| H1–H8 | Harness unit tests | Unit | The report machinery itself: level gating, capability gating, `assert_conformant` in both directions, residue accounting |
| C1–C8 | Suites | Integration | Seven in-tree implementations plus the trait defaults, natively and under wasm (a ninth, `NoAsyncStore`, is added by the Phase 4 review) |
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
| `sibling05` | A key refused as data is not addressable as a *directory* either | 1 | Directories | ReadOnly |
| `dir01` | A directory holding children is addressable by `is_dir` and `contains` | 2 | Directories | CreateOnly |
| `dir02` | `is_dir` on an absent key is `Ok(false)`, never `Err` | 2 | Directories | ReadOnly |
| `dir03` | Every entry `listdir` calls a directory answers `is_dir == true` | 2 | Directories | CreateOnly |
| `dir04` | A directory's metadata has `is_dir == true` and carries its key | 2 | Directories | CreateOnly |
| `dir05` | `contains` falls back to `is_dir` | 2 | Directories | CreateOnly |
| `dir06` | The agreement holds in reverse: a key answering `is_dir == true` appears in its parent's `listdir` | 2 | Directories | CreateOnly |
| `dir07` | Directory metadata does **not** populate `children` | 2 | Directories | CreateOnly |
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
| `prefix03` | `is_supported` is false for a key whose *shape* the store cannot address | 6 | — | ReadOnly |
| `prefix04` | **`is_supported` is *true* for a key inside the prefix the store can address** | 6 | — | ReadOnly |
| `keyshape01` | Every fallible key-taking method refuses a relative key with `KeyNotAbsolute` | 7 | — | CreateOnly |
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
| `ReadOnly` | 10 | 10 |
| `CreateOnly` | +14 | 24 |
| `Scratch` | +8 | 32 |

*(Counts as implemented. `prefix04` was added by the Phase 4 review — `prefix02` and `prefix03` both
assert `is_supported` is **false**, and the trait default returns `false` unconditionally, so
without a positive case a store that refuses everything passed both and looked conformant.)*

Two findings fall out, and both belong in the tool's output rather than in this document alone:

- **`ReadOnly` is under a third of the suite and misses every rule this project was created for.**
  It shrank further at the Phase 4 review: `keyshape01` moved to `CreateOnly` on the finding that
  checking a relative key is refused means *calling* `set`, `remove` and `removedir` with one — so
  a store whose refusal is broken would mutate at the level advertised as safe against real data. The
  sibling rule, the `removedir` postcondition and the derived-directory lifecycle all need
  `Scratch`. A clean `read-only` report is genuinely weak evidence, which is why the tool prints
  the not-run counts rather than a bare "conformant".
- **The count removed a level.** Phase 1 specified a fourth, `unrestricted`; nothing in the
  inventory reaches for it, because every rule is satisfied at `scratch` or below. A level no rule
  needs can only permit damage no check asked for, so it is gone — added back when something
  actually requires it. This is the clearest return on counting before fixing the interface, rather
  than after.

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

### Scenario 2 — **deferred with the tool**

This scenario showed `liquers-store-check` against a configuration document: the provenance
defaults, a `create-only` run printing its residue before the summary, and a report naming the
rules the level excluded. It moved, with the rest of the tool's design, to
[`STORE-CONFORMANCE-VALIDATION-TOOL`](../../issues/STORE-CONFORMANCE-VALIDATION-TOOL.md).

What it demonstrated that **still applies here**: the safety levels are a property of the *rules*,
not of the tool. Every rule declares the lowest level it runs at, `run_all` gates on it, and the
report distinguishes "not run at this level" from "passed" — all exercised by `H2` and by the C
suites, which run at `Scratch` against fixtures they own.

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
| Passed | 12 | `absence01`–`absence02`, `prefix01`–`prefix03`, `keyshape01`, `sidecar01`, `keys01`–`keys02`, `data01`–`data02`, `remove03` |
| SkippedCapability | 18 | every `dir*` and `explicit*`, `sibling01`, `sibling02`, `sibling04`, `sibling05`, `absence03`, `remove01`–`remove02`, `sidecar02` |
| SkippedPrecondition | 1 | `sibling03` — it needs a `FreshPrefixPair`, and numeric IDs have none |

**Twelve of thirty-one, and that is the correct answer for this store.**

Note which mechanism did the work: **capability gating accounts for 18 of the 19 gaps, and the
`KeyRequest` decline for one.** Capability gating fires first, so a store with no directories never
reaches `keys_for` for a directory rule at all. That is worth knowing before Phase 4 — the
`Capability` vocabulary carries most of the weight for a restricted store, and `KeyRequest` is the
long tail that catches what capabilities cannot express (a store that *has* directories but whose
names can never form a prefix pair). Both are needed; they are not redundant. The guide's counterpart
to `LANGUAGE-INTEGRATION_GUIDE.md` §3 says so explicitly: for a deliberately restricted store many
`NA`s are expected, and what keeps that from becoming an excuse is that each one is *argued* — the
`reason` string is in the report, reviewable, and wrong reasons are visible.

**And the trap.** Two rules that would pass whatever the store does:

Both snippets are **rule bodies** — `async fn(..) -> Result<(), RuleOutcome>`, where `?` is legal
because `From<Error> for RuleOutcome` makes a store error an `Errored` outcome (Phase 2). The
registry holds the wrapper, which cannot return `Err`.

```rust
// VACUOUS — the two-branch match. Passes if removedir works, and equally if it is a no-op
// that reports success, which is the exact defect remove01 exists to catch.
match store.removedir(&dir).await {
    Ok(()) => return Ok(()),
    Err(_) => return Ok(()),   // "some stores refuse removedir"
}

// CORRECT — assert the postcondition, and let capability gating handle refusal.
store.removedir(&dir).await?;                       // a store error becomes Errored
if store.is_dir(&dir).await? {
    return Err(failed("removedir returned Ok but the directory is still there"));
}
Ok(())
```

```rust
// VACUOUS — an existence check on something never absent. `subway` was created by this rule,
// so of course it is there; nothing about `sub` was tested.
if store.contains(&subway).await? { Ok(()) } else { Err(failed("subway is gone")) }

// CORRECT — read it, and compare the bytes. A removedir that truncated rather than
// unlinked would pass the check above and fail this one.
let bytes = store.get_bytes(&subway).await?;
if bytes != original { return Err(failed("removedir(sub) altered subway's content")); }
Ok(())
```

**`?` and `Err(failed(..))` mean different things, and the distinction is load-bearing:** `?`
reports that the store *errored*, `Err(failed(..))` that it *disagreed with the contract*. A rule
that collapses the two makes a permissions failure look like a conformance defect.

The test for every rule is the question `LANGUAGE-INTEGRATION_GUIDE.md` §3 poses: *what
implementation change would make this fail?* If nothing plausible would, the rule is decoration —
and a decorative rule in a conformance suite is worse than a missing one, because it reports safety
it never checked.

### Traceability: the eleven divergences the issue enumerated

The issue's claim is specific — *"a suite run against every implementation would have caught rows
1, 2, 3, 4, 7, 8, 9 and 10 at the commit that introduced them."* That claim is only worth anything
if the inventory actually does. Row by row:

| Issue row | The divergence | Caught by |
|---|---|---|
| 1 | `is_dir` on an absent key: `Ok(false)` vs `Err` | `dir02` |
| 2 | A directory with children not addressable on a flat backend | `dir01` |
| 3 | `contains` falls back to `is_dir` — or does not | `dir05` |
| 4 | `removedir` scoped to the directory vs the path prefix (**the P0 data loss**) | `sibling01`, `sibling03` |
| 5 | Is `removedir` recursive? Doc said no, every implementation said yes | `remove02` |
| 6 | `removedir` on a directory that does not exist | `absence03` |
| 7 | Does `makedir` create anything? (**P0**) | `explicit01` |
| 8 | Does `is_supported` consult the prefix? | `prefix02` |
| 9 | Does `key_prefix()` report the configured prefix? | `prefix01` — **only once `Fixture` gains `expected_prefix()`; see below** |
| 10 | What does `keys()` return? | `keys01`, `keys02` |
| 11 | How directory structure is derived on a flat backend | **behaviourally only** — see below |

Ten of eleven are caught directly, including both P0s and all eight the issue named — **with one
correction the Phase 4 final review forced.**

**Row 9 needs ground truth the fixture does not yet supply.** As first drafted, `prefix01` could
only compare `store.key_prefix()` against itself, because `Fixture` exposed no independent notion of
the configured prefix — so `AsyncOpenDALStore` returning `Key::new()` (`opendal_store.rs:296`), the
divergence row 9 *is*, would have passed. `keys01` was circular for the same reason, and passes
unconditionally for any store whose prefix is root, `AsyncStoreRouter` included. The design's own
examples disagreed about this without noticing: Phase 3 Scenario 1 derived its keys from
`self.store.key_prefix()`, Scenario 3 from a fixture field.

**`Fixture` therefore gains `fn expected_prefix(&self) -> Key`,** rules are forbidden to derive
anything from `store.key_prefix()`, and `prefix01` compares the two. Recorded here rather than only
in Phase 2 because this table is the design's claim to solve the issue, and it was briefly wrong.

**Row 11 is different in kind and is deliberately not caught as stated.** It complains of "four
private mechanisms, no two alike, and one store with none" — a statement about *implementation*,
not about observable behaviour, and a conformance suite cannot police how a store derives an
answer, only that the answer is right. `dir01`–`dir07` check that whatever mechanism a store uses
produces consistent, agreeing answers, which is the part that can be checked from outside. The
structural half was fixed by `design/opendal-path-mapping/` extracting `DirectoryIndex`
(`CORE-DIRECTORY-INDEX-NOT-SHARED`, closed), and nothing here re-opens it. Recorded so that nobody
later reads row 11 as an uncovered gap.

### One contract claim deliberately left uncovered

`STORE_SEMANTICS.md` §8 requires that **a path a store cannot decode is skipped by a listing rather
than failing it** — one unexpected object in a shared bucket must not make a directory unlistable.
No rule checks it, and none can: producing an undecodable path means writing *behind* the store,
directly to the backend, which is precisely what `AsyncStore` does not expose. A fixture hook for
it would be a backend-shaped escape hatch in an interface whose value is that it has none.

It stays a per-store test, and this is one reason `pathmap02`–`pathmap07` are kept rather than
replaced in the mapping below. The guide records it as a claim a store author must verify
themselves, with the OpenDAL tests as the worked example.

## Corner Cases

| # | Case | Why it bites | Handling |
|---|---|---|---|
| 1 | `LocalStorageStore` persists across tests in one browser session | A suite that assumes a fresh profile passes locally and fails the second run | Fixture namespaces every key and clears its namespace first, as `store_local_STORE.rs` already does |
| 2 | Two temp directories collide | Nanosecond-stamped names collide under parallel `cargo test` | Reuse the `unique_temp_dir` pattern from `store_key_absolute.rs`, plus the rule's own unique stem |
| 3 | Check-then-write is not atomic at `Scratch` | Phase 1 accepted this; a concurrent writer in the gap is overwritten | Documented as a limit, not a guarantee; unit test H6 asserts the *check* happens, not that it is atomic |
| 4 | A rule creates and does not record | Residue is under-reported and cleanup misses it — the one failure the levels exist to prevent | H7 runs every rule against a recording stub store and asserts `created_keys()` covers every key the stub saw written |
| 5 | `AsyncStoreRouter` mixes implementations | A rule's key may land in a different store than its precondition assumed | The router fixture supplies keys from **one** member store per request and says which. Two router-specific rules are added per Phase 4 finding F9 — the issue's Impact section is entirely about the router, so leaving composition unchecked would miss the point |
| 6 | Report must serialize on wasm | `serde_json` is present, but `Key` and `ErrorType` must round-trip | H8 asserts a report round-trips through JSON on both targets |
| 7 | The feature-off build | `store-conformance` off must compile everywhere, including the `create_fixture` `#[cfg]` | Added as a row to `scripts/check-build-matrix.sh` |

## Adoption mapping (decision 2)

The table Phase 4 executes row by row. **No row is deleted until its replacement passes, and fails
when the behaviour is broken.**

| Adopted ID | Existing test | Location | Replaced? |
|---|---|---|---|
| `sibling01`–`sibling04` | `sibling01`–`sibling04` | `liquers-store/src/opendal_store.rs` | Yes, once the rule runs against `AsyncOpenDALStore` |
| `dir01`–`dir05` | `dir01`–`dir05` | `liquers-store/src/opendal_store.rs` | Yes — `dir05` (directory metadata is marked as a directory) generalizes as the rule of the same name |
| `remove01`–`remove02` | `remove01`–`remove02` | `liquers-store/src/opendal_store.rs` | Yes |
| `prefix01`, `router01` | same | `liquers-store/src/opendal_store.rs` | `prefix01` yes; `router01` **kept** — router selection is not a store rule |
| `sidecar01` | `pathmap01`–`pathmap07` | `liquers-store/src/opendal_store.rs` | **Partly.** The refusal generalizes; the path-mapping internals do not. Keep `pathmap02`–`pathmap07` |
| `explicit01`–`explicit03` | **all of** `diridx01`–`diridx09`, `memdir01`–`memdir05` | `store_dir_index.rs`, `store.rs` | **All kept.** `DirectoryIndex` is a component with its own unit tests, and `memdir*` test `AsyncMemoryStore`'s use of it. The rules test *stores* through `AsyncStore`; these test the component beneath. No deletion here |
| `keyshape01` | `keyabs07`, `keyabs08`, `keyabs10`, `keyabs16`, `keyabs17` | `store.rs`, `opendal_store.rs` | Yes, per store |
| — | `keyabs09` | `liquers-core/src/store.rs` | **Kept.** It tests the *synchronous* `FileStore`, which Phase 1 decision 4 puts out of scope; no async rule can replace it |
| `prefix02`, `prefix03` | `keyabs11` | `liquers-core/src/store.rs` | Yes — `keyabs11_is_supported_false_on_directly_held_store` is about `is_supported`, not relative keys |
| — | `keyabs12`–`keyabs14` | `tests/store_key_absolute.rs` | **Kept.** They test refusal through evaluation and recipe CWD, which is not a store rule |
| **`dir05`** | `traitdef01` | `liquers-core/src/store.rs` | Yes — `traitdef01_default_contains_falls_back_to_is_dir` is about the `contains`→`is_dir` fallback, **not** absence. Mapping it to `absence01`–`absence03` would have deleted the only coverage of issue row 3 for the trait defaults |

**The table accounts for every existing test in the named files**; an ID present there and absent
here would be an unreviewed deletion, which is the failure this table exists to prevent.

Counted honestly: about **twelve** functions are replaced and about **twenty** kept — the whole
`diridx` and `memdir` families, `router01`, `pathmap02`–`pathmap07`, and `keyabs12`–`keyabs14`.
The suite generalizes considerably less than a count of matching IDs suggests, because a shared
rule tests a *store through `AsyncStore`* while many of these test a component beneath it or a path
through evaluation above it. That is worth knowing before Phase 4 promises a tidy-up.

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

`C8` at `Scratch` requires the stub to implement **every** method `JsStore` forwards: it returns
`KeyNotSupported` for any the JS object omits (`js_store.rs:211–256`), so a partial stub would
report capability gaps that belong to the stub rather than to `JsStore`.

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
