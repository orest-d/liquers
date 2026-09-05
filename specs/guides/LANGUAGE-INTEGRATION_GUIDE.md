---
title: Language Integration Guide
kind: guide
audience: internal
area: [web, py, core/commands, core/plan, core/assets]
reviewed: 2026-09-05
---
# Liquers Language Integration Guide

Status: Draft  
Audience: integration designers, implementers, reviewers, and coding agents

## 1. Purpose

This guide defines independently targetable features for integrating Liquers with a programming or scripting *language*. It is not an API specification for one *language*. An *integration* design should select features, answer their design questions, and report implementation and test status by feature ID.

Short-term priorities are:

1. JavaScript in the browser through `wasm-bindgen`, `web-sys`, and `js-sys`
2. Starlark through `starlark-rust`
3. Python through `liquers-py`

Longer-term candidates include Rhai, JavaScript/TypeScript on Node or Deno, RustPython, generic Wasm guests, C/C++, and GDScript.

`liquers-py` is a useful but partial reference, not the conformance definition. In particular, it contains many basic wrappers and an experimental `pycall`, but also incomplete paths. See [PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md](../design/python-wrapper/phase1-high-level-design.md), [PYTHON-WRAPPER-ARCHITECTURE.md](../design/python-wrapper/phase2-architecture.md), and [FEATURES/PYTHON-BASIC-OBJECTS.md](../archive/PYTHON-BASIC-OBJECTS.md).

### Supplying a recipe working directory

A language integration that exposes recipes must preserve the distinction between authored recipe
data and host-provided execution context:

- For a recipe loaded from `recipes.yaml`, let `DefaultRecipeProvider` set `Recipe::cwd` to the
  logical key of the file's containing directory. Do not add `cwd` to authored YAML; the provider
  rejects an authored value so it cannot conflict with provenance.
- For a recipe constructed by the integration, set the public `Recipe::cwd` field when relative
  keys or linked queries should be anchored somewhere other than logical root `/`.
- Pass the recipe through the normal recipe/asset APIs. Do not rewrite its query or links to
  absolute strings in the wrapper. `Recipe::to_plan` records one leading `SetCwd`, and the
  interpreter applies that CWD in execution order, including later `-R-cwd` changes and nested
  links.

If no CWD is supplied, the first relative operand uses logical root `/` and the evaluation records
the warning `Relative key/query has no CWD; using logical root '/'.` once. Absolute operands are
independent of that fallback. The executable examples in
[`recipe_cwd_resolution.rs`](../../liquers-core/tests/recipe_cwd_resolution.rs) cover provider and
programmatic recipes, nested recipes, context entry points, dependency identity, and root fallback.

## 2. Terminology and Conventions

Terms defined here are written in *italics* when used in their precise, normative sense. Ordinary uses of the same English words need not be italicized.

- *Integrated language* (or *language*): the scripting or programming language being connected to Liquers, such as JavaScript, Starlark, or Python.
- *Language runtime*: the interpreter, virtual machine, event loop, or browser realm that executes the *integrated language*.
- *Integration*: the complete adapter between Liquers/Rust and one *integrated language*.
- *Core object*: a Liquers Rust object such as `Query`, `Key`, `Recipe`, `Plan`, `Metadata`, or `Error`.
- *Wrapper*: a language-visible object that owns or safely refers to a Rust *core object* or service.
- *Language value*: a value native to the *integrated language*, such as a JavaScript object, Starlark value, or Python object.
- *Value type*: the Rust type selected by the environment that implements `ValueInterface`.
- *Value bridge*: the conversion and wrapping layer between *language values* and the *value type*.
- *Opaque language value*: a *language value* retained by identity inside the *value type* without structural conversion.
- *Structural conversion*: copying a *language value* into ordinary variants of the *value type*, such as none, boolean, number, string, bytes, array, or object.
- *Upcast*: in this guide, wrapping a concrete *language value* as an *opaque language value* in the more general *value type*.
- *Downcast*: recovering a concrete *language value* or Rust extension value from the *value type* after checking its variant/type tag.
- *State*: `State<V>`, meaning a *value type* value together with Liquers metadata and execution state.
- *Language command*: a Liquers command whose callable implementation is written in the *integrated language*.
- *Service adapter*: an implementation of a Liquers service trait, such as `AsyncStore` or `AsyncRecipeProvider`, backed by the *integrated language*.
- *UI backend*: the renderer and event-loop implementation used to display Liquers UI elements, such as egui or a browser UI.
- *Web handler*: a framework-compatible function that maps an HTTP request to a Liquers operation and maps its result to an HTTP response.
- *Embedding host*: the outer process or application that owns Liquers and one or more *language runtimes*.
- *Hard dependency*: a prerequisite feature without which the dependent feature cannot be implemented or meaningfully tested. If a feature is selected, each of its *hard dependencies* must also be selected; a *hard dependency* may not be `NA`.
- *Soft dependency*: a prerequisite feature that the dependent feature interacts with when both are selected, but that it can be implemented without. A *soft dependency* may be `NA`; the design must then state how the dependent feature behaves in its absence.

Unless qualified, “value” is informal. Designs should use *language value*, *value type*, or *State* when the distinction matters.

## 3. How to Use the Features

**The “Objects/API to map or implement” lists say what the *integration* must cover, not what already exists.** Some entries are already provided by `liquers-core` or `liquers-lib` and only need exposing; others do not exist yet and must be added. Adding to core is legitimate and expected — `CommandRegistry::unregister` was added for exactly this reason — provided the addition is **additive**: new inherent methods and new trait methods with defaults, never a changed signature, since every implementor including `liquers-py` must keep compiling. Check before designing around an absence.

A feature ID is preferably a pronounceable uppercase word of at most eight alphanumeric characters. A familiar abbreviation is acceptable; exceptionally, a longer ID may be used when shortening it would reduce clarity. IDs contain no underscores. IDs are stable and must not be renamed for one *integrated language*. A specific design may split a feature into milestones, but status and tests must still roll up to the ID.

**The requirement levels and implementation states are defined in
[`reference/CONFORMANCE_TERMS.md`](../reference/CONFORMANCE_TERMS.md)** — Essential / Profile /
Optional, and `NA` / `NS` / `DESIGN` / `PARTIAL` / `COMPLETE` / `BLOCKED` / `CONFORMANT` — shared
with `guides/STORE_IMPLEMENTATION_GUIDE.md` so there is one definition rather than two. `Profile` in
this guide means essential for some *hosts*: `ASYNCQ` in a browser is the example.

Dependencies constrain these states. A feature may not be claimed `COMPLETE` or `CONFORMANT` before every *hard dependency* has reached at least `COMPLETE`. A *soft dependency* imposes no such ordering, but when it is `NA` — or merely not yet implemented — the dependent feature's design must say what it does instead, and that statement is part of the feature's documented limitations. Selecting a feature whose *hard dependency* is `NA` is a design error, not a limitation to be recorded.

A status claim should link to design sections, implementation, and test evidence. Recommended matrix:

| Feature | Selected level | Status | Limitations | Test evidence |
|---|---|---|---|---|
| `VALUE` | Essential | PARTIAL | Opaque values cannot be serialized | `test_VALUE01_*` … |

### Choosing a test harness

The harness is *language*-specific, so this guide prescribes questions rather than an answer. Answer
them in the design, before writing tests: getting this wrong is not a correctness problem but it
reliably costs hours, and the failures are obscure.

1. **What does each test actually need — the full *language runtime* host, or only its value
   semantics?** These are usually different, and the difference is large. A *value bridge* test
   exercising numbers, strings, byte arrays, containers and object identity typically needs only the
   language's core semantics; a test of the event loop, UI thread, or module system needs the real
   host. Classify the inventory on this axis first.
2. **Is there a lighter runtime for the tests that do not need the host?** Browser JavaScript has
   one: all thirteen `VALUE` tests for `liquers-web` are pure ECMAScript and run under Node with no
   browser and no WebDriver, while `RUNTIME`, `ASYNCQ` and `PACKAGE` need a real browser. Python has
   the same split between a bare interpreter and an application host. Using the lighter runtime
   where it suffices removes an entire class of setup failure from most of the suite.
3. **What version coupling does the harness impose, and does a mismatch fail loudly?** Toolchains
   around foreign runtimes are often version-locked in ways that fail *late*: a `wasm-bindgen` CLI
   that does not match the crate version fails at bindgen time rather than at compile time, and a
   WebDriver whose major version does not match the browser fails with a bare HTTP 404. Record the
   coupled versions and how to check them.
4. **Does the harness need a separate driver or process, and is it present in every environment the
   tests must run in?** A test suite that silently requires a browser driver is a test suite that
   does not run in CI.
5. **Does the test *binary* run natively at all?** For a cross-compiled target it may not — a
   `wasm32` test binary is not executable, and without a configured runner the failure is an opaque
   `Exec format error` rather than anything about tests.
6. **Which tests must run in the heavyweight harness even though a lighter one would pass?** Some
   contracts are only meaningful in the real host: event-loop behaviour, cancellation, and anything
   `PACKAGE` asserts about a delivered artifact. Mark those explicitly so nobody "optimises" them
   into the fast harness.
7. **Can a single test drag the entire suite into the heavyweight harness?** Frequently yes, and it
   is discovered by breaking the fast loop rather than by reading anything. Test frameworks often
   apply a runtime requirement per *binary*, *module* or *session* rather than per test, so one
   file that demands the heavy runtime makes every other file demand it too. Establish this before
   writing the first such test, and if it is true, **gate those files off by default** — behind a
   build flag, a marker, or a separate suite — so the light loop keeps its light harness. Adding
   the first heavyweight test to a previously light suite is the moment this bites.

#### Let the harness shape the design, not only the test list

Answering the questions above will usually reveal some logic that *cannot* be tested in the light
harness because it is entangled with the host — and some that only *looks* entangled. Separating
the two is a design decision worth making deliberately, because it decides where the tests that
matter most end up running.

The rule of thumb: **the logic that can silently corrupt or misroute data should not require the
heavyweight harness.** Byte encoding and decoding, address validation, name/URL construction,
metadata derivation — these are functions over plain data, and expressing them as such puts them in
the fast loop, leaving only plumbing behind the slow one. In `specs/design/liquers-web-store/` this
turned "every store test needs a browser" into "four free functions and their corpus run under
Node, and only the storage API itself needs a browser". That is a requirement on the
*implementation*, not a testing trick, and it belongs in the architecture phase.

### Test naming

Each test has a logical ID `<FEATURE><number>`, for example `VALUE01`. Put the logical ID at the start of the test-specific part of the framework name:

```python
def test_VALUE01_primitive_roundtrip(): ...
def test_COMMAND03_exception_crosses_command_boundary(): ...
```

Rust may use `fn value01_primitive_roundtrip()`, and browser tests may use `test("ASYNCQ02 promise rejects with structured error", ...)`. File or module names should also include the feature ID when practical, for example `test_VALUE_value_bridge.py`. Do not reuse a logical test ID for a different contract.

