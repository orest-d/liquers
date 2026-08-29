---
title: "Phase 5: Documentation — Store configuration and factories in liquers-core"
kind: design
audience: internal
area: [core/store, store/config, store/backends, web, docs]
---
# Phase 5: Documentation — Store Configuration and Factories in `liquers-core`

## Completion Preconditions

| Criterion | Status |
|---|---|
| Steps 1–10 complete, or deferred with a reason | Steps 1–8, 10 done. **9 and 11 deferred** on `STORE-OPENDAL-SERVICES-NOT-ENABLED` |
| Every testing-plan command run and green | Yes, including the `--no-default-features` run and the browser suite |
| The three rewritten tests reviewed as deliberate changes | Yes, each with a doc comment saying what it asserts now and why |
| `Error::parse_error` answered | Yes — added; no new `ErrorType` needed |
| Every "prose that is now false" site fixed | Yes — three code sites, five documents |
| No issue found during implementation left unrecorded | Four filed |

## Implementation Summary

`liquers-core` gained two modules. `store_config.rs` holds `StoreRouterConfig`, `StoreConfig` and
`expand_env_vars`, moved from `liquers-store` — git recorded it as a rename, and **all 11 moved
tests passed with their assertions unchanged**, which was the stated test of whether the move
preserved behaviour. `store_factory.rs` is new: the `StoreFactory` trait, `ChainedStoreFactory`,
`StoreTypeMap`, `StoreTypeInfo` with arguments/availability/coverage, `core_store_factory`,
`StoreRouterBuilder` and the unclaimed-type error.

`liquers-store` lost `config.rs` and `store_builder.rs` — deleted, not shimmed — and gained
`store_factory.rs` with `OpendalStoreFactory`, the OpenDAL type table and a core-then-OpenDAL
`default_store_factory()`. `liquers-web` deleted its `liquers-store` dependency and now describes
its three store types with their arguments, which were previously documented only in a module
doc comment.

| Suite | Result |
|---|---|
| `liquers-core --lib` | 663 pass |
| `liquers-core --test store_router_STORE` | 5 pass |
| `liquers-store` / `--no-default-features` | 19 / 13 pass |
| `liquers-lib --lib --tests` | 302 pass, `registry_export` green |
| `liquers-axum` | pass |
| `liquers-web` on wasm32 | 15 suites, 141 tests, 0 failures |
| `check-build-matrix.sh` | 14/14 |

### Scope changes made during design, all at a gate

The design that shipped is materially wider than the issue as filed, and each widening was a
maintainer decision recorded at a gate:

| Change | From |
|---|---|
| `StoreFactory` and `StoreRouterBuilder` move too, so `liquers-web` drops the dependency | Gate 1 |
| First-wins chaining; no overlap warning | Gate 2 |
| No built-in fallback; unclaimed type lists supported types; factories describe arguments | Gate 3 |
| JSON argument types; **no compatibility shims**; per-crate factory convention; validate on construction | Gate 4 |
| `ArgumentCoverage`, and deriving argument names | Gate 5 |
| `claims` → `resolve`, so a factory can infer the store type | Gate 6 |

Complexity was reclassified **M → L** at the first of these.

## Deviations from the Approved Design

**Two steps deferred, both on the same P0.** `STORE-OPENDAL-SERVICES-NOT-ENABLED` blocks Step 11
(offline S3 tests) as planned, and Step 9 (deriving argument names) for a reason the plan did not
anticipate: deriving requires *naming* a config type, and `opendal::services::S3Config` sits behind
`#[cfg(feature = "services-s3")]`, which `liquers-store` does not enable. Attempted, reverted,
reasoning left in `OpendalStoreFactory::common_arguments`'s doc comment.

**`core04` changed meaning.** Planned as "a `Complete` type rejects an unknown key". It became
`core04_missing_required_argument_fails_at_construction`, because rejecting unknown keys is
behaviour `ArgumentCoverage::Complete` *permits* and this change does not implement — a test
asserting unimplemented behaviour would be a lie. `coverage02` covers the `Partial` half, which is
implemented.

**`opendal04` added.** `factory04` checks the error *message*; `opendal04` checks the
*declaration*. They can fail independently.

**A planned build-matrix row dropped.** `liquers-core --no-default-features` was in Step 10's list
and does not compile — see below.

## Documentation Delivered

| Document | Change |
|---|---|
| `guides/STORE_FACTORY_GUIDE.md` | **New.** Choosing a chain, adding a type by map or by trait, overriding by chaining earlier, declaring unavailability, `ArgumentCoverage`, the `create` contract, inference rules, and what the descriptions do not express |
| `reference/STORE_CONFIG_FSD.md` | Rescoped by crate; new §"Building stores: the factory model"; `StoreFactory` section rewritten; **corrected** the claim that factories precede built-in types; corrected the `opendal` feature's rationale. `History` + `reviewed:` |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | **Recommendation reversed** — see below. `STORE12` restated in terms of chain order with an `NA` condition; the extension-seam rule updated. `History` + `reviewed:` |
| `reference/api/DOC_01_ARCHITECTURE_REFERENCE.md` | The import table now names `liquers_core::store_config` / `store_factory` |
| `README.md` (root) | Same split |
| `CLAUDE.md` | "Adding a Store Backend" rewritten around factories |
| `DOCS_STRUCTURE_GUIDE.md` §3 | `store/config` redefined by topic rather than by two filenames that no longer exist |
| `specs/README.md` | Capability line for this design |

