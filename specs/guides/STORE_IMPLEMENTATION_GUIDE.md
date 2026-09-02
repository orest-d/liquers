---
title: Store Implementation Guide
kind: guide
audience: internal
area: [core/store, store/backends, web]
reviewed: 2026-09-02
---
# Store Implementation Guide

How to implement an `AsyncStore` that satisfies
[`reference/STORE_SEMANTICS.md`](../reference/STORE_SEMANTICS.md), and how to check that it does.

The contract says *what* a store must do. This says *how* to build one that does it, what decisions
you have to make before writing code, and how to run the shared conformance suite against your
store so the answer is a report rather than an opinion.

This guide plays the role for a **store** that
[`LANGUAGE-INTEGRATION_GUIDE.md`](LANGUAGE-INTEGRATION_GUIDE.md) plays for a **language
integration**, and shares its vocabulary through
[`reference/CONFORMANCE_TERMS.md`](../reference/CONFORMANCE_TERMS.md). One deliberate difference:
that guide fixes its contract as pseudocode in an appendix, because no single Rust suite could run
inside every integrated language. Every store here is a Rust `AsyncStore`, so **the suite is
implemented once and applied to any implementation** — there is no appendix to drift from the code.

## 1. What implementing a store actually means

Not one shape. Work out which of these you are doing before you start, because the last two are
where a store that "works" turns out to be unreachable:

| Question | If yes |
|---|---|
| A new struct implementing `AsyncStore`? | Always. Everything below follows from it. |
| Does an existing factory already build types like yours? | Add a `StoreTypeInfo` to it. |
| Do you need a bespoke `resolve` — inferring the type from a URI, say? | Implement `StoreFactory`. |
| Must a configuration document be able to name your type? | Chain your factory into the relevant `default_store_factory()`. **Order matters: first to resolve wins.** |

`guides/STORE_FACTORY_GUIDE.md` owns the factory procedure and
`reference/STORE_CONFIG_FSD.md` the configuration format; neither is repeated here. What this guide
adds is *which* of these paths applies to you.

**A store nobody can construct from configuration is half delivered.** The most common way to
finish a store and have nothing work is to implement the trait and stop.

## 2. The questions to answer first

Write the answers down. The conformance suite checks most of them, and the ones it cannot check are
the ones that bite later.

### The key space

- **What plays the role of an internal key?** A path, an object name, a row ID, a URL, a
  `localStorage` string.
- **How do Liquers keys map onto internal keys, and does the mapping round-trip?**
- **Which Liquers keys are *unrepresentable*?** Refuse them from `is_supported` and from the path
  builders rather than letting them collide silently. The in-tree instance is the `.__metadata__`
  suffix: the *data* path of key `foo.__metadata__` is byte-identical to the *metadata* path of key
  `foo`, so that key is refused.
- **Is the key space constrained in shape, not just extent?** A store addressing database rows by
  numeric ID cannot accept `sub/a.txt` and has no directories. That is a legitimate store; see §6.
- **Does the backend address by string prefix?** If so, read §3 before anything else.

### Data and metadata

- **Can the backend store metadata as well as data?** If not: a sidecar key, a native metadata
  facility, or derivation on the fly — and **which source wins when they disagree**. Liquers
  dispatches deserialization on the data format, which usually derives from the name, so a
  backend's own content-type guess is often the *worse* source.
- **Is the store read-only or writable?** A read-only store is legitimate and common. Say how it
  refuses writes and with which error.

### Directories

- **Does the backend meaningfully have directories, and can children be found?**
- **Which of the three sources of directory truth does it offer?** `stat` the path (a real
  filesystem), a bounded listing (an object store), or neither — in which case use
  [`DirectoryIndex`](../../liquers-core/src/store_dir_index.rs).
- **Do your directories *retire*?** A derived directory stops existing when its last child goes; a
  real filesystem directory does not. This is a capability, not a detail — see §4.
- **Is the backend authoritative and writable by others?** Then keep **no** write-side index: it
  goes stale, and rebuilding means listing everything.

