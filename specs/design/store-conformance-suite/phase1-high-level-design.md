# Phase 1: High-Level Design - store-conformance-suite

## Feature Name

`AsyncStore` conformance: an implemented suite, a completed contract, and a store implementation
guide

## Purpose

`AsyncStore` has seven in-tree implementations across three crates, each tested only against
itself; eleven ways they disagree were found one at a time by people tripping over them, one of
them a P0 that destroyed data. This project gives the trait an **executable** contract — a shared,
parameterized suite that any store implementation can be run against, in-tree or out — completes
`specs/reference/STORE_SEMANTICS.md`, which today records three questions as unsettled, and adds
`specs/guides/STORE_IMPLEMENTATION_GUIDE.md`, the operational counterpart that tells someone
writing the eighth store what decisions they have to make and how to check that they made them
consistently.

## Core Interactions

### Query System
None directly. The suite builds its keys with `parse_key` and never evaluates a query.

### Store System
The entire feature. In scope: `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter`
(`liquers-core`), `AsyncOpenDALStore` (`liquers-store`), `FetchStore`, `LocalStorageStore`,
`JsStore` (`liquers-web`), and the `AsyncStore` trait defaults themselves — **all seven are run
against the suite**, which is what "validate the existing implementations against the contract"
means concretely. The rules to be checked are the sections of `STORE_SEMANTICS.md`: the sibling
rule, directory derivation, derived versus explicit directories, absence versus failure, removal,
prefixes and routing, key shape, metadata sidecars, and `keys()`.

### Command System
None. No new commands, no namespace.

### Asset System
Indirectly: `get_asset_info` and `listdir_asset_info` are built on `get_metadata`, so directory
metadata is contract surface the suite must check.

### Web/API (if applicable)
No endpoint changes. `liquers-web`'s three stores must run the suite under `wasm32`, which is what
forbids a `#[tokio::test]`-shaped suite and makes this an `L`. `liquers-axum`'s store handlers are
the exposure that made the sibling rule a P0, but they are unchanged.

### Value Types
None.

### UI (if applicable)
None.

## Crate Placement

**`liquers-core`** — a public module (`store_conformance`) behind a non-default feature, beside the
trait it specifies. It must be *shipped*, not test-only: `liquers-store` and `liquers-web` consume
it as a dependency, and so does an out-of-tree store written through a language integration. It
must be runtime-agnostic — plain `async fn`s taking a store, no `tokio`, no test attributes — so
each crate drives it with its own harness (`#[tokio::test]` natively, `#[wasm_bindgen_test]` on
wasm).

Not `liquers-lib`: it sits above `liquers-store`, so a suite there could not reach either.

## Documentation Intent

**Reference:** **Extend** `specs/reference/STORE_SEMANTICS.md` — the normative *what*. It already
answers most of the contract; this project resolves its three ⚠ rows (the trait-default
`removedir`, `AsyncMemoryStore::is_supported`, and what `keys()` returns), and replaces each
section's *Enforced by* line with the suite's rule IDs. A second reference would split one contract
across two files.

