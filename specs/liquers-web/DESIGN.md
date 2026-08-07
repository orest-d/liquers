# liquers-web Design Tracking

**Created:** 2026-08-06

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (all 7 questions decided)
- [x] Phase 2: Solution & Architecture (reviewed; Option Y decided, awaiting approval)
- [x] Phase 3: Examples & Testing (reviewed; full 83-test inventory, awaiting approval)
- [x] Phase 4: Implementation Plan (reviewed; 26 steps in 6 milestones, awaiting approval)
- [ ] Implementation Complete

## Notes

**Phase 1 scope:** the LANGUAGE-INTEGRATION_GUIDE "Browser JavaScript" profile —
`OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND` + `ASYNCQ`, plus `ASYNCCMD` (promoted by
user decision: server-fetching commands are a primary case) and minimal `STUBS`/`PACKAGE`
so the artifact is loadable. `STORE`, `RECIPE`, `UIUSE`, `UIDEF` deferred;
`MODULE`, `POLYGLOT`, `WEBSERV`, `WEBAPI` are `NA` for this milestone.

**User decisions closing Phase 1 open questions 1 and 3-7:** relax `ValueExtension` and reuse
`CombinedValue` (1); `name` + JS function required, everything else defaulted, argument specs
inferred from the function only where honestly possible (3); global singleton first plus
explicit instances (4); re-entrant evaluation runs inline, tradeoffs accepted (5); Promises
supported from the start (6); trunk first, CDN/plain-page loadable, single-file wasm wanted
next, npm later (7).

**Question 2 (opaque `JsValue`) closed — all Phase 1 questions resolved.** Structural conversion
by default, opaque retention opt-in, `JsValue` held directly (registry-plus-ID reserved for
callables under `COMMAND`). Liquers guarantees query determinism, **not** `roundtrip(obj) === obj`,
so direct retention is an optimization and structural conversion is a legitimate fallback.
Follows from that: `===` may hold incidentally but is not promised; opaque values are immutable
by discipline rather than enforcement (the Python implementation allowed the same and it caused
fewer problems than expected — the browser deliberately trades guarantees for flexibility, and
`liquers-axum`/backend must not inherit that posture); mutable state belongs in the language
runtime (`window`, closures, IndexedDB via `web-sys`) and such commands should be `volatile`
since that state is invisible to dependency tracking; retention is session-and-realm-scoped,
converting or erroring at the boundary; serialization fails cleanly, which the core already
absorbs (`assets.rs:2994`/`:3016`).

Opaque retention is also the **fast path** — structural conversion is O(size) boundary crossings
(one `Reflect` call per property, UTF-16→UTF-8 per string) versus O(1) for a heap-table slot — so
the opt-in must be ergonomic. It still does not flip the default, because primitives must convert
or `identifier`/`type_name`/media type/`as_bytes` and every Rust command break. Magnitude is a
**Phase 3 measurement**, not an assumption. Wasm *size* is not a factor here (the conversion layer
exists either way); size belongs to the packaging milestone.

**Phase 1 findings:**
- The wasm foundation already exists: `MaybeSend`/`MaybeSync`, `BoxFuture`, and
  `ImmediateAssetManager` were delivered by `specs/async-wasm-refactor` (complete), and
  `liquers-core` + `liquers-lib` already compile to `wasm32-unknown-unknown` with a Playwright
  e2e proof (`liquers-lib/examples-web/ui_spec_demo`).
- **Resolved (user decision):** `liquers_lib::value::extended::ValueExtension` still requires
  `Send + Sync + 'static` (`liquers-lib/src/value/extended.rs:12`), which an opaque `JsValue`
  cannot satisfy. It will be **relaxed to `MaybeSend + MaybeSync + 'static`**, matching
  `ValueInterface` (`liquers-core/src/value.rs:49-50`), and `liquers-web` reuses `CombinedValue`
  with a `JsExt` extension. One implementor (`ExtValue`), all bounds local to `extended.rs`,
  native behaviour unchanged.
- `liquers-wf` was designed but never implemented (not a workspace member); `liquers-web`
  supersedes it.

**Phase 2 outcome.** No new `Environment` and no new `CommandExecutor` are needed —
`DefaultEnvironment` is already generic over the value type and cfg-selects `ImmediateAssetManager`
on wasm, and the executor closure aliases already drop `Send`/`Sync` there, so a JS command is an
ordinary async command whose closure owns a `js_sys::Function`.

**Option Y decided:** the opaque JS value is a cfg-gated `ExtValue::Js` variant in `liquers-lib`
(14 match sites to extend), so `liquers-web` reuses `liquers_lib::value::Value` and the existing
wasm-viable command library. Precedent: `liquers-py` already carries `Py { value: Py<PyAny> }`.
**Extensibility requirement (user):** the bridge, command adapter and Promise bridge stay generic
over `E`/`V` behind a `JsValueBridge` trait, with the concrete type appearing only in the exported
`#[wasm_bindgen]` wrappers, so a downstream crate can supply its own value type and environment.
Most third-party JS types need no custom value type at all — the opaque path covers them.