### Errors, cost and concurrency

- **What does absence look like, and can you tell it from a failure?** An S3 403 reported as
  `Ok(false)` from `is_dir` is a lie about existence. Match the backend's not-found condition
  specifically. This is the single most commonly failed rule in tree.
- **How do backend errors map onto `ErrorType`?** Callers match on the type, never on the message.
- **What does enumeration cost?** Is `keys()` a full backend scan? Is that acceptable?
- **Atomicity, concurrency and limits.** Are data and metadata written together? What happens on a
  concurrent `set`? Can a write partially succeed at a quota boundary? (Refusing is fine; refusing
  *after* a partial write is not.)

### Prefix

- **What is the prefix, and is it part of the backend path or stripped?** Every in-tree store keeps
  it except `FetchStore`, which documents that it is the exception.
- `AsyncStoreRouter` selects on `key_prefix()` **alone** for `is_dir` and `listdir`, so a store that
  under-reports its prefix answers for keys belonging to stores listed after it.

## 3. The sibling rule, first

> **No operation on a key may read, list, or delete anything under a different key.**

A key whose name is a *prefix* of another key's name is a different key: `data` and `database` are
unrelated, as are `sub` and `subway`.

This is the rule most easily broken by a store whose backend addresses by string prefix, and
breaking it destroys data: `removedir("data")` deleted `database/`, reachable through
`DELETE /api/store/removedir/{*key}`.

**Have one place that produces your directory form, and make every call site that names a directory
use it.** Spreading the rule across call sites is what let two of three instances survive.

In JavaScript, in SQL, in an object-store prefix — the same trap:

```js
// WRONG: deletes `subway/` too.
for (const k of keys) if (k.startsWith(key)) remove(k);
// RIGHT: the separator is what makes it a directory.
const p = key === "" ? "" : key + "/";
for (const k of keys) if (k.startsWith(p)) remove(k);
```

## 4. Declaring what your store can do

`StoreCapabilities` is how the suite knows which rules apply to you. It has **no `Default`** on
purpose: you must name every field, so when a capability is added your fixture stops compiling
instead of silently answering `false` and skipping the new rules while still reporting green.

| Capability | Means |
|---|---|
| `write` | `set` and `set_metadata` are accepted |
| `remove` | a single key can be deleted |
| `directories` | `is_dir` and `listdir` answer meaningfully |
| `derived_directories` | a directory retires when its last child goes |
| `explicit_directories` | `makedir` creates a childless directory that persists |
| `remove_directories` | `removedir` removes a directory and its subtree |
| `stored_metadata` | metadata written with `set_metadata` reads back |
| `enumerate_keys` | `keys()` enumerates the store |

**`directories` and `derived_directories` are different questions,** and conflating them is the
easiest mistake. `AsyncFileStore` has directories that do *not* retire — a real directory outlives
its last file — so it declares `directories: true, derived_directories: false`. `AsyncOpenDALStore`
declares `derived_directories` differently *per service*: true on the memory service, false on the
filesystem one.

**Declaring a capability `false` is a claim, not a way to skip a rule — and it is checked.** For
each capability there is a *refuting* rule that runs only when you declare it absent, and asserts
your store really does refuse: `nowrite01`, `noremove01`, `nodir01`, `nomakedir01`,
`noremovedir01`, `nokeys01`. Without them, a fully writable store could declare everything `false`,
skip every write, removal and enumeration check, and still report conformant — and the store least
likely to be *given* a capability is the one whose implementation of it is broken, which is exactly
how a `makedir` that records nothing would escape `explicit01`.

Declare what your store is meant to do, then let the report tell you whether it does.

## 5. Testing your store

### Write a fixture

The suite never constructs stores: there is no universal way to make an empty one — a filesystem
store needs a temporary directory, an HTTP-backed store needs something serving it. You supply a
`Fixture`, which answers four questions: what the store can do, how much the test may do to it,
what its configured prefix is, and what keys satisfy a given precondition.