**Guide:** **Create** `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the operational *how*, playing
the role for a store that `LANGUAGE-INTEGRATION_GUIDE.md` plays for a language integration, and
following its structure: independently selectable capabilities with stable IDs, requirement levels
(Essential / Profile / Optional), implementation states (`NA`, `NS`, `PARTIAL`, `COMPLETE`,
`BLOCKED`, `CONFORMANT`), a per-store status matrix, per-capability design questions, and the rule
that `NA` must be argued and must carry a reversing condition.

**One deliberate divergence from that guide, and it is the point:** where
`LANGUAGE-INTEGRATION_GUIDE.md` fixes its contract in Appendix A as Python pseudocode — because no
one Rust suite could run in every integrated language — every store here is a Rust `AsyncStore`, so
the suite is **implemented once and applied to any implementation**. The guide names rule IDs that
resolve to real functions in `liquers_core::store_conformance`; there is no appendix of pseudocode
to drift from the code.

### What the guide must answer

The questions a store author has to settle before writing anything, which the guide poses per
capability and the suite then checks:

- **What plays the role of an internal key?** A path, an object name, a row key, a URL, a
  `localStorage` string key.
- **How do Liquers keys map onto internal keys?** Does the mapping round-trip? Which Liquers keys
  are *unrepresentable*, and are they refused (`is_supported`, path builders) rather than silently
  colliding — the `.__metadata__` collision is the existing instance.
- **Does the backend address by string prefix?** If so, the sibling rule (`data` versus `database`)
  is the first thing to get right; it is the class of bug that destroyed data here.
- **Can the backend store metadata as well as data, or is a fallback needed?** A sidecar, a native
  metadata facility, or derivation on the fly — and **which source wins when they disagree**.
- **Is the store read-only or writable?** How does it refuse writes, and with which error?
- **Are all keys supported, or only a subset?** A view of a database table with predefined
  projections supports a fixed key set; `is_supported` is where that is expressed, and it is what
  makes layering work.
- **Does the backend meaningfully support directories, and can children be found?** Which of the
  three sources of directory truth does it offer — `stat`, a bounded listing, or neither (then
  `DirectoryIndex`)? Can an explicitly created empty directory be distinguished from a derived one?
- **Is the backend authoritative and writable by others?** If yes, no write-side index may be kept,
  because it goes stale.
- **What does absence look like, and can it be told apart from a failure?** An S3 403 reported as
  `Ok(false)` is a lie about existence.
- **What is the prefix, and is it part of the backend path or stripped?** Every store but
  `FetchStore` keeps it; the router selects on `key_prefix()` alone.
- **What are the atomicity, concurrency and limit behaviours?** Are data and metadata written
  together? What happens on a concurrent `set`? Is a partial write possible at a quota boundary?
- **What does enumeration cost?** Is `keys()` a full backend scan, and is it acceptable at all?
- **How do backend errors map onto `ErrorType`?**
- **What does implementing a store actually mean here?** The guide must say plainly what work is
  in front of the author, because it is not one shape: is a new struct implementing the trait
  enough; does an existing factory need extending; is a whole new `StoreFactory` required; and if
  so, must it be chained into a `default_store_factory()` so a configuration document can name the
  type at all? A store nobody can construct from configuration is only half delivered.
  `guides/STORE_FACTORY_GUIDE.md` owns the factory procedure and is linked, not repeated — what
  the new guide adds is *which* of these paths applies and how to tell.

**Keeping the contract and the guide synchronized** is a mechanism, not an intention: the suite's
rule IDs are the shared spine, every contract section and every guide capability cites them, and a
test asserts that the IDs registered in code, cited in `STORE_SEMANTICS.md`, and cited in the guide
are the same set. The precedent is `registry_export`, which holds `specs/command_registry.yaml` to
the registered commands.

**Other documents to create:** None.

**Specific documents to update:**
- `specs/README.md` — link the new guide in the capability map.
- `CLAUDE.md` §"Adding a Store Backend" — replace the five steps with a pointer to the guide plus
  running the suite.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE — its direction-2 questions ("which host
  storage, is it read-only", "where does metadata come from", "directory semantics on a backend
  that has none") are the new guide's subject; cross-link rather than answer twice.
- `specs/guides/STORE_FACTORY_GUIDE.md`, `specs/reference/STORE_CONFIG_FSD.md` — cross-links, so
  the three store guides form one path: implement (new) → declare a type → configure.
- `specs/index.csv`; and the issue files closed or depended on below.

**Audience:** whoever writes any further `AsyncStore`, in this repository or against it — not just
the next one. They should be able to read the guide, answer its questions, run the suite, and find
out where they disagree with the contract, without ever opening this design folder.

## Decisions Taken

Settled at the Phase 1 gate, and now normative for Phase 2:

1. **`keys()` returns data keys plus directories plus the prefix**, and **every key returned must
   start with the store's prefix**. This is the contract for row 10 and closes
   `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS`; `AsyncMemoryStore`, which returns data keys only,
   is the implementation that must change.
2. **`removedir` is specified as a postcondition, not as a return convention.** If it returns
   `Ok(())`, the directory does not exist afterwards; failing to remove it is an error. Two things
   follow rather than being stipulated separately: **recursion** (a directory derived from its
   children still exists while a child remains, so a non-recursive removal that reported success
   would break the postcondition), and the disposition of row 6 — on an absent directory the
   postcondition already holds, so `Ok(())` is right, while the trait default's
   `Err(KeyNotSupported)` remains legitimate for a store that declares no directory removal at all.
   It is a claim of success that is forbidden without the effect.
3. **One vocabulary across the guide, the contract and the code.** A store declaring a capability
   in Rust makes the same claim as its row in the guide's status matrix. The guide's terminology
   and status vocabulary are shared with `LANGUAGE-INTEGRATION_GUIDE.md`: Phase 2 chooses between
   duplicating that section and extracting it into a reference both guides link. Extracting is
   preferable — two copies of a normative vocabulary drift exactly as two copies of a format do —
   but duplication is acceptable if extraction turns out to disturb the other guide.
4. **The synchronous `Store` trait is out of scope, and obsolete.** No synchronous rules are
   implemented and no synchronous tests are written. Filed as
   [`CORE-SYNC-STORE-TRAIT-OBSOLETE`](../../issues/CORE-SYNC-STORE-TRAIT-OBSOLETE.md) (P2, M) —
   nothing can hold one: `Environment` exposes only `get_async_store`, and `AsyncStoreWrapper`, the
   adapter that used to bridge them, has already been deleted. A synchronous store may return one
   day to let a *realm* evaluate queries synchronously, so `STORE_SEMANTICS.md` states its rules in
   trait-neutral terms wherever they are the same for both, and records that only the asynchronous
   case must be satisfied today. A future synchronous store then inherits the contract instead of
   re-deriving it.
   Filed alongside it:
   [`DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS`](../../issues/DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS.md)
   (P2, S) — `CLAUDE.md`, `UNITTEST_GUIDE.md` and `STORE_CONFIG_FSD.md` still teach that deleted
   type, and the guide's setup snippet does not compile.
5. **A store failing its own suite is fixed inside this project**, unless the fix is large enough to
   deserve its own issue at complexity `M` or greater — then it is filed, the rule stays in the
   suite as a named expected failure citing that issue, and the store's row reads `BLOCKED` rather
   than being quietly excused. Expected: `AsyncMemoryStore`'s `keys()` (decision 1) and its
   `is_supported` (which already has `async-memory-store-prefix-support`) are `S`-sized and fixed
   here.

## Open Questions

1. **How does one rule map to one reported failure**, under two harnesses that disagree about what
   a test is? Discussed below; a recommendation, not yet a decision.
2. **How does a rule get a fresh, empty store?** A factory closure the harness supplies and calls
   per rule, or one store handed in and cleaned between rules? `LocalStorageStore` persists across
   tests within a browser session and `AsyncFileStore` needs a temporary directory, so this is not
   cosmetic. Related but separate: some stores (`FetchStore`, `JsStore`) cannot be *populated*
   through `AsyncStore` at all, so the fixture, not the rule, must place the preconditions.
3. **How tightly is the capability vocabulary coupled to Rust?** Decision 3 fixes that guide and
   code share one vocabulary; it does not fix whether that is a `bitflags`-style struct, a set of
   marker methods on a trait, or an associated const — and the answer decides how a store *outside*
   this repository declares its capabilities.

### On question 1 — the recommendation

Three shapes, of which the third is a small superset of the other two:

- **A report.** One test per store calls `run_all(fixture)` and asserts the report is clean. Rules
  return outcomes rather than panicking, so **every** rule runs and one execution shows every
  divergence at once — which is exactly what a project chartered to enumerate divergences wants,
  and what feeds the guide's per-store status matrix. But one giant test cannot be run rule by rule
  and names the failing rule only inside a message.
- **Generated tests.** A macro expands to one test function per rule. Each is individually
  runnable, named in `cargo test` output, and fails independently. But the macro must satisfy two
  attributes (`#[tokio::test]` natively, `#[wasm_bindgen_test]` on wasm), and there is no whole-store
  view.