**Argument declaration: explicit path plus inference over a verified-safe subset.** Explicit
`arguments` is the reliable path and always wins. Inference (regex over `toString()`) is accepted
**only** when every parameter is a plain identifier and the token count equals `fn.length`;
defaults, rest, destructuring and bound/native functions are refused with a specific
`ParameterError` rather than mangled — measured case by case with `node`. Minification stays
undetectable, giving correct arity and wrong names; because Liquers binds arguments **positionally**
that degrades labels rather than behaviour, and a heuristic `console.warn` surfaces it. A
parser-based inference can widen the accepted subset later without changing the contract. No
JavaScript parser is linked into the wasm artifact.

**Namespaces:** root is the primary path; any explicit namespace is supported (no forced `js`
namespace); duplicates replace, inheriting `add_command`'s existing behaviour, and **every
replacement emits a `console.warn`** — with a distinct message when a JS command shadows a built-in
Rust one. `web` is reserved for platform-dependent commands (`alert`, DOM), registers nothing in
this phase, and rejects registration from JS.

**Unregistration in scope (user decision).** Justification, since replace already covers page
reload: without removal, a JS command closure owns a `js_sys::Function` whose scope can retain DOM
nodes, buffers, listeners and socket handles, and the registry holds that closure in an `Arc` for
the environment's lifetime — so an SPA registering commands per route leaks them all on navigation,
with no recourse because the primary path is a *singleton* environment. `unregister` drops the entry
and releases the handles (`RUNTIME05`). Secondary: singleton test isolation (`ENVIRON05`) and
`COMMAND06` conformance without a carve-out. **Known limitation:** replacing a Rust command destroys
it (`insert` overwrites, `commands.rs:497`), so unregister removes the replacement and cannot
restore the built-in — hence the warning on replacement.

`liquers-core` gains two additive inherent methods:
`CommandRegistry::unregister` and `CommandMetadataRegistry::remove_command` — neither registry has
any removal method today. The correctness requirement is that metadata and *both* executor maps are
removed together, since planning consults metadata while execution consults executors; removing only
one leaves a command that plans and then fails, or an unreachable executor. Additive inherent
methods, so no implementor (including `liquers-py`) is affected. Documented consequence: `unregister`
discards the `impl_version` that `add_command` preserves across a replace, so re-registering later
starts fresh and expires assets computed by the earlier command.

**Phase 2 has no open questions remaining.**

**Phase 3 outcome — full conformance inventory.** Per user directive, **all 83 prescribed tests**
for the 11 selected features are enumerated, named per the guide's scheme
(`fn value01_primitive_roundtrip()`; `test("PACKAGE03 …")`; files carry the feature ID), and
assigned a tier: **82 required**, 1 `NA` (`PACKAGE06` — no optional Cargo extra exists yet; the
reversing condition is recorded). Four tiers: native Rust (fast loop), `wasm-bindgen-test` in
headless Chromium (the bulk — `JsValue` panics natively, so conversion tests cannot be native), CI
build steps (`STUBS`, build matrix), Playwright (built artefact).

Five `NA` marks from the first draft were overturned on review as evasions rather than
dispositions: `RUNTIME01` became a native static assertion that decision 1's relaxation did not
weaken `Send + Sync` on native — the one real risk that relaxation carries; `VALUE08`/`VALUE11`,
`EVAL05`, `ASYNCQ07` (asserts the exported surface contains *no* blocking API) and
`STUBS02/04/06` are all testable as scoped.

**A Phase 2 correction came out of Phase 3.** Specifying how `RUNTIME04` would assert its claim
showed the deadlock case it guarded *cannot occur*: JavaScript cannot block on a Promise, so a sync
command re-entering `evaluate` either returns it (handled on the async path) or ignores it. The
typed error Phase 2 specified was removed rather than implemented.

**A `liquers-core` defect was found by validating an example query:** `PARAMETER-ESCAPING-INCOMPLETE` (filed in
`specs/ISSUES.md`). `encode_token` emits unparseable text for any string containing `:`, and no
lone-colon entity exists in the grammar. Affects every programmatic query builder, not just the
browser integration.

**Phase 4 outcome — 26 steps in 6 milestones.** M1 (groundwork in `liquers-core`/`liquers-lib`) is
the only milestone touching existing crates and is separately gated: if it cannot be made green the
design is wrong and nothing should be built on it. Rollback is cheap everywhere else, since
`liquers-web` is a new crate excluded from `default-members`.

Review found one blocking defect worth carrying into implementation:
`DefaultValueSerializer::as_bytes` (`liquers-lib/src/value/mod.rs:190`) **already has a `_ =>` arm**
(with `Error::new`, also against project rules), so it is the one site among the 14 where adding
`ExtValue::Js` compiles silently — the build matrix cannot protect it. The catch-all must be removed
and replaced with explicit arms. Also fixed: `opaque()` and `version()` were specified but assigned
to no step (both test-before-implementation gaps), and the three API layers that share method names
are now enumerated per owning file.

**This repository has no CI** (no `.github/`), so every gate is a developer-run command and the
multi-step gates ship as scripts. Adopting CI is a separate decision for the project owner.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