The tests listed below are the default conformance inventory. An *integration* design must mark each one required or `NA` with a reason; `CONFORMANT` means all applicable tests pass. [Appendix A](#appendix-a-reference-test-implementations) gives a reference implementation of every one of them as Python pseudocode, fixing the contract each test must establish.

### When a prescribed test does not apply

`NA` means **intentionally not applicable**, and it is the only way a required test may go unwritten. Because it is the sole escape hatch, it attracts reasons that sound sufficient and are not. The default answer for a test belonging to a selected feature is **required**; `NA` has to be argued.

**A test may be marked `NA` when:**

1. **The capability does not exist in the *integrated language*.** A language with no async model cannot satisfy `ASYNCCMD01`. Usually the whole feature is `NA`, and its tests follow.
2. **The prescribed subject does not exist in this *integration*.** `PACKAGE06` (an optional extra installs and activates its feature) has nothing to install when the design exposes no extras. The test would be vacuous, not failing.
3. **The test's premise is unreachable by construction.** If the design makes the state the test describes impossible, the test cannot be written as specified. Prefer, where you can, to *assert the unreachability* instead and keep the ID: a test that fails if someone later makes the state reachable is worth more than an `NA`.
4. **Another selected test establishes the identical contract.** Only when it genuinely does — overlapping subject matter is not the same as an identical contract.

**A test may *not* be marked `NA` because:**

1. **It runs in a different harness.** Whether a check is a CI step, a compile-time assertion, a browser test, or a type-checker invocation says nothing about whether it applies. `STUBS02` and `STUBS06` are type-checker runs, not runtime tests; that makes them build steps, not `NA`.
2. **The feature is deferred to a later milestone.** That is what the implementation states (`NS`, `PARTIAL`) are for. `NA` is a statement about applicability, not about schedule. A design that marks future work `NA` loses the record that it was ever required.
3. **The behaviour is hard to observe.** `RUNTIME05` (shutdown releases handles) is awkward — it may need a debug-only counter, a weak reference, or an instrumented allocator. Difficulty of observation calls for a mechanism, not an exemption. A test whose assertion would pass with the bug present is worse than an absent test, because it reports safety it never checked.
4. **The test's literal wording assumes a different host.** `RUNTIME01` ("native adapter satisfies required thread bounds") reads as inapplicable to a browser-only *integration* — but the contract behind it, that the *integration* has not weakened the thread bounds the native build relies on, is both testable and exactly where such an *integration* is most likely to do damage. **Restate the contract in the *integration*'s own terms and keep the ID.** Reinterpretation is expected; the IDs are contracts, not test scripts.
5. **No obvious instance is at hand.** Before concluding that nothing exists to test — no representative *value type* variant, no enum with unknown variants — check whether the *integration* provides one under a different name. An *opaque language value* is a representative variant; an unrecognised string arriving from the *integrated language* is an unknown variant.

**Some prescribed tests are conditional, and the condition is normative.** Where a test's entry in §5 or its reference implementation in [Appendix A](#appendix-a-reference-test-implementations) scopes it — "only for a *language* with no async model", "only where an optional extra exists" — a *language* outside that scope marks it `NA` and cites the condition. That is a genuine disposition, not an evasion. Check the appendix before reinterpreting a test: its pseudocode often fixes the contract more narrowly than the one-line summary suggests, and following it literally is usually right. `VALUE04`, for instance, asserts that a byte array and a string map to *different type names* — a statement about the mapping, not a claim that the *value type* must refuse to decode bytes as text later.

**Every `NA` carries a reversing condition.** State what would make the test required again — "when any optional extra is defined", "when a payload type other than `()` is selected". Without it, an `NA` written for a good reason at one milestone silently outlives that reason.

**A selected feature with many `NA` tests is a warning.** Its tests define what the feature *is*; excusing most of them usually means the feature should not have been selected, or the design does not implement the contract it claims. Two or three `NA`s across a whole *integration* is ordinary. A feature where half the inventory is excused deserves a second look before the design is approved, not after.

### Conformance tests that pass whatever the code does

A prescribed test names a claim; it is the *assertion* that decides whether the claim is checked. Two shapes pass regardless of what the implementation does, and both look reasonable while being written:

- **The two-branch match.** "Either it was cancelled, or it had already finished." In `specs/design/liquers-web/`, all three cancellation tests were written this way and all three passed — while the cancellation path was in fact unreachable and `cancel()` did nothing. The same assertion would have kept passing if `cancel()` had begun throwing or hanging. When a test accommodates two outcomes, find out which one actually occurs; if it is always the same one, assert *that*, and the test becomes a tripwire for the day the other becomes possible.
- **The existence check on something that was never absent.** Asserting that a forbidden name is missing from a namespace it was never in proves nothing. Assert the property instead — for a "does not block" claim, that the call *returns* before the work runs, timed.

Neither is caught by review of the test list, because the ID is present and the test is green. The check is to ask, of each assertion: *what implementation change would make this fail?* If the answer is "none that anyone would plausibly make", the test is decorative.

**Five prescribed tests exist because of this.** `OBJECT09`, `ENVIRON07`, `COMMAND12`, `COMMAND13` and `COMMAND14` were added after review of `specs/design/liquers-web/` found defects that the inventory as it then stood could not have caught — a declaration flag parsed by nobody, a state-passing mode silently degraded to another, a wrapper class with no methods, an accessor that threw in one lifecycle state, and a retained declaration aliasing the caller's mutable object. Four existing tests were also amended rather than left to be re-learned: `VALUE02` now asserts the *outbound* container type, `COMMAND05` includes a zero-argument command, `COMMAND08` requires the positive half of the opaque opt-in, and `ERROR03` must observe the exception on the evaluation path.

This risk concentrates where a section did **not** warn about difficulty. The three tests `specs/design/liquers-web/` singled out as hard to assert were all fine, because they had specified mechanisms; the vacuous ones were in the group nobody had flagged.

## 4. Feature Overview

| ID | Feature | Level | Depends on |
|---|---|---|---|
| `OBJECT` | Basic object model | Essential | — |
| `ERROR` | Error bridge | Essential | `OBJECT` |
| `RUNTIME` | Runtime, ownership, and portability constraints | Essential | — |
| `VALUE` | Value bridge | Essential | `OBJECT`, `ERROR`, `RUNTIME` |
| `ENVIRON` | Environment setup and lifecycle | Essential | `OBJECT`, `ERROR`, `RUNTIME` |
| `EVAL` | Query evaluation API | Essential | `ENVIRON`, `VALUE`, `ERROR` |
| `COMMAND` | Language command registration | Essential | `OBJECT`, `ENVIRON`, `VALUE`, `ERROR`, `RUNTIME` |
| `ASYNCQ` | Async query execution | Profile | `EVAL`, `RUNTIME` |
| `ASYNCCMD` | Async language commands | Optional | `COMMAND`, `ASYNCQ`, `RUNTIME` |
| `STORE` | Language-defined async store | Optional | `OBJECT`, `ENVIRON`, `ERROR`, `RUNTIME` |
| `RECIPE` | Language-defined recipe provider | Optional | `OBJECT`, `ENVIRON`, `EVAL`, `ERROR`, `RUNTIME` |
| `MODULE` | Load language modules from the store | Optional | `ENVIRON`, `ERROR`, `RUNTIME`, `ASYNCQ` (soft), `STORE` (soft) |
| `UIUSE` | Use an existing Liquers UI | Optional | `EVAL`, `VALUE`, `ERROR`, `ASYNCQ` (soft) |
| `UIDEF` | Define UI elements or a UI backend in the language | Optional | `UIUSE`, `COMMAND`, `RUNTIME` |
| `POLYGLOT` | Multiple-language interoperability | Optional | `ENVIRON`, `VALUE`, `COMMAND`, `ERROR`, `RUNTIME` |
| `WEBSERV` | Start and extend `liquers-axum` | Optional | `ENVIRON`, `ERROR`, `ASYNCQ` (soft), `COMMAND` (soft) |
| `WEBAPI` | Integrate Liquers handlers into a language web framework | Optional | `ENVIRON`, `VALUE`, `ERROR`, `ASYNCQ` (soft) |
| `STUBS` | Static type declarations and editor support | Optional | `OBJECT`, `ASYNCQ` (soft) |
| `PACKAGE` | Build, packaging, and distribution | Optional | `ENVIRON`, `RUNTIME`, `STUBS` (soft) |

Dependencies are *hard* unless marked `(soft)`. The soft cases are those where Liquers, not the *integrated language*, can supply the asynchrony or the callable:

- `UIUSE` → `ASYNCQ`: a *UI backend* may deliver asset status, progress, and events through callbacks scheduled onto the *language runtime*, so a *language* with no async model can still drive a UI. Without `ASYNCQ`, the design must state how progress and cancellation are delivered.
- `WEBSERV` → `ASYNCQ`: the Axum server owns a Rust async runtime, so starting and shutting it down does not require language-visible awaitables. Without `ASYNCQ`, the design must state how foreground/background start, readiness, and shutdown are expressed.
- `WEBSERV` → `COMMAND`: needed only to attach *language*-implemented handlers or middleware. Serving the standard Rust routes requires no *language command*.
- `WEBAPI` → `ASYNCQ`: an ASGI-style adapter awaits Liquers directly, but a WSGI-style synchronous adapter needs only a controlled runtime bridge. Without `ASYNCQ`, the design must state which adapters are supported and how blocking is bounded.
- `MODULE` → `ASYNCQ`: module bytes may be prefetched from the store before entering the *language runtime*, so a synchronous import hook is workable without language-visible awaitables. Without `ASYNCQ`, the design must state how store reads are performed during resolution.
- `MODULE` → `STORE`: modules are read through the environment's store, which need not be a *language*-defined one. `STORE` matters only when a *language*-defined store is also expected to serve modules.
- `STUBS` → `ASYNCQ`: entry points need awaitable declarations only where `ASYNCQ` is selected.
- `PACKAGE` → `STUBS`: declarations must ship inside the artifact when `STUBS` is selected; packaging is otherwise independent of them.

Recommended minimum profiles:

- **General baseline**: `OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND`
- **Browser JavaScript**: baseline plus `ASYNCQ`; `STORE`, `RECIPE`, `UIUSE`, `UIDEF`, and web features are selected as needed
- **Starlark baseline**: baseline with a deliberately restricted `VALUE`; `ASYNCQ` and `ASYNCCMD` may initially be `NA`
- **Python baseline**: general baseline; later milestones should add `ASYNCQ`, `ASYNCCMD`, `STORE`, `RECIPE`, and selected UI/web features

`STUBS` and `PACKAGE` are delivery features rather than capabilities: they describe how the *integration* is declared to tooling and shipped to users, not what it can do. Any *integration* consumed outside this repository should select `PACKAGE`, and `STUBS` wherever the *integrated language* has a stub format at all.

**Out of scope: cache.** There is deliberately no cache feature. The `liquers_core::cache` module (`BinCache`, `Cache`) is legacy and scheduled for removal — assets provide caching, as noted in [PROJECT_OVERVIEW.md](../reference/PROJECT_OVERVIEW.md). An *integration* should not expose it, and an existing wrapper such as `liquers-py/src/cache.rs` should be dropped rather than modernized. This is a scope decision for the guide as a whole, so it does not need a per-design `NA` entry.

## 5. Feature Guidance

### OBJECT — Basic object model

**Contract.** Expose stable *language* representations for at least `Query`, `Key`, command metadata, metadata/status, `Recipe`, `Plan`, and structured `Error` data. *Wrappers* should preserve the Rust *core object* rather than duplicate its semantics. Provide parse/encode, equality, and deterministic inspection.

**Objects/API to map or implement:**

- `Query`, `Key`, `Position`, and their component types (`ActionRequest`, parameters, segments, and resource names)
- `CommandKey`, `CommandMetadata`, `ArgumentInfo`, and `ArgumentType`
- `Metadata`, `MetadataRecord`, `AssetInfo`, status/log/progress types, and expiration types
- `Recipe`, `RecipeList`, `Plan`, and their public component enums/structs
- Shared enum and collection conversion conventions used by all other features

**The design must answer:** Which binding technique is used—such as PyO3 classes, `wasm-bindgen` exported types, Starlark custom values, C ABI handles, or plain serialized data—and why? What common conventions govern constructors, ownership, cloning, getters/setters, `repr`/`toString`, errors, and Rust-to-language conversion? Which objects are wrapped, copied, or represented as plain *language values*? How are Rust enums represented: native language enums, tagged classes/objects, strings, or integer discriminants, and how are unknown future variants handled? Are *wrappers* mutable? How are identity, equality, hashing, subclassing, and invalidated handles handled? Which Rust fields are intentionally hidden?

**Issues and patterns.** Reimplementing the Liquers query/key parser in the *integrated language* can produce different results and can fall out of date when the Rust grammar changes; route strings through the Liquers parser instead.

The **encode direction needs the same discipline and is easier to overlook.** Any *integration* that lets host code supply a string parameter must turn that string into query text, and query text has its own escaping (tilde entities: `~~`, `~_`, `~.`, `~/`, `~h`, `~H`, `~f`, `~P`, `~<digit>` for a negative number, `~U<hex>~` for any code point, and `~n<name>~` for a named entity). Percent-encoding is *not* part of the grammar and will not parse. Do not hand-roll an encoder in the *integrated language*: build the `Query` programmatically and call `encode()`, so the escaping comes from Rust — `liquers-web` used to carry its own encoder and it was deleted rather than corrected, because two implementations of one grammar is how the original escaping defect went unnoticed. **Every value is now representable**: `encode_token` is total, so an *integration* no longer needs a refusal path for unencodable characters. Pass the host string through unchanged and let `encode()` escape it; the parameter holds the *decoded* value, so nothing should be pre-escaped on the way in. `QUERY-BUILDER-TOOLING` remains open — there is still no supported query-construction utility — so assembling the `Query` is the integration's job. Avoid *wrappers* that borrow a temporary Rust object or temporary *language runtime* value. Prefer owned handles or immutable snapshots. For enums, prefer names/tags over unstable numeric ordinals and define a forward-compatibility policy. Keep convenience APIs above a thin parity layer.

**Meaningful tests:** `OBJECT01` query parse/encode roundtrip; `OBJECT02` key equality/hash; `OBJECT03` command metadata roundtrip; `OBJECT04` invalid parse produces `ERROR`; `OBJECT05` *wrapper* remains valid for its documented lifetime; `OBJECT06` every selected enum variant roundtrips; `OBJECT07` unknown enum variant follows the compatibility policy; `OBJECT08` all *wrappers* follow the documented naming and ownership conventions; `OBJECT09` a host string that cannot be represented in query text **raises** rather than producing text that will not parse.

### ERROR — Error bridge

**Contract.** Map every `liquers_core::error::ErrorType` to a language-visible error while preserving message, position, query, key, and command key where available. Convert language exceptions back to Liquers errors without panicking.

**Objects/API to map or implement:**

- `Error`
- `ErrorType` (an integration may expose the clearer language alias `ErrorKind`)
- `Position` and `CommandKey` values carried by an error
- A language-native base exception/error and, if selected, typed subclasses
- Bidirectional `Error` ↔ language exception/rejection conversion functions

**The design must answer:** Is there a typed exception hierarchy or one structured error? How are traceback, cause chain, rejected Promise value, and non-error throws represented? Which error type is the fallback? **Can the *integrated language* construct a Liquers error, or only receive one?**

The last question is easy to answer by accident. Exposing an error *type* with readable fields is
the obvious half of the bridge, and it is enough until the *language* implements a service —
`STORE`, `RECIPE` — at which point the *language* has to be able to say *which* failure occurred
and not merely that one did. Without a constructor, every failure a *language* store reports
collapses onto the adapter's fallback type, and the distinction between "absent", "forbidden" and
"backend broke" is lost at the boundary. Decide this when `ERROR` is designed, not when the first
service adapter needs it.

**Issues and patterns.** String-only conversion loses diagnostics. Use a structured payload and preserve the original language class/stack as context. Default an unmapped command exception to `ExecutionError`; conversion failures should be `ConversionError`, not execution failures. Never allow a foreign exception to unwind through FFI.

**Meaningful tests:** `ERROR01` every `ErrorType` maps; `ERROR02` fields survive Rust-to-language-to-Rust; `ERROR03` language exception includes class and stack **as observed on the evaluation path** — raised by a *language command* and caught by the caller, not by calling the conversion helper directly; `ERROR04` invalid or non-error throw has a safe fallback; `ERROR05` no panic crosses the boundary; `ERROR06` an error *raised* in the *language* keeps its type when Liquers receives it — the reverse direction of `ERROR02`, and `NA` only where the design deliberately gives the *language* no way to construct one, which then constrains every service adapter.

### RUNTIME — Runtime, ownership, and portability constraints

**Contract.** Define the integration's execution, thread, ownership, lifetime, reentrancy, cancellation, and shutdown model. Native Liquers requires `Send + Sync`; on `wasm32`, `MaybeSend`/`MaybeSync` and boxed futures deliberately permit single-threaded, non-`Send` values and callbacks. `'static` means no borrowed runtime values may escape into stored commands or background futures.

**Objects/API to map or implement:**

- Rust-side use of `MaybeSend`, `MaybeSync`, and `BoxFuture` (normally constraints, not language-visible objects)
- An integration-defined runtime/interpreter handle
- Integration-defined owned callback/callable handles
- Task, cancellation, and shutdown handles where asynchronous/background work exists
- A reentrancy/nested-evaluation policy

**The design must answer:** Which thread/event loop owns the language runtime? Can callbacks move between threads? What is owned across `await`? How are nested evaluation, locks, cancellation, teardown, and callbacks after shutdown handled? Which compile targets are supported?

#### The thread bounds are chosen by the *target*, not by the *integration*

`MaybeSend`/`MaybeSync` are gated on `target_arch`, **never** on a Cargo feature — deliberately, because Cargo feature unification is additive across a workspace and a `non_send`-style feature would silently strip `Send` from the native multi-threaded build everywhere (`liquers-core/src/maybe_send.rs`). So an *integration* does not select its thread model. What it must determine, early, is whether its language's value handles can satisfy `Send + Sync` **at all**, because that decides which targets the *integration* can exist on:

| Handle | `Send + Sync`? | Consequence |
|---|---|---|
| Python `Py<PyAny>` | yes | the *integration* runs natively |
| Starlark owned/frozen value | to be established | determines whether a native Starlark *integration* is possible |
| JavaScript `JsValue` | **no, on any target** | the *integration* is confined to `wasm32`, where the markers are vacuous |

A *language* whose handles are not `Send + Sync` is not thereby excluded — but it is confined to a target where the markers are vacuous, and that is a fact to establish in the high-level design rather than discover during implementation.

#### Relaxing a thread bound is transitive, and native builds cannot detect it

If a *value type* becomes non-`Send` on some target, **every trait that stores that value under a hard `Send + Sync` bound must be relaxed too, transitively.** This is the single most under-estimated cost in this guide's experience so far: relaxing `ValueExtension` for `liquers-web` was costed as "one implementor, bounds local to one file", and it cascaded to `UIElement` (whose implementors hold a value behind a lock) and then to `AppState` (which stores `dyn UIElement` handles).

Three things follow, and they generalize to any *integration*:

- **Budget for the closure of the relation, not the first hop.** Trace what stores your value, then what stores *that*, until the chain closes.
- **Every native configuration compiles throughout.** The markers still mean `Send + Sync` there, so nothing fails until the constrained target is built. A build-configuration matrix that includes that target is the only thing that finds it.
- **Establish where the chain stops, and say so.** Relax one trait, observe the next failure, repeat, then record the closure. For this repository the chain is `ValueExtension → UIElement → AppState`, and the only hard `Send + Sync` bound remaining on a trait that could carry a value is the legacy synchronous `Store`, which is already excluded on `wasm32`.

**Issues and patterns.** Do not hold a GIL, VM lock, Rust mutex guard, Starlark heap borrow, or JS borrow across blocking work or `await`. Copy, freeze, root, or use an owned handle before crossing the boundary. Separate target-specific type aliases for trait objects; `dyn Trait + MaybeSend` is not a substitute for conditional `+ Send`. Define and test a reentrancy policy rather than relying on the host runtime.

**Meaningful tests:** `RUNTIME01` native adapter satisfies required thread bounds; `RUNTIME02` wasm accepts a non-`Send` callback; `RUNTIME03` stored callback outlives registration scope safely; `RUNTIME04` nested evaluation does not deadlock; `RUNTIME05` cancellation and shutdown release handles; `RUNTIME06` panic/exception containment.

### VALUE — Value bridge

**Contract.** Define a *value bridge* between *language values* and the selected *value type*. Cover none/null, booleans, integers, floats, text, bytes, arrays, string-keyed objects, `Query`, `Key`, metadata-bearing values, and selected `ExtValue` variants. Define `identifier`, `type_name`, default media type, filename, data format, JSON conversion, and byte serialization.

**Objects/API to map or implement:**

- A concrete *value type* implementing `ValueInterface` and `DefaultValueSerializer`
- A language-visible *wrapper* for the *value type*
- A language-visible *wrapper* for `State<V>` or an explicit decision to expose it only through `EVAL`
- Bidirectional structural conversion functions
- Checked *upcast*/*downcast* functions for *opaque language values* and `ExtValue` variants
- Serializer/codec registry and conversion options
- Callable handle/ID representation if callables may be stored

**The design must answer:**

1. Does the *integrated language* have a universal value representation, such as JavaScript `JsValue`, Starlark `Value<'v>`, a JSON value, or Python `Py<PyAny>`? Can it safely be retained inside the *value type*?
2. Which *language values* receive *structural conversion*, which are exposed as *wrappers* around existing Rust values, and which must remain *opaque language values*? Provide a bidirectional mapping table for all supported types.
3. Is the *value type* a new implementation, a generic composition such as `CombinedValue`, or an extension of an existing Liquers value? How are *upcast* and *downcast* operations exposed, checked, and reported on failure? **Before answering, see “Retaining an opaque language value” below — the shared mechanism usually makes a bespoke variant unnecessary.**
4. Are conversions strict, permissive, or configurable? How are integer range, NaN/infinity, cycles, shared identity, mutation, and unknown objects handled?
5. Can an *opaque language value* cross stores, threads, processes, Wasm boundaries, or another *language runtime*? Which codecs are supported and trusted?
6. Does the *integrated language* support function pointers, bound methods, closures, or callable objects? May they be retained as *opaque language values*? Are they serializable? Are they accepted as *language commands* through `COMMAND`, even when they are not ordinary data values?
7. Can users operate conveniently on a *value type* *wrapper* or *State* as if it were a language scalar? Which conversions are implicit versus explicit? Can arithmetic, comparison, indexing, iteration, truthiness, and display operators be defined without hiding conversion errors or metadata loss?

#### Prefer a native variant; retain a foreign value only when you must

**The convention: when a conversion is possible and not too expensive, convert a *language value*
into the Rust-native *value type* variant rather than retaining it as an *opaque language value*.**
A Python `int` becomes `I32`/`I64`, a JavaScript string becomes `Text`, a list of scalars becomes
`Array`. Retention is for what genuinely cannot be represented — a live object graph, a callable, a
handle with identity that matters.

The reason is not tidiness. A native variant is serializable, storable, comparable, and
understandable by every other realm; an opaque handle is none of those. It also means an
integration normally defines **exactly one** extra variant — the foreign value container — and the
rest of its bridge is conversion, which is why `ExtValue::Foreign` is shared across all languages
rather than one variant per language.

"Not too expensive" is the judgement call, and it is the integration's to make. Converting a
million-element array eagerly to `Array` may be worse than retaining it; converting a small
dictionary is not. Record the threshold you chose in the mapping table required by design question
2 above, so it can be reviewed rather than rediscovered.

#### Typing an integrated value

Every value carries a **type identifier**, and since `value-type-system` the write path refuses one
it does not recognise, so this is not optional metadata. See
`specs/reference/VALUE_TYPE_SYSTEM.md` for the model and
`specs/guides/TYPE_SYSTEM_GUIDE.md` for the procedure. What an integration needs to know:

- **Naming.** A foreign type carries a provider prefix: `js.Value`, `py.Object`. A bare identifier
  asserts that Liquers *owns* the concept and is reserved for `liquers-core` and `liquers-lib`.
  Local names are CamelCase and name the concept, not the backing struct.
- **`ForeignValue::identifier`** supplies that string, and `type_name` the runtime detail — the
  language's own name for the value's type. `type_name` is informational and is never dispatched on.
- **One identifier per variant.** `ExtValue::Foreign` is a single variant, so it carries a single
  identifier for the whole build. What varies per instance is `type_name` — `JsOpaque` reports a
  constant `js.Value` and the JavaScript `constructor.name`. Do not type per class: there is no
  `js.Uint8Array`, and no provider-wide wildcard either.

**Registering it.** Your identifier lives in *your* crate, so the value type cannot describe it
statically. Extend the base registry and pass it to the environment constructor — the registry is
frozen from there, which is what keeps it lock-free:

```rust
// your value.rs — one constant, one construction site
pub const JS_VALUE_TYPE_IDENTIFIER: &str = "js.Value";

pub fn js_value_type_info() -> TypeInfo {
    TypeInfo::new(JS_VALUE_TYPE_IDENTIFIER)
        .with_type_name("JsValue")
        .with_defaults("json", "json", "application/json", "value.json")
    // No `.with_data_formats` while `as_bytes` refuses — see below.
}

impl ForeignValue for JsOpaque {
    fn identifier(&self) -> Cow<'static, str> { JS_VALUE_TYPE_IDENTIFIER.into() }
    fn type_info(&self) -> TypeInfo { js_value_type_info().with_type_name(self.type_name()) }
}

// your environment setup — the one place every rebuild path funnels through
let mut types = TypeRegistry::from_value_type::<Value>();
types.register(js_value_type_info())?;

// Through the builder, which is the recommended construction path:
let mut builder = EnvironmentBuilder::<Value>::new().with_type_registry(types);
// … register commands into `builder.command_registry` …
let envref = builder.build()?;

// Or on an environment you are assembling by hand:
let mut env = DefaultEnvironment::<Value>::new_with_type_registry(types);
```

`EnvironmentBuilder::build` returns an `EnvRef` whose asset manager is already started, so an
integration does not have to arrange initialization or worry about a first evaluation racing it.
An integration that defines its **own** `Environment` rather than using a built-in one carries that
obligation itself, in `init_with_envref`: construct the manager with the supplied reference,
install it, start it. See
[Building and Configuring an Environment](./ENVIRONMENT_CONSTRUCTION_GUIDE.md) §Implementing your
own `Environment`.

Four things worth getting right, each of which has cost somebody time:

1. **Extend `from_value_type`, never start from `TypeRegistry::new()`.** An empty base describes
   nothing, so the build cannot store ordinary text either — and the symptom appears far from the
   cause.
2. **Register where your rebuild path already goes.** If your integration reconstructs its
   environment (as `liquers-web` does on command registration), put the registration in the
   function *both* the first build and the rebuild call. Anywhere else and it silently vanishes on
   the first rebuild.
3. **Declare no data formats while `as_bytes` refuses.** `supported_data_formats` says what
   `as_bytes` *accepts*. A formatless type is a first-class case: the asset layer persists it as
   metadata with no bytes, exactly as it does a UI element. Declaring a format you cannot produce
   makes `set_binary` accept bytes that can never be materialized, moving the failure from write
   time to read time.
4. **Test that the constant and the instance agree.** The identifier is needed statically and
   per-instance, and the type system cannot tie the two together (`ForeignValue` must stay
   object-safe). One assertion — `js_value_type_info().type_identifier == instance.identifier()` —
   is the whole guarantee.

**A Python-style integration needs none of this.** If your language handle is a variant of a value
type *you* define — `liquers-py`'s `Value::Py` — its identifier is statically knowable and belongs
in your `type_descriptions()`. The constructor route is only for a type implemented in a different
crate from its value type.

Reference: `specs/reference/VALUE_TYPE_SYSTEM.md`, "Registering a type an integration owns".
Executable example: `liquers-lib/tests/foreign_value_registration.rs`, which does all of this
natively with a mock `ForeignValue` and needs no wasm toolchain.

**Design for conversion that does not exist yet.** Automatic value conversion — including coercing
a value to a command parameter's declared Rust type — is designed but not implemented
(`specs/issues/VALUE-CONVERSION-CAPABILITY.md`, with the proposal in
`specs/design/value-type-system/type-conversion-draft.md`). Two consequences for an integration
being written now:

1. **Keep conversions declarative and separable.** Conversion logic that lives inline inside a
   bridge function is hard to hand over to a conversion registry later; a table or a set of small
   named conversion functions is not.
2. **Do not build a private coercion layer.** If your bridge is tempted to guess — accepting a
   string where a number is wanted, silently truncating an integer — that is exactly the behaviour
   the conversion project must own centrally, with a lossy/fallible classification the framework
   applies uniformly. Refuse for now and record the case; a refusal is easy to relax later, whereas
   a silent coercion becomes behaviour someone depends on.

#### Retaining an *opaque language value*: use the shared `ForeignValue`

`liquers-lib` provides one variant for *all* integrated languages rather than one per language:

```rust
// liquers-lib/src/value/mod.rs — ungated: no target_arch, no feature.
ExtValue::Foreign { value: Arc<dyn ForeignValue> }

// liquers-lib/src/value/foreign.rs
pub trait ForeignValue: Debug + MaybeSend + MaybeSync + 'static {
    fn origin(&self) -> &'static str;          // "javascript" | "starlark" | "python"
    fn as_any(&self) -> &dyn core::any::Any;   // object-safe downcast hook
    fn identifier(&self) -> Cow<'static, str>;
    fn type_name(&self) -> Cow<'static, str>;
    fn default_extension(&self) -> Cow<'static, str>;
    fn default_filename(&self) -> Cow<'static, str>;
    fn default_media_type(&self) -> Cow<'static, str>;
    fn try_into_string(&self) -> Result<String, Error>;      // refuses by default
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error>;  // refuses by default
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error>;         // refuses by default
}
```

**An *integration* implements this trait in its own crate and adds nothing to `liquers-lib`.** The
concrete wrapper — `JsOpaque` in `liquers-web`, a frozen-value wrapper for Starlark, `Py<PyAny>` for
Python — stays where it belongs, and every `match` arm on the variant inside `liquers-lib` is a
one-line delegation to the trait. Adding a language costs no variant and no match arms.

Why this rather than a per-language variant:

- **Languages are separated at *downcast* time, not at variant time.** `as_any().downcast_ref::<T>()`
  returning `None` means the value came from a different runtime, and `origin()` names which — so a
  cross-language mistake produces a diagnosable error rather than a bare conversion failure. That is
  `POLYGLOT03` and `POLYGLOT04` satisfied by the mechanism instead of by extra machinery.
- **The variant is ungated**, so a missing match arm is a compile error in *every* build
  configuration rather than in one of them. Prefer this shape generally: a mechanism whose omissions
  the compiler catches everywhere beats one it catches only where someone remembered to build.
- **The refusing defaults are the right ones.** Refusing byte serialization is safe because the asset
  layer already absorbs it, falling back to a time-based version and metadata-only persistence, so an
  unserializable value degrades instead of breaking evaluation.

Two cautions when adding any variant to a shared enum: **audit for pre-existing `_ =>` arms first**,
because those absorb a new variant silently and are the one place the compiler will not help; and
note that structural conversion of *compound* values goes through `try_from_json_value`, which is the
only constructor every *value type* supports — so bytes nested inside an array or object degrade to
an array of numbers, while top-level bytes are preserved.

**Issues and patterns.** Prefer lossless *structural conversion* and an explicit opaque fallback. A JSON-like representation is portable but cannot preserve arbitrary objects, callable identity, cycles, bytes without a convention, or all numeric types. *Opaque language values* must advertise their ownership and serialization limits. JavaScript `Number` cannot losslessly represent every `i64`; use checked conversion or `BigInt`. Use `Uint8Array` for bytes. Starlark values are heap-lifetime-bound, so copy primitives/containers or retain an owned frozen value; never store a borrowed `Value<'v>`. Python may permissively retain `Py<PyAny>` and use an opt-in pickle codec, with the security implications documented.

Callable values should normally be registered in a callable registry owned by `COMMAND`, while the *value type* stores only a stable callable ID or an explicitly opaque owned handle. This prevents accidental serialization and makes callable lifetime/replacement visible. Operator overloads on a *wrapper* can be ergonomic, but must define whether they operate on the underlying *language value*, convert through a scalar, or execute a Liquers operation. Operations on *State* must state whether metadata is preserved, combined, or discarded; explicit `.value`/`.to_*()` access is the safe baseline.

**Meaningful tests:** `VALUE01` primitive roundtrip; `VALUE02` nested array/object roundtrip **in both directions**, ending in the *host-idiomatic* container type; `VALUE03` integer boundaries; `VALUE04` bytes are not confused with text; `VALUE05` unknown object follows configured policy; `VALUE06` opaque serialization fails or uses its codec explicitly; `VALUE07` cycles/shared references follow policy; `VALUE08` representative `ExtValue` roundtrip; `VALUE09` checked *upcast* and *downcast*; `VALUE10` language-only object retains documented identity/lifetime; `VALUE11` callable retention or rejection follows policy; `VALUE12` scalar operators produce the documented result; `VALUE13` *State* operations preserve or deliberately discard metadata.

```python
def test_VALUE05_unknown_object_uses_opaque_value():
    obj = object()
    assert lq.from_value(lq.to_value(obj)) is obj  # if identity retention is promised
```

### ENVIRON — Environment setup and lifecycle

**Contract.** Construct and expose a coherent `EnvRef` with one command metadata registry, command executor, async store, recipe provider, asset manager, session/payload policy, and *value type*. Define default initialization, explicit configuration, repeated initialization, test reset, and shutdown.

**Objects/API to map or implement:**

- A language-visible environment/`EnvRef` *wrapper*
- An environment builder or configuration object
- A language-visible validation-report value, or an equivalent collection of diagnostics with
  severity, message, and command identity where present
- `CommandMetadataRegistry` access
- Handles for the selected `CommandExecutor`, `AsyncStore`, `AsyncRecipeProvider`, and `AssetManager`
- `User`, `Session`, and payload configuration where supported
- Initialization, capability/version inspection, reset-for-test, and shutdown functions

**The design must answer:** Is the environment global, scoped, or injectable? Which services have defaults? When is configuration frozen? Can registries be reloaded? How are browser module startup, VM startup, package loading, and resource cleanup reported? Is version/capability discovery available?

**Issues and patterns.** Avoid hidden creation of multiple incompatible environments. Builders are preferable before publication; after publication, either reject mutation or define atomic replacement. A global Python environment may be ergonomic, while embedders and tests still need explicit instances. Browser startup should return a `Promise`; do not expose a blocking initializer.

**Validation reports are part of the integration boundary.** Before publishing an environment,
an integration using `EnvironmentBuilder` must make the builder's `validate()` result available to
the host language. It may wrap `IssueReport` directly or map each `Issue` into a language-native
diagnostic, but it must preserve severity and message and preserve command identity for
command-registry issues. The normal language-visible build/initialization operation may raise the
compact error summary; a GUI, editor, or custom logger needs the complete report before that
consuming operation. On wasm, the default full-report emission goes to the browser console, but
it does not replace this programmatic access. See
[Building and Configuring an Environment](./ENVIRONMENT_CONSTRUCTION_GUIDE.md#validating-imported-command-metadata).

**Registration after publication is the shape of the problem, and it is not a core limitation.** Rust code builds the registry, then the environment, then calls `to_ref` — but `Environment::to_ref` *consumes* the environment into an `Arc`, registration needs `&mut CommandRegistry`, and `get_command_executor` hands back a reference, so the executor cannot live behind a lock either. Once the environment is shared there is no mutable path to it. An *integration* whose host registers commands *at runtime* therefore cannot simply hold an `EnvRef` from the start, and it is easy to misdiagnose this as a missing core capability. It is not: the resolution is to do what Rust does, twice.

- Keep the environment **un-shared and mutable** until something actually needs to share it, and create the `EnvRef` lazily on first evaluation. Registering everything before the first evaluation then costs nothing, which is the path to document.
- For registration *after* sharing, **retain the original declarations** and replay them into a fresh environment along with the new one, then swap the handle atomically. Retain the host-language declarations rather than the parsed results, and replay through the same registration function used the first time — one code path, so first registration and replay cannot drift.
- State the cost, because it is real: the rebuilt environment has an **empty asset cache**, and an evaluation already in flight keeps the old `EnvRef` and completes against it, so it does not see the new command. Neither is a bug; both are surprises if undocumented.

**An environment with an empty registry passes almost every `ENVIRON` test.** `ENVIRON01`'s contract is that a default environment *evaluates a built-in command*, and the other five are about lifecycle, so a design can satisfy five of six while registering no Rust commands at all. Register the host's built-in command set as part of environment construction, and make `ENVIRON01` evaluate one of those commands. This is also what makes composition testable — a *language command* feeding a Rust command in a single query — which is the practical argument for structural conversion over opaque pass-through.

**Meaningful tests:** `ENVIRON01` default environment evaluates a built-in command; `ENVIRON02` custom services are the services returned by the environment; `ENVIRON03` repeated initialization follows policy; `ENVIRON04` failed initialization is recoverable; `ENVIRON05` isolated test environments do not leak registration, asserted **through the language-visible API**; `ENVIRON06` shutdown is idempotent; `ENVIRON07` every documented environment operation is callable, in every state the documented lifecycle can be in; `ENVIRON08` invalid imported metadata exposes a complete language-visible validation report before initialization returns its compact error.

### EVAL — Query evaluation API

**Contract.** Accept a query string or `Query`, evaluate it through the configured environment, and return a *language value* plus access to metadata/error information. Specify queued versus immediate evaluation and whether the API returns a *language value*, *State*, or asset handle.

**Objects/API to map or implement:**

- `Query` input and convenience string parsing
- `State<V>` result *wrapper*
- `AssetRef<E>` *wrapper* with status/value/metadata/cancel operations
- `Context<E>` *wrapper* for nested evaluation and logging where exposed
- Synchronous/immediate evaluation functions and convenience value-returning functions
- Evaluation options carrying payload/session/context where supported

**The design must answer:** What is the simplest entry point? How are payload/session/context supplied? When is evaluation considered complete? How are logs, progress, cancellation, asset status, and nested evaluation exposed? Does a sync API block, run inline, or reject use from an active event loop?

**Issues and patterns.** Do not bypass the environment, planner, or asset lifecycle merely to simplify the *wrapper*. Keep a low-level asset/*State* API and a convenience *language value* API. A browser must not simulate synchronous evaluation. Nested evaluation from a *language command* needs the `RUNTIME` reentrancy policy.

**Before designing a cancellation surface, check that the selected asset manager leaves a window to cancel in.** `ImmediateAssetManager` — the wasm default, and the right choice where no background scheduler exists — evaluates *during* `get_asset`, so the asset has already reached a terminal status by the time the caller holds the handle. A `cancel()` exposed on top of that is inert: it succeeds and does nothing. Exposing it anyway is defensible, since the surface will not change when a deferred manager arrives, but only if the inertness is **documented and asserted**. `EVAL06` and `ASYNCQ04` will otherwise pass vacuously — this is the concrete case behind "the two-branch match" in §3. Measure which status the asset actually has on arrival rather than assuming a race exists.

**Meaningful tests:** `EVAL01` evaluate a built-in query; `EVAL02` string and wrapped query agree; `EVAL03` metadata and logs are available; `EVAL04` invalid query maps through `ERROR`; `EVAL05` payload/context reaches a command; `EVAL06` cancellation has a defined terminal result — the design **names which** result occurs for its asset manager, and the test asserts that one.

### COMMAND — Language command registration

**Contract.** Register a callable from the *integrated language* as a *language command* with complete `CommandMetadata`, bind *State* and query parameters, inject supported services/context, invoke the callable, and convert its result or exception. Define first-command versus transform-command behavior and duplicate registration policy.

**Declaration format.** Do not invent a declaration vocabulary for your *language*. The
[Command Declaration Format](../reference/COMMAND_DECLARATION.md) defines what a
declaration means for every *language*: how its keys map to `CommandMetadata`, how a declaration
**composes** over what introspection discovered rather than replacing it, and how defaults —
including labels derived from `snake_case` or `camelCase` names — are created. Your *integration*
supplies introspection (stage 1) and the handover that keeps the callable out of the portable data;
everything from the merge onward is shared. Facts specific to your *language*, such as how the
*State* is passed or whether a variadic arrives spread, go in `hints`, which core carries without
interpreting.

**Objects/API to map or implement:**

- Language-facing command declaration/decorator/builder object, over the shared declaration format
- `CommandMetadata`, `CommandKey`, `ArgumentInfo`, `ArgumentType`, and registry *wrappers*
- A callable registry and stable callable handle/ID
- A Rust `CommandExecutor`/`CommandRegistry` adapter for language callables
- Argument binder for `State`, ordinary parameters, variadics, and injected `Context`/services
- Register, inspect, replace, and unregister operations. Core provides
  `CommandRegistry::unregister` and `CommandMetadataRegistry::remove_command`; both clear the
  metadata registry *and* both executor maps together, because planning consults the metadata while
  execution consults the executors, so a partial removal leaves a command that plans and then fails

**The design must answer:** What is the most natural, small, and readable declaration in this *integrated language*: a decorator/annotation, function call, builder, module scan, callable object, or data object containing a function and metadata? Show minimal and complete examples. How do signature, annotations, defaults, variadics, documentation, namespace, realm, volatility, and async status map to metadata? Which fields require explicit metadata? How is callable identity retained? Which state-passing modes exist? Is registration reversible?

**Issues and patterns.** Optimize the common declaration without making metadata invisible. Python decorators can derive metadata from annotations and docstrings. A JavaScript declaration can naturally be an object whose `run`/`execute` property is a function or closure and whose other properties contain metadata:

```javascript
liquers.registerCommand({
  name: "repeat",
  state: "string",
  arguments: [{ name: "count", type: "integer", default: 2 }],
  doc: "Repeat the input text.",
  run: (state, count) => state.repeat(count),
});
```

Starlark may use a host function such as `command(name="repeat", fn=repeat, ...)`. Metadata is the planning contract; do not infer silently when the *integrated language* lacks enough type information. Allow explicit overrides and provide metadata inspection after registration.

**Where inference is possible at all, restrict it to a subset where the parse is provably exact, and refuse the rest.** Introspection quality varies sharply — Python's `inspect.signature` and Starlark's parameter lists are exact, while JavaScript offers only `Function.prototype.toString` and `Function.length`. The failure mode of a permissive inferencer is not an error but a *silent misbinding*, because Liquers binds arguments positionally: guess the wrong parameter list and the command runs, with arguments shifted.

The rule that works:

- Accept only the shapes the parse handles exactly — for JavaScript, every parameter a plain identifier — and **refuse anything else with an error naming the offending parameter**, rather than mangling it into metadata. A default, rest, or destructured parameter is a refusal, not a best effort.
- **Cross-check against a second signal, and refuse on disagreement.** `Function.length` counts parameters *before the first default or rest parameter*, so a naive "regex plus arity check" agrees with itself on exactly the inputs it gets wrong. Where the two disagree, the parse is unreliable; do not pick a winner.
- **Report through metadata inspection whether the arguments were inferred or declared.** Some corruption is undetectable — a minified build yields correct arity with meaningless names — and since binding is positional that degrades labels rather than behaviour. The only way an author notices is by being told which names were guessed.
- Keep the explicit declaration as the documented reliable path, and let it win outright when both are present.

**Warn on every replacement, and say what was replaced.** A duplicate registration and an accidental name collision are indistinguishable at the point they happen, and shadowing a Rust built-in stays invisible until a query quietly returns the wrong thing. Distinguish the two cases in the message — replacing a *language command* is routine, replacing a built-in usually is not. Resolve plan arguments before language binding. Release runtime/VM locks around Rust work and reacquire only for the callback. A bridge command plus callable registry is useful when direct generic registration is awkward, but aliases and callable IDs must remain observable and debuggable.

**Examples.** Base worked examples on the
[Command Declaration Format](../reference/COMMAND_DECLARATION.md) §8, which shows a
baseline, a declaration, the merged result and the derived defaults for one command. An example that
restates every argument in full misrepresents the format — the property to demonstrate is that an
author writes only the difference.

**Meaningful tests:** `COMMAND01` register and execute a first command; `COMMAND02` transform receives state and typed parameters; `COMMAND03` exception maps through `ERROR`; `COMMAND04` defaults/enums/variadics bind; `COMMAND05` metadata matches the callable declaration, **including a command with no arguments**; `COMMAND06` duplicate/unregister policy, and registration **after the environment is already in use** takes effect; `COMMAND07` context injection; `COMMAND08` returned opaque value follows `VALUE` — **both** that an un-opted-in value is refused and that an opted-in one survives the round trip; `COMMAND09` minimal declaration has useful metadata defaults; `COMMAND10` complete declaration preserves every supported metadata field; `COMMAND11` closure captures and retains state according to `RUNTIME`; `COMMAND12` a declared flag that changes planner behaviour actually reaches the planner; `COMMAND13` every declared state-passing mode delivers its documented content; `COMMAND14` a retained declaration is unaffected by later mutation of the object the caller passed.

```python
def test_COMMAND02_transform_receives_state_and_parameter(env):
    @env.command
    def repeat(state: str, count: int = 2) -> str:
        return state * count
    assert env.evaluate("hello/repeat-3") == "hellohellohello"
```

### ASYNCQ — Async query execution

**Contract.** Determine whether the *integrated language* has native async execution and, when it does, expose Liquers futures through that abstraction while preserving success, error, cancellation, and lifetime semantics. This is required for browser JavaScript.

**Objects/API to map or implement:**

- Rust `BoxFuture` ↔ native Promise/future/coroutine/awaitable adapter
- Integration-defined evaluation task/handle for languages without awaitables
- Cancellation handle/token
- Optional progress/log stream, iterator, callback, or subscription
- Blocking or polling compatibility API, only where safe

**The design must answer:** Does the *integrated language* support futures, Promises, coroutines, async functions, callbacks, or only synchronous calls? Can Rust/Liquers futures be driven by or bridged to its event loop? What event loop drives the future? Is the result awaitable more than once? How does host cancellation reach Liquers and vice versa? Are progress events ordered? Can a sync *wrapper* be called inside the event loop? If native bridging is impossible, which workaround is supported and what are its limitations?

**Issues and patterns.** Bridge, do not block: Rust `Future` to JavaScript `Promise`, Python awaitable, or an explicit host task. On wasm, `JsFuture` and language callbacks are non-`Send`; use the core's wasm execution model. Exported wasm futures generally need owned (`'static`) inputs. If the *integrated language* has no async model, possible restricted workarounds are: a blocking call executed outside any async worker/event-loop thread; a task handle with `poll`/`wait`/`cancel`; callbacks scheduled onto the *language runtime*; or an explicitly synchronous inline environment. State which operations can deadlock or starve. Never offer blocking wait on a single-threaded browser event loop.

**Meaningful tests:** `ASYNCQ01` await successful evaluation; `ASYNCQ02` failure rejects/raises structured `ERROR`; `ASYNCQ03` two evaluations make progress; `ASYNCQ04` cancellation propagates; `ASYNCQ05` dropping the host handle follows policy; `ASYNCQ06` no event-loop blocking; `ASYNCQ07` documented non-async workaround completes safely — **conditional: only for a *language* with no async model**, and correctly `NA` where the *language* has one; `ASYNCQ08` nested event-loop use is rejected or works without deadlock.

### ASYNCCMD — Async language commands

**Contract.** Detect or explicitly register async callables, await them without blocking Liquers workers, and convert their result/error exactly as for `COMMAND`.

**Objects/API to map or implement:**

- Async callable/coroutine/Promise detector or explicit async command declaration
- Language awaitable ↔ Rust `BoxFuture` invocation adapter
- Async command entry in the callable registry
- Cancellation/timeout bridge for a running *language command*
- Async-aware result and exception conversion

**The design must answer:** How is an async callable declared? Which runtime owns the coroutine/Promise? Can it call back into Liquers? What happens on cancellation, timeout, or runtime shutdown? Are sync and async commands represented differently in metadata?

**Issues and patterns.** Never hold *language runtime* borrows or locks across `await`. Convert arguments to owned values first. JavaScript Promise ↔ Rust Future is the natural browser bridge. Python needs a deliberate Rust/Python event-loop adapter. A Starlark *integration* may mark this feature `NA` if execution is intentionally synchronous.

**Meaningful tests:** `ASYNCCMD01` async command result; `ASYNCCMD02` async exception; `ASYNCCMD03` cancellation in both directions; `ASYNCCMD04` nested async evaluation; `ASYNCCMD05` concurrent calls do not corrupt callable state; `ASYNCCMD06` sync and async metadata differ correctly.

### Service adapters — two rules that apply to all of them

`STORE` and `RECIPE` both adapt something written in the *integrated language* to a Rust service
trait. Two mistakes are available in both, in any *language*, and both produce a *passing* test
suite:

**A missing optional method must not inherit a permissive default.** Service traits give many
methods a default so that a minimal implementation compiles — `contains` returns false, a listing
returns empty, `recipe_opt` returns none. Those defaults are right for a Rust store that genuinely
has nothing to list. They are wrong for an *adapter*, where the same value means "the *language*
object did not implement this". A half-written store then looks exactly like an empty one, and the
listing-invariant tests pass by agreeing that there is nothing to see. **Make the adapter fail
loudly for a method the *language* object did not provide** — `KeyNotSupported` or the feature's
equivalent — and reserve the trait default for the case where the object *did* answer. The one
exception is a method whose "no" makes the adapter invisible rather than broken: a store answering
false to `is_supported` is never routed to, so *there* the absent method must mean yes.

**Resolve the protocol once, when the object is adapted — not on every call.** Look up every method
at construction, fail immediately if a mandatory one is missing, and name it in the error. Three
things follow, and they matter in every *language*: a typo in a method name is reported at
registration instead of surfacing as an `AttributeError`/`TypeError` from inside an unrelated query;
the *language* object cannot change the adapter's behaviour by gaining or losing a method later,
which is otherwise a genuine action-at-a-distance bug in any *language* with mutable objects; and
the adapter records what it can do, so the "absent method" rule above has something to consult.

**The design must answer, for each adapter:** which methods are mandatory, what an absent optional
method does, and at what moment the *language* object is inspected.

### STORE — Language-defined async store

**Contract.** Two directions, and a selected `STORE` may include either or both:

1. **A store written in the *integrated language*** — adapt a *language value* to the complete
   `AsyncStore` contract: data and metadata get/set, key support, containment, directory
   listing/creation/removal, and capability reporting.
2. **Stores the *integration itself* provides**, backed by whatever storage the host offers.
3. **Composition and configuration** — a *router* that dispatches a key to one of several stores,
   built from a declarative document rather than assembled in code.

Directions 2 and 3 are easy to leave out of a design, because direction 1 is what the word
"language-defined" suggests — and together they are usually the larger half of the delivered work.
A browser integration's most useful stores are backed by browser storage and HTTP rather than
written in JavaScript; a Python integration's are backed by the filesystem or an object store. And
a deployment almost never wants *one* store: it wants reference data in one place and scratch space
in another, described by a document it can edit. Decide explicitly which directions are in scope.

**Objects/API to map or implement:**

- `AsyncStore` *service adapter*
- Language-visible store protocol/base class/interface
- `Key`, `Metadata`, `MetadataRecord`, and `AssetInfo`
- Byte-buffer conversion
- Store backends appropriate to the host, where direction 2 is selected
- `AsyncStoreRouter` from `liquers-core` — the composition primitive; normally **reused unchanged**
  rather than reimplemented, since routing is not *language*-specific
- `StoreRouterConfig` / `StoreConfig` and `StoreRouterBuilder` from **`liquers-core`**, where
  direction 3 is selected — an *integration* needs no store crate for these
- An extension seam on that builder, if the *integration* contributes store types the shared crate
  does not know — implement `StoreFactory` and chain it (see "Taking only part of the store support
  crate", and `guides/STORE_FACTORY_GUIDE.md`)

**A limitation worth knowing before you design around it:** the seam is for the *integration* — Rust
code — not for the *language*. A store type contributed from JavaScript or Python is **not
supported**. See "What the *language* cannot contribute" below.

**The design must answer:** Which methods are mandatory versus safely defaulted? Are bytes copied or viewed? Are keys normalized? What are atomicity and consistency guarantees? How are language sync methods scheduled? Can callbacks re-enter Liquers?

For a store the *integration* provides (direction 2), additionally — and
[`guides/STORE_IMPLEMENTATION_GUIDE.md`](STORE_IMPLEMENTATION_GUIDE.md) is where these are answered
at length, along with the conformance suite that checks the answers:

- **Which host storage is appropriate, and is it read-only?** A read-only store is a legitimate,
  common backend; say how it refuses writes rather than leaving the trait default to decide.
- **Where does metadata come from when the backend carries none?** Filename extension, a response
  header, a stat call — and **which wins when they disagree**. Liquers dispatches deserialization
  on the data format, which usually derives from the name, so a backend's own content-type guess
  is often the *worse* source and preferring it silently breaks every command downstream.
- **What are the directory semantics on a backend that has none?** A flat key-value store and a
  listing-free protocol both need an answer, and the answer has to keep `contains`, `is_dir` and
  the listing consistent with what a read will actually return.
- **Are there size or quota limits, and what happens at the boundary?** Refusing is fine;
  refusing *after* a partial write is not.
- **How are bytes represented if the backend cannot store them?** A text-only backend needs an
  encoding, and the choice must be **recorded with the data**, never re-derived on read from a
  metadata hint that may be wrong.

#### What the *language* cannot contribute: a store *type*

Direction 1 lets the *language* define a store **instance** — a value adapted to `AsyncStore`. It
does **not** let the language define a store **type**, and the difference bites exactly where a
configuration document is involved.

In `liquers-web` a page registers an object by name and a document reaches it through the single
Rust-defined type `js`:

```yaml
- type: js
  prefix: custom
  config: { object: myStore }
```

A page cannot declare `type: myprotocol` with arguments of its own, because that means implementing
`StoreFactory` — `store_types`, `resolve`, `create` — and the trait has no binding in any integrated
language.

**Note the asymmetry with commands**, which is the clearest way to see what is missing:
`registerCommand` lets a page define a genuinely new command that then appears in the registry like
any other. `registerStoreObject` is closer to registering one *implementation* under a fixed name.

Three consequences to design around, not merely to note:

- **A page-defined store is invisible to `store_types()`**, which is what the unclaimed-type error
  enumerates and what any generated table or configuration UI would list. A document naming it is
  told the type is unknown, and offered a list that could never contain it.
- **Its arguments cannot be declared.** They live inside the page's object, so its `config:` block
  is unvalidated and undocumented.
- **The document leaks how the store is provided** rather than saying what it is, so a document
  written for a browser build cannot be read as naming the same type a native build might implement
  in Rust.

If your *integration* needs language-defined store types, that is
[`LANGUAGE-STORE-TYPE-NOT-DEFINABLE`](../issues/LANGUAGE-STORE-TYPE-NOT-DEFINABLE.md) — treat it as
prerequisite work rather than something to improvise, and expect it to overlap
`COMMAND-DECLARATION-FORMAT`, which is the same problem for commands.

#### Composing stores: the router

`AsyncStoreRouter` is *language*-neutral and should be reused as-is. What an *integration* has to
understand, because it decides how its own stores behave in a composition:

- **Routing is "first store whose key prefix matches *and* which reports the key supported".** Both
  halves matter. `is_supported` **defaults to `false`** on the trait, so a store that does not
  override it is silently never selected — the most likely way a new store appears to do nothing,
  and it produces no error anywhere.
- **The router does not retry.** If the selected store refuses, that is the answer; it does not
  fall through to the next store whose prefix also matches. This is what makes a read-only prefix
  genuinely read-only, and it is worth asserting rather than assuming (`STORE09`).
- **Order decides overlaps.** Prefixes may nest (`data` and `data/scratch`), and the first match in
  configuration order wins, so a broader prefix listed first shadows a narrower one listed later.
  Say whether the *integration* warns about that or accepts it silently.
- **Does the store see the whole key or the key minus its routing prefix?** Both are defensible and
  the answer may differ per store — a store that maps keys onto an external namespace usually
  strips, one whose own namespace already separates it usually does not. State it per store; a
  single blanket rule in the design will be wrong for one of them.

**The design must answer:** which of these the *integration* inherits unchanged, and whether any
store it provides needs behaviour the router cannot express.

#### Configuration

- **Does one configuration document mean the same thing on every target?** It can, and it is worth
  arranging: the same store type can name a native backend and a host-specific one, so a
  deployment description ports unchanged. That requires the composition builder to consult the
  *integration*'s backends **before** its own built-ins, so a shared type name can be overridden.
- **How does a store that cannot be written into a document get configured?** A *language*-defined
  store is an object, and no document can contain one. The workable shape is a name: the document
  carries an identifier, the host registers the object under that identifier, and the builder
  resolves the two. Then say what happens when the name is unregistered — failing when the
  configuration is applied, naming the identifier, beats failing at first use.
- **What does variable substitution mean in this host?** A host with no environment must not
  silently expand `${VAR}` to nothing. Leaving the text unexpanded and warning is better than
  quietly producing a configuration nobody wrote.
- **Is the configuration re-appliable?** Reconfiguring a live environment is an ordinary request,
  and the *integration* has to say what happens to work already derived from the previous store.

#### Taking only part of the store support crate

**Resolved, and the recommendation reversed.** This section used to recommend option 3 below and
reject option 2. Option 2 is what the project took, in
`specs/design/store-factories-in-core/`: the configuration format, the `StoreFactory` seam and
`StoreRouterBuilder` are `liquers-core`'s, and `liquers-store` keeps the OpenDAL backends. An
*integration* now needs **no store crate at all** for configuration and construction — `liquers-web`
dropped its `liquers-store` dependency entirely.

The rejection was reasonable when written and did not survive its own reasoning. It objected that
moving the types "widens core for one consumer's benefit"; the consumer turned out not to be one
integration but `liquers-core` itself, which must be able to describe a store to own an environment
configuration. Its second objection — that the move separates the format from the crate whose
reference documents it — was answered by rescoping `STORE_CONFIG_FSD.md` rather than by leaving the
code where it was.

The three options, kept because the reasoning is still useful when the same shape recurs — and it
does, for any *language* packaged for a restricted host:

1. **Duplicate the configuration types in the *integration*.** Fastest, and wrong: two definitions
   of one format drift, and the drift is silent until a document behaves differently on two
   targets.
2. **Move the shared types down to the crate everything depends on.** **Taken.** Correct when the
   types are pure data and the bottom crate has a use for them itself — which is the test worth
   applying, rather than counting consumers.
3. **Make the heavy backends an optional feature of the support crate, enabled by default.** Still
   the right answer for the *backends*, and still in force: `liquers-store`'s `opendal` feature
   remains optional. It was the wrong answer for the *format*.

Option 3's three costs are all still live for the surviving feature, and none is obvious:

- **Making a dependency optional exposes every feature the rest of the graph was silently
  providing.** Cargo unifies features additively, so a crate can compile for years while relying on
  a feature some *other* dependency happened to enable. Removing that dependency produces errors in
  files the change never touched — a missing derive macro in the configuration module, for
  instance. Budget for this; it is not a sign the approach is wrong.
- **The reduced configuration must be in a build matrix.** The default build never exercises it, so
  every missed conditional-compilation guard is invisible until someone builds the way the
  *integration* does.
- **A type that exists but is unavailable in this build must say *why*.** "Unknown store type" for
  a type that is real, documented, and merely gated off sends the reader hunting for a typo. Name
  the feature or the target responsible (`STORE13`). Target-gated types need this as much as
  feature-gated ones.

And a rule that follows from direction 2: **the shared builder must be extensible from outside,
not edited to know about the *integration*.** A support crate that names an *integration*'s store
types depends on that *integration*, which is backwards and does not scale past the first one. A
registration seam keeps the dependency pointing the right way.

The seam's shape has changed and the difference matters when composing one: the builder has **no
built-in types at all**, and factories chain with the **first to resolve an entry** building it. So
overriding a shared type name is done by chaining your factory *earlier*, not by relying on
factories preceding built-ins. See `STORE_CONFIG_FSD.md` §"Building stores: the factory model".

**Issues and patterns.** Data and metadata must remain consistent; do not treat `set_metadata` as optional. Preserve `KeyNotFound`, read, and write error distinctions. Run blocking host I/O outside async workers. In a browser, IndexedDB-backed methods are naturally Promise-based and non-`Send`.

**Every selected store gets the suite, not just one of them.** `STORE01`–`STORE07` describe *a*
store; an *integration* shipping three must run them against all three, and say per store where a
test does not apply and why. A suite that exercises only the first store leaves the others
unasserted while reporting full coverage.

**Meaningful tests:** `STORE01` set/get data and metadata; `STORE02` missing key error; `STORE03` directory listing invariants; `STORE04` remove/removedir behavior; `STORE05` a relative key is refused as `KeyNotAbsolute`, on direct calls and not only through a router (`STORE05b` guards the ENOENT trap that makes this test lie); `STORE06` concurrent update policy; `STORE07` store works in end-to-end evaluation; `STORE08` an *integration*-provided store satisfies the same contract as a *language*-defined one; `STORE09` a read-only store refuses every write with the documented error, and composition does not fall through to a writable store; `STORE10` metadata inference follows the documented precedence; `STORE11` a store router built from a configuration document routes by prefix, first match wins for overlapping prefixes, and an unmatched key is reported absent; `STORE12` the *integration*'s own store types are constructible from a configuration document, and where two factories in one chain claim a type name, the one chained **earlier** resolves it — restated in terms of chain order, because store construction is now first-wins over a composed chain rather than "factories before built-ins"; an *integration* that composes its own chain and shares no type name with another factory in it may record `STORE12`'s override half as `NA` with that reason; `STORE13` a store type that exists but is unavailable in this build is refused with a message naming the feature or target responsible.

Dispositions by direction, each stated with its reason so that selecting a direction later makes
the tests required again:

| Selected | `STORE01`–`STORE07` | `STORE08`, `STORE10` | `STORE09`, `STORE11`, `STORE12` | `STORE13` |
|---|---|---|---|---|
| 1 only — a *language*-defined store | required | `NA` | `NA` | `NA` |
| 1 + 2 — plus *integration* backends | required, **per store** | required | `NA` without composition | required if any backend is optional |
| 1 + 2 + 3 — plus composition | required, **per store** | required | required | required if any backend is optional |

`STORE13` is a build-configuration check rather than a runtime one for most *integrations*, and
that does not make it `NA` — §3 is explicit that the harness a check runs in says nothing about
whether it applies.

### RECIPE — Language-defined recipe provider

**Contract.** Adapt a provider from the *integrated language* to `AsyncRecipeProvider`: recipe lookup, optional lookup, containment, plan creation, folder recipe discovery, and asset information.

**Objects/API to map or implement:**

- `AsyncRecipeProvider<E>` *service adapter*
- Language-visible recipe-provider protocol/base class/interface
- `Recipe`, `RecipeList`, `Plan`, `Key`, `ResourceName`, and `AssetInfo`
- Provider composition/precedence/configuration object where multiple providers are supported

**The design must answer:** Does the provider return `Recipe`, query text, or plain data? Which methods may use Liquers defaults? May provider callbacks evaluate queries? How are caching, invalidation, volatility, and provider precedence handled?

**Issues and patterns.** Implement `recipe_opt` without translating “not found” into an execution error. Keep `contains`, `recipe`, and listing mutually consistent. Provider callbacks receive an environment; apply the `RUNTIME` reentrancy rules. Prefer immutable recipe snapshots across the boundary.

**Meaningful tests:** `RECIPE01` found and missing recipe; `RECIPE02` list/contains consistency; `RECIPE03` recipe produces a valid plan; `RECIPE04` provider error maps through `ERROR`; `RECIPE05` volatility/expiration metadata survives; `RECIPE06` end-to-end keyed evaluation; `RECIPE07` nested environment use follows policy.

### MODULE — Load language modules from the store

**Contract.** Where the *integrated language* supports module loading, allow modules to be resolved and loaded from designated locations in the Liquers store, in addition to or instead of the host filesystem. Define the search path, the precedence against native and standard-library modules, the trust boundary, and the caching and invalidation policy. Where an executing command or script has a current working directory, that *cwd* participates in resolution, in the way `PYTHONPATH` and `sys.path` participate in a Python import.

**Objects/API to map or implement:**

- A store-backed module loader or resolution hook for the *integrated language*, such as a `sys.meta_path`/`sys.path_hooks` finder in Python, a `load()` resolver in Starlark, or a module resolver for a Wasm/JavaScript host
- Module search-path configuration: an ordered list of store `Key` prefixes, configured on the environment
- `Context::get_cwd_key` and `Recipe::cwd`/`Recipe::get_cwd` as the source of the relative resolution base
- `Key::to_absolute`, `Key::parent`, and `Key::filename` for mapping a module name to a store key
- A module cache with an explicit invalidation and reload policy, keyed by store key and version
- Dependency registration, so loaded module keys participate in the tracking in `liquers-core/src/dependencies.rs` and in asset expiration
- A trust policy controlling which store prefixes may supply executable code

**The design must answer:** Does the *integrated language* have a module system, and is its resolution hook public and stable? Which store prefixes form the search path, and who configures them — environment, session, or query? What is the precedence between store modules, host filesystem modules, and the standard library? Is *cwd* taken from `Context::get_cwd_key`, from the `Recipe`, or supplied explicitly, and what happens when it is absent? Are relative imports resolved against *cwd* only, or may they escape upward out of it? How does a module name map to a store key, including packages and submodules? The store is asynchronous while most import systems are synchronous — how is that bridged, and where can it deadlock? When a module's bytes change in the store, is it reloaded, and are dependent assets expired? Which store locations are trusted to supply executable code, and how is that enforced? What happens on a name collision with a native module?

**Issues and patterns.** Loading code from a store is arbitrary code execution, and a store may be remote, shared, or writable by users who are not trusted to run code inside the host process. Treat the module search path as a security boundary: default it to empty and require explicit opt-in per prefix, rather than exposing the whole store. Never let a store module shadow the standard library — resolve native modules first unless the design deliberately states otherwise and accepts the consequence.

The synchronous/asynchronous mismatch is the central technical problem. Python's import system is synchronous while the Liquers store is `AsyncStore`, so a store-backed finder invoked from inside an async worker can deadlock in exactly the way `ASYNCQ` describes. Prefetching module bytes before entering the *language runtime* is usually safer than bridging an async store into a synchronous import hook.

Module caches are a staleness hazard: `sys.modules` will happily retain a module whose store key has since changed. State whether reload is automatic, explicit, or never, and register loaded module keys as dependencies so that editing a module expires the assets whose commands it defined — otherwise a code change silently produces stale results. `Recipe::cwd` is set automatically when recipes are loaded from a folder (`liquers-core/src/recipes.rs`), which makes it the natural default base for relative resolution. `Context` owns the live runtime CWD; integrations should read it through `Context::get_cwd_key` rather than duplicate resolution state or depend on its private storage representation. A sandboxed *integrated language* such as Starlark should map this feature onto its own `load()` resolver rather than exposing a general filesystem-like path.

**Meaningful tests:** `MODULE01` module loads from a configured store prefix; `MODULE02` module outside the search path is not loaded; `MODULE03` relative import resolves against *cwd*; `MODULE04` absent *cwd* follows the documented policy; `MODULE05` package/submodule resolution; `MODULE06` native and standard-library modules take the documented precedence; `MODULE07` changed module bytes follow the reload policy; `MODULE08` a loaded module key is registered as a dependency and expires dependent assets; `MODULE09` an untrusted prefix is refused with a typed `ERROR`; `MODULE10` store failure maps through `ERROR`; `MODULE11` import from inside a running command does not deadlock; `MODULE12` a command registered by a store module is executable end to end.

### UIUSE — Use an existing Liquers UI

**Contract.** Allow code in the *integrated language* to start, configure, populate, and interact with an existing Liquers *UI backend*, such as egui. Define how UI-capable values, commands, asset state, progress, logs, and cancellation are exposed.

**Objects/API to map or implement:**

- `UIElement`, `UiCommand`, and legacy `ExtValue::Widget`/`WidgetValue` as opaque or usable *wrappers*
- `UIHandle`, `UIContext`, `AppState`, `AppStateRef`, `UIPayload`, and `SimpleUIPayload` where public interaction is supported
- `UpdateMessage`, `UpdateResponse`, `ElementSource`, and view-mode enums
- Existing backend runner/configuration and UI lifecycle handle
- Event subscription and disposal handles

**The design must answer:** Which existing *UI backends* are available and who owns their primary thread/event loop? Can the UI run in the foreground or background? Which UI objects are exposed as *wrappers*? How does *language* code open views, submit queries, receive events, and update existing elements? Who owns subscriptions and callbacks? How are stale assets, backpressure, thread affinity, and disposal handled?

**Issues and patterns.** Keep UI transport separate from `VALUE` primitive conversion. An existing Rust UI usually must retain control of its event loop; schedule *language* callbacks onto the *language runtime* instead of invoking them under UI locks. Use stable IDs and explicit unsubscribe/dispose. Batch or throttle progress notifications. Browser DOM objects must stay on the browser thread; opaque UI handles must not be persisted as ordinary values.

**Meaningful tests:** `UIUSE01` start or attach to an existing *UI backend*; `UIUSE02` render or inspect a representative UI value; `UIUSE03` event reaches the correct command/context; `UIUSE04` progress/status ordering; `UIUSE05` unsubscribe releases callbacks; `UIUSE06` stale update is ignored; `UIUSE07` UI error maps through `ERROR`; `UIUSE08` shutdown obeys thread-affinity rules.

### UIDEF — Define UI elements or a UI backend in the language

**Contract.** Allow the *integrated language* to implement new `UIElement` behavior for an existing *UI backend*, or to implement a new *UI backend* that consumes Liquers UI/state/events.

**Objects/API to map or implement:**

- Language-defined adapter for the `UIElement` trait
- Element descriptor/component registration API
- `UpdateMessage`/`UpdateResponse` and render/event callback adapters
- `UIHandle`, `AppState`, and backend-neutral element tree/state access
- Backend adapter/renderer protocol for a wholly language-defined *UI backend*
- Serialization/type registry for language-defined element types

**The design must answer:** Is the extension point a Rust trait adapter, declarative schema, render callback, virtual tree, message protocol, or custom component registry? Which `UIElement` methods/properties/events can *language* code implement? Can one language-defined element be used by existing egui/browser renderers? What is required to implement an entire backend? How are layout, state, event routing, async updates, accessibility, serialization, hot reload, and version compatibility represented?

**Issues and patterns.** Prefer a stable declarative UI/event model over calling foreign code for every render primitive. A language-defined element adapter must own/root its callback safely and must not hold a renderer lock while entering the *language runtime*. Separate backend-neutral element semantics from backend-specific native handles. Define fallback rendering for unknown element types.

**Meaningful tests:** `UIDEF01` register and render a language-defined element; `UIDEF02` properties and state roundtrip; `UIDEF03` event invokes the correct callback; `UIDEF04` element works in each claimed existing backend; `UIDEF05` unknown element has a fallback/error; `UIDEF06` dispose releases callbacks; `UIDEF07` minimal language-defined backend renders a standard element; `UIDEF08` async updates respect UI thread affinity.

### POLYGLOT — Multiple-language interoperability

**Contract.** Allow *language commands* and *language values* from more than one *integrated language* to share an environment with explicit ownership and conversion rules.

**Objects/API to map or implement:**

- Embedded-language runtime/configuration handle, such as a Starlark runtime owned by `liquers-lib`
- Embedded-language command registry/namespace adapter
- Neutral value/interchange representation and explicit codec registry
- Origin/runtime tag for commands, values, and errors
- Cross-runtime scheduler, cancellation, and shutdown coordination
- Outer-integration facade exposing embedded-language capabilities

**The design must answer:** Is an *integration* a top-level binding, an embeddable Liquers library feature, or both? For example, can a Starlark command/runtime implemented in `liquers-lib` be registered in an environment that is itself exposed through `liquers-py` or browser Wasm? Is there one command namespace or language-qualified realms? What is the neutral interchange set? Which *opaque language values* may cross language boundaries? Who owns each *language runtime* and schedules cross-runtime calls? How are capabilities, codecs, trust, cancellation, and shutdown propagated through the outer *integration*?

**Issues and patterns.** Treat an embedded language such as Starlark as an environment capability, not as a second top-level API that must be reimplemented by Python or JavaScript. The outer *integration* should expose registration/configuration and neutral results while `liquers-lib` owns the Starlark runtime adapter. Prefer Liquers primitives, bytes plus media/data format, and Rust `ExtValue` as the neutral boundary. Never hand a Python object directly to JavaScript or a borrowed Starlark value to another runtime. Reject unsupported opaque transfer with a typed error or require an explicit codec. Detect call cycles across runtimes and avoid holding one runtime lock while entering another.

**Meaningful tests:** `POLYGLOT01` language A command feeds language B command through primitives; `POLYGLOT02` bytes/metadata preserve type; `POLYGLOT03` opaque transfer is rejected or explicitly encoded; `POLYGLOT04` errors retain origin; `POLYGLOT05` cross-runtime nested call does not deadlock; `POLYGLOT06` name collision policy; `POLYGLOT07` shutdown releases both runtimes; `POLYGLOT08` `liquers-lib` embedded-language command works through an outer Python/browser *integration*; `POLYGLOT09` outer cancellation reaches the embedded runtime.

### WEBSERV — Start and extend liquers-axum

**Contract.** Allow the *integrated language* to configure and start the `liquers-axum` server in the foreground or background and, when selected, attach *language*-implemented HTTP handlers or middleware.

**Objects/API to map or implement:**

- Integration-defined Axum server configuration and owned server handle
- Startup/readiness/shutdown API
- Route, middleware, and language-handler registration objects
- Language-visible HTTP request, response, headers, body, and authentication/session context
- `ApiResponse<T>` and `BinaryResponse` mappings where they form part of the public API

**The design must answer:** Who owns the async runtime and listening socket? How are address, routes, store/environment, graceful shutdown, startup failure, TLS, and background-thread lifetime configured? Can *language* handlers or middleware be registered, and what request/response/authentication types do they receive? Are handlers allowed to evaluate queries or call other *language commands*?

**Issues and patterns.** Prefer `liquers-axum` routes implemented in Rust for the standard API. Keep the server bound to the same `EnvRef` as the *integration*. A foreground start may block the calling thread but must drive the runtime correctly; a background start must return an owned server handle with readiness and shutdown operations. Enter the *language runtime* only for explicitly language-defined handlers, without holding Axum/Rust locks, and apply timeouts/backpressure.

**Meaningful tests:** `WEBSERV01` start on an ephemeral port and reach readiness; `WEBSERV02` standard query route uses the configured environment; `WEBSERV03` startup error maps through `ERROR`; `WEBSERV04` graceful shutdown; `WEBSERV05` background handle owns server lifetime; `WEBSERV06` language handler receives and returns documented types; `WEBSERV07` handler exception becomes a safe HTTP error; `WEBSERV08` concurrent handlers do not block the *language runtime* unexpectedly.

### WEBAPI — Integrate with a language web framework

**Contract.** Provide efficient, framework-neutral *web handlers* for the Liquers web API that can be adapted to frameworks supported by the *integrated language*, such as Flask or FastAPI in Python.

**Objects/API to map or implement:**

- Framework-neutral internal request/response and streaming-body types
- Reusable *web handlers* for query, asset/data, metadata, recipe, command, and status operations
- Framework adapter interface
- Concrete adapters selected by the integration, such as ASGI/FastAPI, WSGI/Flask, or Fetch
- Authentication/session/context extractor
- Disconnect/cancellation and streaming adapters

**The design must answer:** What is the smallest framework-neutral request/response contract? Are adapters WSGI, ASGI, Fetch-style, callback-based, or framework-specific? Which routes can stream data, metadata, logs, and progress? How are path/query/body parsing, status codes, media types, authentication context, cancellation on disconnect, and large bodies represented? Can handlers reuse a shared `EnvRef` without copying payloads?

**Issues and patterns.** Put Liquers HTTP semantics in reusable Rust handler/service functions rather than duplicating route logic in every framework. Separate protocol logic from Axum extractors so Axum, ASGI, WSGI, Node/Deno, and other adapters can translate their native request into a small internal request model. Prefer zero-copy or bounded-copy byte bodies where the binding permits. Async frameworks should await Liquers directly; synchronous frameworks need a controlled runtime bridge or worker, never one new runtime per request.

**Meaningful tests:** `WEBAPI01` framework-neutral query handler success; `WEBAPI02` Liquers error maps to the specified HTTP response; `WEBAPI03` media type and metadata headers/body survive; `WEBAPI04` FastAPI/ASGI-style async adapter; `WEBAPI05` Flask/WSGI-style sync adapter does not create a runtime per request; `WEBAPI06` disconnect cancellation propagates; `WEBAPI07` large/streaming response follows memory limits; `WEBAPI08` authentication/session context reaches evaluation; `WEBAPI09` route behavior matches `liquers-axum`.

### STUBS — Static type declarations and editor support

**Contract.** Publish a machine-checkable declaration of the *integration*'s public surface in the *integrated language*'s own type system, covering every *wrapper*, function, and error type exposed by the selected features. Declarations are generated from or verified against the implementation, never maintained by hand alone.

**Objects/API to map or implement:**

- Declaration files for every exposed module, such as `.pyi` plus `py.typed` for Python or `.d.ts` for TypeScript consumers of a Wasm build
- Declared types for every `OBJECT` *wrapper*, and for the *value type* and `State` *wrappers* from `VALUE`
- Declared signatures for `EVAL` entry points, including awaitable variants where `ASYNCQ` is selected
- The `COMMAND` declaration/decorator surface, typed so it does not erase the declared callable's own signature
- Exception/error type declarations from `ERROR`
- A generation or verification step wired into the build, and the marker/metadata that makes declarations visible to type checkers

**The design must answer:** Does the *integrated language* have a static type system or a conventional stub format at all? Are declarations emitted by the binding tool, generated from Rust, or written by hand and checked? `wasm-bindgen` emits `.d.ts`; PyO3 emits no `.pyi`. How are generic Rust types, overloads, and *opaque language values* declared? How is a command decorator typed so the user's signature survives? Which conversions are expressed as unions and which degrade to `Any`/`unknown`? How do declarations stay in sync — a CI check, a generation step, or review? What is declared for features that are `NA`?

**Issues and patterns.** Hand-written declarations drift silently, and a stale declaration is worse than none because the checker actively endorses wrong code; make the build fail instead. Declaring the surface as `Any`/`unknown` satisfies the checker while giving users nothing. A decorator that returns an untyped callable erases the signature of every command a user declares; use the *integrated language*'s typed-decorator pattern. Tool-generated declarations such as `wasm-bindgen`'s `.d.ts` are review targets, not authored files. Python declarations are ignored entirely unless `py.typed` ships in the distribution, which ties this feature to `PACKAGE`. Declarations for *opaque language values* should be honest about what is not statically known rather than overstating structure.

**Meaningful tests:** `STUBS01` declarations exist for every exposed module; `STUBS02` the type checker accepts a representative usage sample; `STUBS03` declared names match the runtime surface, with none missing or extra; `STUBS04` the command declaration preserves the declared callable's signature; `STUBS05` `ASYNCQ` entry points are declared awaitable; `STUBS06` the type checker rejects a deliberately incorrect usage; `STUBS07` declarations and their marker metadata are present in the distributed artifact.

### PACKAGE — Build, packaging, and distribution

**Contract.** Define how the *integration* is built, versioned, and distributed as an installable artifact in the *integrated language*'s ecosystem, including which Cargo features are enabled, which platforms are supported, and how the artifact's version relates to the Liquers core version.

**Objects/API to map or implement:**

- Build configuration and Cargo feature selection for the integration crate
- The distributable artifact: wheel/sdist, npm package, Wasm bundle, or shared library
- Version and compatibility metadata relating the artifact to the Liquers core version
- Supported target/platform matrix and toolchain or system-library requirements
- Release pipeline and reproducible build entry point
- Installation and quick-start path for a new user
- Declaration files from `STUBS`, where selected

**The design must answer:** Which build tool is used — `maturin`, `wasm-pack`, `trunk`, `cargo`, or the language's own packager? Which Cargo features are enabled by default, and which are exposed as installable extras? Which targets, platforms, and ABIs are published, and which remain source-only? Is the artifact version locked to, ranged against, or independent of the Liquers core version? Are native dependencies vendored or taken from the system? How is a Wasm artifact delivered — ES module, bundler-specific, or inline? What exactly does a new user run to install the *integration* and evaluate a first query?

**Issues and patterns.** Cargo feature selection is part of the public contract, not a build detail: a feature that changes the *value type* changes what `VALUE` promises, so the default set belongs in the design. Prefer locking the core version, since `OBJECT` and `VALUE` *wrappers* track Rust types that are not yet stable across releases. Vendoring native dependencies trades build time for portability — this repository takes the opposite choice for `openssl`, documented in `CLAUDE.md`, which makes system development headers a stated build requirement rather than an accident. A Wasm artifact that assumes a bundler silently excludes plain `<script type="module">` users; state which delivery forms are supported. A quick-start that a new user can run end to end is the cheapest test that packaging actually works.

**Meaningful tests:** `PACKAGE01` a clean build produces the artifact for every claimed platform; `PACKAGE02` installing the artifact into a clean environment loads the module successfully; `PACKAGE03` the documented quick-start evaluates a query end to end from the installed artifact; `PACKAGE04` declared version/compatibility metadata matches the linked core; `PACKAGE05` the default Cargo feature set produces the documented *value type*; `PACKAGE06` each optional extra installs and activates its feature; `PACKAGE07` the artifact carries declarations, license, and required metadata.

## 6. Language-Specific Guidance

### Browser JavaScript

Expose async APIs as Promises and bytes as `Uint8Array`; check `BigInt`/`i64` conversions. Keep `JsValue`, DOM handles, closures, and `JsFuture` on the wasm thread. Use the wasm-selected inline asset manager and the core `MaybeSend` model. Test in an actual browser with `wasm-bindgen-test`, including rejected Promises and disposal of JS closures.

Four things cost real time in `specs/design/liquers-web/` and are worth knowing in advance:

- **`serde_wasm_bindgen::to_value` serializes maps as JavaScript `Map`s, not objects.** Every `serde_json::Value::Object` and every `HashMap` goes through `serialize_map`, so a page reading `result.a`, `Object.keys(result)` or `JSON.stringify(result)` sees nothing. Rust *structs* serialize as objects either way, which is what makes this survive review: the affected values are the ones that came *from* JavaScript as objects. Use `Serializer::new().serialize_maps_as_objects(true)` everywhere, from one wrapper.
- **Let `wasm-bindgen` generate the `.d.ts`; do not hand-write one.** A hand-written declaration file is a second source of truth defended only by a freshness check somebody has to run, and a stale declaration is worse than none — a type checker confidently accepts code that fails at runtime. What the generator cannot do is see inside a `JsValue`, so a `JsValue` parameter emits as `any` and type-checks anything. Fix that at the source with `typescript_custom_section` plus `typescript_type`-annotated extern types, and the generated file carries real types with no drift possible.
- **`#[serde(skip_serializing_if)]` is right for a config file and wrong for an API.** `CommandMetadata` skips empty vectors, so `describeCommand(n).arguments` was `undefined` for a zero-argument command — working for every command except the ones the caller was least likely to special-case. Normalize the shape at the boundary.
- **Structural conversion is cheaper than it sounds.** Measured round trip, `--release`: an object with 10 properties costs 78 µs, 1 000 properties 5.2 ms, and a 1 MB `Uint8Array` 0.87 ms. Opaque retention is flat (~6 µs), so the *ratio* grows without bound and is the wrong number to quote. Justify an opaque path by **identity**, not by speed, unless the *integrated language* genuinely passes very large structures.

### Starlark

Treat sandboxing and determinism as product requirements. Prefer copied Liquers primitives for ordinary *language values*. `starlark::values::Value<'v>` belongs to a `Heap`; values retained after evaluation must be frozen/owned or converted. Host functions registered through Starlark globals are a natural `COMMAND` adapter. Document whether evaluation budgets, cancellation, filesystem/network access, and async features are intentionally unavailable.

### Python

Modernize `liquers-py` rather than treating its current behavior as normative. Prefer owned `Py<T>` handles across async/thread boundaries, short explicit interpreter-lock scopes, permissive `VALUE` conversion with an *opaque language value* fallback, and explicit typed exceptions. Pickle may be an opt-in codec, never an implicit trusted interchange format. Organize pytest suites using the IDs in this guide.

## 7. Design and Review Checklist

This guide does not define the shape or location of a design document. An *integration* is a substantial feature, so its design follows the standard Liquers design workflow (the `liquers-designer` skill), which creates `specs/<integration-name>/` and the phase documents within it. `specs/design/async-wasm-refactor/` is an existing example of that layout.

The checklist below is therefore not a document outline but the *integration*-specific content that must appear somewhere in those phase documents. It maps naturally onto them: scope and selection (items 1 and 9) belong in the high-level design; the architecture items (2 through 7) in the architecture phase; the test items (8 and 10) in the examples and test-plan phase; and milestone sequencing in the implementation plan.

Every language-specific design should contain:

1. A feature matrix with level, status, limitations, milestone, and test links.
2. A disposition for every item in each selected feature's “Objects/API to map or implement” list: mapped, implemented, internal-only, deferred, or `NA`, with the exposed language name.
3. The exact Rust value and environment types used.
   - **If the design offers downstream crates a "bring your own value type" path, check it against the orphan rule before promising it.** The natural sketch — `impl MyIntegrationTrait for CombinedValue<SimpleValue, MyExt>` — does *not* compile from a downstream crate: the trait belongs to the integration crate and `CombinedValue` to `liquers-lib`, so both are foreign there, and `CombinedValue` is not `#[fundamental]`, meaning instantiating it with a local type does not make the self type local (E0117). The integration crate itself never notices, because its own trait is local to it. Put the extension point on the *extension* type (`MyExt`, which the downstream crate owns) and provide a blanket impl carrying it up to the value type — then the integration crate uses the same route it documents, rather than a first-class one nobody else can reach.
4. Ownership diagrams or precise prose for every stored foreign handle.
5. Sync, async, thread, reentrancy, cancellation, and shutdown policies.
6. Conversion tables, including loss and serialization behavior.
7. Complete command metadata and argument-binding rules.
8. A test inventory whose logical IDs use the feature prefixes.
9. Explicit `NA` decisions; absence is not an `NA` decision.
10. A small end-to-end test that registers a language command and evaluates it through the real environment.
11. **The thinnest possible end-to-end path for each selected feature, scheduled in the *first*
    milestone that has any code — not the last.** For `STORE`, that is one key written and one
    `-R/` query evaluating; for `RECIPE`, one recipe resolving. It need not be the real backend: a
    memory store or a hard-coded provider is enough, because what is being proved is that the
    *path* exists, not that the *implementation* is good.

    This is not a testing preference, it is a sequencing rule, and it is the most expensive lesson
    in this guide's experience so far. `specs/design/liquers-web-store/` built four stores, a
    configuration layer and sixty-odd passing tests across five milestones before its end-to-end
    milestone discovered that keyed evaluation could not work on its target at all, for two
    defects in code the *integration* does not own. Every one of those tests was correct and every
    one still passes; none of them touched the path that was broken. A one-line evaluation in the
    first milestone would have found it before any of that was designed around.

    A *language* whose thin path fails against an *integration*-independent defect should file it,
    mark the feature `BLOCKED` naming that defect, and decide **then** — with the information —
    whether to continue, rather than discovering the choice was already made.

## 8. References

- Liquers core integration boundaries: `liquers-core/src/value.rs`, `error.rs`, `context.rs`, `commands.rs`, `store.rs`, `recipes.rs`, `assets.rs`, and `maybe_send.rs`
- Shared opaque-value mechanism for every *integrated language*: `liquers-lib/src/value/foreign.rs` (`ForeignValue`) and the `ExtValue::Foreign` variant in `liquers-lib/src/value/mod.rs`
- Command removal: `CommandRegistry::unregister` (`liquers-core/src/commands.rs`) and `CommandMetadataRegistry::remove_command` (`liquers-core/src/command_metadata.rs`)
- A worked *integration* design following this guide: [`specs/design/liquers-web/`](../design/liquers-web/) — browser JavaScript, phases 1-4 with the full 83-test disposition
- [Command Declaration Format](../reference/COMMAND_DECLARATION.md) — the shared declaration vocabulary, composition over introspection, and the defaulting rules every *language* guide builds on
- [Command Registration Guide](COMMAND_REGISTRATION_GUIDE.md)
- [Async/Wasm Refactor Design](../design/async-wasm-refactor/DESIGN.md)
- [Liquers Web API Specification](../reference/WEB_API_SPECIFICATION.md)
- [Liquers UI Payload Design](../archive/2026-03-02-ui-payload-design.md)
- [`wasm-bindgen`: JavaScript Promises and Rust Futures](https://rustwasm.github.io/docs/wasm-bindgen/reference/js-promises-and-rust-futures.html)
- [`starlark-rust` value model](https://docs.rs/starlark/latest/starlark/values/index.html)
- [`starlark-rust` evaluator overview](https://docs.rs/starlark/latest/starlark/)

## Appendix A. Reference Test Implementations

This appendix gives a reference implementation of every prescribed test from §5, written as Python pseudocode.

**It is pseudocode, not a runnable suite.** Its purpose is to fix *what each test must establish*, not the API used to establish it. The names below — `lq.evaluate`, `env.command`, `lq.ErrorType` — are placeholders for whichever surface the *integration* actually defines; a JavaScript or Starlark *integration* substitutes its own and keeps the logical IDs. Where a test cannot be expressed without a decision the design has not yet made, the pseudocode marks the decision point in a comment rather than inventing an answer.

Python is used throughout because it reads as pseudocode for the widest audience, not because these tests belong to `liquers-py`.

**The queries are not pseudocode.** While the API surface is a placeholder, every query string,
key and recipe below is real Liquers syntax and has been checked with `liquers-validate` (see
`CLAUDE.md`, “Validating queries”), against the registry plus an overlay declaring the fixture
commands. An *integration* substituting its own API keeps the queries verbatim.

### Query input has to come from somewhere

The single most common mistake when writing Liquers test queries is treating the first segment as
a literal input. **Every segment of a query is a command.** `x/boom` does not mean “apply `boom`
to the string `x`”; it means “run command `x`, then run command `boom` on its result”, and it
fails with `Action 'x' not registered` unless an `x` command exists. There is no literal-value
segment in the query language.

Test input therefore comes from one of two places, and every query in this appendix uses one of
them:

- a **source command** — a command declared without a state argument, which produces a value
  rather than transforming one. This is the `world` in `world/greet` in
  `liquers-core/tests/async_hellow_world.rs`. Source commands may take parameters, which is how a
  test parameterises its input: `number-42/…`.
- a **store resource** — `-R/<key>`, optionally continued with `/-/` and an action chain:
  `-R/d/in.txt/-/greet`. Note that `-R/` swallows everything up to `/-/` into the key, so
  `-R/d/in.txt/greet` would fetch a file *named* `greet`.

Segments ending in a recognised filename (`out.txt`, `preview.csv`) are `Filename` steps, not
actions, and are what makes a recipe *stored* under a key rather than ad-hoc.

### Conventions and harness

```python
# One test module per feature, named for the feature (§3):
#   test_VALUE_value_bridge.py, test_COMMAND_registration.py, test_MODULE_store_imports.py
# The logical ID leads the test-specific part of the name.
import pytest
import liquers as lq

def na(reason):
    """A feature marked NA still needs its tests present and skipped with the
    reason recorded in the design (§3: absence is not an NA decision)."""
    return pytest.mark.skip(reason=f"NA: {reason}")

def register_fixture_commands(e):
    """The three source commands every query in this appendix draws its input from.
    An integration must provide equivalents before any of these tests can run."""

    @e.command                                   # text input:    hello/greet
    def hello() -> str:
        return "hello"

    @e.command                                   # integer input: number-42/idem
    def number(n: int = 0) -> int:
        return n

    @e.command(volatile=True)                    # slow input:    sleep-60
    async def sleep(context, seconds: float = 60.0) -> str:
        # Must report progress through `context` for UIUSE04, and must observe
        # cancellation for EVAL06/ASYNCQ04/RUNTIME05.
        await async_sleep_reporting_progress(context, seconds)
        return "done"

@pytest.fixture
def env():
    """Isolated environment per test — see ENVIRON05."""
    e = lq.Environment()
    register_fixture_commands(e)
    yield e
    e.shutdown()

@pytest.fixture
def store(env):
    return env.store
```

Beyond the source commands, a few tests need store content the design must supply. Nothing else
in this appendix depends on data existing in the store:

| Key | Needed by | Content |
|---|---|---|
| `d/in.txt` | STORE07 | written by the test itself |
| `d/f.txt` | WEBAPI09 | any small file, for the store data/metadata routes |
| `big.bin` | WEBAPI07 | large enough to exceed the streaming threshold |
| `data/table.csv` | UIUSE02 | a table the *UI backend* can render |
| `data/sales.csv` | WEBAPI03 | a CSV, so `ns-pl/head-10` yields `text/csv` |
| `d/recipes.yaml` | RECIPE01–07 | `D_RECIPES_YAML` below |
| `proj/recipes.yaml`, `proj/helper.py` | MODULE03 | `PROJ_RECIPES_YAML` below; `helper.py` is written by the test |

The two recipe files are:

```yaml
# d/recipes.yaml (D_RECIPES_YAML) — fixture for the RECIPE tests
recipes:
- query: hello/greet/known.txt          # -> key d/known.txt
  title: Known
  description: The recipe RECIPE01, RECIPE03 and RECIPE06 look up.
- query: sleep-1/greet/volatile.txt     # -> key d/volatile.txt
  title: Volatile
  description: Carries the volatility and expiration RECIPE05 asserts.
  volatile: true
  expires: in 5 minutes
- query: recipe_that_evaluates/nested.txt   # -> key d/nested.txt
  title: Nested
  description: Its provider callback evaluates a query (RECIPE07).
```

```yaml
# proj/recipes.yaml (PROJ_RECIPES_YAML) — fixture for MODULE03;
# loading it from the folder sets Recipe::cwd to proj
recipes:
- query: uses_helper/value.txt          # -> key proj/value.txt
  title: Uses a module imported from the recipe folder
  description: uses_helper imports `helper` relative to the recipe cwd (proj/).
```

### OBJECT

```python
def test_OBJECT01_query_parse_encode_roundtrip():
    # Parse level only — none of these need a registry. The last two cover the
    # forms most likely to break a reimplemented parser: the `q` instruction and
    # a resource query with an action chain after `/-/`.
    for text in ["hello/greet", "a/b-1-2/c",
                 "data/q/query_to_string/output.txt",
                 "-R/data/report.txt/-/to_text"]:
        assert lq.parse_query(text).encode() == text

def test_OBJECT02_key_equality_and_hash():
    a, b = lq.parse_key("dir/file.txt"), lq.parse_key("dir/file.txt")
    assert a == b and hash(a) == hash(b)
    assert a != lq.parse_key("dir/other.txt")
    assert len({a, b}) == 1                      # usable as a dict/set key

def test_OBJECT03_command_metadata_roundtrip(env):
    md = env.commands.metadata("greet")
    assert lq.CommandMetadata.from_dict(md.to_dict()) == md

def test_OBJECT04_invalid_parse_produces_error():
    with pytest.raises(lq.Error) as e:
        lq.parse_query("///bad///")
    assert e.value.error_type == lq.ErrorType.ParseError
    assert e.value.position is not None          # diagnostics survive (see ERROR)

def test_OBJECT05_wrapper_valid_for_documented_lifetime(env):
    q = lq.parse_query("hello/greet")
    del env                                      # an owned handle must outlive the env
    assert q.encode() == "hello/greet"

@pytest.mark.parametrize("variant", list(lq.ErrorType))
def test_OBJECT06_every_enum_variant_roundtrips(variant):
    # repeat for each enum the design selects: ArgumentType, DependencyRelation, ...
    assert lq.ErrorType(str(variant)) is variant

def test_OBJECT07_unknown_enum_variant_follows_policy():
    # The design states one policy; assert that one. Reject is the safe default.
    with pytest.raises(lq.Error):
        lq.ErrorType("VariantFromAFutureRelease")

def test_OBJECT08_wrappers_follow_naming_and_ownership_conventions():
    # Largely a review criterion (§3) — mechanise only what is enumerable.
    for name in lq.public_surface():
        assert not name.startswith("_")

def test_OBJECT09_any_parameter_value_round_trips(env):
    # The encode direction. Every host string must survive the trip through query text —
    # PARAMETER-ESCAPING-INCOMPLETE made that true, so an integration no longer needs a
    # refusal path. Round-trip through the REAL parser, not through a second copy of the
    # escaping rules: an encoder checked against its own table agrees with itself by
    # construction.
    for value in ["two words", "12:30", "a,b", "a?b", "caf\u00e9", "\u65e5\u672c",
                  "-5", "~X~", "a/b", "\U0001F600"]:
        query = lq.parse_query(f"filter-{lq.encode_param(value)}")
        assert query.action().parameters[0].string_value() == value
```

### ERROR

```python
@pytest.mark.parametrize("et", list(lq.ErrorType))
def test_ERROR01_every_error_type_maps(et):
    exc = lq.exception_for(et)
    assert issubclass(exc, lq.Error)             # or: one structured type carrying `et`

def test_ERROR02_fields_survive_rust_language_rust():
    e = lq.Error(lq.ErrorType.ParseError, "boom",
                 position=lq.Position(line=1, column=3),
                 query=lq.parse_query("hello/greet"), key=lq.parse_key("d/f"))
    r = lq.Error._from_rust(e._to_rust())
    assert (r.message, r.position, r.query, r.key) == (e.message, e.position, e.query, e.key)

def test_ERROR03_language_exception_includes_class_and_stack(env):
    # Note `env.evaluate` — the exception must be raised by a command and observed by the
    # caller. Calling the bridge's conversion helper directly is the easy version of this test
    # and it can pass while the shipped path drops the fields: an exception travelling through
    # the planner and asset lifecycle is carried by `liquers_core::Error`, which has no slot
    # for language context (LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT).
    @env.command
    def boom(state): raise ValueError("inner")
    with pytest.raises(lq.Error) as e:
        env.evaluate("hello/boom")
    assert e.value.error_type == lq.ErrorType.ExecutionError   # documented fallback
    assert "ValueError" in e.value.cause and "inner" in e.value.cause

def test_ERROR04_non_error_throw_has_safe_fallback(env):
    @env.command
    def odd(state): raise BaseException("not an Exception subclass")
    with pytest.raises(lq.Error) as e:
        env.evaluate("hello/odd")
    assert e.value.error_type == lq.ErrorType.ExecutionError

def test_ERROR05_no_panic_crosses_the_boundary(env):
    # A Rust panic must surface as an Error, never abort the process.
    #
    # State what "contained" means on your target before writing this. Where a panic is *not*
    # catchable — a wasm panic aborts the instance — the observable failure is not an exception but
    # a call that never returns, and the assertion has to be a timeout rather than `raises`. An
    # integration on such a target should say so and treat any reachable panic as a defect, since
    # the caller cannot tell it apart from a hang.
    with pytest.raises(lq.Error):
        env._provoke_panic_for_test()

def test_ERROR06_language_raised_error_keeps_its_type(env, store):
    # The reverse of ERROR02: the *language* constructs a Liquers error and Liquers receives it
    # with the type intact. Without this, every failure a language-implemented service reports
    # collapses onto the adapter's fallback type.
    class Denying:
        async def get(self, key):
            raise lq.Error(lq.ErrorType.KeyNotSupported, f"reading {key} is not allowed")
    env.set_store(lq.adapt_store(Denying(), prefix="denied"))
    with pytest.raises(lq.Error) as e:
        await env.store.get(lq.parse_key("denied/x"))
    assert e.value.error_type == lq.ErrorType.KeyNotSupported   # not the fallback
```

### RUNTIME

```python
def test_RUNTIME01_native_adapter_satisfies_thread_bounds(env):
    # Native builds require Send + Sync; compiling the adapter is the real assertion.
    # Exercise it from a non-main thread to prove the bound is honoured at runtime.
    import threading
    out = []
    threading.Thread(target=lambda: out.append(env.evaluate("hello/greet"))).start_and_join()
    assert out == ["Hello, hello!"]

@pytest.mark.wasm
def test_RUNTIME02_wasm_accepts_non_send_callback(env):
    # On wasm32 a non-Send closure must be accepted (MaybeSend model).
    env.register_command("cb", lambda state: state, non_send=True)
    assert env.evaluate("hello/cb") is not None

def test_RUNTIME03_stored_callback_outlives_registration_scope(env):
    def register():
        local = {"n": 0}
        @env.command
        def bump(state):
            local["n"] += 1                      # closure capture must be rooted
            return local["n"]
    register()                                   # scope exits; callable must survive
    assert env.evaluate("hello/bump") == 1

def test_RUNTIME04_nested_evaluation_does_not_deadlock(env):
    @env.command
    def outer(state, context):
        return context.evaluate("hello/greet")   # reentrancy policy under test
    assert env.evaluate("hello/outer", timeout=5) is not None

def test_RUNTIME05_cancellation_and_shutdown_release_handles(env):
    h = env.evaluate_async("sleep-60")           # the slow source command (harness)
    h.cancel()
    env.shutdown()
    assert h.is_terminal() and env.live_handle_count() == 0

def test_RUNTIME06_panic_and_exception_containment(env):
    @env.command
    def bad(state): raise RuntimeError("x")
    with pytest.raises(lq.Error):
        env.evaluate("hello/bad")
    assert env.evaluate("hello/greet") is not None   # env still usable afterwards
```

### VALUE

```python
@pytest.mark.parametrize("v", [None, True, False, 0, -1, 3.5, "", "text", b"\x00\xff"])
def test_VALUE01_primitive_roundtrip(v):
    assert lq.from_value(lq.to_value(v)) == v

def test_VALUE02_nested_array_object_roundtrip():
    v = {"a": [1, 2, {"b": None}], "c": {"d": [True, "x"]}}
    back = lq.from_value(lq.to_value(v))
    assert back == v
    # Assert the OUTBOUND container type too, not only equality of contents. A bridge can
    # return a type that compares equal but is not what host code expects — serde-wasm-bindgen
    # maps every map to a JavaScript `Map`, which carries the same data and supports none of
    # `obj.a`, `Object.keys(obj)` or `JSON.stringify(obj)`. Equality-only assertions pass.
    assert isinstance(back, dict) and isinstance(back["a"], list)
    assert isinstance(back["c"], dict)

@pytest.mark.parametrize("n", [0, 2**31, 2**53 - 1, 2**53, 2**63 - 1, -(2**63)])
def test_VALUE03_integer_boundaries(n):
    # JS: beyond 2**53 must use BigInt or raise — never silently lose precision.
    try:
        assert lq.from_value(lq.to_value(n)) == n
    except lq.Error as e:
        assert e.error_type == lq.ErrorType.ConversionError

# The corpus is the point. Every entry below breaks a different plausible implementation, and a
# single ASCII string breaks none of them: `b"abc"` survives a bridge that decodes bytes as text,
# one that re-encodes them, and one that stops at the first NUL.
BYTES_CORPUS = [
    b"",                       # empty
    b"hello",                  # ASCII
    "héllo — ok".encode(),     # multi-byte UTF-8
    "🦀".encode(),              # outside the BMP: two UTF-16 code units in some hosts
    b"a\x00b",                 # embedded NUL: truncates a C-string round trip
    b"\xff\xfe",               # not valid UTF-8 at all
    b"\xed\xa0\x80",           # UTF-8-encoded lone surrogate — valid-looking, not valid UTF-8
    bytes(range(256)),         # every byte value
]

def test_VALUE04_bytes_are_not_confused_with_text():
    b, s = lq.to_value(b"abc"), lq.to_value("abc")
    assert b.type_name != s.type_name
    assert isinstance(lq.from_value(b), bytes) and isinstance(lq.from_value(s), str)

    # Bytes survive the boundary unchanged, whatever they contain. Applies to every hop that
    # carries bytes — the value bridge here, and a store's own encoding if it has one.
    for data in BYTES_CORPUS:
        assert lq.from_value(lq.to_value(data)) == data, f"corrupted: {data!r}"

def test_VALUE05_unknown_object_uses_opaque_value():
    obj = object()
    assert lq.from_value(lq.to_value(obj)) is obj    # if identity retention is promised

def test_VALUE06_opaque_serialization_fails_or_uses_its_codec():
    v = lq.to_value(object())
    with pytest.raises(lq.Error) as e:
        v.to_bytes()                                  # implicit serialization refused
    assert e.value.error_type == lq.ErrorType.SerializationError
    assert v.to_bytes(codec="pickle") is not None     # only when explicitly opted in

def test_VALUE07_cycles_follow_policy():
    a = {}; a["self"] = a
    with pytest.raises(lq.Error):                     # or: assert shared identity kept
        lq.to_value(a)

def test_VALUE08_representative_extvalue_roundtrip():
    for v in [lq.parse_query("hello/greet"), lq.parse_key("d/f"), lq.DataFrame({"x": [1, 2]})]:
        assert lq.from_value(lq.to_value(v)) == v

def test_VALUE09_checked_upcast_and_downcast():
    v = lq.to_value(lq.parse_key("d/f"))
    assert v.downcast(lq.Key) == lq.parse_key("d/f")
    with pytest.raises(lq.Error) as e:
        v.downcast(lq.DataFrame)                      # wrong tag must be reported
    assert e.value.error_type == lq.ErrorType.ConversionError

def test_VALUE10_language_only_object_retains_documented_identity():
    class C: pass
    c = C()
    assert lq.from_value(lq.to_value(c)) is c

def test_VALUE11_callable_retention_or_rejection_follows_policy():
    f = lambda x: x
    v = lq.to_value(f)
    assert v.is_callable_handle()                     # stored as an ID, not as data
    with pytest.raises(lq.Error):
        v.to_bytes()                                  # never accidentally serialized

def test_VALUE12_scalar_operators_produce_documented_result():
    v = lq.to_value(2)
    assert v + 3 == 5 and bool(lq.to_value(0)) is False and str(lq.to_value("x")) == "x"

def test_VALUE13_state_operations_preserve_or_discard_metadata(env):
    s = env.evaluate_state("hello/greet")
    assert s.metadata is not None
    assert isinstance(s.value, str)                   # .value is the safe explicit path
    # The design states whether derived states keep metadata; assert that choice:
    assert s.map(str.upper).metadata == s.metadata
```

### ENVIRON

```python
def test_ENVIRON01_default_environment_evaluates_builtin(env):
    # `commands_doc` is a real registered command, not a fixture: it needs no
    # store and no test registration, so it proves the *default* wiring works.
    assert env.evaluate("commands_doc") is not None

def test_ENVIRON02_custom_services_are_the_ones_returned(env):
    st = lq.MemoryStore()
    e = lq.Environment(store=st)
    assert e.store is st

def test_ENVIRON03_repeated_initialization_follows_policy():
    a, b = lq.Environment(), lq.Environment()
    assert a is not b                                 # or: assert a is b, if global

def test_ENVIRON04_failed_initialization_is_recoverable():
    with pytest.raises(lq.Error):
        lq.Environment(store=lq.Store.from_url("bogus://nowhere"))
    # A bare environment, so a built-in rather than a harness fixture command.
    assert lq.Environment().evaluate("commands_doc") is not None

def test_ENVIRON05_isolated_test_environments_do_not_leak_registration():
    a = lq.Environment()
    a.register_command("only_in_a", lambda: "a")   # a source command: the whole query
    with pytest.raises(lq.Error) as e:             # is the one command under test
        lq.Environment().evaluate("only_in_a")
    assert e.value.error_type == lq.ErrorType.ActionNotRegistered

def test_ENVIRON06_shutdown_is_idempotent(env):
    env.shutdown(); env.shutdown()                    # second call must not raise

def test_ENVIRON07_documented_operations_callable_in_every_state():
    # Two failures this catches, both of which leave every other ENVIRON test green.
    #
    # 1. A surface that is declared and not implemented. An environment wrapper exposing only
    #    a constructor is constructible and useless; nothing else here would notice.
    ops = ["evaluate", "get_asset", "describe_command", "command_names"]
    e = lq.Environment()
    for op in ops:
        assert callable(getattr(e, op, None)), f"{op} is documented but not callable"
    assert e.evaluate("hello") is not None
    # An operation the design deliberately does not support must REFUSE, with a message
    # naming the supported path — not be silently absent, and not silently do nothing.
    if not lq.SUPPORTS_INSTANCE_REGISTRATION:
        with pytest.raises(lq.Error) as err:
            e.register_command("x", lambda: 1)
        assert "register" in str(err.value)

    # 2. A state the lifecycle reaches in which the accessors do not work. The window between
    #    "initialized" and "first used" is the one that gets missed, because every test that
    #    evaluates first walks straight past it.
    lq.shutdown()
    lq.init()
    assert lq.is_initialized()
    g = lq.Environment.global_()          # must not raise here
    assert g.evaluate("hello") is not None
```

### EVAL

```python
def test_EVAL01_evaluate_builtin_query(env):
    assert env.evaluate("hello/greet") == "Hello, hello!"

def test_EVAL02_string_and_wrapped_query_agree(env):
    assert env.evaluate("hello/greet") == env.evaluate(lq.parse_query("hello/greet"))

def test_EVAL03_metadata_and_logs_available(env):
    s = env.evaluate_state("hello/greet")
    assert s.metadata.status is not None and isinstance(s.metadata.log, list)

def test_EVAL04_invalid_query_maps_through_error(env):
    with pytest.raises(lq.Error) as e:
        env.evaluate("///bad///")
    assert e.value.error_type == lq.ErrorType.ParseError

def test_EVAL05_payload_and_context_reach_a_command(env):
    @env.command
    def echo_payload(context):                     # source command: no state argument
        return context.payload["k"]
    assert env.evaluate("echo_payload", payload={"k": "v"}) == "v"

def test_EVAL06_cancellation_has_defined_terminal_result(env):
    # The design must NAME which terminal result its asset manager produces, and this test
    # asserts that one. Do not write `if cancelled ... else if finished ...`: an asset manager
    # that evaluates during `get_asset` reaches a terminal status before the caller can cancel,
    # so cancellation is inert and the accommodating form passes without checking anything
    # (WEB-CANCELLATION-INERT). Measure which branch actually runs, then assert it.
    h = env.evaluate_async("sleep-60")
    h.cancel()
    if lq.CANCELLATION_IS_EFFECTIVE:            # deferred asset manager
        with pytest.raises(lq.Error) as e:
            h.result()
        assert e.value.error_type == lq.ErrorType.Cancelled
    else:                                        # immediate asset manager: inert by design
        assert h.status() == "ready"             # terminal on arrival, unchanged by cancel
        assert h.result() is not None            # and an inert cancel damages nothing
```

### COMMAND

```python
def test_COMMAND01_register_and_execute_first_command(env):
    @env.command
    def constant() -> str: return "c"
    assert env.evaluate("constant") == "c"

def test_COMMAND02_transform_receives_state_and_parameter(env):
    @env.command
    def repeat(state: str, count: int = 2) -> str:
        return state * count
    assert env.evaluate("hello/repeat-3") == "hellohellohello"

def test_COMMAND03_exception_crosses_command_boundary(env):
    @env.command
    def boom(state): raise ValueError("inner")
    with pytest.raises(lq.Error) as e:
        env.evaluate("hello/boom")
    assert e.value.error_type == lq.ErrorType.ExecutionError

def test_COMMAND04_defaults_enums_and_variadics_bind(env):
    @env.command
    def f(state, mode: str = "a", *rest: int) -> str:
        return f"{mode}:{sum(rest)}"
    # The variadic must map to an ArgumentInfo with multiple=True, otherwise the
    # planner silently drops the extra parameters (PLAN-EXCESS-ACTION-PARAMETERS-DROPPED
    # in specs/issues/) and this asserts "b:1" instead of failing loudly.
    assert env.evaluate("hello/f") == "a:0"
    assert env.evaluate("hello/f-b-1-2") == "b:3"

def test_COMMAND05_metadata_matches_the_declaration(env):
    @env.command(label="Repeat", doc="Repeat the input.")
    def repeat(state: str, count: int = 2) -> str: ...
    md = env.commands.metadata("repeat")
    assert md.label == "Repeat" and md.doc.startswith("Repeat")
    assert [a.name for a in md.arguments] == ["count"]
    assert md.arguments[0].type == lq.ArgumentType.Integer

    # Include a command with NO arguments. Metadata serializers routinely skip empty
    # collections — right for a config file, wrong for an API — so `arguments` can be absent
    # rather than empty, and a caller iterating it breaks on exactly the commands that are
    # least likely to be special-cased.
    @env.command
    def nullary(): ...
    assert env.commands.metadata("nullary").arguments == []
    assert md.arguments[0].default == 2

def test_COMMAND06_duplicate_and_unregister_policy(env):
    # Includes registration AFTER the environment is already in use. An environment that is
    # frozen once shared makes this fail, and it fails only here — every other COMMAND test
    # registers before evaluating.
    env.evaluate("hello")
    env.register_command("late", lambda: "late")
    assert env.evaluate("late") == "late"
    env.register_command("dup", lambda state: state)
    with pytest.raises(lq.Error) as e:                 # or: assert replacement wins
        env.register_command("dup", lambda state: state)
    assert e.value.error_type == lq.ErrorType.CommandAlreadyRegistered
    env.unregister_command("dup")
    assert "dup" not in env.commands

def test_COMMAND07_context_injection(env):
    @env.command
    def logs(state, context):
        context.info("hello from command")
        return state
    s = env.evaluate_state("hello/logs")
    assert any("hello from command" in str(m) for m in s.metadata.log)

def test_COMMAND08_returned_opaque_value_follows_value_rules(env):
    # BOTH halves. A negative-only test — "an un-opted-in object is refused" — passes while
    # the opt-in itself has never once run, which is how a broken `opaque()` return path can
    # ship: the wrapper an explicit opt-in produces is itself an unrecognised object, and
    # structural conversion rejects it.
    class Unregistered: pass
    @env.command
    def unopted(): return Unregistered()
    with pytest.raises(lq.Error) as e:
        env.evaluate("unopted")
    assert e.value.error_type == lq.ErrorType.ConversionError

    sentinel = object()
    @env.command
    def give(): return lq.opaque(sentinel)         # source command, explicit opt-in
    assert env.evaluate("give") is sentinel

def test_COMMAND09_minimal_declaration_has_useful_metadata_defaults(env):
    @env.command
    def plain(state: str) -> str: return state
    md = env.commands.metadata("plain")
    assert md.name == "plain" and md.realm is not None and md.namespace is not None

def test_COMMAND10_complete_declaration_preserves_every_field(env):
    @env.command(label="L", doc="D", namespace="ns", realm="r",
                 filename="out.txt", volatile=True)
    def full(state): ...
    md = env.commands.metadata("full")
    assert (md.label, md.doc, md.namespace, md.realm, md.filename, md.volatile) \
        == ("L", "D", "ns", "r", "out.txt", True)

def test_COMMAND11_closure_captures_retained_per_runtime_rules(env):
    def make(n):
        @env.command(name=f"add{n}")
        def add(state: int) -> int: return state + n
    make(5)
    assert env.evaluate("number-1/add5") == 6      # `number` supplies the integer input

def test_COMMAND12_declared_planner_flags_take_effect(env):
    # COMMAND10 asserts the metadata FIELD holds what was declared. This asserts the planner
    # acts on it — the two come apart when a declaration property is parsed by nobody, which
    # leaves every command at the default and is invisible until a nondeterministic command
    # returns a stale result.
    calls = []
    @env.command(volatile=True)
    def tick():
        calls.append(1)
        return len(calls)
    assert env.commands.metadata("tick").volatile is True
    assert env.evaluate("tick") == 1
    assert env.evaluate("tick") == 2, "a volatile command must not be served from cache"

def test_COMMAND13_every_state_mode_delivers_its_content(env):
    # One case per state-passing mode the design offers. A mode that silently degrades to
    # another — metadata access falling back to the bare value — is not observable from any
    # other test, because the command still runs and still returns something.
    @env.command(state="value")
    def as_value(v): return type(v).__name__
    @env.command(state="text")
    def as_text(t): return t.upper()
    @env.command(state="state")
    def as_state(s): return f"{s.value}|{s.status}|{s.metadata is not None}|{len(s.log) >= 0}"

    assert env.evaluate("hello/as_text") == "HELLO"
    assert env.evaluate("hello/as_value") is not None
    # The state mode must carry metadata, status and log — not just the value again.
    assert env.evaluate("hello/as_state").count("|") == 3

def test_COMMAND14_retained_declaration_is_immune_to_caller_mutation(env):
    # Registration must capture what it was given, not alias it. Where the declaration is a
    # mutable host object — a dict, an object literal, a builder — retaining a reference means
    # a caller reusing a template silently rewrites an already-registered command, and any
    # later internal replay picks up the mutation.
    #
    # `NA` for a language whose declaration form is immutable (a Starlark struct); the
    # reversing condition is a mutable declaration form being accepted.
    decl = {"name": "snap", "run": lambda: "original"}
    env.register_command(decl)
    assert env.evaluate("snap") == "original"

    decl["run"] = lambda: "mutated"
    env.register_command({"name": "unrelated", "run": lambda: 1})   # may trigger a rebuild
    assert env.evaluate("snap") == "original"
```

### ASYNCQ

```python
async def test_ASYNCQ01_await_successful_evaluation(env):
    assert await env.evaluate_async("hello/greet") is not None

async def test_ASYNCQ02_failure_rejects_with_structured_error(env):
    with pytest.raises(lq.Error) as e:
        await env.evaluate_async("///bad///")
    assert e.value.error_type == lq.ErrorType.ParseError

async def test_ASYNCQ03_two_evaluations_make_progress(env):
    import asyncio
    a, b = await asyncio.gather(env.evaluate_async("hello/greet"),
                                env.evaluate_async("hello/greet"))
    assert a == b

async def test_ASYNCQ04_cancellation_propagates(env):
    h = env.evaluate_async("sleep-60")
    h.cancel()
    with pytest.raises(lq.Error) as e:
        await h
    assert e.value.error_type == lq.ErrorType.Cancelled

async def test_ASYNCQ05_dropping_host_handle_follows_policy(env):
    h = env.evaluate_async("sleep-60")
    del h                                              # detach or cancel — assert which
    assert env.live_handle_count() == 0

async def test_ASYNCQ06_no_event_loop_blocking(env):
    import asyncio, time
    ticks = 0
    async def ticker():
        nonlocal ticks
        while True:
            await asyncio.sleep(0.01); ticks += 1
    t = asyncio.create_task(ticker())
    await env.evaluate_async("sleep-1")            # short: this one runs to completion
    t.cancel()
    assert ticks > 0                                   # loop kept running throughout

def test_ASYNCQ07_documented_non_async_workaround_completes(env):
    # Only for a language with no async model; must be outside any event loop.
    assert env.evaluate_blocking("hello/greet") is not None

async def test_ASYNCQ08_nested_event_loop_use_is_rejected_or_safe(env):
    with pytest.raises(lq.Error):                      # or: assert it completes
        env.evaluate_blocking("hello/greet")           # called from inside the loop
```

### ASYNCCMD

```python
async def test_ASYNCCMD01_async_command_result(env):
    @env.command
    async def slow(state: str) -> str:
        await asyncio.sleep(0); return state.upper()
    assert await env.evaluate_async("hello/slow") == "HELLO"

async def test_ASYNCCMD02_async_exception(env):
    @env.command
    async def boom(state): raise ValueError("x")
    with pytest.raises(lq.Error) as e:
        await env.evaluate_async("hello/boom")
    assert e.value.error_type == lq.ErrorType.ExecutionError

async def test_ASYNCCMD03_cancellation_in_both_directions(env):
    @env.command
    async def hang(state):                         # not named `slow`: ASYNCCMD01 uses that
        await asyncio.sleep(60)
    h = env.evaluate_async("hello/hang"); h.cancel()
    with pytest.raises(lq.Error):
        await h

async def test_ASYNCCMD04_nested_async_evaluation(env):
    @env.command
    async def outer(state, context):
        return await context.evaluate_async("hello/greet")
    assert await env.evaluate_async("hello/outer") is not None

async def test_ASYNCCMD05_concurrent_calls_do_not_corrupt_state(env):
    @env.command
    async def idem(state: int) -> int:
        await asyncio.sleep(0); return state * 2
    results = await asyncio.gather(
        *[env.evaluate_async(f"number-{i}/idem") for i in range(50)])
    assert results == [i * 2 for i in range(50)]

def test_ASYNCCMD06_sync_and_async_metadata_differ(env):
    @env.command
    def s(state): ...
    @env.command
    async def a(state): ...
    assert env.commands.metadata("s").is_async is False
    assert env.commands.metadata("a").is_async is True
```

### STORE

```python
async def test_STORE01_set_get_data_and_metadata(store):
    k = lq.parse_key("d/f.txt")
    await store.set(k, b"data", lq.Metadata(media_type="text/plain"))
    assert await store.get(k) == b"data"
    assert (await store.get_metadata(k)).media_type == "text/plain"

async def test_STORE02_missing_key_error(store):
    with pytest.raises(lq.Error) as e:
        await store.get(lq.parse_key("d/absent"))
    assert e.value.error_type == lq.ErrorType.KeyNotFound

async def test_STORE03_directory_listing_invariants(store):
    await store.set(lq.parse_key("d/a"), b"1", lq.Metadata())
    await store.set(lq.parse_key("d/b"), b"2", lq.Metadata())
    names = await store.listdir(lq.parse_key("d"))
    assert set(names) == {"a", "b"}
    for n in names:
        assert await store.contains(lq.parse_key(f"d/{n}"))

async def test_STORE04_remove_and_removedir(store):
    k = lq.parse_key("d/x")
    await store.set(k, b"1", lq.Metadata())
    await store.remove(k)
    assert not await store.contains(k)
    await store.removedir(lq.parse_key("d"))
    assert not await store.contains(lq.parse_key("d"))

async def test_STORE05_unsupported_key(store):
    # A store requires an *absolute* key: no segment may be "." or "..". Relative keys are a
    # plan-level construct, resolved against a working directory while the plan is built; nothing
    # below that layer resolves them, so one arriving at a store is a malformed address. It is not
    # a parse error — lq.parse_key("../escape") succeeds and -R/../escape plans as an ordinary
    # GetAsset — so refusing it is the store's job.
    #
    # Five things this test has to get right, each of which a shipped store got wrong somewhere:
    #
    # 1. Check *every* segment, not the first. A guard that inspects only the leading segment
    #    passes on "../escape" and still lets "a/../../etc" through — which is the shape an
    #    attacker writes, and the only one that survives query-level CWD resolution.
    # 2. Refuse; do not normalize. A key is an address, not a path: quietly resolving "a/../b" to
    #    "b" makes two distinct addresses alias one asset, which is worse than rejecting a key
    #    nobody meant to write.
    # 3. Refuse on writes too, and refuse routing. A read-only guard leaves the write path open.
    # 4. Refuse on *direct* calls, not only through a router. is_supported gates routing, and only
    #    a router consults it, so a store guarded there alone passes a routed test and is wide open
    #    when held directly — which is how an environment is usually configured. Call the store
    #    directly here, never through a router.
    # 5. Assert the error *type*. KeyNotAbsolute says the address is malformed; KeyNotSupported
    #    says this store does not serve it (an empty segment, or an unrouted prefix). Asserting
    #    only that an error was raised conflates the two, and would also accept an error raised for
    #    an entirely unrelated reason — see the trap below.
    for text in ["../escape", "a/../../etc", "a/./b"]:
        k = lq.parse_key(text)
        with pytest.raises(lq.Error) as e:
            await store.get(k)
        assert e.value.error_type == lq.ErrorType.KeyNotAbsolute
        with pytest.raises(lq.Error):
            await store.set(k, b"x", lq.Metadata())
        assert not store.is_supported(k)

    # The negative half, in two parts. Without it, a store that refuses everything passes the
    # loop above — and a guard written as `segment.startswith(".")` or `".." in key` would too,
    # while breaking ordinary dotted filenames. Only the exact segments "." and ".." are relative.
    assert store.is_supported(lq.parse_key("d/ordinary.txt"))
    for text in ["d/.hidden", "d/a..b", "d/...", "d/..x", "d/v1.2.3"]:
        assert store.is_supported(lq.parse_key(text)), text

    # An empty segment is malformed rather than relative, so it keeps the store-scoped error.
    # Only reachable where keys are built programmatically; skip if the language cannot express it.

async def test_STORE05b_the_trap_that_makes_this_test_lie(store):
    # Read this before trusting a green STORE05 on a filesystem-backed store.
    #
    # An operating system resolves ".." by walking *real* directories. So "a/../../etc" against a
    # store with no directory named "a" fails with ENOENT — an error, from an unguarded store,
    # which a `pytest.raises(lq.Error)` assertion happily accepts. The test passes, the guard is
    # absent, and the deep-traversal case looks covered.
    #
    # Two corrections, both needed: create the intermediate directory so the traversal genuinely
    # resolves, and assert the error type so ENOENT (a read error) cannot pass for a refusal.
    await store.makedir(lq.parse_key("a"))
    k = lq.parse_key("a/../../etc")
    with pytest.raises(lq.Error) as e:
        await store.get(k)
    assert e.value.error_type == lq.ErrorType.KeyNotAbsolute

async def test_STORE06_concurrent_update_policy(store):
    k = lq.parse_key("d/race")
    await asyncio.gather(*[store.set(k, bytes([i]), lq.Metadata()) for i in range(10)])
    assert len(await store.get(k)) == 1              # last write wins, not a torn value

async def test_STORE07_store_works_in_end_to_end_evaluation(env, store):
    await store.set(lq.parse_key("d/in.txt"), b"hello", lq.Metadata())
    # Write -R/ explicitly: it is the canonical encoding, and without the /-/
    # separator the whole string would be read as one key.
    assert await env.evaluate_async("-R/d/in.txt/-/greet") == "Hello, hello!"

# --- STORE08-STORE13: for an integration that also provides its own stores (direction 2) and
# composes them from configuration (direction 3). See the disposition table in §5 for which apply
# to which selection; `NA` needs the direction stated as its reason, since selecting that
# direction later makes the tests required again.

async def test_STORE08_integration_provided_store_satisfies_the_contract(each_store):
    # Not a test body of its own: the assertion is that STORE01-STORE07 above are parameterised
    # over *every* store the integration ships, language-defined and integration-provided alike.
    # Its failure mode is a store added to the design and not to the parameter list, which a
    # reviewer catches and a runtime assertion cannot.
    ...

async def test_STORE09_read_only_store_refuses_writes(router, read_only_prefix, writable_prefix):
    with pytest.raises(lq.Error) as e:
        await router.set(lq.parse_key(f"{read_only_prefix}/new.txt"), b"x", lq.Metadata())
    assert e.value.error_type == lq.ErrorType.KeyNotSupported
    # The second assertion is the one that matters: a router that retried the next matching store
    # on failure would make a read-only prefix silently writable *somewhere else*, and the refusal
    # alone would not reveal it.
    assert not await router.contains(lq.parse_key(f"{writable_prefix}/new.txt"))
    # And the writable prefix still works, so the refusal was about the store, not the router.
    await router.set(lq.parse_key(f"{writable_prefix}/ok.txt"), b"y", lq.Metadata())

def test_STORE10_metadata_inference_follows_documented_precedence():
    # Express inference as a pure function over (key, backend hints) and this needs no backend.
    # The case that matters is *disagreement*: a name saying one thing and the backend another.
    assert infer_metadata(lq.parse_key("d/input.csv"), content_type="text/plain").media_type \
        == "text/csv"                       # the name wins, per the documented rule
    assert infer_metadata(lq.parse_key("d/blob"), content_type="application/json").media_type \
        == "application/json"               # the backend fills in when the name says nothing
    assert infer_metadata(lq.parse_key("d/blob"), content_type=None).media_type \
        == "application/octet-stream"       # an honest unknown, not an empty string

async def test_STORE11_configured_router_routes_by_prefix(build_router):
    # Built from a configuration *document*, not assembled in code — the document is the thing
    # under test, because that is what a deployment edits.
    router = build_router("""
    stores:
      - {type: memory, prefix: a}
      - {type: memory, prefix: b}
    """)
    await router.set(lq.parse_key("a/one.txt"), b"1", lq.Metadata())
    assert await router.contains(lq.parse_key("a/one.txt"))
    assert not await router.contains(lq.parse_key("b/one.txt"))     # separate stores
    with pytest.raises(lq.Error) as e:
        await router.get(lq.parse_key("zzz/nope.txt"))              # unmatched prefix
    assert e.value.error_type == lq.ErrorType.KeyNotFound

    # Listing a store's own prefix is the most ordinary call there is, and an off-by-one in the
    # "next segment of a store's prefix is a directory" rule makes it fail — it did.
    assert await router.listdir(lq.parse_key("a")) == ["one.txt"]
    assert sorted(await router.listdir(lq.parse_key(""))) == ["a", "b"]

    # Overlapping prefixes: first match in document order wins, so a broader prefix listed first
    # shadows a narrower one listed later. Assert the documented order, whichever it is.
    shadowed = build_router("""
    stores:
      - {type: memory, prefix: data}
      - {type: memory, prefix: data/scratch}
    """)
    await shadowed.set(lq.parse_key("data/scratch/x"), b"1", lq.Metadata())
    assert await shadowed.store_for(lq.parse_key("data/scratch/x")).key_prefix() \
        == lq.parse_key("data")

async def test_STORE12_integration_store_types_are_configurable(build_router, register_object):
    # An integration-provided backend is reachable from a document like any other type...
    router = build_router("""
    stores:
      - {type: hoststorage, prefix: local, config: {namespace: test}}
    """)
    await router.set(lq.parse_key("local/x.txt"), b"1", lq.Metadata())
    assert await router.get(lq.parse_key("local/x.txt")) == b"1"

    # ...a language-defined store is named rather than embedded, since no document can hold an
    # object, and an unregistered name fails when the configuration is applied, not at first use.
    with pytest.raises(lq.Error) as e:
        build_router("stores: [{type: language, prefix: m, config: {object: absent}}]")
    assert "absent" in str(e.value)

    # ...and where the design says a shared type name is overridden on this host, it resolves to
    # the integration's implementation. This is the load-bearing assertion: if contributed
    # factories were consulted *after* the built-ins, everything else here would still pass while
    # the shared name silently produced a backend that cannot run on this target.
    register_object.reset_calls()
    overridden = build_router("stores: [{type: http, prefix: web, config: {url_prefix: '/x/'}}]")
    assert register_object.calls == 1, "the integration's factory must be consulted first"
    assert overridden.store_for(lq.parse_key("web/a")).backend_name() == "host-http"

def test_STORE13_unavailable_store_type_names_the_reason(build_router):
    # Only meaningful in the reduced build the integration actually uses — mark it as running
    # there, do not mark it NA. A type that is real and merely gated off must not be reported as
    # unknown, or the reader goes looking for a typo.
    with pytest.raises(lq.Error) as e:
        build_router("stores: [{type: s3, prefix: remote}]")
    message = str(e.value)
    assert "opendal" in message or "not available" in message   # names feature or target
    assert "Unknown store type" not in message
```

### RECIPE

```python
# All keys below come from d/recipes.yaml in the harness section. A recipe key is
# the recipe's *filename segment* under the recipe folder, so the keys carry an
# extension: `hello/greet/known.txt` in folder d yields key d/known.txt. A query
# ending in `known` would have no filename segment and the recipe would be ad-hoc.

async def test_RECIPE01_found_and_missing_recipe(env):
    p = env.recipes
    assert await p.recipe_opt(lq.parse_key("d/known.txt")) is not None
    assert await p.recipe_opt(lq.parse_key("d/absent.txt")) is None   # not an error

async def test_RECIPE02_list_and_contains_are_consistent(env):
    p = env.recipes
    for k in await p.list(lq.parse_key("d")):
        assert await p.contains(k) and await p.recipe_opt(k) is not None

async def test_RECIPE03_recipe_produces_a_valid_plan(env):
    r = await env.recipes.recipe(lq.parse_key("d/known.txt"))
    # provider SetCwd[d] -> hello -> greet -> Filename[known.txt]
    assert r.cwd == "d"
    assert len(r.to_plan().steps) == 4

async def test_RECIPE04_provider_error_maps_through_error(env):
    env.recipes = lq.FailingRecipeProvider()
    with pytest.raises(lq.Error):
        await env.evaluate_async("-R/d/known.txt")

async def test_RECIPE05_volatility_and_expiration_metadata_survive(env):
    r = await env.recipes.recipe(lq.parse_key("d/volatile.txt"))
    assert r.volatile is True and r.expires is not None

async def test_RECIPE06_end_to_end_keyed_evaluation(env):
    # -R/ is required: `d/known.txt` alone parses as the action chain d()/known.txt.
    assert await env.evaluate_async("-R/d/known.txt") == "Hello, hello!"

async def test_RECIPE07_nested_environment_use_follows_policy(env):
    # A provider callback that evaluates a query must obey the RUNTIME reentrancy rules.
    assert await env.evaluate_async("-R/d/nested.txt", timeout=5) is not None
```

### MODULE

```python
async def test_MODULE01_module_loads_from_configured_prefix(env, store):
    await store.set(lq.parse_key("code/mymod.py"), b"VALUE = 42", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    assert env.import_module("mymod").VALUE == 42

async def test_MODULE02_module_outside_search_path_is_not_loaded(env, store):
    await store.set(lq.parse_key("other/secret.py"), b"VALUE = 1", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    with pytest.raises(lq.Error) as e:
        env.import_module("secret")
    assert e.value.error_type == lq.ErrorType.KeyNotFound

async def test_MODULE03_relative_import_resolves_against_cwd(env, store):
    await store.set(lq.parse_key("proj/helper.py"), b"VALUE = 7", lq.Metadata())
    await store.set(lq.parse_key("proj/recipes.yaml"),
                    PROJ_RECIPES_YAML, lq.Metadata())   # see the harness section
    # cwd is set automatically when recipes load from a folder (Recipe::cwd)
    @env.command
    def uses_helper(context):                    # source command; the recipe supplies it
        assert context.cwd_key == lq.parse_key("proj")
        return context.import_module("helper").VALUE
    # The recipe `uses_helper/value.txt` in proj/recipes.yaml gives key proj/value.txt.
    assert await env.evaluate_async("-R/proj/value.txt") == 7

async def test_MODULE04_absent_cwd_follows_policy(env):
    @env.command
    def no_cwd(context):
        assert context.cwd_key is None           # ad-hoc query: no recipe folder
        with pytest.raises(lq.Error):            # or: falls back to module_path only
            context.import_module("helper")
    await env.evaluate_async("no_cwd")

async def test_MODULE05_package_and_submodule_resolution(env, store):
    await store.set(lq.parse_key("code/pkg/__init__.py"), b"", lq.Metadata())
    await store.set(lq.parse_key("code/pkg/sub.py"), b"VALUE = 3", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    assert env.import_module("pkg.sub").VALUE == 3

async def test_MODULE06_native_module_takes_documented_precedence(env, store):
    await store.set(lq.parse_key("code/json.py"), b"VALUE = 'shadow'", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    import json as stdlib_json
    assert env.import_module("json") is stdlib_json     # stdlib must win by default

async def test_MODULE07_changed_module_bytes_follow_reload_policy(env, store):
    k = lq.parse_key("code/m.py")
    await store.set(k, b"VALUE = 1", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    assert env.import_module("m").VALUE == 1
    await store.set(k, b"VALUE = 2", lq.Metadata())
    # Assert the documented policy — automatic, explicit, or never:
    assert env.import_module("m", reload=True).VALUE == 2

async def test_MODULE08_module_key_is_a_dependency_and_expires_assets(env, store):
    k = lq.parse_key("code/m.py")
    await store.set(k, b"def f(x): return x * 2", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    # `uses_m` is a transform command calling m.f on its input; the design must
    # register it alongside the module fixture.
    first = await env.evaluate_async("number-2/uses_m")
    await store.set(k, b"def f(x): return x * 3", lq.Metadata())
    assert await env.evaluate_async("number-2/uses_m") != first  # stale must not persist

async def test_MODULE09_untrusted_prefix_is_refused(env, store):
    await store.set(lq.parse_key("untrusted/evil.py"), b"import os", lq.Metadata())
    env.module_path = [lq.parse_key("code")]           # default is empty, not the root
    with pytest.raises(lq.Error) as e:
        env.import_module("evil", search=lq.parse_key("untrusted"))
    assert e.value.error_type == lq.ErrorType.NotSupported

async def test_MODULE10_store_failure_maps_through_error(env):
    env.store = lq.FailingStore()
    env.module_path = [lq.parse_key("code")]
    with pytest.raises(lq.Error) as e:
        env.import_module("mymod")
    assert e.value.error_type in (lq.ErrorType.KeyReadError, lq.ErrorType.General)

async def test_MODULE11_import_inside_a_command_does_not_deadlock(env, store):
    await store.set(lq.parse_key("code/m.py"), b"VALUE = 1", lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    @env.command
    def importer(context):
        return context.import_module("m").VALUE     # sync import from an async worker
    assert await env.evaluate_async("importer", timeout=5) == 1

async def test_MODULE12_command_registered_by_store_module_is_executable(env, store):
    await store.set(lq.parse_key("code/cmds.py"),
                    b"def register(env):\n"
                    b"    env.register_command('shout', lambda s: s.upper())\n",
                    lq.Metadata())
    env.module_path = [lq.parse_key("code")]
    env.import_module("cmds").register(env)
    assert await env.evaluate_async("hello/shout") == "HELLO"
```

### UIUSE

```python
def test_UIUSE01_start_or_attach_to_existing_backend(env):
    ui = env.ui.start(backend="egui", background=True)
    assert ui.is_running()
    ui.shutdown()

def test_UIUSE02_render_or_inspect_representative_ui_value(env, ui):
    # Needs a tabular fixture in the store at data/table.csv.
    el = ui.open(env.evaluate_state("-R/data/table.csv"))
    assert el.element_type is not None

def test_UIUSE03_event_reaches_correct_command_and_context(env, ui):
    seen = []
    @env.command
    def on_click(context): seen.append(context.query); return "clicked"
    ui.dispatch(ui.open_query("on_click"), lq.UIEvent.click())
    assert len(seen) == 1

def test_UIUSE04_progress_and_status_ordering(env, ui):
    events = ui.subscribe_progress(env.evaluate_async("sleep-1"))
    seq = [e.fraction for e in events]
    assert seq == sorted(seq) and seq[-1] == 1.0

def test_UIUSE05_unsubscribe_releases_callbacks(env, ui):
    sub = ui.subscribe(lambda e: None)
    sub.dispose()
    assert ui.live_subscription_count() == 0

def test_UIUSE06_stale_update_is_ignored(env, ui):
    h = ui.open_query("sleep-60")
    ui.deliver(h, lq.UpdateMessage(generation=1))
    ui.deliver(h, lq.UpdateMessage(generation=0))     # older generation
    assert h.generation == 1

def test_UIUSE07_ui_error_maps_through_error(env, ui):
    with pytest.raises(lq.Error):
        ui.open_query("///bad///")

def test_UIUSE08_shutdown_obeys_thread_affinity(env, ui):
    import threading
    err = []
    threading.Thread(target=lambda: err.append(ui.shutdown_from_wrong_thread())).start_and_join()
    assert isinstance(err[0], lq.Error)                # refused, not undefined behaviour
```

### UIDEF

```python
def test_UIDEF01_register_and_render_language_defined_element(env, ui):
    @env.ui_element("gauge")
    class Gauge:
        def render(self, ctx): return ctx.text(f"{self.value}%")
    assert ui.render_element("gauge", {"value": 50}).contains_text("50%")

def test_UIDEF02_properties_and_state_roundtrip(env, ui):
    el = ui.create_element("gauge", {"value": 10})
    el.set_property("value", 20)
    assert el.get_property("value") == 20

def test_UIDEF03_event_invokes_correct_callback(env, ui):
    calls = []
    @env.ui_element("btn")
    class Btn:
        def on_event(self, e): calls.append(e)
    ui.dispatch(ui.create_element("btn", {}), lq.UIEvent.click())
    assert len(calls) == 1

@pytest.mark.parametrize("backend", ["egui", "web"])
def test_UIDEF04_element_works_in_each_claimed_backend(env, backend):
    ui = env.ui.start(backend=backend, background=True)
    assert ui.render_element("gauge", {"value": 1}) is not None
    ui.shutdown()

def test_UIDEF05_unknown_element_has_fallback_or_error(ui):
    r = ui.render_element("no_such_type", {})
    assert r.is_fallback() or isinstance(r, lq.Error)

def test_UIDEF06_dispose_releases_callbacks(env, ui):
    el = ui.create_element("btn", {})
    el.dispose()
    assert ui.live_callback_count() == 0

def test_UIDEF07_minimal_language_backend_renders_standard_element(env):
    backend = lq.TestRendererBackend()                # implemented in the language
    ui = env.ui.start(backend=backend)
    assert ui.render_element("text", {"text": "hi"}).contains_text("hi")

def test_UIDEF08_async_updates_respect_ui_thread_affinity(env, ui):
    h = ui.open_query("sleep-1")
    ui.await_settled(h)
    assert ui.all_updates_applied_on_ui_thread()
```

### POLYGLOT

```python
def test_POLYGLOT01_language_a_command_feeds_language_b(env):
    env.register_command("a_upper", lambda s: s.upper())            # Python
    env.eval_starlark("def b_wrap(s): return '[' + s + ']'")        # Starlark
    assert env.evaluate("hello/a_upper/b_wrap") == "[HELLO]"

def test_POLYGLOT02_bytes_and_metadata_preserve_type(env):
    env.eval_starlark("def give_bytes(): return b'\\x00\\xff'")
    s = env.evaluate_state("give_bytes")
    assert isinstance(s.value, bytes) and s.metadata.media_type is not None

def test_POLYGLOT03_opaque_transfer_is_rejected_or_encoded(env):
    obj = object()
    env.register_command("give_opaque", lambda: obj)          # source command
    env.eval_starlark("def starlark_consume(v): return str(v)")
    with pytest.raises(lq.Error) as e:
        env.evaluate("give_opaque/starlark_consume")
    assert e.value.error_type == lq.ErrorType.ConversionError

def test_POLYGLOT04_errors_retain_origin(env):
    env.eval_starlark("def boom(): fail('from starlark')")
    with pytest.raises(lq.Error) as e:
        env.evaluate("boom")
    assert e.value.origin_runtime == "starlark"

def test_POLYGLOT05_cross_runtime_nested_call_does_not_deadlock(env):
    # A Python source command that evaluates a Starlark command which evaluates
    # back into Python; the design must register the whole cycle.
    assert env.evaluate("py_calls_starlark_calls_py", timeout=5) is not None

def test_POLYGLOT06_name_collision_policy(env):
    env.register_command("dup", lambda s: s)
    with pytest.raises(lq.Error):                     # or: language-qualified realms
        env.eval_starlark("def dup(s): return s")

def test_POLYGLOT07_shutdown_releases_both_runtimes(env):
    env.shutdown()
    assert env.live_runtime_count() == 0

def test_POLYGLOT08_embedded_command_works_through_outer_integration(env):
    # A Starlark command owned by liquers-lib, reached through the Python integration.
    env.enable_starlark()
    env.eval_starlark("def sl(s): return s + '!'")
    assert env.evaluate("hello/sl") == "hello!"

def test_POLYGLOT09_outer_cancellation_reaches_embedded_runtime(env):
    h = env.evaluate_async("slow_starlark")        # a long-running Starlark source command
    h.cancel()
    with pytest.raises(lq.Error) as e:
        h.result()
    assert e.value.error_type == lq.ErrorType.Cancelled
```

### WEBSERV

Route prefixes follow the `liquers-axum` builders and the default base path of
[WEB_API_SPECIFICATION.md](../reference/WEB_API_SPECIFICATION.md) §2.1: queries are served under
`/liquer/q/{*query}`, the store under `/liquer/api/store/{data,metadata}/{*key}`, and assets under
`/liquer/api/assets/{data,metadata}/{*query}`. A design that configures different base paths
substitutes them here.

```python
def test_WEBSERV01_start_on_ephemeral_port_and_reach_readiness(env):
    srv = env.serve(port=0, background=True)
    srv.wait_ready(timeout=5)
    assert srv.port != 0
    srv.shutdown()

def test_WEBSERV02_standard_route_uses_configured_environment(env, srv):
    env.register_command("marker", lambda: "from-this-env")     # source command
    assert http_get(f"{srv.url}/liquer/q/marker").text == "from-this-env"

def test_WEBSERV03_startup_error_maps_through_error(env, srv):
    with pytest.raises(lq.Error):
        env.serve(port=srv.port, background=True)     # port already bound

def test_WEBSERV04_graceful_shutdown(env, srv):
    srv.shutdown(graceful=True)
    assert srv.inflight_requests() == 0 and not srv.is_running()

def test_WEBSERV05_background_handle_owns_server_lifetime(env):
    srv = env.serve(port=0, background=True)
    del srv                                           # assert the documented policy
    # either the server stops with the handle, or it is explicitly detached

def test_WEBSERV06_language_handler_receives_documented_types(env, srv):
    @env.route("/custom")
    def handler(request):
        assert isinstance(request.headers, dict)
        return lq.Response(200, b"ok", media_type="text/plain")
    assert http_get(f"{srv.url}/custom").text == "ok"

def test_WEBSERV07_handler_exception_becomes_safe_http_error(env, srv):
    @env.route("/boom")
    def handler(request): raise ValueError("secret internal detail")
    r = http_get(f"{srv.url}/boom")
    assert r.status == 500 and "secret internal detail" not in r.text

def test_WEBSERV08_concurrent_handlers_do_not_block_runtime(env, srv):
    rs = parallel_get([f"{srv.url}/liquer/q/hello/greet"] * 20, timeout=5)
    assert all(r.status == 200 for r in rs)
```

### WEBAPI

```python
def test_WEBAPI01_framework_neutral_query_handler_success(env):
    resp = lq.handlers.query(env, lq.Request(path="/liquer/q/hello/greet"))
    assert resp.status == 200

def test_WEBAPI02_error_maps_to_specified_http_response(env):
    resp = lq.handlers.query(env, lq.Request(path="/liquer/q///bad///"))
    assert resp.status == 400 and resp.json()["error_type"] == "ParseError"

def test_WEBAPI03_media_type_and_metadata_survive(env):
    # Needs data/sales.csv in the store and the polars command group; any query
    # whose result carries a non-default media type serves the same purpose.
    resp = lq.handlers.query(
        env, lq.Request(path="/liquer/q/-R/data/sales.csv/-/ns-pl/head-10/preview.csv"))
    assert resp.headers["content-type"].startswith("text/csv")

async def test_WEBAPI04_asgi_adapter(env):
    app = lq.asgi_app(env)
    assert (await asgi_get(app, "/liquer/q/hello/greet")).status == 200

def test_WEBAPI05_wsgi_adapter_does_not_create_runtime_per_request(env):
    app = lq.wsgi_app(env)
    before = lq.runtime_count()
    for _ in range(20):
        wsgi_get(app, "/liquer/q/hello/greet")
    assert lq.runtime_count() == before

async def test_WEBAPI06_disconnect_cancellation_propagates(env):
    app = lq.asgi_app(env)
    task = asgi_get(app, "/liquer/q/sleep-60")
    task.disconnect()
    assert env.last_evaluation_status() == lq.Status.Cancelled

def test_WEBAPI07_large_streaming_response_follows_memory_limits(env):
    resp = lq.handlers.data(env, lq.Request(path="/liquer/api/store/data/big.bin"))
    assert resp.is_streaming and peak_memory_during(resp.consume) < LIMIT

def test_WEBAPI08_authentication_context_reaches_evaluation(env):
    @env.command
    def whoami(context): return context.user.name      # source command
    resp = lq.handlers.query(
        env, lq.Request(path="/liquer/q/whoami", user=lq.User("ada")))
    assert resp.body == b"ada"

def test_WEBAPI09_route_behavior_matches_liquers_axum(env, srv):
    for path in ["/liquer/q/hello/greet",
                 "/liquer/api/store/data/d/f.txt",
                 "/liquer/api/store/metadata/d/f.txt"]:
        neutral = lq.handlers.dispatch(env, lq.Request(path=path))
        axum = http_get(srv.url + path)
        assert (neutral.status, neutral.headers["content-type"]) \
            == (axum.status, axum.headers["content-type"])
```

### STUBS

```python
def test_STUBS01_declarations_exist_for_every_exposed_module():
    for mod in lq.exposed_modules():
        assert declaration_file_for(mod).exists()

def test_STUBS02_type_checker_accepts_representative_sample():
    assert run_type_checker("tests/typing/sample_usage.py").exit_code == 0

def test_STUBS03_declared_names_match_runtime_surface():
    for mod in lq.exposed_modules():
        assert set(declared_names(mod)) == set(runtime_public_names(mod))

def test_STUBS04_command_decorator_preserves_signature():
    # A decorator returning an untyped callable erases every user command's signature.
    src = """
    import liquers as lq
    env = lq.Environment()
    @env.command
    def repeat(state: str, count: int = 2) -> str: return state * count
    reveal_type(repeat)   # must reveal (str, int) -> str, not (*args) -> Any
    """
    assert "(str, int) -> str" in run_type_checker_reveal(src)

def test_STUBS05_async_entry_points_declared_awaitable():
    assert is_declared_awaitable("liquers.Environment.evaluate_async")

def test_STUBS06_type_checker_rejects_incorrect_usage():
    src = "import liquers as lq; lq.parse_query(42)"
    assert run_type_checker_src(src).exit_code != 0

def test_STUBS07_declarations_ship_in_the_artifact():
    names = artifact_namelist(built_artifact())
    assert any(n.endswith(".pyi") for n in names)
    assert "liquers/py.typed" in names            # without this, stubs are ignored
```

### PACKAGE

```python
@pytest.mark.parametrize("target", CLAIMED_PLATFORMS)
def test_PACKAGE01_clean_build_produces_artifact(target):
    assert build_artifact(target).exists()

def test_PACKAGE02_install_into_clean_environment_loads():
    with clean_virtualenv() as venv:
        venv.install(built_artifact())
        assert venv.run("import liquers; print(liquers.__version__)").exit_code == 0

def test_PACKAGE03_quickstart_evaluates_a_query_end_to_end():
    with clean_virtualenv() as venv:
        venv.install(built_artifact())
        r = venv.run(read_quickstart_snippet("README.md"))
        assert r.exit_code == 0 and r.stdout.strip() != ""

def test_PACKAGE04_version_metadata_matches_linked_core():
    assert lq.__core_version__ == cargo_locked_version("liquers-core")

def test_PACKAGE05_default_feature_set_produces_documented_value_type():
    assert lq.value_type_name() == DOCUMENTED_DEFAULT_VALUE_TYPE

@pytest.mark.parametrize("extra", DECLARED_EXTRAS)     # e.g. "polars", "ui", "web"
def test_PACKAGE06_optional_extra_installs_and_activates(extra):
    with clean_virtualenv() as venv:
        venv.install(f"{built_artifact()}[{extra}]")
        assert venv.run(f"import liquers; liquers.require('{extra}')").exit_code == 0

def test_PACKAGE07_artifact_carries_declarations_license_and_metadata():
    names = artifact_namelist(built_artifact())
    assert any("LICENSE" in n for n in names)
    assert artifact_metadata(built_artifact())["requires_python"] is not None
```

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-05 | ENVIRON now requires language-visible builder validation reports before environment publication, including severity, message, command identity, and a preflight test. | `design/variadic-metadata-tail-check` |
| 2026-09-02 | §3's requirement levels and implementation states moved to `reference/CONFORMANCE_TERMS.md`, so the store implementation guide shares one definition rather than copying it. The `NA` discipline and its language-specific examples stay here. §STORE's direction-2 questions now cross-link `guides/STORE_IMPLEMENTATION_GUIDE.md` instead of answering them twice. | `design/store-conformance-suite/` Phase 4 step 14 |
| 2026-09-01 | Repaired current design, reference, and archive links so the tracked-document link check can validate this guide. | `DOCS-DEAD-LINKS-OUTSIDE-README` |
| 2026-08-31 | §VALUE shows `EnvironmentBuilder::with_type_registry` alongside `new_with_type_registry`, and records that an integration defining its own `Environment` carries the readiness obligation in `init_with_envref`. Links to the new construction guide. | `design/environment-builder/phase-5` |
| 2026-08-30 | Declaration links now resolve to `reference/COMMAND_DECLARATION.md`, the format having landed. | `design/command-declaration/` |
| 2026-08-30 | §COMMAND points at the shared [Command Declaration Format](../reference/COMMAND_DECLARATION.md) instead of leaving each *integration* to invent a declaration vocabulary: the key-to-`CommandMetadata` mapping, composition over introspection, the defaulting rules, and `hints` for language-specific facts. Adds an **Examples** note — a worked example that restates every argument misrepresents the format. Listed in §8. A section on writing *language* documentation and user guides is still missing; filed as `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`. | `design/command-declaration/` |
| 2026-08-29 | §STORE "Taking only part of the store support crate": **recommendation reversed.** The project took option 2 — configuration, the `StoreFactory` seam and `StoreRouterBuilder` moved to `liquers-core`, so an integration needs no store crate for them and `liquers-web` dropped its dependency. Records why the original rejection did not survive (the consumer was `liquers-core` itself, not one integration), keeps option 3 as still correct for the *backends*, and restates the extension-seam rule: there are no built-in types and factories chain first-wins, so overriding a shared type name means chaining earlier. `STORE12`'s override clause restated in terms of chain order, with an `NA` condition. Added §"What the *language* cannot contribute: a store *type*" — the seam is for the integration, not the language, and a page can supply a store instance but not a named type with declared arguments. | `design/store-factories-in-core/` |
| 2026-08-26 | §VALUE "Typing an integrated value": registration is no longer an open problem. Records the one-identifier-per-variant rule for the foreign container, the extend-and-freeze registration recipe with its four traps, the constant-plus-test guarantee, and that a value type you define yourself needs none of it. | `design/foreign-value-type-registration/` |
| 2026-08-18 | Added the VALUE convention that a language value is converted to a native variant when that is possible and not too expensive, so an integration normally defines only the foreign container; added type-identifier naming and registration guidance pointing at the type system, and the two rules that keep a bridge ready for automatic conversion. | `design/value-type-system/` |
| 2026-08-17 | `STORE05` follows the absolute-key rule: a relative key is `KeyNotAbsolute`, not `KeyNotSupported`; the store must be called directly rather than through a router; dotted-but-ordinary names are explicit negatives; and `STORE05b` records the ENOENT trap that lets a filesystem-backed `STORE05` pass with no guard present. | `design/store-key-guard/` |
| 2026-08-14 | Every parameter value is now encodable, so the encode-direction guidance drops the refusal path; OBJECT09 becomes a round-trip test. Added the numeric and named entities to the escaping summary. | PARAMETER-ESCAPING-INCOMPLETE |
| 2026-08-11 | Documented provider-owned and programmatic recipe CWD setup, interpreter-owned relative resolution, root fallback, and executable integration evidence. | phase-5 |
| 2026-08-09 | Reviewed against a completed `STORE` integration. Added a `BLOCKED` implementation state; harness question 7 (one test dragging the whole suite into the heavy harness) and "let the harness shape the design"; the mutation check for high-risk assertions; a shared "Service adapters — two rules" section; the `ERROR` question of whether the *language* can *construct* an error, plus `ERROR06`; `STORE`'s second direction (integration-provided stores and configuration) with `STORE08`–`STORE11`; and blueprint improvements to `VALUE04` (byte corpus), `STORE05` and `ERROR05`. | `design/liquers-web-store/` |
| 2026-08-09 | `STORE` gained composition and configuration as an explicit third direction: router semantics an *integration* inherits (`is_supported` defaulting to false, no fall-through on refusal, overlap order, whether a store sees the stripped key), configuration questions (naming an object a document cannot hold, re-application, variable substitution), and "Taking only part of the store support crate" — the enable/disable design choice, why an optional default-on feature beats duplicating or relocating the configuration types, and its three costs. Added `STORE12`/`STORE13` and a disposition table by selected direction. | `design/liquers-web-store/` |
| 2026-08-08 | Last substantive edit, carried into `reference/` unchanged. Not reviewed against the implementation since. | migration |
