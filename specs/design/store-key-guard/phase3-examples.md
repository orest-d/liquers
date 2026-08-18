# Phase 3: Examples & Use-cases — Absolute Store Keys

## High-Level Introduction

Phase 1's purpose was a boundary: relative keys are a plan-level feature, and a store requires an
absolute key. That boundary has almost no *user-facing* surface — nobody writes `..` on purpose — so
the examples here are not a feature demo. They are the three audiences the rule actually has.

**Scenario 1** is the caller's view: the refusal, what it says, and the attack it stops. It is the
issue's own reproduction, and it doubles as the proof that the fix works.
**Scenario 2** is the backend author's view: what adopting the rule in a new store looks like, and
why `is_supported` alone is not it — the detail that Phase 2's architecture turns on.
**Scenario 3** is the pitfalls, and it earns its place here more than usual: the escape verification
done for this phase found a way for a *correct-looking test to pass for the wrong reason*, which
would leave the guard untested while appearing to cover it.

The tests then pin all three, plus the regression surface — CWD resolution, recipes and normal keys
must keep working, because the guard's whole risk is refusing something that legitimately worked.

## Example Type

**Runnable, not conceptual.** Every example below is a test that lands in Phase 4, in the file named
with it. The reason is specific to this design: the "examples" of a refusal *are* its tests, and a
conceptual snippet of a key being rejected would demonstrate nothing that the test does not.

This was assumed rather than asked — the design is a P0 fix landing in one PR, and conceptual
examples would produce no artifact. **Say so if you wanted separate prose examples**; they would go
in the trait rustdoc rather than in an `examples/` binary.

## Verified Before Planning: the escape is real, and deeper than the issue states

`STORE-FILESTORE-PATH-TRAVERSAL` closes by noting the `PathBuf::push` behaviour was *read from the
code, never demonstrated*, and asks for confirmation before the fix. Done, reproducing
`key_to_path`'s exact `path.push(key.to_string())`:

| Key | Intermediate dir | Result |
|---|---|---|
| `../SECRET.txt` | n/a | **read** — 22 bytes from outside the root |
| `../WRITTEN.txt` | n/a | **written** — file created outside the root |
| `a/../../SECRET.txt` | `a/` exists | **read** — escapes |
| `a/../../SECRET.txt` | `a/` absent | refused, `ENOENT` |
| `missing/../../SECRET.txt` | absent | refused, `ENOENT` |

Both read and write escape, confirming the issue's Impact section. The last two rows are the finding
that shapes the tests: Linux resolves `..` by walking **real** directories, so a deep traversal only
escapes when the intermediate directory exists. A store holding any directory at all satisfies that.

## Overview Table

| # | Type | Name | Purpose |
|---|---|---|---|
| 1 | Example | Refusing the traversal | The caller's view: the issue's reproduction, before and after |
| 2 | Example | Adopting the rule in a new backend | The implementor's view: where the check goes and why not only `is_supported` |
| 3 | Example | Pitfalls | The false-negative test, `to_absolute` vs `as_absolute`, and error-type confusion |
| 4 | Unit tests | `keyabs01`–`keyabs06` | `Key` predicate and accessors, the new error, the `CwdCursor` rename |
| 5 | Unit tests | `keyabs07`–`keyabs11` | Per-store refusal, path builders, routers, `is_supported` |
| 6 | Integration | `keyabs12`–`keyabs16` | End-to-end query, recipes, regression surface, HTTP status, OpenDAL |
| 7 | Conformance | `STORE05` (revised) | The cross-language rule, in `liquers-web` and the integration guide |

---

## Example 1: Refusing the traversal

### Connection to the High-Level Design

This is Phase 1's purpose reduced to one call. The key is a legal `Key`, the query plans cleanly,
the store is an ordinary `AsyncFileStore` — and the store is the component that says no, because it
is the boundary where "absolute" starts being required.

### Scenario

A deployment serves the query API over HTTP with a file store rooted at `/srv/data`. A caller
requests `-R/../../etc/passwd`. Nothing upstream rejects it: `..` is a legal `ResourceName`, the
parser accepts it, and the planner emits an ordinary `GetAsset`. Today the store reads the file.