### The reversal worth reading

`LANGUAGE-INTEGRATION_GUIDE.md` §"Taking only part of the store support crate" enumerated three ways
to let a constrained integration use the configuration format without the backends, **recommended
option 3 and explicitly rejected option 2 — which is what this design did.**

The rejection was reasonable when written and did not survive its own reasoning. It objected that
moving the types "widens core for one consumer's benefit"; the consumer turned out not to be an
integration but `liquers-core` itself, which must describe a store to own an environment
configuration. Its second objection — separating the format from the crate whose reference documents
it — was answered by rescoping the reference rather than by leaving the code in place.

Option 3 remains correct for the *backends*: `liquers-store`'s `opendal` feature is still optional,
and its three recorded costs are still live. It was the wrong answer for the *format*. Both are now
recorded, because a guide that quietly flips a recommendation teaches nothing.

## Issues Filed

Four, none absorbed into this change.

| Issue | Pri | Why it is separate |
|---|---|---|
| `STORE-OPENDAL-SERVICES-NOT-ENABLED` | P0 | `liquers-store` enables no OpenDAL service features, so all 21 advertised types are unbuildable by a consumer. A user-facing defect fix does not belong buried in a refactor |
| `CORE-NO-DEFAULT-FEATURES-BROKEN` | P2 | `liquers-core --no-default-features` has never compiled; the failing imports are in files this change does not touch |
| `STORE-OPENDAL-LIST-OPTION-MISPARSED` | P2 | `config_as_string_map` flattens a JSON array to JSON text while OpenDAL splits on commas. Crosses the move intact |
| `CORE-CONFIGURATION-ERROR-KIND` | P3 | Three configuration failure paths carry three unrelated error kinds; a taxonomy change over code this design does not touch |

`STORE-CONFIG-FROM-URI` (P3) was also filed, with `design/store-config-uri/` — that design's audit
is what caught the `claims` → `resolve` change before the gate.

## Important Learning

**The design's own audit paid for itself once, concretely.** `store-config-uri` was written to check
whether future URI support would conflict with this design. Its first pass said "no change
required"; the maintainer's reframing — *a URI may be deliberately ambiguous, a store type may not,
and the store type may be inferred* — showed that `claims(&str) -> bool` could express neither half
of resolution. Changing it after shipping would have broken every implementor. Changing it at the
gate cost one method's shape.

**Two defects were found by running validation commands, not by reading code.**
`CORE-NO-DEFAULT-FEATURES-BROKEN` surfaced from a matrix row this plan proposed adding, and the
`services-*` gap surfaced from probing what `create_store` could actually build. Both had been
invisible for as long as they existed because nothing exercised the configuration.

**A test suite can be green *because of* a dev-dependency.** `liquers-store`'s OpenDAL tests pass
only because dev-dependencies add `services-fs` while the library links none. That is worse than a
missing test: it actively conceals the defect. Worth looking for elsewhere.

**A new test reproduced a defect another design is about to fix.** `opendal03` originally asserted
`key_prefix()` and failed, because `AsyncOpenDALStore` returns an empty key — `opendal-path-mapping`
Phase 2 lists that exact function. The assertion was changed, not the backend, with the reason in
the test.

**`ArgumentCoverage` generalised beyond its motivating case.** It was proposed as protection against
OpenDAL drift. The maintainer's justification is better and is the one recorded: *any* externally
owned backend can only be described incompletely, so without a way to say "partial" every such
backend forces a choice between claiming completeness and being silently wrong, or describing
nothing.

## Conformance and Remaining Work

**Requested and delivered:** configuration types in core; the factory seam, chaining and builder in
core; `liquers-web` free of `liquers-store`; per-crate default factories; unclaimed types reporting
what is supported; argument descriptions; validation on construction.

**Requested and deferred:** deriving OpenDAL argument names, and the offline S3 tests — both on
`STORE-OPENDAL-SERVICES-NOT-ENABLED`. Neither is lost: the plan step and the test bodies are
written, and the issue records what unblocks them.

**Not requested, deliberately not done:** URI configuration (`STORE-CONFIG-FROM-URI`, designed);
rejecting unknown keys on `Complete` types (permitted, unimplemented); removing `liquers-axum`'s
unused `liquers-store` dependency (unrelated scope); the four filed issues.

## Validation

```
liquers-core --lib                                    663 pass
liquers-core --test store_router_STORE                  5 pass
liquers-core --doc store_config                         1 pass
liquers-store                                          19 pass
liquers-store --no-default-features --features async_store   13 pass
liquers-lib --lib --tests                             302 pass (registry_export green)
liquers-axum                                          pass
liquers-web wasm32 --features debug-handles           15 suites, 141 tests, 0 failures
scripts/check-build-matrix.sh                         14/14
scripts/docs_index.py --check                         0 errors
```