- **Both, which costs almost nothing extra.** Make each rule an `async fn(&Fixture) -> RuleOutcome`
  that never panics. Then `run_all` is a fold over the rule set, and the macro is a thin generator
  emitting one test per rule that calls that one rule and asserts its outcome. The harness
  attribute is a macro parameter rather than a `cfg` inside the macro, so `liquers-web` passes
  `#[wasm_bindgen_test]` and nothing in `liquers-core` needs to know wasm exists.

**Recommended: the third.** The non-panicking rule signature is what makes it cheap, and it is
required anyway — a rule that panics cannot report `BLOCKED` for a known divergence, and decision 5
depends on being able to.

## References

- `specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md` — the issue, with the
  eleven-row divergence table this project answers.
- `specs/reference/STORE_SEMANTICS.md` — the contract as far as it goes today.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — the model for the new guide, and §STORE is the
  half of it that moves.
- `specs/design/opendal-path-mapping/` — wrote the contract, and rejected building the suite inside
  a P0 fix.
- `liquers-core/tests/store_key_absolute.rs` and the `keyabs` family — the one existing
  cross-implementation suite, and the shape the issue asks this one to follow.
- `liquers-core/src/store_dir_index.rs` — `DirectoryIndex`, the shared directory source of truth.
- `liquers-lib/tests/registry_export.rs` — the precedent for holding a document to the code.
