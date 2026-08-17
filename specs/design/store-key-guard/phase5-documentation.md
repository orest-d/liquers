# Phase 5: Documentation — Absolute Store Keys

## Completion Preconditions

- [x] Implementation finished and validated — `liquers-core` 568 lib + 3 new integration tests,
      `liquers-store` 27, `liquers-axum`, `liquers-lib` full loop, `liquers-web` wasm suites, all green
- [x] All user comments answered or incorporated (`KeyNotAbsolute` confirmed; `AbsoluteKey` filed;
      the P0/P1 contradiction explained and resolved)
- [x] All review comments from Phases 2–4 answered or incorporated
- [x] Documentation checked against implemented behaviour, not against the plan
- [x] Documentation lands in the implementation PR

## Implementation Summary

A store now requires an **absolute key**: no element may be `.` or `..`. `Key::is_relative`,
`Key::as_absolute` and `Key::try_into_absolute` express the rule, `ErrorType::KeyNotAbsolute` names
the violation, and every store applies the check before using a key. The file stores and the OpenDAL
store get it structurally — their path builders return `Result`, so the backend cannot be reached
without the key having passed. The routers check ahead of `find_store`, so a relative key reports a
malformed address rather than "no store matched". `liquers-web`'s private guard now delegates.

This conforms to the approved design. Three deviations, all smaller than planned:

1. **The file stores needed no per-method guard.** Every fallible key-taking method reaches a path
   builder, directly or through `acquire_lock`/`write_metadata_file`, so the choke point covers
   them. Phase 2 called for both; adding redundant checks would have been noise. Verified by
   auditing each method rather than assuming.
2. **`store.rs:1019` needed no restructuring.** Phase 4 predicted that `?` inside a `format!`
   argument would not work. It does — `?` is an expression, and the enclosing method returns
   `Result`.
3. **`DOC_02_QUERY_LANGUAGE_REFERENCE.md` was updated after all.** Phase 2 discarded it on the
   grounds that the language is unchanged. That still holds, but the new methods live on `Key`,
   which is DOC-02's own subject, so its concept table was inaccurate as written.

Nothing was omitted. Counts matched Phase 2's corrected estimate: 28 call sites in `liquers-core`,
14 in `liquers-store`.

## Documentation Delivered

### New Reference Documents

**None**, as decided in Phases 1–2. `specs/reference/api/DOC_07_STORES_PERSISTENCE.md` is the
reference that will own this rule and does not exist; writing a one-rule reference now would
pre-empt it and create a second place to keep current. The requirement is recorded in
`API_DOCS_GAP_ANALYSIS.md` §7 and its progress tracker. The rule's home is rustdoc: the
`liquers_core::store` module documentation, both trait docs, `is_supported` on each, the three
`Key` methods, `Key::to_absolute` (reverse cross-reference), and `ErrorType::KeyNotAbsolute`.

### New Guide Documents

**None.** The accumulated material is trait rustdoc and one conformance cell, not a narrative. The
Phase 1 `neither` decision was reconsidered here and stands.

### Existing Documents Reviewed or Updated

Authoritative `affects_docs`:

| Path | Result |
|---|---|
| `specs/reference/api/API_DOCS_GAP_ANALYSIS.md` | §7 + tracker record the absolute-key rule as required DOC-07 content (committed early, `30f2f3e`) |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | `STORE05` rewritten: `KeyNotAbsolute`, direct calls not routing, error-type assertions, dotted-name negatives; new `STORE05b` for the ENOENT trap |
| `specs/reference/PROJECT_OVERVIEW.md` | §5 Storage **corrected** — it claimed "safe encoding prevents arbitrary file access", which was untrue |
| `specs/reference/WEB_API_SPECIFICATION.md` | Error table gains `KeyNotAbsolute` (400) and `KeyNotSupported` (404) |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | Added late (see above): `Key`'s store-facing notion of relative and how it diverges from the cursor's |
| `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` | Closed with a resolution note; stale P1 paragraph deleted |

**Discarded candidates**, reviewed and found unaffected: `DOC_01_ARCHITECTURE_REFERENCE.md` (lists
"store semantics" only as a scope boundary); `QUERY_ESCAPING_GUIDE.md` (escaping, not key shape);
`STORE_CONFIG_FSD.md` (configuration only); `specs/design/liquers-web-store/` (design history, not
current state — its now-stale "until that lands" note is corrected in the live `key_guard.rs`).