### Sequence of Steps

1. `parse_query` accepts the text and yields a resource segment whose key is `["..","..","etc","passwd"]`.
2. The plan is built — `GetAsset[.., .., etc, passwd]`. No step resolves `..`, because
   `CwdCursor::needs_cwd`-style resolution only runs for CWD-relative operands and this key is
   already treated as absolute.
3. The interpreter asks the store for the key.
4. **The store checks the precondition first** — `key.as_absolute()?` — and returns
   `ErrorType::KeyNotAbsolute` before any path is built.
5. `liquers-axum` maps that to `400 Bad Request`.

### Core Example Code

```rust
// liquers-core/src/store.rs — mod tests   (keyabs08)
#[tokio::test]
async fn keyabs08_async_file_store_refuses_traversal() -> Result<(), Error> {
    let root = unique_temp_dir("keyabs08").await;          // helper, per existing convention
    let secret = root.parent().unwrap_or(&root).join("SECRET.txt");
    tokio::fs::write(&secret, b"outside the store root").await.unwrap();

    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    store.makedir(&parse_key("a")?).await?;                 // see Pitfall 1: this line is the test

    for text in ["../SECRET.txt", "a/../../SECRET.txt", "a/./b.txt"] {
        let key = parse_key(text)?;
        let error = store.get(&key).await.expect_err("must refuse");
        assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
        assert!(!store.is_supported(&key), "{text} must not route here");
        // writes too — a read-only guard leaves the write path open
        store.set(&key, b"x", &Metadata::MetadataRecord(MetadataRecord::new()))
            .await.expect_err("must refuse writes");
    }

    // the file outside the root is untouched
    assert_eq!(tokio::fs::read(&secret).await.unwrap(), b"outside the store root");
    Ok(())
}
```

**What makes this the reproduction rather than a restatement of the guard:** the `SECRET.txt`
assertion. Asserting only the error type would still pass if the store errored for an unrelated
reason; asserting the outside file is unchanged after an attempted `set` is what actually pins the
escape closed.

---

## Example 2: Adopting the rule in a new backend

### Connection to the High-Level Design

Phase 2's architecture rests on one claim: `is_supported` is not the enforcement point, because only
the routers consult it. This example is that claim made concrete, and it is what a future backend
author copies.

### Scenario

Someone adds an IndexedDB store (`WEB-NATIVE-IO-TIER2`). They implement `AsyncStore`. The question
they need answered is where the precondition goes — and the wrong answer looks entirely reasonable.

### Sequence of Steps

1. Every fallible, key-taking method opens with `key.as_absolute()?`, shadowing the parameter so the
   unchecked key cannot be used afterwards.
2. `is_supported` adds `&& !key.is_relative()`, so the routers stop selecting the store.
3. Methods that only delegate to a checked method inherit the check and are not double-guarded.
4. Backends that build a path get it structurally too: the path builder is fallible.