For most stores, `GenericFixture` is enough:

```rust
let fixture = GenericFixture::new(
    "MyStore(temp dir)",
    Box::new(store),
    prefix,
    capabilities,
    SafetyLevel::Scratch,
)
.with_outside_prefix(parse_key("elsewhere/x.txt")?)   // so prefix02 can run
.with_unsupported_shape(parse_key("collide.__metadata__")?);

let report = run_all(&fixture).await;
eprintln!("{report}");            // print before asserting: a bare rule id helps nobody
report.assert_conformant(&[])?;
```

Anything you do not supply is **declined with a reason that reaches the report** — which is how a
store that genuinely cannot offer a precondition differs from one that quietly skips a check.

### Preconditions, not invented keys

A rule never invents a key name; it asks for one. That is what lets a general suite reach a
specialized store. The vocabulary is `KeyRequest`: `Fresh`, `FreshSiblings`, `FreshPrefixPair`,
`FreshNested`, `Existing`, `ExistingDirectory`, `OutsidePrefix`, `UnsupportedShape`, `Supported`,
`Relative`, `MetadataCollision`.

Implement `Fixture` directly when `GenericFixture` cannot express your key space — see §6.

### Safety levels

| Level | Permits | Refuses |
|---|---|---|
| `ReadOnly` | reads and listings | every mutation |
| `CreateOnly` | creating a key that does not exist | overwriting, removing, `removedir` |
| `Scratch` | anything, **to keys this run created** | anything that was already there |

Each rule declares the lowest level it can run at, and a rule needing more is reported as **not
run** — naming the level that would run it. *Not run is not passed*: `ReadOnly` reaches 10 of 32
rules and misses every divergence this suite was built for, so a clean `ReadOnly` report is weak
evidence, not conformance.

**Level 3 is rule discipline, not a guarantee.** Rules check before they mutate, but
check-then-write is not atomic, and a buggy rule can breach it. It is safe against a scratch store,
not against somebody's data.

### Safety precautions

- Test against a **temporary folder or a throwaway database**. Treat any store under test as
  expendable.
- **Do not run against a third-party service** unless that has been explicitly permitted, and never
  against one holding data you did not create.
- A `CreateOnly` run **leaves everything it created behind**, by definition — it may not remove
  anything. The report's residue list is what makes that visible; clearing it is your job.
- A store that persists between runs — browser `localStorage` is the in-tree case — needs
  `with_run_id`, or the second run meets the first run's leftovers.

### Allowed failures

When a rule fails for a reason you cannot fix now, list it with the issue that permits it:

```rust
report.assert_conformant(&[AllowedFailure {
    rule: "absence01",
    issue: "WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND",
}])?;
```

`assert_conformant` fails in **both** directions: a disallowed failure is an error, and **an allowed
rule that passed is also an error**, naming the entry to delete. A fixed issue forces its own
bookkeeping out rather than waiting for someone to remember. Such a store's row reads `BLOCKED`,
not `PARTIAL` — it is finished; something it depends on is not.

## 6. A store the suite mostly does not apply to

A view onto one database table: each "file" is a serialized row, the key is a numeric row ID, there
are no subdirectories, and IDs are assigned rather than chosen.

```rust
fn capabilities(&self) -> StoreCapabilities {
    StoreCapabilities {
        write: true, remove: true,
        directories: false, derived_directories: false,
        explicit_directories: false, remove_directories: false,
        stored_metadata: false,      // derived from the column types
        enumerate_keys: true,        // SELECT id FROM t
    }
}

async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
    match request {
        KeyRequest::Fresh => Ok(vec![self.prefix.join(self.next_id()?.to_string())]),
        KeyRequest::FreshPrefixPair => Err(Unavailable::new(
            "row IDs are numeric; no ID is a proper prefix of another")),
        KeyRequest::FreshNested { .. } => Err(Unavailable::new(
            "the key space is one level deep: a row ID")),
        // … every variant, exhaustively: the enum is deliberately not `non_exhaustive`, so a new
        // precondition breaks this match rather than silently declining a rule.
    }
}
```

