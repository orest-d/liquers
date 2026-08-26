---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE5
kind: design
title: "Phase 5: Documentation — foreign and Python value types in the type registry"
status: in_review
phase: documentation
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 5: Documentation — Foreign Value Type Registration

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR ([#42](https://github.com/orest-d/liquers/pull/42))

## Implementation Summary

A value whose type identifier is known only to an integration can now be registered and stored.
`FOREIGN-VALUE-TYPES-NOT-REGISTERED` was the reported problem: `TypeRegistry` is seeded from the
**static** `ValueInterface::type_descriptions()`, and `ExtValue::Foreign` supplies its identifier at
runtime, so the write path refused every JavaScript, Python or Starlark handle.

**What was built.** Five environments gained `new_with_type_registry`, with `new()` delegating, so an
integration extends the base registry and hands the finished one over; the registry is still written
only before construction and needs no lock. `ForeignValue` gained an instance `type_info()` with a
default derived from its existing methods, routed through `ValueExtension`, `ExtValue` and
`CombinedValue` so a foreign value describes itself rather than falling back to a generic derivation.
`liquers-web` registers `js.Value` inside `new_environment()` — the funnel both rebuild paths already
use, so nothing needs retaining and nothing can drift. `liquers-py` gained `type_descriptions()` and
one identifier per variant.

**Conformance.** The delivered behaviour matches the approved design, with three deliberate
departures, each recorded at the point it was made:

| Departure | Why |
|---|---|
| **The `error` type identifier was removed** | Directed by the user during implementation. It was contrary to the intended model: the type axis says what a value *is*, and a failure is a metadata property. This was not in any phase's scope |
| **`liquers-py` gained a `cargo` feature split** | The crate's tests could not link. Phase 4 asserted they could, on a measurement of an *empty* harness |
| **`liquers-py`'s `value.rs` was repaired and an `AssetInfo` variant added** | Anticipated in Phase 2 as four compile errors; the `Vec<AssetInfo>` trait signature forced the variant |

**Neither shape the issue proposed was taken.** A mutable registration point would have given up the
lock-free registry; a provider *family* (`js.*`) is incompatible with one identifier per variant.

## Documentation Delivered

### New Reference Documents

None. Phase 1's rationale held: this is one mechanism inside an existing model, and
`VALUE_TYPE_SYSTEM.md` is where it belongs.

### New Guide Documents

None. The two existing guides had the right homes for it.

### Existing Documents Reviewed or Updated

`affects_docs`: `VALUE-TYPE-SYSTEM`, `LANGUAGE-INTEGRATION_GUIDE`, `TYPE_SYSTEM_GUIDE`,
`ASSET_SET_OPERATION`, `ASSET_LIFECYCLE`, `ASSETS`.

The three asset documents were **discarded** as candidates in Phase 2 and are now kept. That
decision was made before the error type was removed, when the write path's behaviour genuinely was
unchanged. Removing the error type changed how a *failed asset* is typed, which is asset-lifecycle
behaviour, so the candidates had to be reconsidered — and one of them carried a claim that had
become false.

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | The one-identifier-per-variant rule stated where the cardinality was only implied; a new "Registering a type an integration owns" section; the removal of the `error` identifier and what the type axis reports for a failed asset; the identifier list extended |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | §VALUE's "**Registration is an open problem**" replaced by the procedure, with the four traps that cost time: extend don't replace, register where the rebuild path goes, declare no formats while `as_bytes` refuses, test the constant against the instance |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | §2 gains the cardinality rule and the absence of an error identifier; §4 records where step 4 moves for an integration-owned type |
| `CLAUDE.md` | "Adding a Value Type" notes the cardinality rule and the integration-owned case |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | A dedicated "How a failure is typed" section: an error is a metadata property, the value is none, the identifier reports what is available rather than what was intended, and why a failed asset is storable at all |
| `specs/reference/ASSET_SET_OPERATION.md` | **Corrected a false claim** — the format-check exemption said an error state's format "contradicts its `error` identifier" |
| `specs/reference/ASSET_LIFECYCLE.md` | The failure routines re-type the metadata as the none type after clearing the value; a new subsection on how a failed asset is typed |
| `specs/reference/ASSETS.md` | The typing fact beside the existing `Error`/`Cancelled` status explanation |

Discarded candidates, per §9: `WEB_API_SPECIFICATION.md` (no endpoint change), `PROJECT_OVERVIEW.md`
(no core concept changed; Query/Key encoding untouched), `DEPENDENCIES_STATUS.md` (mentions error
statuses but makes no claim about typing).

### Links and Capability Map

`specs/README.md` regenerated: the design moves to `complete`, and the capability is reachable
through `VALUE_TYPE_SYSTEM.md` and `LANGUAGE-INTEGRATION_GUIDE.md`. The guide's pointer to the open
issue is replaced by a pointer to the reference section and to the executable example.

## Issues Filed

`PY-VALUE-SERIALIZER-IS-A-STUB` (P2) — filed from a Codex review finding on PR #42: `liquers-py`'s
`as_bytes` writes `txt`/`html` for eight of sixteen variants and `deserialize_from_bytes` reads
nothing at all, so no Python value round-trips through a store. The registry declarations were
corrected in this PR to describe only what the codec accepts; the codec gap is that issue.

`ERROR-STATE-FROM-ERROR-NOT-STORABLE` was filed during step 6 and then **deleted**: it described a symptom of the error type existing, and removing that type dissolved it.
Three issues closed: `FOREIGN-VALUE-TYPES-NOT-REGISTERED`, `PY-VALUE-TYPE-DESCRIPTIONS-MISSING`,
`WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH`. `PY-MODULES-NOT-DECLARED-IN-LIB` stays open, annotated
with the two modules now declared and the testability change.

## Important Learning

**Verify a wasm-only claim with a native mock.** The issue had sat with a "not verified against a
build" caveat because `liquers-web` is `wasm32`-only. A mock `ForeignValue` reproduced it natively in
minutes. The same mock is now the regression suite, and it is what the guide links as its executable
example — a reader can run it without a toolchain.

**An empty harness is not evidence that tests work.** `cargo test -p liquers-py` reported "0 tests"
and linked fine, and Phase 4 recorded that as proof ordinary tests were possible. With real test code
the linker failed on `_Py_Dealloc`: pyo3's `extension-module` omits the Python symbols, which is right
for the wheel and fatal for a test binary. A measurement of nothing measures nothing.

**A defect can be a symptom of a model error.** `State::from_error` could not be stored, because
`Metadata::with_error` set `type_identifier` to `"error"` while nothing set `type_name`, and the guard
in `sync_metadata_with_value` that protected the identifier dropped the name. The one-line fix — supply
the missing name — would have entrenched the error type. Removing the type instead made the whole class
of problem disappear: an errored state holds `V::none()`, is typed accordingly by the ordinary path, and
stores as metadata with no bytes.

**Where the type axis stands.** The identifier reports **what is available, not what was intended**. A
failed `report.csv` is typed `None`; the intent survives in the query, key and filename. This is now
stated in the reference, because it is the kind of thing a reader will otherwise infer wrongly.

**Realms are not obstructed.** `TypeKey` already carries a realm and `TypeInfo` is serializable, so a
registry assembled from descriptions received over the wire is just another registry — which is what
the constructor accepts. `TYPE-REGISTRY-NOT-REALM-AWARE` gained the direction that both sides should
hold a registry complete for all realms, and that an untransferable value (a JavaScript closure) needs
a third case beside convert and degrade.

## Conformance and Remaining Work

| Scope | Status |
|---|---|
| Requested: fix `FOREIGN-VALUE-TYPES-NOT-REGISTERED` | Delivered and tested |
| Added by the user: `PY-VALUE-TYPE-DESCRIPTIONS-MISSING` | Delivered and tested |
| Added by the user: the `bytes`/`Bytes` assertion | Delivered — three real failures, not one |
| Added by the user: remove the `error` type identifier | Delivered and tested |
| Considered, not done: a diagnostic "list known types" command | Declined by the user; remains a candidate on `value-type-system`'s own open question |
| Considered, not done: the rest of `PY-MODULES-NOT-DECLARED-IN-LIB` | Out of scope; issue stays open |

Nothing is deferred without an issue, and no partial-design status is required.

## Validation

| Check | Result |
|---|---|
| `cargo test -p liquers-core` | green (613 unit + all integration suites) |
| `cargo test -p liquers-lib --lib --tests` | green (302 unit + 15 suites) |
| `cargo test -p liquers-py --lib --no-default-features --features async_store` | green (5) |
| `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` | green (16 targets, 141 tests) |
| `bash scripts/check-build-matrix.sh` | 11/11 |
| `cargo test -p liquers-lib --test registry_export` | green; `specs/command_registry.yaml` unchanged |
| `python3 scripts/docs_index.py --check` | 160 documents, **0 errors** |
| CI on PR [#42](https://github.com/orest-d/liquers/pull/42) | both checks green, `mergeable_state: clean` |