### Links and Capability Map

`specs/README.md` § Stores gains **Absolute store keys — built**, pointing at the rustdoc and the
two summaries rather than at this folder, plus **Type-enforced key absoluteness — planned**, and a
paragraph stating the rule and that enforcement is per-method convention.

## Issues Filed

| ID | Why |
|---|---|
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | P3/L. The `AbsoluteKey` newtype, rejected here on cost. Enforcement is ~60 hand-written checks that a signature does not require. |
| `QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS` | P3/S. `Query::absolute` means "had a leading `/`", colliding with `to_absolute` (resolves) and `as_absolute` (asserts). Phase 2 deferred the rename. |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | P2/L. Found in passing: ~100 `unwrap`/`expect` in library code across four crates, against a `CLAUDE.md` hard rule, with nothing enforcing it. |

## Important Learning

**A guard test can pass for the wrong reason, twice over.** Both discoveries came from *running*
things rather than reasoning about them:

- The OS resolves `..` by walking **real** directories, so `a/../../SECRET.txt` against a store with
  no directory `a` fails with `ENOENT`. An unguarded store therefore raises an error, and a test
  asserting only "an error occurred" goes green with no guard present. Fix: create the intermediate
  directory *and* assert the error type. Now in `STORE05b`.
- The Phase 4 mutation check first failed on an implementation-detail assertion, not on behaviour:
  `AsyncFileStore::get` reads the bytes and only then touches metadata, so with the guard removed it
  still returned `KeyNotAbsolute` — raised by the metadata write, **after** the secret file had been
  read. `keyabs08` now asserts `get_bytes` directly and compares the returned bytes.

**The vulnerability was differently shaped than reported.** Under `evaluate`, a *leading* `..` never
reached the store: with no CWD in scope the cursor defaults to the logical root and resolves it
there. Only an *interior* `..` survived, because the cursor's test is "does this key start with `.`
or `..`". The HTTP store API resolves nothing, so both forms reached the store there. This is the
sharpest argument for the rule living at the store rather than in query resolution, and it is
recorded in `keyabs12` and DOC-02.

**A precondition belongs on the path every caller takes.** `is_supported` gates *routing*; only the
routers consult it. A backend guarded there alone passes a routed test and is open when held
directly — which is how an `Environment` is usually configured. `keyabs11` therefore tests directly.

**Two predicates, deliberately.** `Key::is_relative` (any element — is this a store address?) and
`CwdCursor::needs_cwd` (first element — must a CWD be consumed?) are both correct and diverge on
`a/../b`. Renaming the second was necessary; widening it would have broken CWD resolution.

## Conformance and Remaining Work

| Scope | Status |
|---|---|
| Requested: close `STORE-FILESTORE-PATH-TRAVERSAL` | Done, with the escape demonstrated as the issue asked |
| User-set framing: a precondition, with a dedicated error | Done — `KeyNotAbsolute`, confirmed by the user |
| User request: `is_relative` on `Key`, plus a consuming convenience | Done — `is_relative`, `as_absolute` (borrowing, used by stores), `try_into_absolute` |
| User request: document in API docs, note the DOC-07 gap | Done |
| User request: file `AbsoluteKey` as a refactoring issue | Done |
| Approved but reduced | Redundant per-method guards in the file stores; `store.rs:1019` restructuring — both unnecessary, see above |
| Deferred | Three filed issues above. Nothing is partially implemented. |

One item remains open for the user and blocks nothing: confirming that `pl`, `img` and `lui`/`egui`
are the right command namespaces to have checked for programmatic store-key construction. None
builds store keys.

## Validation

```
cargo test -p liquers-core --lib                568 passed
cargo test -p liquers-core --doc                 13 passed (2 new doctests)
cargo test -p liquers-core --tests              all suites green, incl. store_key_absolute (3)
cargo test -p liquers-store                      27 passed
cargo test -p liquers-axum --lib -- keyabs        1 passed
cargo test -p liquers-lib --lib --tests         all suites green
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
                                                 all suites green
cargo check --workspace                          clean
python3 scripts/docs_index.py --check            0 errors
```

Mutation check (Phase 4): removing the guard from `AsyncFileStore::key_to_path` fails `keyabs08`
with `READ THE FILE OUTSIDE THE ROOT`; restored.