It conforms to a **subset**, and that is the correct answer. Roughly half the rules skip: capability
gating does most of the work, and `KeyRequest` declines catch what capabilities cannot express — a
store that *has* directories but whose names can never form a prefix pair.

Unlike a language integration, **many `NA`s here are expected, not a smell**
(`CONFORMANCE_TERMS.md`). What keeps that from becoming an excuse is that each decline carries a
reason, in the report, where a reviewer sees it.

## 7. Running the suites

```bash
cargo test -p liquers-core  --features store-conformance      # C1–C5
cargo test -p liquers-store --features store-conformance      # C6–C7 (OpenDAL, two services)
cargo test -p liquers-web --target wasm32-unknown-unknown     # C8, C10, under Node
CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
  --target wasm32-unknown-unknown --features browser-tests    # C9 (localStorage)
```

Run the wasm loops after `cargo clean`, separately from the native one — they build a different
target, and a combined run exhausts a constrained session's disk allowance.

## 8. Where each rule comes from

Every rule ID cites the contract section it enforces, and every section of
[`STORE_SEMANTICS.md`](../reference/STORE_SEMANTICS.md) lists the rules enforcing it. A test asserts
the two sets agree, so a rule cannot be added without the contract naming it.

| § | Rules |
|---|---|
| §1 sibling rule | `sibling01` `sibling02` `sibling03` `sibling04` `sibling05` |
| §2 directories | `dir01` `dir02` `dir03` `dir04` `dir05` `dir06` `dir07` `dir08` `data01` `data03` |
| §3 derived vs explicit | `explicit01` `explicit02` `explicit03` |
| §4 absence | `absence01` `absence02` `absence03` |
| §5 removal | `remove01` `remove02` `remove03` `data02` |
| §6 prefixes | `prefix01` `prefix02` `prefix03` `prefix04` |
| §7 key shape | `keyshape01` `keyshape02` |
| §8 sidecars | `sidecar01` `sidecar02` `sidecar03` |
| §9 enumeration | `keys01` `keys02` |
| refuting rules | `nowrite01` `noremove01` `nodir01` `nomakedir01` `noremovedir01` `nokeys01` |

## 9. Status of the in-tree stores

As of 2026-09-02, from the suites above.

| Store | Rules run | Status | Notes |
|---|---|---|---|
| `AsyncMemoryStore` | 29 | `CONFORMANT` | `dir07` blocked pending the contract decision |
| `AsyncFileStore` | 28 | `CONFORMANT` | `derived_directories: false` — real directories persist |
| `AsyncStoreRouter` | 28 | `CONFORMANT` | needs each member's prefix to exist (`CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER`) |
| `AsyncOpenDALStore` (memory) | 31 | `CONFORMANT` | the widest coverage in tree |
| `AsyncOpenDALStore` (fs) | 31 | `CONFORMANT` | `derived_directories: false` |
| Trait defaults | 8 | `CONFORMANT` | no directory support, no enumeration |
| `NoAsyncStore` | 4 | `CONFORMANT` | accepts no key, and says so correctly |
| `FetchStore` | 6 | `CONFORMANT` | read-only; its configured key set is the subject source |
| `JsStore` | 28 | `BLOCKED` | `WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND`, `WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA` |
| `LocalStorageStore` | — | `NS` | behind `browser-tests`; needs a chromedriver to run |

This table is **generated from the reports**, not maintained by hand — `ConformanceReport` derives
serde for exactly this reason. Regenerate it rather than editing it.

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-02 | Created. The operational counterpart to `reference/STORE_SEMANTICS.md`: what implementing a store means, the questions to answer first, the sibling rule, the capability model, how to write a fixture and run the suite, the safety levels and their precautions, a worked restricted store, and the status of the ten in-tree implementations. | `design/store-conformance-suite/` Phase 4 step 14 |