### Core Example Code

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for IndexedDbStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let key = key.as_absolute()?;   // shadows: the unchecked key is now unreachable
        // … backend read using the checked `key`
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;   // writes too, not only reads
        // … backend write
    }

    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix) && !key.is_relative()
    }
}
```

### The trap this example exists to prevent

```rust
// WRONG — compiles, passes a routed test, and is exploitable.
fn is_supported(&self, key: &Key) -> bool {
    key.has_key_prefix(&self.prefix) && !key.is_relative()
}
// …with no check in `get`/`set`.
```

A store reached through `AsyncStoreRouter` behaves correctly here, because `find_store` consults
`is_supported` (`store.rs:1793`). A store held directly — which is how `Environment` is often
configured, and how every store unit test in the tree constructs one — never runs it. The store
therefore *tests* clean through the router and is wide open in production.

**`keyabs11` asserts against a directly held store, never through a router**, precisely so it cannot
pass this way.

---

## Example 3: Pitfalls

### Pitfall 1 — the test that passes for the wrong reason

```rust
// LOOKS like it covers deep traversal. Does not.
let store = AsyncFileStore::new(&root, &Key::new());
store.get(&parse_key("a/../../SECRET.txt")?).await.expect_err("refused");
```

Against an **unfixed** store this still fails — with `ENOENT`, because the kernel resolves `..` by
walking real directories and `a/` does not exist. The test is green, the guard is absent, and the
suite says the deep form is covered.

Two corrections, both required:

1. **Create the intermediate directory** (`store.makedir(&parse_key("a")?)`), so the traversal
   genuinely resolves and only the guard can stop it.
2. **Assert the error *type***, not merely that an error occurred — `ENOENT` surfaces as
   `KeyReadError`, `KeyNotAbsolute` is the guard.

This is the mutation check the `STORE05` blueprint asks for: remove the guard and the test must go
red. Without both corrections it stays green.

### Pitfall 2 — `to_absolute` resolves, `as_absolute` asserts

```rust
key.to_absolute(&cwd)   // RESOLVES `.` and `..` against cwd — plan level, returns a Key
key.as_absolute()?      // ASSERTS there is nothing to resolve — store level, returns Result
```

One word apart, opposite operations. Reaching for `to_absolute` inside a store would silently
normalize `a/../b` to `b` and make two distinct addresses alias one asset — the failure mode Phase 1
rejected normalization to avoid. `keyabs04` pins that `to_absolute` still resolves as before, so the
new neighbours cannot quietly change it.

### Pitfall 3 — two refusals that are not the same refusal

| Key shape | Error | Why |
|---|---|---|
| `a/../b`, `../b`, `a/./b` | `KeyNotAbsolute` | Relative — a plan-level construct at the wrong layer |
| `a//b` (empty segment) | `KeyNotSupported` | Malformed, not relative |
| `x/y` where no store serves `x` | `KeyNotSupported` | Routing, nothing wrong with the key |

Asserting `is_err()` conflates all three. Every test below asserts `error_type`.

### Pitfall 4 — keys that look relative and are not

`...`, `..x`, `a..b`, `.hidden`, `a.b` are all legal resource names, none is `.` or `..`, and none
is relative. A guard written as `name.starts_with('.')` or `name.contains("..")` would refuse
`.hidden` and `a..b` — breaking ordinary dotfiles and versioned filenames. `keyabs01` pins these as
**accepted**.

---

## Test Plan

### Unit tests — `Key`, `Error`, `CwdCursor` (`liquers-core/src/query.rs`, `src/error.rs`)

| ID | Test | Asserts |
|---|---|---|
| `keyabs01` | `is_relative` truth table | True: `..`, `.`, `../x`, `a/..`, `a/./b`, `a/../../etc`. **False: `...`, `..x`, `a..b`, `.hidden`, `a.b`, `a/b`, empty key.** Every segment inspected, not the first |
| `keyabs02` | `as_absolute` | `Ok` returns the same key for absolute; `Err` for each relative shape, `error_type == KeyNotAbsolute`, `key` field is `Some(encode())` |
| `keyabs03` | `try_into_absolute` | Consuming form: returns an equal key; same error on the relative shapes |
| `keyabs04` | `to_absolute` unchanged | Regression: existing resolution cases (`to_absolute1`) still produce the same keys — the new neighbours changed nothing |
| `keyabs05` | `CwdCursor::needs_cwd` | Rename preserves **first-segment-only** semantics: `../b` true, `a/../b` false. The two predicates mean different things and both are correct |
| `keyabs06` | `Error::key_not_absolute` | `error_type`, message contains the encoded key, `key: Some(...)`, `position` unknown |

### Unit tests — stores (`liquers-core/src/store.rs`, `liquers-store/src/opendal_store.rs`)

| ID | Test | Asserts |
|---|---|---|
| `keyabs07` | `AsyncMemoryStore` + `MemoryStore` refuse | Every fallible key-taking method returns `KeyNotAbsolute` for each relative shape. Uniformity: a map-backed store cannot traverse, but the rule must not vary by backend |
| `keyabs08` | `AsyncFileStore` refuses, and the file outside stays untouched | Example 1. Intermediate directory created; reads *and* writes; outside file byte-compared afterwards |
| `keyabs09` | `FileStore` (sync) refuses | Same shape as `keyabs08` |
| `keyabs10` | Routers | `StoreRouter`/`AsyncStoreRouter` return `KeyNotAbsolute`, **not** `key_not_supported(key, "store router")` — the key is malformed, not unrouted. `is_supported` false |
| `keyabs11` | `is_supported` on a **directly held** store | Never through a router (Example 2's trap). False for every relative shape, true for the equivalent absolute key |
| `keyabs16` | `AsyncOpenDALStore` (memory backend) | Same refusals; path builder is fallible |

### Integration tests (`liquers-core/tests/`)

| ID | Test | Asserts |
|---|---|---|
| `keyabs12` | `store_key_absolute.rs` — end-to-end | The issue's reproduction through a real `Environment` with an `AsyncFileStore`: evaluating `-R/../../etc/passwd` fails with `KeyNotAbsolute`, and a file planted outside the root is unread |
| `keyabs13` | Recipes | Phase 2 finding B2: a `recipes.yaml` with `cwd: ../../etc` is refused when its key reaches the store. Pins the second source of a relative key |
| `keyabs14` | **Regression surface** | Nothing that worked breaks: normal keys round-trip; `-R-key/.` link-argument CWD resolution still works; `recipe_cwd_resolution.rs` and `plan_cwd_freeze.rs` stay green; a `.hidden` file is readable |
| `keyabs15` | HTTP status (`liquers-axum`) | `error_to_status_code(ErrorType::KeyNotAbsolute) == 400`. Asserted at the mapping, not through a handler — `AXUM-HANDLER-TEST-COVERAGE` means no handler scaffolding exists, and building it is out of scope |

### Cross-language conformance — `STORE05` revised

| Site | Change |
|---|---|
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md:1767` | `../escape`, `a/../../etc`, `a/./b` now expect `KeyNotAbsolute`; a new empty-segment case keeps `KeyNotSupported`; add the "check on direct calls, not only routing" requirement and the intermediate-directory trap |
| `liquers-web/tests/store_pure_STORE.rs`, `store_local_STORE.rs`, `store_js_STORE.rs` | Expect `KeyNotAbsolute` for `.`/`..`; empty segment unchanged |
| `liquers-web/src/error.rs` | Name round-trip covers `key_not_absolute` |

### Corner cases considered

| Area | Finding |
|---|---|
| **Concurrency** | None. The check is a pure function of an immutable `&Key`, holds no lock, adds no shared state |
| **Memory** | `is_relative` allocates nothing; `as_absolute` returns a borrow. `try_into_absolute` moves. No clone added to any store path |
| **Serialization** | `ErrorType` derives `Serialize`/`Deserialize` and an `Error` can be stored in a `.__metadata__` record, so metadata written by a build knowing `KeyNotAbsolute` will not deserialize on one that does not. Accepted (no such metadata can predate the change) and covered by the web name-table round-trip |
| **Metadata/lock sibling paths** | A relative key must be refused *before* `key_to_path_metadata` or `key_to_lock_path` builds anything — the fallible path builders make that structural rather than ordering-dependent |
| **Empty key** | `Key::new()` is absolute (vacuously — no segment is `.` or `..`) and must stay routable: it is a store's own prefix, and `listdir` on it is the most ordinary call there is |

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

Phase 2 committed to *no new guide*: the rule's audience is backend authors, who read the trait, and
the `STORE05` conformance cell. That holds — but three fragments here are the rustdoc's source
material rather than throwaway illustration, and each has an executable counterpart to link instead
of duplicating code:

| Fragment | Destination | Answers | Links |
|---|---|---|---|
| Example 2's `impl` skeleton + the `is_supported`-only trap | `AsyncStore` / `Store` trait rustdoc | "How do I implement a store correctly?" | `keyabs11` |
| Pitfall 2's two-line contrast | `Key::to_absolute` and `Key::as_absolute` rustdoc, cross-linked both ways | "Which one do I want?" | `keyabs04` |
| Pitfall 1's intermediate-directory trap | `STORE05` in `LANGUAGE-INTEGRATION_GUIDE.md` | "Why did my guard test pass without a guard?" | `keyabs08` |

No new executable example needs creating in Phase 4 — the tests are the examples.

### Usage, Meaning, and Connections

- The rule matters because it is a *layer boundary*, not a blocklist: `.` and `..` are meaningful at
  plan level and meaningless below it. Stated that way, a backend author knows what to do with a
  shape the list does not mention.
- It connects to CWD resolution (`to_absolute`, `CwdCursor`, `Step::SetCwd`), to routing
  (`is_supported` and `find_store`), and to recipes (`Recipe.cwd`) — three places that produce or
  consume relative keys, none of which is a store.
- It is **not** authorization. `CORE-SESSION-AND-KEY-ACL` is the orthogonal per-key permission
  question; a well-formedness check that happens to block an attack must not be mistaken for one,
  and DOC-07 should say so.

### Repeatable Development Guidance

- Where a precondition goes when only *some* consumers call the gate: `is_supported` gates routing,
  so anything relied on for safety must also sit on the path every caller takes.
- How to test a guard so removing it fails: assert the error **type**, and make the environment such
  that the unguarded code would genuinely succeed (Pitfall 1).
- The escape-verification technique itself: reproduce the exact path construction (`path.push(key)`)
  in a standalone binary before designing around it.

### Corrections and Unexpected Learning

- **Phase 1's claim that all in-tree dot-segment keys are pre-store was wrong** — `Recipe.cwd` is an
  unvalidated deserialized string (Phase 2 finding B2). Corrected, and now `keyabs13`.
- **Deep traversal is conditional.** `a/../../x` escapes only when `a/` exists; otherwise the kernel
  refuses it with `ENOENT`. Neither the issue nor Phase 1/2 knew this, and it silently defeats the
  obvious test. This is the single most valuable thing Phase 3 produced and belongs in `STORE05`.
- **The issue's own request is discharged**: the `PathBuf::push` behaviour it declined to assert is
  now demonstrated, for reads and writes.
- Phase 1's `neither` decision on a guide still stands; the accumulated material is trait rustdoc
  and one conformance cell, not a narrative. Revisit only if DOC-07 stalls.

## Review Outcomes

Three review passes were run — Phase 1 conformity, Phase 2 conformity, and codebase/query
validation. This host does not launch parallel review agents, so they were performed sequentially
and recorded unchanged, per the skill's host-compatibility rule.

**Reviewer 1 — Phase 1 conformity.** No findings. Every Phase 1 interaction has a test: query system
(`keyabs12`, `keyabs14`), store system (`keyabs07`–`keyabs11`, `keyabs16`), error system
(`keyabs06`, `keyabs15`), web/API (`keyabs15`), and the "no language change" claim
(`keyabs04`, `keyabs14`).

**Reviewer 2 — Phase 2 conformity.** One finding, fixed.

| # | Finding | Resolution |
|---|---|---|
| 2.1 | Phase 2 asserts `is_supported` is not the enforcement point, but the draft test plan exercised stores only through a router — the exact configuration in which the wrong implementation passes. | `keyabs11` respecified to assert against a **directly held** store, and Example 2 now shows the passing-but-exploitable implementation explicitly. |

**Reviewer 3 — codebase and query validation.** Two findings, both fixed.

| # | Finding | Resolution |
|---|---|---|
| 3.1 | The deep-traversal case does not escape unless the intermediate directory exists, so the obvious test passes against unfixed code via `ENOENT`. | Verified by execution (table above); `keyabs08` now creates the directory and asserts `error_type`, and the trap is written up as Pitfall 1 and added to `STORE05`. |
| 3.2 | Draft `keyabs01` listed only relative shapes, so a `starts_with('.')`-style guard would have passed it while breaking `.hidden` and `a..b`. | `keyabs01` extended with the accepted look-alikes as explicit negatives. |

**Queries used** (`-R/../../etc/passwd`, `-R/a/../../etc/passwd`) were validated with
`liquers-validate` in Phase 1: both parse and plan clean as `GetAsset`, which is the premise of
`keyabs12`. No spaces, newlines or special characters; the environments in `keyabs12`/`keyabs13`
define a store, as `-R/` requires; no new command is used, so no registry change is needed.
