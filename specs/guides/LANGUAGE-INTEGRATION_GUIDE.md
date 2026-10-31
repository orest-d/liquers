---
title: Language Integration Guide
kind: guide
audience: internal
area: [web, py, core/commands, core/plan, core/assets, core/store, core/value, lib/commands]
reviewed: 2026-10-30
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

`liquers-py` implements every Essential feature of the Python baseline profile, designed in [`design/liquers-py/`](../design/liquers-py/DESIGN.md); all seven are `COMPLETE` with every required check passing, so the profile is `CONFORMANT` (matrix in `liquers-py/README.md`). The guide stays the definition: where the two differ, the guide wins and the difference is an issue. Its predecessor's design, [`design/python-wrapper/`](../design/python-wrapper/phase1-high-level-design.md), is superseded.

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
- *Data node*: a value seen as a read-only tree of scalars, lists and maps through core's `DataNode` / `AsyncDataNode` traits (`liquers-core/src/data_node.rs`), read in place rather than converted.
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
   one: in `liquers-web` the `VALUE` and `ASYNCQ` suites and `RUNTIME02`-`06` are pure
   ECMAScript and run under Node with no browser and no WebDriver; `RUNTIME01` is a native test in
   `liquers-lib`; `PACKAGE01` and `PACKAGE05` are script checks (`check-stubs.sh`). Only the suites
   that need a real browser API (`localStorage`) and the Playwright delivery-form checks
   (`PACKAGE02`-`04`, `PACKAGE07`) need a real browser. Classify per test, not per feature. Python has
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
| `DATANODE` | Language data as `DataNode` | Optional | `VALUE`, `ERROR`, `RUNTIME`, `ASYNCQ` (soft) |
| `TEMPLATE` | Templates: template commands and programmatic rendering | Optional | `ENVIRON`, `EVAL`, `VALUE`, `ERROR`, `RUNTIME`, `COMMAND` (soft), `DATANODE` (soft), `ASYNCQ` (soft) |
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
| `DOCS` | Documentation for the integration's users | Optional | `ENVIRON`, `EVAL`, `COMMAND`, `PACKAGE` (soft), `STUBS` (soft) |

Dependencies are *hard* unless marked `(soft)`. The soft cases are those where Liquers, not the *integrated language*, can supply the asynchrony or the callable:

- `UIUSE` → `ASYNCQ`: a *UI backend* may deliver asset status, progress, and events through callbacks scheduled onto the *language runtime*, so a *language* with no async model can still drive a UI. Without `ASYNCQ`, the design must state how progress and cancellation are delivered.
- `WEBSERV` → `ASYNCQ`: the Axum server owns a Rust async runtime, so starting and shutting it down does not require language-visible awaitables. Without `ASYNCQ`, the design must state how foreground/background start, readiness, and shutdown are expressed.
- `WEBSERV` → `COMMAND`: needed only to attach *language*-implemented handlers or middleware. Serving the standard Rust routes requires no *language command*.
- `WEBAPI` → `ASYNCQ`: an ASGI-style adapter awaits Liquers directly, but a WSGI-style synchronous adapter needs only a controlled runtime bridge. Without `ASYNCQ`, the design must state which adapters are supported and how blocking is bounded.
- `MODULE` → `ASYNCQ`: module bytes may be prefetched from the store before entering the *language runtime*, so a synchronous import hook is workable without language-visible awaitables. Without `ASYNCQ`, the design must state how store reads are performed during resolution.
- `MODULE` → `STORE`: modules are read through the environment's store, which need not be a *language*-defined one. `STORE` matters only when a *language*-defined store is also expected to serve modules.
- `STUBS` → `ASYNCQ`: entry points need awaitable declarations only where `ASYNCQ` is selected.
- `PACKAGE` → `STUBS`: declarations must ship inside the artifact when `STUBS` is selected; packaging is otherwise independent of them.
- `DOCS` → `PACKAGE`: the user guide's installation section and the published API reference describe the artifact `PACKAGE` produces. Without it, the guide documents building from the repository.
- `DOCS` → `STUBS`: where the *language* has declaration files, the API reference is generated from them. Without `STUBS`, it is generated from the *language*'s docstrings or equivalent.
- `DATANODE` → `ASYNCQ`: only the outbound view of an *async* node needs awaitables. Without `ASYNCQ`, the design must state that such a node is refused, not blocked on.
- `TEMPLATE` → `COMMAND`: needed to declare template commands and to render from inside a *language command*. Without it, the design offers top-level rendering only and states that template commands are registered from Rust.
- `TEMPLATE` → `DATANODE`: lets a native structure be template data in place. Without it, data is structurally converted to the *value type* first, and the design must state the cost and what conversion refuses (§DATANODE explains why that is more than a copy).
- `TEMPLATE` → `ASYNCQ`: a render evaluates queries, so in a *host* where `ASYNCQ` is Profile (the browser) the programmatic render is awaitable. Without `ASYNCQ`, the design must state how a render waits, as for `EVAL`.

Recommended minimum profiles:

- **General baseline**: `OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND`
- **Browser JavaScript**: baseline plus `ASYNCQ`; `STORE`, `RECIPE`, `DATANODE`, `TEMPLATE`, `UIUSE`, `UIDEF`, and web features are selected as needed
- **Starlark baseline**: baseline with a deliberately restricted `VALUE`; `ASYNCQ` and `ASYNCCMD` may initially be `NA`
- **Python baseline**: general baseline; later milestones should add `ASYNCQ`, `ASYNCCMD`, `STORE`, `RECIPE`, `DATANODE`, `TEMPLATE`, and selected UI/web features

`STUBS`, `PACKAGE` and `DOCS` are delivery features rather than capabilities: they describe how the *integration* is declared to tooling, shipped and explained to users, not what it can do. Any *integration* consumed outside this repository should select `PACKAGE` and `DOCS`, and `STUBS` wherever the *integrated language* has a stub format at all.

**Out of scope: cache.** There is deliberately no cache feature. The legacy `liquers_core::cache` module (`BinCache`, `Cache`) was removed, with `liquers-py`'s wrapper of it — assets provide caching, as noted in [PROJECT_OVERVIEW.md](../reference/PROJECT_OVERVIEW.md). An *integration* should not reintroduce a separate result cache. This is a scope decision for the guide as a whole, so it does not need a per-design `NA` entry.

**No separate ID for configured stores.** A mount's access policy and metadata handling
(`read_only`, `metadata`) belong to `STORE` direction 3, composition and configuration. They have
no feature ID of their own: a separate ID would let a design select configuration without the store
contract it depends on.

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

**Meaningful tests:** `OBJECT01` query parse/encode roundtrip; `OBJECT02` key equality/hash; `OBJECT03` command metadata roundtrip; `OBJECT04` invalid parse produces `ERROR`; `OBJECT05` *wrapper* remains valid for its documented lifetime; `OBJECT06` every selected enum variant roundtrips; `OBJECT07` unknown enum variant follows the compatibility policy; `OBJECT08` all *wrappers* follow the documented naming and ownership conventions; `OBJECT09` any host string supplied as a parameter round-trips through query text: encoded, then parsed by the real parser, it yields the same value.

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

**Map the categories too.** `ErrorType::categories()` groups types into overlapping
`ErrorCategory` sets (`Store`, `KeyedAsset`, `NotFound`; `KeyNotFound` is in `Store` and
`NotFound`). A language with typed exceptions gives each category a base class and derives each
type's class from all of its categories' classes (Python: `StoreError`, `KeyedAssetError`,
`NotFoundError`); one with a single error class exposes the names (JavaScript:
`LiquersError.categories`, `["store", "not_found"]`). Generate both from the enums, so a new type
or category needs no binding change.

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
4. Are conversions strict, permissive, or configurable? How are integer range, NaN/infinity, cycles, shared identity, mutation, and unknown objects handled? For NaN/infinity, whatever crosses into JSON must follow Liquers' policy (`liquers_core::value::json_f64`; see [`SERIALIZATION_GUIDE.md`](SERIALIZATION_GUIDE.md)): `try_into_json_value` refuses, and the JSON byte path writes `"NaN"` / `"Infinity"` / `"-Infinity"` where the value is known to be a float.
5. Can an *opaque language value* cross stores, threads, processes, Wasm boundaries, or another *language runtime*? Which codecs are supported and trusted?
6. Does the *integrated language* support function pointers, bound methods, closures, or callable objects? May they be retained as *opaque language values*? Are they serializable? Are they accepted as *language commands* through `COMMAND`, even when they are not ordinary data values?
7. Can users operate conveniently on a *value type* *wrapper* or *State* as if it were a language scalar? Which conversions are implicit versus explicit? Can arithmetic, comparison, indexing, iteration, truthiness, and display operators be defined without hiding conversion errors or metadata loss?

#### A third category: values whose *buffers* are shared

Conversion and opacity are not the only two options. A **columnar value** — a record batch, in the
sense of `design/record-streams/` — is a third case: its data is a set of primitive buffers laid out
so that the *integrated language* can read them **in place**, with no conversion and no copy.

This is not an *opaque language value* (the language reads the contents directly), and it is not
*structural conversion* (nothing is rebuilt). It is a lending of memory, and it brings obligations
the other two do not have:

1. **Reads only.** The buffers are immutable to Rust and shared behind an `Arc`. A language writing
   through them violates the aliasing assumptions the owning value relies on.
2. **A view may be invalidated without being freed.** In Wasm, growing the linear memory **detaches**
   every existing `ArrayBuffer` view while leaving the Rust allocation exactly where it was. A
   borrowed view is therefore valid only until the next call into Wasm, and an integration must
   either re-create views at point of use or revalidate them — comparing the view's buffer against
   the module's current `memory.buffer` is an O(1) identity check.
3. **A handle owns the lifetime.** The *wrapper* retains the value, and every borrowed view must not
   outlive it. Releasing the handle while a view is live is use-after-free with no diagnostic.
4. **An always-correct copy must be offered.** A caller that needs a value outliving the handle, or
   that cannot reason about (2), takes a copy. Zero-copy is the fast path, never the only path.

**The design must also answer:** does the *integrated language* have a native representation for the
lent buffers — JavaScript typed arrays, Python's buffer protocol, the Arrow C Data Interface — and is
the layout compatible enough to hand over, or must it be converted after all?

**Meaningful tests:** a borrowed view reads the same values as a copy; a view survives an operation
that grows the host heap (or fails with a clear error rather than silently reading the wrong memory);
releasing the handle releases the underlying value, observed through a live handle count.

#### RECORDS — the columnar record types, and the two ways to bridge them

`design/record-streams/` defines a tabular value type in its own crate, `liquers-records`, which
depends on `liquers-core` only; `liquers-lib` adds the value variants and the `ns-rec` commands
behind its `records` feature. An integration may depend on `liquers-records` alone — implementing
its `RecordValue` adapter for its own value type — or reach the records through `liquers-lib`. It
must answer one question before any other.

**The question: a native Arrow hand-off, or wrappers around the record types?**

| | Arrow hand-off | Wrappers |
|---|---|---|
| What the user gets | *their* ecosystem's object — a `pyarrow.Table`, a `pandas.DataFrame` | a Liquers `RecordView` with Liquers methods |
| Data movement | zero-copy through the C Data Interface | zero-copy to read; a conversion to leave |
| What is preserved | columns and logical types | columns, **field roles, chunk origin, provenance, the manifest** |
| Cost | an FFI implementation with `unsafe`; the binding's Arrow package becomes a dependency | ordinary wrapper code |
| Fails when | the *language* has no mature Arrow binding | the user wants their own dataframe and must convert |

**Neither answer is "instead of" the other, and an integration that picks only one is usually wrong.**
The wrapper is the bridge, because it is the only thing that can carry the concepts Arrow has no
place for — which field is the identity, where a row came from, what the chunk depends on. The Arrow
export is then a *method on that wrapper* (`.to_arrow()`, `.to_pandas()`), for the moment the user
leaves Liquers and enters their own ecosystem.

Two things worth knowing before choosing:

- **Arrow schemas carry custom key-value metadata**, so roles and origin *can* ride along into the
  hand-off. The consumer must know to read them, so this preserves the information without
  preserving the convenience.
- **The Arrow route's cost is concentrated and one-off.** Most of it is the C Data Interface
  implementation, which is per-integration but not per-type, and the only `unsafe` in the bridge.

**Recommended default:** wrappers first, so the *language* has a complete and honest view of the
data; Arrow export where a mature binding exists, as a documented exit. Python is the clear case for
both — pyarrow is ubiquitous, and the C Data Interface is what it is designed to consume.

**The record design draws the same line in its own types.** A table is a `RecordView` trait object;
the one implementation that *holds* Arrow buffers is `RecordBatch`. So the two answers attach to two
kinds of object:

| Crossing | Route | Cost |
|---|---|---|
| a `RecordBatch` | Arrow hand-off, or lent buffers | zero-copy |
| any other view — a projection, a filter, a derived or generated table | a wrapper over the trait: `len`, `schema`, `column`, `value`, the view constructors | nothing until read; each read produces a column |
| any view, when Arrow is wanted | `materialize()` first, then as a batch | free for a batch; a copy of the selected rows or a computation otherwise |
| a source | **as its manifest only**; its rows through `materialize`, which is asynchronous and bounded | the whole source in memory |

**What `liquers-web` does — lent buffers, not Arrow.** The browser binding has no Arrow route; a
batch crosses as the lent-buffer handle of §"A third category". A single-cell `RecordView` reaches
JavaScript as its scalar (a null cell as `null`, a binary cell as a `Uint8Array`); any larger view
is materialized (free for a batch) and wrapped in `LiquersRecordBatch`, exported to JavaScript as
`RecordBatch` (`liquers-web/src/records.rs`): `numRows`, `numColumns`, `schemaJson`, `column(i)`
returning a **descriptor** (`kind`, `ptr`, `len`, `validity`, plus `dim` for vectors and
`offsets`/`data` for text and binary) from which the caller builds a typed-array view over
`memory.buffer`, `columnCopy(i)` as the always-safe detached copy, and `columnView(i)` building the
`RecordColumn` companion (`records_column.js`) that re-creates a view detached by `memory.grow`.
The handle holds the batch by `Arc`, so buffers stay valid until `free()`. A `RecordSource` crosses
unmaterialized as a `RecordSource` handle: `chunks()` lists `{kind, query}` without evaluating
anything (route 1), `stream()` resolves to an async iterable yielding one `RecordBatch` per view,
and `materialize(maxRows)` resolves to one batch; traversal evaluates chunks in the web
environment, resolved at each call. Both handle kinds carry a registry id
(`__liquersRecordHandle`), so a handle a JavaScript command returns crosses back as the same `Arc`
(`ExtValue::RecordView` / `RecordSource`), and a freed one is refused by name. A table a page
builds itself is not recognized at the boundary: it crosses as plain data, and the query says
`ns-rec/from_json-records`. The `rec` commands are registered in the web environment. All of this is behind
`liquers-web`'s `records` feature, which is off by default because it adds about 4 MB to a release
module; a binding should weigh the same cost before making records part of its default build. `RECORDS01`,
`02`, `04`-`08` and `10` run in `liquers-web/tests/records_RECORDS.rs`; `03`, `09` and `11` are
`NA` (no Arrow route; JavaScript has an async model; JavaScript cannot implement the traits).

A single-cell view reads as a scalar (`record-streams` Phase 2, "A view as a value"), so a wrapper
should expose that as the *language*'s own scalar rather than as a one-by-one table.

**A *language* may also implement the traits.** With `RecordView` and `RecordSource` as interfaces, a
language-defined table — a Python object, a JavaScript generator — can be a Liquers value, not just a
consumer of one. The rule that decides which: **a `RecordView` is synchronous**, because scalar
reading and argument binding are. A *language* callback answering `column_range` must therefore
return without awaiting; asynchronous *language* code implements a `RecordSource` instead, whose
stream may await. A language-defined implementation also carries `ForeignValue`'s concerns — the
thread bounds on native, and the handle's lifetime.

**The type inventory an integration maps.** Everything the record design defines, so a binding can be
checked for completeness rather than assembled by inspection:

| Kind | Types |
|---|---|
| **Values** (cross a query boundary) | `ExtValue::RecordView` holding `Arc<dyn RecordView>`, `ExtValue::RecordSource` holding `Arc<dyn RecordSource>` |
| **Traits** | `RecordView` (sync, random access), `RecordViewMut` (a table being built or edited), `RecordSource` (async, re-openable), `RecordStream` (one traversal), `ChunkResolver` (how a source evaluates), `RecordValue` (the adapter a value type implements to carry records) |
| **Structs — data** | `RecordBatch` (the materialized view), `RecordBatchMut` and `ColumnMut` (the mutable table and column; `with_capacity`, `append_row`, `freeze`), `RecordSchema`, `FieldSchema`, `FieldRole`, `RowId` (the implicit id: chunk index and row) |
| **Structs — reference implementations** | `ManifestSource`, `InMemorySource`; the views `ColumnsView`, `RowRangeView`, `RowIndexView`, `DerivedColumnView`, `AppendedColumnsView`, `RowFnView`; `ContextResolver`, `EnvResolver` |
| **Structs — identity and provenance** | `ChunkOrigin`, `LocatorRule`, `ChunkDescriptor` |
| **Structs and enums — manifest** | `ManifestSpec` (a manifest's parsed form), `ManifestKind`, `ChunkTemplate`, `ChunkNaming`; `ChunkValue` (what a resolver hands back: a view, a source, or bytes) |
| **Enums — identity** | `ChunkId` — `Query(..)` for an unkeyed stream, `Key(..)` for a keyed one |
| **Structs — memory** | `Bitmap`, `AlignedBuffer`, `Buffer<T>` |
| **Enums** | `Column`, `FieldValue`, `FieldType`, `KeyRole`, `IndexKind`, `Analyzer`, `VectorMetric`, `CompareOp`, `ChunkList<'a>` |
| **Type alias, functions, extension trait** | `BoxRecordStream`; `record_stream`; `to_record`, `to_record_source` (what a record command accepts: views, sources, bytes, text, JSON, keys); `RecordStreamExt` (`materialize`) |

A minimal binding maps the two values as wrappers over their traits, `RecordBatch` for Arrow,
`RecordSchema`/`FieldSchema`/`FieldType`, `FieldValue` and `ChunkOrigin`. The individual view
structs need not be bridged: a *language* reaches them through the constructors on the view wrapper
(`select_columns`, `filter`, `slice`, `row`, `cell`), and sees only `RecordView`. `Bitmap`, `AlignedBuffer` and `Buffer<T>` are implementation detail
and should **not** be exposed: a language sees columns, not buffers, except through the lent-buffer
mechanism below.

**The design must answer:**

1. Arrow hand-off, wrappers, or both — and if Arrow, does the binding's Arrow package become a hard
   dependency or an optional extra?
2. Do field roles and chunk origin survive an Arrow export, through schema metadata or not at all?
3. Which of the inventory above is exposed, and which is deliberately hidden?
4. How does a *language* without an async model traverse a stream — see below.
5. May the *language* implement `RecordView` or `RecordSource` itself — and if so, how does a
   `RecordView` callback stay synchronous?

#### Traversing a record stream from a language with no async model

`RecordSource::stream()` is async, and `STORE`/`ASYNCQ` already establish that a *language* without
an async model cannot await. Three routes, in the order an integration should prefer them:

1. **Iterate the chunk list, not the stream.** `RecordSource::chunks()` returns chunk ids — queries,
   or keys for a keyed stream — which are plain data. A sync *language* iterates those and evaluates one at a time through whatever
   synchronous evaluation `EVAL` already provides. **This is the preferred route**: it needs no new
   mechanism, it keeps one chunk resident, and it maps naturally onto the *language*'s own iterator
   protocol — Python's `__iter__`/`__next__`, a Starlark iterable. Available whenever the source is
   manifest-backed, which is the normal case for a stored stream.
2. **Block on the stream.** Drive the future to completion on the host runtime and expose
   `next_batch()`. Correct natively; **impossible in Wasm**, where blocking is not available — so an
   integration offering it must say where it does not work, and `RUNTIME`'s portability rules apply.
3. **Materialize, then iterate.** Collect the whole source into one view (`RecordSource::materialize`,
   or the `ns-rec/materialize` command, both taking a `max_rows` limit) and hand it back. Simple, and it **defeats the purpose of the design** for
   anything large. Acceptable only with a documented size bound — which `max_rows` enforces.

Route 1 works for a `ChunkList::Known`. For `ChunkList::Unbounded` the chunk list is not enumerable
in advance, so a sync *language* must either use route 2 or walk the template's chunks until one
comes back short — which is route 1 with the termination condition made explicit.

An async *language* exposes the stream directly (`__aiter__`/`__anext__` or the equivalent) and
should do so **in addition to** route 1, not instead of it: iterating chunks by key is what lets a
caller resume, parallelize, or fetch one chunk out of order.

**Meaningful tests:** `RECORDS01` a view round-trips through the *language* with schema, field roles
and chunk origin intact; `RECORDS02` a column reads the same values through the wrapper as through a
copy; `RECORDS03` where an Arrow export exists, the exported table equals the wrapper's data, and the
documented metadata survives or its loss is asserted; `RECORDS04` a lent buffer is read-only, or
writing through it is rejected; `RECORDS05` a borrowed view survives an operation that grows the host
heap, or fails with a clear error rather than reading the wrong memory; `RECORDS06` releasing a batch
handle releases the underlying value, observed through a live handle count; `RECORDS07` a sync
*language* traverses a manifest-backed source one chunk at a time without materializing the whole
stream; `RECORDS08` an async *language* traverses the same source through its async iterator and
yields identical rows; `RECORDS09` `NA` unless the *language* has no async model — the documented
sync route is present and its limitation (route 2 in Wasm) is stated; `RECORDS10` a single-cell view
reaches the *language* as its own scalar, and a larger view refuses that conversion with an error
naming its shape; `RECORDS11` `NA` unless the *language* may implement the traits — a
language-defined view gives the same rows through `column`, `value` and `materialize`, and a
`column_range` callback that tries to await is rejected rather than blocking.

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
   environment (as `liquers-web` does on command registration and unregistration, store
   configuration, and store-object registration while a configuration is installed, each once the
   environment is shared), put the registration in the
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

**Only an integration that defines its own value type skips this.** If your language handle is a
variant of a value type *you* define, its identifier is statically knowable and belongs in your
`type_descriptions()`. Both shipped integrations take the constructor route instead: `liquers-web`
for `js.Value`, and `liquers-py`, which holds a Python object as `PyObjectValue` (`py.Object`) in
`liquers-lib`'s `ExtValue::Foreign` and registers its `TypeInfo` when an environment is built
(`liquers-py/src/value.rs` `py_type_info`).

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

**Store configuration compiles against the registry.** An environment built from a store configuration with `metadata` sections compiles their chains against the environment's own command registry, so a *language* registration that such a configuration names must happen before the environment, or its store, is built. (Only Rust code registers `store_config` entries today; see §STORE "Configured stores". `liquers-web` builds the store with `build_store` from the environment's registry, and its rebuild path re-applies the store after replaying the retained registrations.)

**Validation reports are part of the integration boundary.** Before publishing an environment,
an integration using `EnvironmentBuilder` must make the builder's `validate()` result available to
the host language. It may wrap `IssueReport` directly or map each `Issue` into a language-native
diagnostic, but it must preserve severity and message and preserve command identity for
command-registry issues. The normal language-visible build/initialization operation may raise the
compact error summary; a GUI, editor, or custom logger needs the complete report before that
consuming operation. On wasm, the default full-report emission goes to the browser console, but
it does not replace this programmatic access. See
[Building and Configuring an Environment](./ENVIRONMENT_CONSTRUCTION_GUIDE.md#validating-imported-command-metadata).

An integration that does not use `EnvironmentBuilder` owes the same report on its own import
path: run `CommandMetadataRegistry::check()` over the metadata it imports (a registry holding the
imported commands is enough, since the check is per command), add its own intake refusals to the
same `IssueReport`, and hand the whole report to the language before the consuming call raises.
For a declaration, `CommandDeclaration::check` gives the full report whose summary `build` raises.
`liquers-web` does this in `validateCommands` / `registerCommands`: a batch of declarations or a
JSON/YAML registry document, registered all or nothing, with the report on the thrown error.

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

**The design must answer:** What is the most natural, small, and readable declaration in this *integrated language*: a decorator/annotation, function call, builder, module scan, callable object, or data object containing a function and metadata? Show minimal and complete examples. How do signature, annotations, defaults, variadics, documentation, namespace, realm, volatility, and async status map to metadata? May a *language command* be declared in a foreign realm, such as `store_config`, whose actions are addressable in a query but not executable by the executor (see [Realms](../reference/REALMS.md))? Which fields require explicit metadata? How is callable identity retained? Which state-passing modes exist? Is registration reversible?

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

### DATANODE — Language data as `DataNode`

**Contract.** Let core read a *language value* as a *data node* in place, without converting it to
the *value type* first, and let the *language* read a Liquers value's *data node* without
converting the whole value. `DataNode` is part of `liquers-core` (`liquers-core/src/data_node.rs`),
not of the template crate: the template renderer is its first consumer, store handlers are
designed to reach record fields through it (`design/store-handler-field-access/`, in review), and
any later consumer that walks data reads it the same way. This feature therefore stands on its own; `TEMPLATE` uses it when both
are selected, and nothing in it requires templates.

A *data node* answers five questions — its kind, its scalar, a named field, its items in order, and
itself as JSON — and nothing else. There are two traits and a holder:

| Type | For | Rules an adapter must keep |
|---|---|---|
| `DataNode` | data in memory | **Synchronous**: every method returns at once. Children are `NodeRef`s, owned (`NodeRef::Owned`) when the adapter builds them |
| `AsyncDataNode` | data read on demand (a record source, a store folder) | `kind` is known without I/O; everything else is awaited |
| `AnyNode` | either, behind an `Arc` | What every consumer takes. A sync consumer refuses an async node with `not_supported` rather than blocking |

Two directions, and a selected `DATANODE` includes the first:

1. **Inbound — an adapter.** A Rust `DataNode` implemented over a *language value*: a Python
   `dict`/`list`/`tuple`, a JavaScript object/`Array`/`Map`, a Starlark `dict`/`list`/`struct`.
   This is how a native structure becomes template data (`TEMPLATE`) with no copy, and how a
   retained *opaque language value* stops being opaque to core.
2. **Outbound — a view.** A *language*-visible read-only wrapper over a Liquers value's `AnyNode`,
   so a *language command* can read one field of a record view or a polars frame without
   converting the frame. Optional within the feature; mark `DATANODE11` `NA` with a reversing
   condition if the design leaves it out.

**Why the adapter is needed even where structural conversion exists.** Conversion is
all-or-nothing. `liquers-py` converts a `dict` whose keys are all `str` and whose members all
convert; one non-`str` key, or one member such as a `datetime`, makes the *whole* `dict` a
`py.Object`, which is opaque, so a template renders it as nothing. An adapter answers for every part
it can read and makes only the unreadable member opaque. It also avoids copying a large structure
that a template reads three fields of.

**Objects/API to map or implement:**

- `DataNode` adapter type(s) over the *language*'s mapping, sequence and scalar types, holding an
  owned reference (`Py<PyAny>`, `JsValue`, a frozen Starlark value) and building child adapters on
  request
- `NodeKind`, `Scalar` (including `Scalar::UInt` for an integer above `i64::MAX`) and `NodeRef` as
  the adapter's answers; `index` and `len` overridden where the sequence has random access
- `DataNode::to_json` over the same mapping, for a consumer that needs a snapshot (an override, an
  apply input that is not a value)
- **The hook that lets a retained foreign value answer**: `ForeignValue::data_node`, delegated from
  `ExtValue`'s `data_node` for the `Foreign` variant. **It does not exist yet** — today the
  `Foreign` arm returns `AnyNode::opaque()` (`liquers-lib/src/value/mod.rs`). It is an additive
  default method (opaque by default), filed as `FOREIGN-VALUE-HAS-NO-DATA-NODE`; until it lands,
  an adapter is reachable only where the *integration* hands an `AnyNode` to the consumer itself
  (the programmatic render of `TEMPLATE`)
- An opt-in protocol for the *language*'s other objects (a class whose instances should read as
  maps), if the design offers one
- For the outbound view: a *wrapper* over `AnyNode` with kind, scalar, field, item, length,
  iteration and `to_json`, mapped onto the *language*'s indexing and iteration operators; awaitable
  accessors for an async node where `ASYNCQ` is selected

**The design must answer:**

1. What is the mapping from each *language* type to `NodeKind`? Give it as a table beside `VALUE`'s
   conversion table, and say where the two differ.
2. Which objects other than plain containers read as maps — none, an opt-in protocol, or a fixed
   list such as dataclasses — and does reading one run *language* code?
3. Is the adapter a **live view** or a **snapshot**? A render awaits between field reads (a query
   tag runs in between), so the *language* can mutate the structure mid-render.
4. How is a cycle handled — by lazy navigation, and by `to_json`?
5. On which thread, and under which lock, does a `DataNode` method run, and how is that kept from
   deadlocking against the *language* thread that started the render?
6. Is the outbound view offered, and how does it reach a child without borrowing from its parent?

**Issues and patterns.**

**The adapter and structural conversion must agree.** A template must read the same data whether
the `dict` was adapted in place or converted to `Value::Object` first: the same kinds, the same
scalars, the same refusals. Write the adapter's scalar mapping from `VALUE`'s, not from scratch.
The checks that matter are the ones a fresh mapping gets wrong: a Python `bool` is an `int`
subclass and must be tested first, or `True` reads as `1`; a JavaScript `number` is `Integer` only
when `Number.isSafeInteger`, else `Float`; an integer above `i64::MAX` follows `VALUE`'s range
policy — `Scalar::UInt` where `VALUE` accepts it, the same conversion error where `VALUE` refuses it
(`liquers-py` extracts `i64` and refuses `2**63` today) — and so does one above `u64::MAX`; bytes are `Opaque`, never `Text`,
for the reason `VALUE04` gives; a non-finite float is a `to_json` error, as in
`try_into_json_value`. **Key order is the one sanctioned difference**: `keys()` is "in the node's
own order", so an adapter uses the *language*'s order (Python insertion order, JavaScript's
integer-keys-first property order), while `Value::Object` is a `BTreeMap` and sorts. Mustache does
not iterate a map's keys, so a template never sees the difference; JSON output and a store handler
listing fields do.

**Children are owned.** `NodeRef::Borrowed` borrows from the parent; an adapter has no Rust child
to borrow, so it returns `NodeRef::Owned(Box::new(ChildAdapter { .. }))` holding a new owned
reference to the child *language value*, and `Scalar::Text(Cow::Owned(..))` for text. Building a
child costs one reference-count increment, not a conversion, which is what makes reading three
fields of a large structure cheap.

**Read data, do not run code.** A lookup that runs *language* code — a JavaScript getter or
`Proxy` trap, a Python `property` or `__getattr__`, a `__getitem__` on a `dict` subclass — runs it
from inside the renderer, at a moment the author cannot see, and possibly reentrantly (`RUNTIME`).
Recommended: plain containers are read through their data (Python `PyDict`/`PyList` access, not
the mapping protocol; JavaScript own enumerable data properties via
`Object.getOwnPropertyDescriptor`, skipping accessors and symbols), and any object that runs code
to answer is either opaque or behind the opt-in protocol of question 2, where the author has asked
for it.

**Live, and say so.** A snapshot adapter is structural conversion under another name; the reason
to adapt is to avoid the copy. So the expected answer to question 3 is "live": each lookup reads
the structure as it is at that moment, and a structure mutated mid-render yields a render that
mixes old and new fields. Document it, and let a caller who needs consistency pass a copy.

**Cycles are cheap to navigate and must not overflow to JSON.** Lazy navigation of a structure
that contains itself is finite, because a template names a finite path; `to_json` walks
everything, so it must track the path (as `liquers-py`'s `py_to_simple` does with object
identities) and fail with a typed conversion error, not a stack overflow.

**Thread bounds and locks.** `DataNode: MaybeSend + MaybeSync`, so on native the adapter is
`Send + Sync` and may be called on any Tokio worker that polls the render: `Py<PyAny>` satisfies
that, and each method takes the interpreter lock for itself (`Python::attach`). That is safe only
because the *language* thread that started the render **released the lock while it waits** — the
rule `liquers-py` already follows for every wait (§6 Python, *The GIL*). Taking the lock per lookup
costs a lock round trip per field; measure it on a large render before optimizing. On `wasm32` the
bounds are vacuous and everything runs on the one thread: a lookup happens synchronously inside a
poll, so no other JavaScript runs during it, but other microtasks do run between two lookups — the
live-view caveat again. A Starlark adapter holds only frozen values; an unfrozen `Value<'v>` cannot
outlive its heap and must be converted.

**The outbound view cannot hold a `NodeRef`.** A child `NodeRef<'a>` borrows from its parent, so a
*language wrapper* — which must be `'static` — cannot keep one. Two shapes work: the wrapper holds
the root `AnyNode` plus a path and re-resolves on each access (`O(depth)` per access; simplest), or
it holds a child only when the child is `NodeRef::Shared` or an `AnyNode`, and returns scalars as
*language values* directly. An async node's accessors are awaitables (`ASYNCQ`); a synchronous
accessor on one raises the core `not_supported` error through `ERROR` rather than blocking.

**Meaningful tests:** `DATANODE01` a nested native mapping and sequence answer kind, field, items,
length and keys per the mapping table; `DATANODE02` the adapter agrees with structural conversion
over a corpus, key order excepted; `DATANODE03` scalar edges — `bool` is not an integer, `i64`
limits, above `i64::MAX` follows `VALUE`'s range policy, a non-finite float fails `to_json`; `DATANODE04` bytes are
opaque, not text; `DATANODE05` a structure structural conversion refuses still answers for its
readable parts; `DATANODE06` a cyclic structure navigates lazily and `to_json` fails with a typed
error; `DATANODE07` a lookup runs no *language* code outside the documented protocol; `DATANODE08`
a retained foreign value reads in place through `ValueInterface::data_node`, not as opaque;
`DATANODE09` a mutation between two lookups follows the documented live/snapshot policy;
`DATANODE10` a lookup from a Liquers worker while the *language* thread waits on an evaluation
completes — **conditional: only for a *language runtime* with a global lock or a thread-affine
heap reachable from several threads**, `NA` on single-threaded `wasm32` until values cross to a
worker; `DATANODE11` the outbound view reads one field of a large value without converting the
whole value, and an async node is awaited or refused.

```python
def test_DATANODE05_partial_structure_still_reads(env):
    d = {"name": "Ann", "when": datetime.now()}   # conversion makes the whole dict py.Object
    assert lq.data_node(d)["name"] == "Ann"       # the adapter still reads `name`
```

### TEMPLATE — Templates: template commands and programmatic rendering

**Contract.** Make Liquers' query-aware Mustache templates
([`reference/TEMPLATE_LANGUAGE.md`](../reference/TEMPLATE_LANGUAGE.md),
[`TEMPLATE_GUIDE.md`](TEMPLATE_GUIDE.md)) usable from the *integrated language* in three ways:

1. **The `ns-template` commands** (`render`, `expand`, `validate`) are in the environment's
   built-in command set, so queries and pipelines can use them.
2. **Template commands** can be declared from the *language*: a command whose behaviour is a
   template, with no callable — an alias of `template/render` whose head parameter is the template
   link.
3. **Programmatic rendering**: the *language* calls `render`, `expand` and `validate` directly,
   with the template as text or a link and the data as a native structure — **both from the top
   level, through the environment, and from inside a *language command*, through its context**.

The environment is not optional: a template's query tags (`{{$ q}}`), apply tags (`{{$. q}}`) and
partials (`{{> key}}`) evaluate queries and read the store. A template that uses data only can be
rendered by `MapHost`, which refuses queries; that is a test convenience, not this feature.

**Objects/API to map or implement:**

- The `ns-template` commands in the built-in set: `liquers_lib::register_template_commands!`, which
  needs `liquers-lib`'s `templates` feature (`liquers-py` registers them; `liquers-web` does not
  yet — `WEB-HAS-NO-TEMPLATE-COMMANDS`)
- Template-command declaration over the shared declaration format: a declaration with
  `definition: Alias` of `template/render` and the template link as head parameter
  ([`COMMAND_ALIASES.md`](../reference/COMMAND_ALIASES.md)), registered all or nothing like any
  other declaration (`COMMAND`, `ENVIRON`'s validation report)
- `render`, `expand` and `validate` functions on **two receivers with one signature**: the
  environment handle (top level) and the *language*-visible command context (inside a command)
- The arguments those functions take: the template (text, or a key/query naming it), the state
  (`render` only), the data, `errors` (`strict`, `expand`, `expand_html`, `expand_md`,
  `silent`, as `liquers_template::ErrorMode::parse` reads them), `cwd`, and HTML escaping
- `liquers_template::{parse_template, render, ContextStack, FrameKind, RenderOptions, ErrorMode}`,
  `validate_template` and `Diagnostic`, and `liquers_lib::template::ContextHost` (with
  `ContextHost::node_of` for a state's frame) as the Rust side of the in-command path
- Data frames from `DATANODE` adapters (`AnyNode::Sync(Arc::new(adapter))`) where `DATANODE` is
  selected; otherwise from structural conversion through `ValueInterface::data_node`
- Recovered errors (`expand`, `silent`) as warnings: in the command's log inside a command, and
  returned or inspectable at the top level
- Diagnostics from `validate` as *language values* with severity, message and position

**The design must answer:**

1. What is the programmatic entry point's shape, and which receivers have it? (Recommended: the
   same `render` / `expand` / `validate` on the environment and on the context, so code moves
   between a script and a command unchanged.)
2. How is a template passed: as text, as a link, or both, and how does a caller tell them apart?
3. How do the state and `data` map to the context stack, and is that the commands' layout exactly?
4. Inside a command, does rendering go through the command's context, so its queries become
   dependencies of the command's result? At the top level, which asset does the render run in?
5. How are recovered errors and warnings returned at the top level, where there is no command log?
6. How is HTML escaping chosen, given there is no result filename to derive it from?
7. Is rendering awaitable, blocking, or both, and how does it obey `RUNTIME`'s reentrancy rules
   when the template runs a *language command* that itself renders?
8. How is a template command declared, and what does metadata inspection show for it?

**Issues and patterns.**

**The programmatic call is the command, called directly.** Keep the commands' frame layout, so a
programmatic call and the equivalent query produce the same text from the same inputs, which is
what `TEMPLATE05` asserts:

| Call | Bottom frame | Top frame | `.`, `{{$. q}}` |
|---|---|---|---|
| `render(template, state, data)` | the state (`Data`), unless empty | `data` (`Names`), unless empty | the state |
| `expand(template, data)` | none | `data` (`Data`) | `data` |

`data` must be a map or empty, as in the commands. The usual script call — "fill this template
from this `dict`" — is `expand(template, data)`; `render(template, state)` is "apply this
template to a value", the programmatic form of `…/ns-template/render-~X~-R/templates/x.md~E`. Do
not invent a third layout for convenience; a caller moving a template from a script into a
template command would then get different output.

**Inside a command, render through the command's `Context`.** Build
`ContextHost::new(context.clone(), cwd)`, push frames, and call `liquers_template::render` —
exactly what `liquers_lib::template::commands::render` does, but with the frames supplied by the
*integration*: `host.node_of(&state)` for a state, an adapter or `Value::data_node` for the data.
Through `ContextHost`, every query tag is evaluated as a dependency of the **calling command's**
asset (or applied ad hoc, with its dependencies passed on), partials are read as dependencies,
volatility passes on, and recovered errors become warnings in the command's log. A *language
command* that renders is therefore invalidated when anything its template read changes, with no
declaration. This needs the *language*-visible context of `COMMAND07`; an *integration* that
passes none cannot offer the in-command half.

**The working directory comes in as an argument.** A relative partial (`{{> ./header.md}}`) and a
relative key in a query tag resolve against `cwd`, but a command cannot read its working directory
from its `Context` (`Context::get_cwd_key` is crate-private, by design): it receives it as a
default link argument, `cwd: Key = query "-R-key/."`
([`COMMAND_REGISTRATION_GUIDE.md`](COMMAND_REGISTRATION_GUIDE.md)). A *language command* that
renders relative templates declares that argument and passes it on; `context.render` without a
`cwd` resolves against the root, and the design must say so rather than guess a directory.

**At the top level, render inside an ad-hoc asset; never write a host that bypasses one.** There is
no `Context` outside a command, and a host that evaluates query tags by calling the store or the
executor directly skips the asset lifecycle — caching, dependencies, volatility, cancellation and
the log — which `EVAL` forbids. The route that needs no new Rust surface is an ad-hoc `apply` of the
render command: `AssetManager::apply(recipe, state, payload)` with the recipe
`ns-template/render-~X~-R/templates/greet.md~E` (a link) or with the template text as an encoded
parameter, applied to a state holding the data. `render` with the data as its state lays out the
same frame as `expand` with that data, `_meta` aside. The ad-hoc result is never cached or shared
(`Context::apply` explains why), its log carries the recovered errors, and a trailing filename
(`…/page.html`) turns HTML escaping on. Two costs: the template text travels inside the query, and
the data must be a `Value` — structurally converted, or a retained foreign value read through
`ForeignValue::data_node` once that exists. A direct helper that renders frames in an ad-hoc asset
would remove both; it is filed as `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`.

**One environment, the caller's.** Both receivers already know their environment — the handle is
one, the context belongs to one — so neither function takes an environment argument, and neither
resolves through a global singleton when the caller has a handle (the rule `liquers-web` follows
for handles that evaluate later).

**Choose HTML escaping explicitly.** The commands turn it on when the result's filename is `.html`
or `.htm`. A programmatic call has no filename, so it takes an explicit flag (default off) and
maps it onto `RenderOptions::html_escape`; the top-level route may express it as the trailing
filename. Never infer it from the template's own key: a `.md` template rendered into an HTML page
is ordinary.

**Errors.** In `strict` mode a template error raises through `ERROR` with the template position
(`line:column`) intact — the error's position, not one in the evaluated query's text
(`TEMPLATE_LANGUAGE.md` §6). A syntax error fails in every mode, at parse time. In a recovering
mode the render returns text and the warnings stay reachable: in the command's log inside a
command, and on a result object or a returned list at the top level. Silently dropping them is the
failure mode, because `silent` output with no warnings looks like a template that worked.

**Reentrancy.** A template can run a *language command* (`{{$ my_command}}`) that itself renders.
Inside a command the render is a nested evaluation and follows `RUNTIME`'s reentrancy policy; in
Python, a synchronous `context.render` waits the way `context.evaluate` does (`block_on` with the
interpreter lock released), which is also what lets a `DATANODE` adapter take the lock from a
worker. In the browser the call returns a `Promise` (`ASYNCQ`); never offer a synchronous render
there.

**Template commands are declarations, not callables.** A template command from the *language* is
the alias of [`TEMPLATE_GUIDE.md`](TEMPLATE_GUIDE.md) §3 written in the declaration format: no
function, the template link as head parameter, the arguments after it lining up with `render`'s
(`data`, `errors`, `cwd` with its default link `-R-key/.`). Because the head is a link, the
template is a dependency of every result, so editing it in the store invalidates them. Offer a
one-call form (`env.template_command(name, template=…)`) over the general declaration path rather
than a second registration mechanism, and show the alias in metadata inspection, so an author can
see the command has no callable. Prefer a store key for the template; inline text works as a head
parameter too, but cannot be edited without re-registering.

**Validate before rendering.** `validate` lists every invalid tag and unknown command, positioned,
and runs nothing; it takes the environment's command metadata registry, so it is available on both
receivers and needs no asset. An editor or a test can call it on a template a script is about to
render.

**Meaningful tests:** `TEMPLATE01` the `ns-template` commands are in the default environment's
built-in set; `TEMPLATE02` a template command declared from the *language* renders, and its
metadata shows an alias with no callable; `TEMPLATE03` editing a template command's template
invalidates its result; `TEMPLATE04` top-level `expand` over a native mapping, with a query tag,
renders through the environment; `TEMPLATE05` a programmatic `render` and `expand` produce the same
text as the equivalent command query; `TEMPLATE06` a render inside a *language command* makes the
template's queries dependencies of the command's result; `TEMPLATE07` `strict` raises through
`ERROR` with the template position, and a recovering mode returns text with the warnings
reachable; `TEMPLATE08` HTML escaping follows the explicit flag; `TEMPLATE09` `validate` returns
positioned diagnostics as *language values* and runs nothing; `TEMPLATE10` a template that runs a
*language command* which itself renders completes without deadlock; `TEMPLATE11` a relative
partial resolves against the `cwd` passed in — inside a command, the command's own `-R-key/.`
argument.

```python
def test_TEMPLATE04_top_level_expand_over_a_dict(env):
    assert env.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"}) == "Ann has 42"
```

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
   built from a declarative document rather than assembled in code, together with each mount's
   *access policy and metadata handling* (`read_only`, `metadata`). Both apply to every mount, a
   language-defined one included.

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
- The optional **data-only methods** of the language store protocol, where a `metadata` section
  may wrap a language-defined store: `get_data`, `set_data`, `remove_data`, `rename_data`,
  `listdir_data`, `data_facts` and `get_stored_metadata` on the Rust side. They read and write the
  data object alone and never touch metadata
- `DataCapabilities`, `DataFacts`, `BackendTime` and `TimePrecision` (`liquers-core/src/store.rs`,
  `liquers-core/src/metadata.rs`)
- `AsyncStore::is_read_only` and `AsyncStore::sync` (with its `SyncReport`)
- `MetadataRecord`'s and `AssetInfo`'s `created` and `etag` fields
- `MetadataConfig`, its defaults (`METADATA_DEFAULTS`) and `StoreConfigFunctions`, where direction
  3 is selected. These are *normally reused unchanged*, as the router is: the configured parts run
  in Rust, around the store

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

For configured stores (direction 3, a mount with `read_only` or a `metadata` section), additionally
— "Configured stores: access policy, metadata and write strategies" below has the facts these
questions rely on:

- **Can a store written in the *language* be named in a configuration document?** Do `read_only`
  and `metadata` reach it there, or only the stores the *integration* provides?
- **Which configured parts can each store take?** Which storage models, which write strategies,
  which handlers? Which data-only methods does the *language* protocol expose, and which of them
  are optional? How does the adapter learn the store's data capabilities without inferring them
  from a method that has side effects, such as `get`? When a part is refused, how is it reported?
  The target is at build, naming the mount and the missing capability.
- **How do backend facts (size, modification time, ETag) cross the bridge, and with what time
  precision**, so that `created` compares equal after a round trip?
- **Can the *language* run a compiled `store_config` chain itself, or are chains applied only in
  Rust around the store?** If chains are exposed, what is the call surface, and how does a
  `MetadataRecord` cross?
- **Can the *language* define its own `store_config` entry**: a handler, a storage model or a write
  strategy? If so:
  - how is it declared (the [Command Declaration Format](../reference/COMMAND_DECLARATION.md), realm
    `store_config`)?
  - is it synchronous?
  - must it be registered before the store is built?
  - what does it cost per call on a listing?
  - how does an exception map under each stage's rules?

  If not, record `NA` and link
  [`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`](../issues/LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE.md).
- **Can the *language* see that a store is read-only, run `sync`, and read a recovery warning?**
- **For direction 2: do the *integration*'s own backends implement data-only access**, so that a
  `metadata` section can wrap them? If they do not, say so per store.

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

#### Configured stores: access policy, metadata and write strategies

A configuration entry may carry two fields beside `type`, not inside `config`: `read_only` and a
`metadata` section. The section chooses a storage model, a write strategy and the handler chains of
each stage (`on_default`, `on_validate`, `on_write`, `on_read`), each written as a `store_config`
spec. The builder applies both fields to whatever the factory returned, so they reach a
*language*-defined store reached through `type: js` (or its counterpart) exactly as they reach a
Rust backend. The format is in [Store configuration](STORE_CONFIG_GUIDE.md) and
[`STORE_CONFIG_FSD.md` §"Metadata section"](../reference/STORE_CONFIG_FSD.md#metadata-section); the realm the specs are written in is in
[Realms](../reference/REALMS.md).

**What an *integration* inherits unchanged:**

- **The pipeline.** `read_only` wraps the store in a `ReadOnlyStore`, which refuses every write
  with `NotSupported` before it reaches the store. A `metadata` section wraps it in a
  `MetadataPipelineStore`, which owns the compiled chains and runs them on every access, in Rust.
  The *language* store supplies bytes and facts only, so nothing is reimplemented per *language*.
- **The defaults.** A field left out of the section takes its value from `METADATA_DEFAULTS`:
  storage `native`, writes `direct`, `on_default` and `on_write`
  `defaults/media_type-fill/file_facts`, `on_read` `media_type-fill/file_facts`, `on_validate`
  empty.
- **Refusal at build.** A part the store cannot take is a `configuration_error` raised when the
  store is built, naming the mount and the missing capability. A `metadata` section on a store
  without data-only access is refused this way, whatever the storage model; a Rust backend follows
  the same rule.
- **The command registry.** The chains compile against the environment's command metadata
  registry and its `StoreConfigFunctions`, so the specs are checked when the store is built, not at
  first use.

A store reports what it can take in `data_capabilities()`. An adapter fills it in from the methods
it found on the *language* object at construction (the second rule of "Service adapters" above).
It never infers data access from `get`, because the adapter cannot see `get`'s metadata side
effects.

What each configured part needs from the store. Check a design's stores against this table:

| Part | What the store must provide |
|---|---|
| `read_only` | nothing. Over a store without data-only access its own reads may still write default metadata, and the builder warns |
| any `metadata` section | data-only access: `get_data`, `set_data`, `remove_data`, `listdir_data`, `data_facts` (the pipeline refuses a store without it) |
| storage `per_entry`, `grouped`, `none` | nothing more: the models keep records as data objects of the store |
| storage `native` (the default) | also `get_stored_metadata` and `set_metadata`, for a store that keeps its own metadata |
| writes `direct`, `transactional` | nothing more (`transactional` renames staged objects where it can, else copies and deletes) |
| writes `atomic` | also `rename_data`, or a declared atomic write (`DataCapabilities::atomic_write`) |
| every built-in handler | nothing: it runs on the record in Rust. `data_facts` is what makes `file_facts` and `content_hash-on_change` cheap |

`data_facts` returns size, modification time and ETag. The time is a `BackendTime`: RFC 3339, UTC,
truncated to the backend's `TimePrecision`. A bridge that receives the time as text converts it with
`BackendTime::parse(text, precision)`, which brings it to that canonical form; without a precision,
the precision is the one the text is written in. So a `created` written into a record and read back
compares equal with a fresh reading, from every *language*.

**Compiled chains are not handed to the *language*.** A chain is owned by the pipeline that wraps
the *language* store, so it already runs on every access to that store. Exposing a chain (say, a
`compileStoreConfig(spec, stage)` returning a function from record to record) would be possible,
but nothing needs it, and it would make a second place where handlers run.

**What the *language* cannot contribute: a `store_config` entry.** This is the handler counterpart
of "What the *language* cannot contribute: a store *type*" above. A handler, storage model or write
strategy is a `CommandMetadata` in realm `store_config` plus a builder, and the builder is a plain
`fn` pointer that cannot capture a *language* callable. The same three consequences apply:

- **A *language*-provided entry would be invisible to the registry export.** The Rust-registered
  entries are exported to `specs/command_registry.yaml` as group `store_config`, so
  `liquers-validate` checks a spec that names them. One registered from a page or a module would
  not be there, and a spec naming it would validate nowhere but in the running host.
- **Its arguments cannot be declared** beyond what the declaration carries, so the spec is checked
  only when the store is built.
- **The document leaks how the handler is provided.** A configuration naming a page-defined
  handler cannot be read on a native build as naming the same handler.

Supporting it needs a boxed builder in `StoreConfigFunctions`' table, a synchronous callable (a
chain cannot await), registration before the store is built, a per-call cost on every entry of a
listing, and exceptions mapped through `ERROR` under the stage rules: a warning on read, an abort on
validate. That is
[`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`](../issues/LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE.md),
and it will likely be designed together with `LANGUAGE-STORE-TYPE-NOT-DEFINABLE`.

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

**Issues and patterns.** Data and metadata must remain consistent; do not treat `set_metadata` as optional. Preserve `KeyNotFound`, read, and write error distinctions — and give a *language*-defined store a way to express them: if the *language* side can signal only by raising, every absent key becomes a read failure. `liquers-web`'s `JsStore` uses a sentinel (`null`/`undefined` means absent; a throw means failure), and only an "absent" answer leads it to ask `isDir` for directory metadata. Run blocking host I/O outside async workers. In a browser, IndexedDB-backed methods are naturally Promise-based and non-`Send`.

**Every selected store gets the suite, not just one of them.** `STORE01`–`STORE07` describe *a*
store; an *integration* shipping three must run them against all three, and say per store where a
test does not apply and why. A suite that exercises only the first store leaves the others
unasserted while reporting full coverage.

**Meaningful tests:** `STORE01` set/get data and metadata; `STORE02` missing key error; `STORE03` directory listing invariants; `STORE04` remove/removedir behavior; `STORE05` a relative key is refused as `KeyNotAbsolute`, on direct calls and not only through a router (`STORE05b` guards the ENOENT trap that makes this test lie); `STORE06` concurrent update policy; `STORE07` store works in end-to-end evaluation; `STORE08` an *integration*-provided store satisfies the same contract as a *language*-defined one; `STORE09` a read-only store refuses every write with the documented error, and composition does not fall through to a writable store; `STORE10` metadata inference follows the documented precedence; `STORE11` a store router built from a configuration document routes by prefix, first match wins for overlapping prefixes, and an unmatched key is reported absent; `STORE12` the *integration*'s own store types are constructible from a configuration document, and where two factories in one chain claim a type name, the one chained **earlier** resolves it — restated in terms of chain order, because store construction is now first-wins over a composed chain rather than "factories before built-ins"; an *integration* that composes its own chain and shares no type name with another factory in it may record `STORE12`'s override half as `NA` with that reason; `STORE13` a store type that exists but is unavailable in this build is refused with a message naming the feature or target responsible; `STORE14` `read_only` on a configured mount refuses every write to a *language*-defined store, and `is_read_only` reports it; `STORE15` a `metadata` section on a store without data-only access fails at build, naming the mount and the missing capability; `STORE16` a *language* store with data-only methods round-trips records under `per_entry` and `grouped` storage, applies the default `on_read` chain, and never has its own `get` called for metadata; `STORE17` `created` from the *language*'s backend time compares equal after a write and a re-read; `STORE18` a *language*-defined `store_config` handler runs in its stage, becomes a warning on read and aborts on validate, and is refused if registered after the store is built.

Dispositions by direction, each stated with its reason so that selecting a direction later makes
the tests required again:

| Selected | `STORE01`–`STORE07` | `STORE08`, `STORE10` | `STORE09`, `STORE11`, `STORE12` | `STORE13` | `STORE14`–`STORE18` |
|---|---|---|---|---|---|
| 1 only — a *language*-defined store | required | `NA` | `NA` | `NA` | `NA` without configuration |
| 1 + 2 — plus *integration* backends | required, **per store** | required | `NA` without composition | required if any backend is optional | `NA` without configuration |
| 1 + 2 + 3 — plus composition | required, **per store** | required | required | required if any backend is optional | see below |

With direction 3 selected, `STORE14` is required. `STORE15`–`STORE17` are required when any store
in the *integration* takes a `metadata` section; run `STORE16` and `STORE17` against each such
store. `STORE18` follows the design's answer to the handler question: `NA` unless the design lets
the *language* define `store_config` entries, linking
[`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`](../issues/LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE.md)
as the reason.

`STORE13` is a build-configuration check rather than a runtime one for most *integrations*, and
that does not make it `NA` — §3 is explicit that the harness a check runs in says nothing about
whether it applies.

### RECIPE — Language-defined recipe provider

**Contract.** Adapt a provider from the *integrated language* to `AsyncRecipeProvider`: recipe lookup, optional lookup, containment, plan creation, folder recipe discovery, and asset information.

**Objects/API to map or implement:**

- `AsyncRecipeProvider<E>` *service adapter*
- Language-visible recipe-provider protocol/base class/interface
- `Recipe`, `RecipeList`, `Plan`, `Key`, `ResourceName`, and `AssetInfo`
- Provider composition and configuration — in Rust, every environment's provider is a
  `RecipeProviderChain` configured as a `recipes:` list of entries. Supply the language provider as
  an **entry type**: declare it in a `RecipeProviderFactory` and chain that with
  `EnvironmentBuilder::with_recipe_provider_factory`, so a configuration places it in the list
  (`{type: python, object: notes}`, its constructor looking the object up in a registry the
  integration owns). `with_appended_recipe_provider` remains for a provider supplied in code

**The design must answer:** Does the provider return `Recipe`, query text, or plain data? Which methods may use Liquers defaults? May provider callbacks evaluate queries? How are caching, invalidation, volatility, and provider precedence handled?

**Issues and patterns.** Implement `recipe_opt` without translating “not found” into an execution error. Provider callbacks receive an environment; apply the `RUNTIME` reentrancy rules. Prefer immutable recipe snapshots across the boundary.

**Listing and containment need not agree, and a provider must decide which it is.** `contains` has a
default implementation (`recipes.rs`) that answers by searching `assets_with_recipes` — it
**enumerates**, which is correct only for a provider whose recipes are a finite list it is willing to
show. A *generative* provider that synthesizes a recipe from a pattern — a format produced on demand,
a chunk of a record stream with no known count — legitimately has **addressable ⊋ listed**, and
inheriting the default makes it deny keys it could perfectly well produce, silently. Such a provider
must override `contains` to match the pattern rather than search a list.

**A chain asks every member, so precedence is not a design question.** A key is owned by the one
member whose `recipe_opt` answers `Some`; `recipe`, `recipe_plan` and `get_asset_info` delegate whole
to it. Two members answering `Some` for one key is a configuration error naming both, and an error
from **any** member fails the lookup, owner or not. A language provider therefore answers "no
recipe" cheaply for every key it does not own and raises only for something substantial — an
exception for one malformed file fails every lookup the chain makes. `contains` is any member's
`contains`, so a generative provider still overrides it as above; listing is the union, in member
order. `liquers-lib`'s default list with `records` is `[recipes_yaml, manifest]`, the second a
generative provider for keyed record chunks (`reference/ENVIRONMENT_CONFIG.md` §The recipe provider
chain).

This does not conflict with `reference/STORE_SEMANTICS.md`, which constrains `contains` and `listdir`
on **stores**. A recipe provider sits above the store, and the asset key space is legitimately larger
than the store's — that is what recipes are.

**Meaningful tests:** `RECIPE01` found and missing recipe; `RECIPE02` listing and containment agree
**for an enumerable provider**, and for a generative one that every listed name is addressable while
a pattern-matching name outside the listing is addressable too; `RECIPE03` recipe produces a valid plan; `RECIPE04` provider error maps through `ERROR`; `RECIPE05` volatility/expiration metadata survives; `RECIPE06` end-to-end keyed evaluation; `RECIPE07` nested environment use follows policy.

### MODULE — Load language modules from the store

**Contract.** Where the *integrated language* supports module loading, allow modules to be resolved and loaded from designated locations in the Liquers store, in addition to or instead of the host filesystem. Define the search path, the precedence against native and standard-library modules, the trust boundary, and the caching and invalidation policy. Where an executing command or script has a current working directory, that *cwd* participates in resolution, in the way `PYTHONPATH` and `sys.path` participate in a Python import.

**Objects/API to map or implement:**

- A store-backed module loader or resolution hook for the *integrated language*, such as a `sys.meta_path`/`sys.path_hooks` finder in Python, a `load()` resolver in Starlark, or a module resolver for a Wasm/JavaScript host
- Module search-path configuration: an ordered list of store `Key` prefixes, configured on the environment
- The command's `-R-key/.` default link argument and `Recipe::cwd`/`Recipe::get_cwd` as the source of the relative resolution base (`Context::get_cwd_key` is crate-private)
- `Key::to_absolute`, `Key::parent`, and `Key::filename` for mapping a module name to a store key
- A module cache with an explicit invalidation and reload policy, keyed by store key and version
- Dependency registration, so loaded module keys participate in the tracking in `liquers-core/src/dependencies.rs` and in asset expiration
- A trust policy controlling which store prefixes may supply executable code

**The design must answer:** Does the *integrated language* have a module system, and is its resolution hook public and stable? Which store prefixes form the search path, and who configures them — environment, session, or query? What is the precedence between store modules, host filesystem modules, and the standard library? Is *cwd* taken from `Context::get_cwd_key`, from the `Recipe`, or supplied explicitly, and what happens when it is absent? Are relative imports resolved against *cwd* only, or may they escape upward out of it? How does a module name map to a store key, including packages and submodules? The store is asynchronous while most import systems are synchronous — how is that bridged, and where can it deadlock? When a module's bytes change in the store, is it reloaded, and are dependent assets expired? Which store locations are trusted to supply executable code, and how is that enforced? What happens on a name collision with a native module?

**Issues and patterns.** Loading code from a store is arbitrary code execution, and a store may be remote, shared, or writable by users who are not trusted to run code inside the host process. Treat the module search path as a security boundary: default it to empty and require explicit opt-in per prefix, rather than exposing the whole store. Never let a store module shadow the standard library — resolve native modules first unless the design deliberately states otherwise and accepts the consequence.

The synchronous/asynchronous mismatch is the central technical problem. Python's import system is synchronous while the Liquers store is `AsyncStore`, so a store-backed finder invoked from inside an async worker can deadlock in exactly the way `ASYNCQ` describes. Prefetching module bytes before entering the *language runtime* is usually safer than bridging an async store into a synchronous import hook.

Module caches are a staleness hazard: `sys.modules` will happily retain a module whose store key has since changed. State whether reload is automatic, explicit, or never, and register loaded module keys as dependencies so that editing a module expires the assets whose commands it defined — otherwise a code change silently produces stale results. `Recipe::cwd` is set automatically when recipes are loaded from a folder (`liquers-core/src/recipes.rs`), which makes it the natural default base for relative resolution. `Context` owns the live runtime CWD, but `Context::get_cwd_key` is crate-private: a command receives its working directory as a default link argument (`dir: Key = query "-R-key/."`, [`COMMAND_REGISTRATION_GUIDE.md`](COMMAND_REGISTRATION_GUIDE.md)), and an import hook running inside a command should take it from there rather than duplicate resolution state. A sandboxed *integrated language* such as Starlark should map this feature onto its own `load()` resolver rather than exposing a general filesystem-like path.

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

### DOCS — Documentation for the integration's users

**Contract.** Give the people who will *use* the *integration* — who write *language commands* and
evaluate queries from the *integrated language*, not who change Liquers — what they need to do so
without reading the Rust sources: a contract reference, a user guide whose examples are executed, and
an API reference generated from the public surface. Build all three on the shared references; never
restate them.

`specs/` is internal (`DOCS_STRUCTURE_GUIDE.md` §2), and an *integration*'s users are not
contributors, so the user-facing half of this feature belongs in a future `docs/`. Until that exists,
the two hand-written documents live in `specs/` with `audience: user` or `both`, and move to `docs/`
— never forked — when it is created (`DOCS_STRUCTURE_GUIDE.md` §9.6). The generated reference
belongs to the language's own tooling and is published with the artifact (`PACKAGE`).

**Objects/API to map or implement:**

- **A contract reference**, `specs/reference/<LANGUAGE>_INTEGRATION.md`: how the *integration* is laid
  out (crates, modules, Cargo features), which Rust types it exposes and as what, how the environment
  is built, configured, rebuilt and shut down, the command declaration and call path, the value
  conversion tables in both directions, the error mapping, the runtime model (threads or event loop,
  waits, re-entrancy, cancellation, shutdown), and the limitations, each naming its tracking issue.
  It states what the code does at `HEAD`; the design folder states why
- **A user guide**, `specs/guides/<LANGUAGE>_GUIDE.md`: installing or building, what the package
  contains, a first query, configuring an environment, registering a command (its input, arguments,
  metadata, replacement and removal, errors), running a query (values, metadata, assets,
  cancellation, recipes, checking without running), values, errors, stores where `STORE` is
  selected, testing, and known limitations — in the *integrated language*'s idiom, with an example
  for every task
- **An executed-examples test** that runs every example of the user guide against the built
  artifact, and fails when one fails. An example it cannot run (one needing a real browser, a
  network, credentials) is marked in the guide, with the reason, so the exclusions are visible
- **A generated API reference** in the language's standard form — docstrings with pdoc or Sphinx for
  Python, TypeDoc over the declarations for JavaScript — and a check that every public name carries
  documentation
- **Links**: the *integration*'s `README.md` and this guide's §6 entry for the *language* point at
  the contract reference and the user guide; the guide links the reference for exact rules, and the
  reference links the guide for usage

**The design must answer:** Which of the three documents exist at the milestone that ships, and
which are deferred with an issue? Where does the user guide live — `specs/guides/` now, `docs/`
later, or the package's own documentation — and how is it moved without forking? What runs the
examples: the language's test runner over extracted blocks, a doctest mechanism, or a browser
harness? How is an example that cannot run in that harness marked? How are the examples kept
independent of each other — a fresh environment per example, or a teardown between them? Which
generator produces the API reference, from what (docstrings, declaration files), and what checks its
completeness? Which shared references does each document link instead of restating?

**Issues and patterns.**

- **Document command declaration by reference.** The key-to-metadata mapping, the composition model,
  the defaulting rules and the validation report are
  [`COMMAND_DECLARATION.md`](../reference/COMMAND_DECLARATION.md). A user guide shows the
  *language*'s *syntax* for a declaration — decorator, object literal, dict — and what the
  *integration*'s introspection discovers, so the author knows what never needs writing, then links
  the format for the semantics. A guide that restates the defaulting rules will drift from them, the
  documentation form of two hand-written parsers. The same holds for the query language
  (`PROJECT_OVERVIEW.md`), recipes (`RECIPES_AND_PLANS.md`), store configuration
  (`STORE_CONFIG_FSD.md`) and templates (`TEMPLATE_LANGUAGE.md`).
- **Examples show composition.** An example that writes every field misrepresents the format: it
  suggests every field is needed. Show the shortest declaration first, then one that adds only what
  differs — an explicit argument type, a namespace — and say what the omitted fields default to.
- **An example nobody executes rots.** It is the documentation counterpart of a conformance test that
  passes whatever the code does (§3). Run the user guide's examples in the routine test loop or in
  the harness the *integration* already has; a separate, rarely run step is the same as none.
- **Write what is missing as missing.** A feature the *integration* marks `NA`, `NS` or `BLOCKED`, or a
  limitation it carries, is stated as unavailable in the user guide and listed with its issue in the
  reference. Documenting it as available, because another *integration* or the Rust API has it, is
  the documentation counterpart of a hollow conformance claim.
- **Write for the user, not the implementer.** The reference may name Rust types and files; the user
  guide names the *language*'s objects and links the reference for the rest. Pitfalls that cost the
  implementer time belong in this guide's §6 entry, not in the user guide.
- **Both hand-written documents are reference and guide documents** in the sense of
  `DOCS_STRUCTURE_GUIDE.md` §9: `reviewed:` front-matter and a `## History` table, re-read when a
  design in their `area` lands. A change to the *integration*'s public surface updates them in the
  same change.

**Meaningful tests:** `DOCS01` the contract reference exists and covers layout, exposed types,
environment, commands, values, errors, the runtime model and limitations; `DOCS02` the user guide
exists and covers installing, a first query, configuring an environment, registering a command,
running a query, values, errors and testing, with an example for each; `DOCS03` every user-guide
example runs against the built artifact, and every example that does not is marked with its reason;
`DOCS04` the generated API reference builds, and every public name has documentation; `DOCS05` the
user guide links `COMMAND_DECLARATION.md` for declaration semantics rather than restating its rules,
and its declaration examples show only what differs from the defaults; `DOCS06` every feature marked
`NA`, `NS` or `BLOCKED` and every listed limitation is stated as unavailable in the user guide and named
with its issue in the reference; `DOCS07` the *integration*'s `README.md` and this guide's §6 entry
link both documents. `DOCS01`, `DOCS02`, `DOCS05` and `DOCS06` are review checks, done when the
documents are written and when a design in their area lands; `DOCS03` and `DOCS04` are executed.

## 6. Language-Specific Guidance

### Browser JavaScript

`liquers-web` (`specs/design/liquers-web/`, `specs/design/liquers-web-store/`) is the browser
integration. Its contract is [`reference/JAVASCRIPT_INTEGRATION.md`](../reference/JAVASCRIPT_INTEGRATION.md),
and how to use it is [`JAVASCRIPT_GUIDE.md`](JAVASCRIPT_GUIDE.md), whose examples
`liquers-web/scripts/test-guide-examples.sh` runs under Node.

Expose async APIs as Promises and bytes as `Uint8Array`; check `BigInt`/`i64` conversions. Keep `JsValue`, DOM handles, closures, and `JsFuture` on the wasm thread. Use the wasm-selected inline asset manager and the core `MaybeSend` model. Test in an actual browser with `wasm-bindgen-test`, including rejected Promises and disposal of JS closures.

Four things cost real time in `specs/design/liquers-web/` and are worth knowing in advance:

- **`serde_wasm_bindgen::to_value` serializes maps as JavaScript `Map`s, not objects.** Every `serde_json::Value::Object` and every `HashMap` goes through `serialize_map`, so a page reading `result.a`, `Object.keys(result)` or `JSON.stringify(result)` sees nothing. Rust *structs* serialize as objects either way, which is what makes this survive review: the affected values are the ones that came *from* JavaScript as objects. Use `Serializer::new().serialize_maps_as_objects(true)` everywhere, from one wrapper.
- **Let `wasm-bindgen` generate the `.d.ts`; do not hand-write one.** A hand-written declaration file is a second source of truth defended only by a freshness check somebody has to run, and a stale declaration is worse than none — a type checker confidently accepts code that fails at runtime. What the generator cannot do is see inside a `JsValue`, so a `JsValue` parameter emits as `any` and type-checks anything. Fix that at the source with `typescript_custom_section` plus `typescript_type`-annotated extern types, and the generated file carries real types with no drift possible.
- **`#[serde(skip_serializing_if)]` is right for a config file and wrong for an API.** `CommandMetadata` skips empty vectors, so `describeCommand(n).arguments` was `undefined` for a zero-argument command — working for every command except the ones the caller was least likely to special-case. Normalize the shape at the boundary.
- **Structural conversion is cheaper than it sounds.** Measured round trip, `--release`: an object with 10 properties costs 78 µs, 1 000 properties 5.2 ms, and a 1 MB `Uint8Array` 0.87 ms. Opaque retention is flat (~6 µs), so the *ratio* grows without bound and is the wrong number to quote. Justify an opaque path by **identity**, not by speed, unless the *integrated language* genuinely passes very large structures.

**Configured stores in the browser** (`specs/design/store-adapters/`). A page-implemented store
reached through `type: js` takes `read_only` and a `metadata` section like any other mount, and
`LiquersStore.isReadOnly()` reports whether every write is refused. The section reaches the object
only through optional `JsStore` methods, resolved once at construction like the rest of the
protocol:

- `getData`, `setData`, `removeData`, `listdirData` and `dataFacts` — all five, or the object
  reports no data-only access and a `metadata` section on its mount is refused when the store is
  built, naming the mount. A partial set is treated as none, so a section is never built over a
  store that can read but not list.
- `renameData(from, to)` and an `atomicWrite: true` property — optional; one of them is needed for
  `writes: atomic`.
- `getStoredMetadata(key)` — needed only by `storage: native` (the default), together with the
  base protocol's `setMetadata`. It is not among the five, so a store that keeps no metadata of its
  own takes `per_entry`, `grouped` or `none` without it.
- `dataFacts` returns `{size, modified?, precision?, etag?}`, or `null` when absent. `modified` is
  an RFC 3339 string and `precision` one of `seconds`, `millis`, `micros`, `nanos`; the adapter
  converts it with `BackendTime::parse`, and without `precision` the string's own precision is used.

The adapter does not promise a modification time or an ETag in `data_capabilities()`
(`modified_time` and `etag` are false), but passes on whatever `dataFacts` reports. The built-in
browser stores offer data-only access too: `localstorage` fully (atomic writes and rename, an ETag
hashed from the entry, no modification time), `http` for reads only (`set_data` and `remove_data`
refuse; facts from a HEAD request), so either can take a `metadata` section. `build_store` (in
`liquers-web/src/store/builder.rs`) compiles the sections against the environment's own command
registry and `StoreConfigFunctions`, mirroring the native builder. `sync` is not exposed to pages.
See `liquers-web/README.md` "Read-only stores and metadata sections".

**Templates and data nodes in the browser.** Neither `DATANODE` nor `TEMPLATE` is implemented in
`liquers-web`. It depends on `liquers-lib` without the `templates` feature, so the `ns-template`
commands are not in its built-in set (`WEB-HAS-NO-TEMPLATE-COMMANDS`), and a JavaScript command
receives no JavaScript-visible context (`COMMAND07` is scoped to the Rust side), so the in-command
half of `TEMPLATE` waits on one. A JavaScript object adapter reads own enumerable data properties
(`Object.getOwnPropertyDescriptor`, skipping accessors and symbols), maps `Array` to a list and a
`Map` with string keys to a map, and runs on the one wasm thread inside the render's poll.

### Starlark

Treat sandboxing and determinism as product requirements. Prefer copied Liquers primitives for ordinary *language values*. `starlark::values::Value<'v>` belongs to a `Heap`; values retained after evaluation must be frozen/owned or converted. Host functions registered through Starlark globals are a natural `COMMAND` adapter. Document whether evaluation budgets, cancellation, filesystem/network access, and async features are intentionally unavailable.

### Python

`liquers-py` (`specs/design/liquers-py/`) is the worked example; the choices below are what it
settled on, and the list after them is what cost time. Its contract is
[`reference/PYTHON_INTEGRATION.md`](../reference/PYTHON_INTEGRATION.md), and how to use it is
[`PYTHON_GUIDE.md`](PYTHON_GUIDE.md).

- **Value.** Use `liquers-lib`'s `Value`; a Python object with no structural mapping becomes a
  `PyObjectValue` (`py.Object`) in `ExtValue::Foreign`. Conversion is permissive by default
  (`Environment(conversion="permissive")`), with `strict` refusing the opaque fallback. DataFrames
  cross as Parquet bytes and images as PNG. Pickle is an opt-in codec (`Environment(pickle=True)`),
  never an implicit interchange format. With it off, `py.Object` declares no data formats:
  `store_set` raises `SerializationError`, a computed `py.Object` result is kept as metadata only,
  and stored pickle bytes are read as degraded rather than decoded. The user currently sees "No
  recipe found" for that read (`DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE`). Reading
  pickles back needs a decoder: the module registers
  `register_foreign_decoder("py.Object", decode_pickle)` once at import. The table is
  process-global, and each environment's registry decides whether it may be used
  (`reference/VALUE_TYPE_SYSTEM.md` §"Reading a foreign value back").
- **Environment.** A built environment's registry is frozen, but Python registers commands on
  import, before or after the first evaluation. The handle keeps every declaration, builds lazily,
  and rebuilds when one arrives late, handing the rebuild the first build's store so stored data
  survives. The previous environment is retired, not dropped, so an evaluation in flight finishes.
- **Commands.** Register a Python function as an ordinary async command whose closure owns the
  callable, not as an alias of a generic `pycall`. The decorator reads `inspect.signature` as the
  declaration baseline and Rust runs the shared declaration stages, so a Python command follows the
  `register_command!` conventions (`state` / `text` / any other first parameter name, `context`,
  `*args` as `multiple`). Version a command by its code object, and give a closure or a bound
  method a fresh version: its captures are not in the code.
- **Errors.** A Python exception becomes an `ExecutionError` whose message starts with the class
  (`ValueError: bad`) and whose `traceback` is the formatted Python stack. A `LiquersError`
  raised through Python frames, such as a failed `context.evaluate` a command lets propagate,
  keeps its type and has those frames appended to the traceback it carries, innermost first. So
  a Python → Rust → Python chain reports every Python stack it crossed. The traceback is a flat
  string for now; its structure across languages is open (`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`).
- **The GIL.** Run a Python command under `spawn_blocking` and take the GIL there. Release the GIL
  around every wait (`py.detach`), and wait in short slices, running `py.check_signals()` and
  checking for shutdown between them: otherwise Ctrl-C cannot interrupt `evaluate`, and a wait
  outstanding at shutdown never returns. The caller sees `KeyboardInterrupt` or
  `CancelledError`. Leaving a wait does not cancel the asset; `AssetRef.cancel()` does. Serialization runs on asset-manager workers, so a codec
  that takes the GIL there goes under `tokio::task::block_in_place`.
  Take any lock a Python thread may wait on with PyO3's `lock_py_attached`, and release Python
  objects only after the guard is dropped: a plain `Mutex::lock` held attached stalls the
  interpreter, and a `__del__` that calls back in re-locks on the same thread.
- **Async** (`specs/design/liquers-py-async/`). Awaitables come from `pyo3-async-runtimes`, bound to
  the integration's own runtime with `init_with_runtime` before its first use: each `_async` method
  spawns the core future and wraps its `JoinHandle` (`future_into_py_with_locals`), so a dropped
  awaitable detaches rather than cancels, and results are converted on a blocking thread. An
  `async def` command runs on a pool of loops the environment owns
  (`Environment(async_loops=N)`, default 1) through `run_coroutine_threadsafe`, never on the
  caller's loop; a guard that exists before the coroutine does cancels its future when the command
  future drops. Do not use `into_future_with_locals` for commands: dropping it leaves the task
  running, so cancellation would not reach the coroutine. A coroutine that cancels itself surfaces
  as `concurrent.futures.CancelledError`, a different class from `asyncio.CancelledError`; map both
  to `Cancelled`. Python deviates from the sample tests (Appendix A) in four places, all recorded
  in `liquers-py/README.md`:
  - `ASYNCQ04` and `ASYNCCMD03` cancel through `AssetRef.cancel_async()`: cancelling the awaitable
    only detaches, by policy.
  - `ASYNCQ05`'s policy is detach, shown by the command finishing, not by `live_handle_count()`,
    which counts registrations rather than awaitables.
  - `ASYNCQ08` takes both of its branches: a blocking call on a liquers loop raises `RuntimeError`,
    and one under any other running loop works with a `RuntimeWarning`.
  - The tests call `asyncio.run` in plain test functions (no async plugin), and read metadata with
    `env.command_metadata(name)` rather than `env.commands.metadata(name)`.
- **Free-threaded Python.** Declare `#[pymodule(gil_used = false)]` only after auditing every
  static, lock and `#[pyclass]`; PyO3 0.28+ defaults to it, so state it explicitly. Test on 3.14t,
  whose venv needs its own tag (`sys.abiflags`) and whose Rust tests need `LIBDIR` on
  `LD_LIBRARY_PATH`.

- **Templates and data nodes.** The `ns-template` commands are in the built-in set
  (`liquers-py/src/builtins.rs`); a template command, `render` / `expand` on the environment and
  on `Context`, and a `dict` adapter are not implemented yet (`TEMPLATE` and `DATANODE` are `NS`).
  This matters more than it looks: a `dict` with one non-`str` key or one unconvertible member
  becomes `py.Object`, which a template renders as nothing. A `context.render` would wait like
  `context.evaluate` (`block_on`, interpreter lock released), which is what lets a `dict` adapter
  take the lock per lookup from a worker.

What cost time:

- **`AssetRef::get` returns `Ok` for a failed asset**, holding an error state. Call `.value()` on
  the state, or a failed command looks like one that returned `None`.
- **PyO3 classes report `__module__ == "builtins"`** unless declared with `module = "…"`, so the
  public classes do not appear to belong to the package (`OBJECT08`).
- **Count released callables behind a test-only feature** (`debug-handles`) rather than relying on
  garbage collection. A built environment is freed with its last `EnvRef` (it used to leak, until
  `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` was fixed), but work in flight keeps it alive until it
  finishes, so count after the work is done.
- **The `extension-module` feature cannot link a test binary.** Run the crate's Rust tests with
  `--no-default-features`; `scripts/test-liquers-py.sh` runs both halves.
- **Rust tests that load a Python module race.** Executing a module can release the GIL, so two
  tests installing the same module replace each other's classes; load it once, under a
  `PyOnceLock`.

`MetadataRecord` and `AssetInfo` expose the backend's `created` (an RFC 3339 string) and `etag` as
read-only properties. A configured store's handlers run in Rust, so they cost no interpreter lock.
That changes if Python handlers are ever allowed (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`):
each call would take the GIL, and a listing runs the read chain once per entry, so the cost grows
with the size of a folder. Measure it before allowing them on a listing path.

## 7. Design and Review Checklist

This guide does not define the shape or location of a design document. An *integration* is a substantial feature, so its design follows the standard Liquers design workflow (the `liquers-project` skill), which creates `specs/design/<integration-name>/` and the phase documents within it. `specs/design/async-wasm-refactor/` is an existing example of that layout.

The checklist below is therefore not a document outline but the *integration*-specific content that must appear somewhere in those phase documents. It maps naturally onto them: scope and selection (items 1 and 9) belong in the high-level design; the architecture items (2 through 7) in the architecture phase; the test items (8 and 10) in the examples and test-plan phase; and milestone sequencing in the implementation plan.

Every language-specific design should contain:

1. A feature matrix with level, status, limitations, milestone, and test links.
2. A disposition for every item in each selected feature's “Objects/API to map or implement” list: mapped, implemented, internal-only, deferred, or `NA`, with the exposed language name.
3. The exact Rust value and environment types used.
   - **If the design offers downstream crates a "bring your own value type" path, check it against the orphan rule before promising it.** The natural sketch — `impl MyIntegrationTrait for CombinedValue<SimpleValue, MyExt>` — does *not* compile from a downstream crate: the trait belongs to the integration crate and `CombinedValue` to `liquers-lib`, so both are foreign there, and `CombinedValue` is not `#[fundamental]`, meaning instantiating it with a local type does not make the self type local (E0117). The integration crate itself never notices, because its own trait is local to it. Put the extension point on the *extension* type (`MyExt`, which the downstream crate owns) and provide a blanket impl carrying it up to the value type — then the integration crate uses the same route it documents, rather than a first-class one nobody else can reach.
4. Ownership diagrams or precise prose for every stored foreign handle.
5. Sync, async, thread, reentrancy, cancellation, and shutdown policies.
6. Conversion tables, including loss and serialization behavior — and, where `DATANODE` is selected, the data-node mapping beside them, with every difference stated.
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
12. A documentation plan (§DOCS): which of the contract reference, the user guide and the generated
    API reference ship in which milestone, what executes the guide's examples, and the issue that
    tracks each document deferred.

## 8. References

- Liquers core integration boundaries: `liquers-core/src/value.rs`, `error.rs`, `context.rs`, `commands.rs`, `store.rs`, `recipes.rs`, `assets.rs`, and `maybe_send.rs`
- Data nodes: `liquers-core/src/data_node.rs` (`DataNode`, `AsyncDataNode`, `AnyNode`); implementations to copy from in `liquers-records/src/data_node.rs`, `liquers-lib/src/polars/data_node.rs`, `liquers-lib/src/value/data_node.rs` and `liquers-lib/src/template/folder.rs`; design in [`design/liquers-mustache/data-node-interface.md`](../design/liquers-mustache/data-node-interface.md)
- Templates: [Template Language](../reference/TEMPLATE_LANGUAGE.md), [Template Guide](TEMPLATE_GUIDE.md); `liquers-template/src/` (`render`, `TemplateHost`, `MapHost`) and `liquers-lib/src/template/` (`ContextHost`, the `ns-template` commands)
- Shared opaque-value mechanism for every *integrated language*: `liquers-lib/src/value/foreign.rs` (`ForeignValue`) and the `ExtValue::Foreign` variant in `liquers-lib/src/value/mod.rs`
- Command removal: `CommandRegistry::unregister` (`liquers-core/src/commands.rs`) and `CommandMetadataRegistry::remove_command` (`liquers-core/src/command_metadata.rs`)
- A worked *integration* design following this guide: [`specs/design/liquers-web/`](../design/liquers-web/) — browser JavaScript, phases 1-4 with the 83-test disposition against the 2026-08 inventory; later additions (`ENVIRON08`, `RECORDS01`-`11`, `STORE14`-`18`) are dispositioned in [`specs/archive/2026-10-17-liquers-web-guide-conformance.md`](../archive/2026-10-17-liquers-web-guide-conformance.md)
- [Command Declaration Format](../reference/COMMAND_DECLARATION.md) — the shared declaration vocabulary, composition over introspection, and the defaulting rules every *language* guide builds on
- [Command Registration Guide](COMMAND_REGISTRATION_GUIDE.md)
- [Realms](../reference/REALMS.md) — what a realm is, and why `store_config` actions are addressable but not executable
- [Store configuration guide](STORE_CONFIG_GUIDE.md) — `read_only`, `metadata` sections, storage models, write strategies and handlers
- Configured stores: `liquers-core/src/store/` (`read_only.rs`, `realm.rs`, `storage.rs`, `writes.rs`, `pipeline.rs`); the browser adapter's data-only methods in `liquers-web/src/store/js_store.rs`; design in [`specs/design/store-adapters/`](../design/store-adapters/)
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
| `templates/greet.md`, `name.md`, `count.md`, `page.md`, `header.md`; `d/count.txt` | DATANODE08, TEMPLATE01–11 | listed at the head of the TEMPLATE tests |

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

### DATANODE

```python
# `lq.data_node(x)` stands for "the AnyNode core sees for x": the adapter for a native
# structure, or the outbound view of a Liquers value. A Rust-side test may call the
# adapter's DataNode methods directly; what matters is that core's answers are asserted.

def test_DATANODE01_nested_structure_answers_per_mapping_table():
    n = lq.data_node({"a": [1, {"b": "x"}], "c": None})
    assert n.kind == lq.NodeKind.Map and set(n.keys()) == {"a", "c"}
    assert n["a"].kind == lq.NodeKind.List and len(n["a"]) == 2
    assert n["a"][1]["b"].scalar == "x"
    assert n["c"].kind == lq.NodeKind.Null
    assert n.get("absent") is None                # missing, not an error

CORPUS = [{"s": "x", "i": 3, "f": 1.5, "b": True, "n": None, "l": [1, "two", [3]]},
          {"nested": {"deep": {"deeper": [{"k": 1}]}}}, {}, []]

def test_DATANODE02_adapter_agrees_with_structural_conversion():
    for d in CORPUS:
        adapted, converted = lq.data_node(d), lq.data_node(lq.to_value(d))
        assert adapted.to_json() == converted.to_json()        # key order aside
        assert sorted(adapted.keys()) == sorted(converted.keys())

def test_DATANODE03_scalar_edges():
    assert lq.data_node(True).kind == lq.NodeKind.Bool          # not Integer: bool is an int
    assert lq.data_node(2**63 - 1).scalar == 2**63 - 1
    # 2**63 and 2**64 follow VALUE's documented integer range policy: Scalar::UInt (exact)
    # where VALUE accepts the value, the same ConversionError where it refuses it.
    assert_same_as_value_policy(lq.data_node, 2**63)
    assert_same_as_value_policy(lq.data_node, 2**64)
    with pytest.raises(lq.Error) as e:
        lq.data_node(float("nan")).to_json()
    assert e.value.error_type == lq.ErrorType.ConversionError

def test_DATANODE04_bytes_are_opaque_not_text():
    assert lq.data_node(b"abc").kind == lq.NodeKind.Opaque

def test_DATANODE05_partial_structure_still_reads():
    # Structural conversion refuses the whole dict (a non-str key, an unconvertible
    # member) and retains it opaquely; the adapter answers for what it can read.
    d = {"name": "Ann", "when": datetime.now(), 1: "int key"}
    assert lq.data_node(d)["name"].scalar == "Ann"
    assert lq.data_node(d)["when"].kind == lq.NodeKind.Opaque

def test_DATANODE06_cycle_navigates_lazily_and_fails_to_json():
    d = {"name": "loop"}; d["self"] = d
    assert lq.data_node(d)["self"]["self"]["name"].scalar == "loop"
    with pytest.raises(lq.Error) as e:
        lq.data_node(d).to_json()
    assert e.value.error_type == lq.ErrorType.ConversionError  # not RecursionError

def test_DATANODE07_lookup_runs_no_language_code():
    calls = []
    class Watched(dict):
        def __getitem__(self, k): calls.append(k); return super().__getitem__(k)
    lq.data_node(Watched(a=1))["a"]
    assert calls == []          # or: follows the opt-in protocol the design documents

def test_DATANODE08_retained_foreign_value_reads_in_place(env):
    @env.command
    def person() -> object:                      # retained as an opaque language value
        return Person(name="Ann")                # a class opted into the map protocol
    # Any core consumer will do; with TEMPLATE selected, a render is the natural one.
    assert env.evaluate("person/ns-template/render-~X~-R/templates/name.md~E") == "Ann"

def test_DATANODE09_mutation_between_lookups_follows_policy():
    d = {"a": 1}; n = lq.data_node(d)
    d["a"] = 2
    assert n["a"].scalar == 2   # live view; a snapshot design asserts 1 and says so

@na_unless_multithreaded_runtime   # NA on wasm32: one thread; required where a GIL exists
def test_DATANODE10_worker_lookup_while_language_thread_waits(env):
    # The language thread blocks in evaluate (lock released); the render, polled on a
    # worker, takes the lock for each lookup. Must finish, not deadlock.
    assert env.expand("{{a}}", data={"a": "ok"}, timeout=5) == "ok"

def test_DATANODE11_outbound_view_reads_without_converting(env):
    frame = env.evaluate_state("-R/data/table.csv/-/ns-pl/from_csv")  # any large value
    n = lq.data_node(frame)
    assert n.kind == lq.NodeKind.List and n[0] is not None   # one row built, not all
    with pytest.raises(lq.Error) as e:                       # a sync accessor on an async node
        lq.data_node(env.evaluate_state("-R-dir/data"))["table.csv"]
    assert e.value.error_type == lq.ErrorType.NotSupported
```

### TEMPLATE

```python
# Store content: templates/greet.md = "Hello, {{.}}!", templates/name.md = "{{name}}",
# templates/count.md = "{{$ -R/d/count.txt}}", templates/page.md = "{{> ./header.md}}body",
# templates/header.md = "Header ", d/count.txt = "1". `env.render` / `env.expand` /
# `env.validate` are the top-level receiver; `context.render` etc. the in-command one.

def test_TEMPLATE01_template_commands_are_built_in(env):
    assert env.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E") == "Hello, hello!"
    assert env.evaluate("-R/templates/greet.md/-/ns-template/validate") == []

def test_TEMPLATE02_declared_template_command_renders(env):
    env.template_command("greeting", template="-R/templates/greet.md")
    assert env.evaluate("hello/greeting") == "Hello, hello!"
    m = env.commands.metadata("greeting")
    assert m.definition.alias_of == "template/render" and m.callable is None

def test_TEMPLATE03_editing_the_template_invalidates_results(env, store):
    env.template_command("greeting", template="-R/templates/greet.md")
    assert env.evaluate("hello/greeting") == "Hello, hello!"
    store.set(lq.parse_key("templates/greet.md"), b"Hi, {{.}}.", lq.Metadata())
    assert env.evaluate("hello/greeting") == "Hi, hello."

def test_TEMPLATE04_top_level_expand_over_a_dict(env):
    assert env.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"}) == "Ann has 42"

def test_TEMPLATE05_programmatic_call_matches_the_command(env):
    by_query = env.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E")
    assert env.render(lq.parse_query("-R/templates/greet.md"), state="hello") == by_query
    template = env.evaluate("-R/templates/greet.md")
    assert env.expand(template) == env.evaluate(
        "-R/templates/greet.md/-/ns-template/expand")       # no data: both render "Hello, !"

def test_TEMPLATE06_in_command_render_records_dependencies(env, store):
    @env.command
    def report(context) -> str:
        return context.render(lq.parse_query("-R/templates/count.md"))
    assert env.evaluate("report/out.txt") == "1"
    store.set(lq.parse_key("d/count.txt"), b"2", lq.Metadata())
    assert env.evaluate("report/out.txt") == "2"   # the command's result was invalidated

def test_TEMPLATE07_strict_raises_with_position_recovering_mode_warns(env):
    with pytest.raises(lq.Error) as e:
        env.expand("ok\n{{$ missing_command}}")
    assert (e.value.position.line, e.value.position.column) == (2, 1)
    result = env.expand("ok\n{{$ missing_command}}", errors="silent", full=True)
    assert result.text == "ok\n" and len(result.warnings) == 1

def test_TEMPLATE08_html_escaping_follows_the_flag(env):
    assert env.expand("{{v}}", data={"v": "<b>"}) == "<b>"
    assert env.expand("{{v}}", data={"v": "<b>"}, html=True) == "&lt;b&gt;"

def test_TEMPLATE09_validate_runs_nothing(env):
    runs = []
    @env.command
    def tick() -> str:
        runs.append(1); return "t"
    diagnostics = env.validate("{{$ tick}} {{$ no_such_command}}")
    assert runs == [] and len(diagnostics) == 1
    assert diagnostics[0].severity and diagnostics[0].message and diagnostics[0].position

def test_TEMPLATE10_nested_language_render_does_not_deadlock(env):
    @env.command
    def inner(context) -> str:
        return context.expand("{{x}}", data={"x": "in"})
    @env.command
    def outer(context) -> str:
        return context.expand("[{{$ inner}}]", data={})
    assert env.evaluate("outer", timeout=5) == "[in]"

def test_TEMPLATE11_relative_partial_resolves_against_cwd(env):
    @env.command
    def page(context, cwd: lq.Key = lq.link("-R-key/.")) -> str:
        return context.render(lq.parse_query("-R/templates/page.md"), cwd=cwd)
    assert env.evaluate("-R-cwd/templates/-/page") == "Header body"
    assert env.render(lq.parse_query("-R/templates/page.md"),
                      cwd=lq.parse_key("templates")) == "Header body"
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

# --- STORE14-STORE18: configured stores (direction 3) wrapping a language-defined store. The
# specs in `metadata:` sections are store_config specs, real and validated like the queries:
# `-store_config/<spec>` plans against the registry (STORE18's handler with an overlay).

# A language store offering data-only access. `get` is counted so STORE16 can assert it is never
# called for metadata; `data_facts` reports a fixed time at millisecond precision.
class DataStore(lq.LanguageStore):
    def __init__(self):
        self.data, self.gets = {}, 0
    def get(self, key):
        self.gets += 1
        return None if key not in self.data else (self.data[key], {})
    def get_data(self, key): return self.data.get(key)
    def set_data(self, key, data): self.data[key] = data
    def remove_data(self, key): self.data.pop(key, None)
    def listdir_data(self, key): return children(self.data, key)
    def data_facts(self, key):
        if key not in self.data:
            return None
        b = self.data[key]
        return {"size": len(b), "modified": "2026-10-15T12:00:00.123Z", "etag": f"e{len(b)}"}

async def test_STORE14_read_only_mount_refuses_writes(build_store, register_object):
    register_object("ro", FixedStore({"fixed.txt": b"hi"}))
    store = build_store("""
    stores:
      - {type: language, prefix: ro, config: {object: ro}, read_only: true}
    """)
    assert await store.get(lq.parse_key("ro/fixed.txt")) == b"hi"      # reads pass through
    with pytest.raises(lq.Error) as e:
        await store.set(lq.parse_key("ro/new.txt"), b"x", lq.Metadata())
    # The configured refusal is NotSupported, raised before the write reaches the language object;
    # STORE09's own read-only backend may document a different type.
    assert e.value.error_type == lq.ErrorType.NotSupported
    assert store.is_read_only()

def test_STORE15_metadata_without_data_access_fails_at_build(build_store, register_object):
    register_object("full", FullStore())             # the complete store protocol, no data methods
    with pytest.raises(lq.Error) as e:
        build_store("""
        stores:
          - {type: language, prefix: d, config: {object: full}, metadata: {storage: grouped}}
        """)
    # At build, not at first use, and the message says which mount and what is missing.
    assert "'d'" in str(e.value) and "data-only access" in str(e.value)

@pytest.mark.parametrize("storage", ["per_entry", "grouped"])
async def test_STORE16_records_round_trip_as_data_objects(build_store, register_object, storage):
    source = DataStore()
    register_object("notes", source)
    store = build_store(f"""
    stores:
      - type: language
        prefix: n
        config: {{object: notes}}
        metadata: {{storage: {storage}}}
    """)
    await store.set(lq.parse_key("n/a.md"), b"# A", lq.MetadataRecord(title="First"))
    await store.set(lq.parse_key("n/b.txt"), b"bee", lq.Metadata())
    a = await store.get_metadata(lq.parse_key("n/a.md"))
    assert a.title == "First"
    assert a.media_type == "text/markdown"           # the default chains fill it
    # The storage model's objects are hidden from the listing...
    assert sorted(await store.listdir(lq.parse_key("n"))) == ["a.md", "b.txt"]
    # ...the default on_read chain, media_type-fill/file_facts, applies...
    assert (await store.get_asset_info(lq.parse_key("n/b.txt"))).file_size == 3
    # ...the records are data objects of the language store, where the model puts them...
    present, absent = (("n/__metadata__.json", "n/a.md.__metadata__") if storage == "grouped"
                       else ("n/a.md.__metadata__", "n/__metadata__.json"))
    assert present in source.data and absent not in source.data
    # ...and the language store's own `get` is never called: its metadata side effects, if any,
    # are invisible to the adapter, which is why data access is not inferred from it.
    assert source.gets == 0

async def test_STORE17_created_round_trips_from_backend_time(build_store, register_object):
    register_object("notes", DataStore())
    store = build_store("""
    stores:
      - {type: language, prefix: n, config: {object: notes}, metadata: {storage: per_entry}}
    """)
    await store.set(lq.parse_key("n/a.txt"), b"abc", lq.Metadata())
    record = await store.get_metadata(lq.parse_key("n/a.txt"))
    # The canonical form, at the precision the language reported. A bridge that re-formats the
    # time (another precision, a local zone) makes this differ, and change detection with it.
    assert record.created == "2026-10-15T12:00:00.123Z"
    assert record.etag == "e3"
    assert (await store.get_asset_info(lq.parse_key("n/a.txt"))).created == record.created

async def test_STORE18_language_store_config_handler(env, build_store, register_object):
    # NA unless the design lets the language define store_config entries; record the NA with a
    # link to LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE. Validated with an overlay:
    # --command store_config//page_tag.
    @env.store_config_handler(name="page_tag")
    def page_tag(record):
        if record.title == "boom":
            raise ValueError("boom")
        record.tags.append("page")
        return record
    register_object("notes", DataStore())
    store = build_store("""
    stores:
      - type: language
        prefix: n
        config: {object: notes}
        metadata: {storage: per_entry, on_read: media_type-fill/page_tag, on_validate: page_tag}
    """)
    await store.set(lq.parse_key("n/a.txt"), b"abc", lq.Metadata())
    assert "page" in (await store.get_metadata(lq.parse_key("n/a.txt"))).tags   # runs in its stage
    with pytest.raises(lq.Error):                                                # aborts on validate
        await store.set(lq.parse_key("n/b.txt"), b"x", lq.MetadataRecord(title="boom"))
    # A failing read step becomes a warning on the record, not an error. (Fixture: a record whose
    # title is "boom" written directly into the language store's per_entry object.)
    plant_record(store, "n/c.txt", title="boom")
    assert (await store.get_metadata(lq.parse_key("n/c.txt"))).has_warnings()
    # Chains compile at build, so a registration after build is too late: a store whose document
    # names a handler not yet registered (late_tag) is refused when it is built, not at first read.
    with pytest.raises(lq.Error):
        build_store("""
        stores:
          - {type: language, prefix: m, config: {object: notes}, metadata: {on_read: late_tag}}
        """)
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

### DOCS

```python
# DOCS01, DOCS02, DOCS05, DOCS06 and DOCS07 are review checks on the documents; the two below are
# executed. `liquers-py/tests/test_GUIDE_examples.py` is a worked example of DOCS03.

GUIDE = REPO / "specs/guides/PYTHON_GUIDE.md"

def guide_examples():
    # Every fenced block in the language, except those marked as not run, with the marker's reason.
    return [b for b in fenced_blocks(GUIDE, "python") if not b.marked_not_run]

@pytest.mark.parametrize("example", guide_examples(), ids=lambda b: f"line{b.line}")
def test_DOCS03_every_guide_example_runs(example):
    # A fresh interpreter namespace per example: one example's registrations must not make
    # another pass.
    exec(compile(example.code, f"{GUIDE}:{example.line}", "exec"), {})

def test_DOCS03_unrun_examples_state_why():
    for block in fenced_blocks(GUIDE, "python"):
        if block.marked_not_run:
            assert block.reason, f"line {block.line}: an example that is not run must say why"

def test_DOCS04_every_public_name_is_documented():
    undocumented = [n for n in lq.__all__ if not (getattr(lq, n).__doc__ or "").strip()]
    assert undocumented == []
    assert build_api_reference().exit_code == 0     # pdoc / Sphinx / TypeDoc
```

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-30 | §RECIPE: a language provider is supplied as a recipe-provider entry type placed by the `recipes:` list; a chain asks every member (two owners and any member's error fail), which corrects the claim that a chain's `contains` goes through `recipe_opt`. | design `recipe-provider-chain-config` |
| 2026-10-27 | §6 Python: *The GIL* adds the detached-lock rule; new *Async* entry (the bridge, the loop pool, cancellation, the four test deviations) and *Free-threaded Python* entry. | `design/liquers-py-async/` (phase-5) |
| 2026-10-26 | New delivery feature `DOCS`: the documents an *integration* owes its users — a contract reference, a user guide with executed examples, a generated API reference — built on the shared references rather than restating them; tests `DOCS01`–`07`. §4 row, soft dependencies and the delivery-features note; §7 item 12; Appendix A blueprint. §6 Browser JavaScript links the new `reference/JAVASCRIPT_INTEGRATION.md` and `JAVASCRIPT_GUIDE.md`. | `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` |
| 2026-10-25 | Language status: the `liquers-py` Python baseline profile is `CONFORMANT` now that its core blockers are fixed (PR #176, `key-without-recipe-answer`). | `liquers-py/README.md` |
| 2026-10-25 | Two features. `DATANODE`: core's `DataNode` from the *language* — an adapter so a native structure (a Python `dict`, a JavaScript object) reads in place, agreeing with structural conversion, and an optional outbound view; tests `DATANODE01`–`11`. `TEMPLATE`: the `ns-template` commands in the built-in set, template commands declared as aliases from the *language*, and programmatic `render` / `expand` / `validate` on the environment and on a command's context, keeping the commands' frame layout; tests `TEMPLATE01`–`11`. §4 rows, soft dependencies and profiles; §2 *Data node*; §7 item 6; §8 references; Appendix A blueprints. §MODULE corrected: `Context::get_cwd_key` is crate-private, so the cwd comes from a `-R-key/.` argument. Gaps filed: `FOREIGN-VALUE-HAS-NO-DATA-NODE`, `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`, `WEB-HAS-NO-TEMPLATE-COMMANDS`. | maintainer request |
| 2026-10-25 | §ERROR: map `ErrorCategory` — base classes in a typed hierarchy, a `categories` field otherwise. | `design/key-without-recipe-answer/` |
| 2026-10-22 | §6 Python pitfalls: built environments are now freed; the handle-counting advice stays. | `design/environment-manager-reference-cycle/` |
| 2026-10-19 | §6 Python links the new Python integration reference and Python guide. | `design/liquers-py/` (phase-5) |
| 2026-10-19 | §6 Python: an *Errors* entry. Nested Python tracebacks are concatenated innermost first, and the cross-language structure is an open problem (`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`). | `design/liquers-py/` (phase-5) |
| 2026-10-19 | §RECORDS "What `liquers-web` does": the records bridge is behind an opt-in feature, and why. | `claude/web-records-opt-in` |
| 2026-10-18 | §6 Python rewritten from the implemented `liquers-py`: value, environment, command and GIL choices, and what cost time. §5 VALUE: corrected the claim that `liquers-py` owns a value type; it takes the constructor route like `liquers-web`. §1: `liquers-py` now implements every Essential feature of the Python profile, no longer a partial reference. | `design/liquers-py/` (phase-5) |
| 2026-10-18 | §RECORDS "What `liquers-web` does": a source crosses as a `RecordSource` handle (chunks, async iterator, bounded materialize), handles cross back as the same `Arc`, the `rec` commands run in the browser; the `RECORDS` dispositions. | `design/liquers-web-records-bridge/` |
| 2026-10-18 | §RECORDS "What `liquers-web` does": a single-cell view reaches JavaScript as its scalar; `RECORDS10` runs there. | `WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH`, `design/web-records-single-cell-scalar/` |
| 2026-10-18 | §ENVIRON: an integration without `EnvironmentBuilder` runs the registry check on its own import path and reports it before raising; `liquers-web`'s `validateCommands` / `registerCommands`. | `design/web-environ-validation-report/` |
| 2026-10-18 | Corrected four descriptive claims about `liquers-web`: §3 harness question 2 (classified per test: Node, native, script and browser tiers), the §OBJECT `OBJECT09` one-liner (round trip, matching Appendix A), the rebuild triggers in the TypeRegistry advice, and the worked-example entry's 83-test disposition (dated to the 2026-08 inventory). | `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`, `design/language-guide-web-claims/` |
| 2026-10-16 | §6 Browser JavaScript: `localstorage` and `http` now offer data-only access and can take a `metadata` section. | `design/store-adapters/` phase-5 review |
| 2026-10-15 | Configured stores. §4: `read_only` and `metadata` belong to `STORE` direction 3, with no feature ID of their own. §STORE: direction 3 covers a mount's access policy and metadata handling; the inventory adds the data-only methods, `DataCapabilities`, `DataFacts`, `BackendTime`, `is_read_only`, `sync`, `created`/`etag` and `MetadataConfig`/`StoreConfigFunctions`; new design questions for configured stores; new subsection "Configured stores: access policy, metadata and write strategies" with the capability table and the `store_config` counterpart of "What the *language* cannot contribute"; tests `STORE14`–`STORE18` with a disposition column and Appendix A blueprints. §COMMAND asks about foreign realms; §ENVIRON states that chains compile against the environment's registry. §6: `JsStore`'s optional data methods and the built-in browser stores' refusal; Python's GIL cost for a per-entry handler. | phase-5, `design/store-adapters/` |
| 2026-10-13 | §Design questions, question 4: NaN/infinity follows the JSON policy in `SERIALIZATION_GUIDE.md`. | phase-5, `design/value-to-json-non-finite-float/` |
| 2026-10-10 | §4 *Out of scope: cache*: the cache module and `liquers-py`'s wrapper were removed, not scheduled for removal. | `CORE-SYNC-STORE-TRAIT-OBSOLETE`, `design/sync-store-removal/` |
| 2026-10-08 | §7: the design workflow is the `liquers-project` skill (the `liquers-designer` skill was removed), and design folders live under `specs/design/`. | skill consolidation |
| 2026-09-30 | STORE: a *language*-defined store needs a way to express absence as distinct from failure, with `JsStore`'s sentinel as the example. | phase-5 (`design/store-conformance-backlog/`) |
| 2026-09-27 | Reviewed against the implemented `record-streams` code (Phase 5). RECORDS: states that `liquers-web` crosses a batch as lent buffers, not Arrow — `LiquersRecordBatch` (JS `RecordBatch`) with column descriptors, `columnCopy` and `columnView`, a view materialized first, a source not yet mapped; inventory corrected (`ChunkKeys` does not exist and is removed; `RecordValue` and the manifest types added). RECIPE: provider composition is `RecipeProviderChain` via `with_appended_recipe_provider`, and a chain answers `contains` through each provider's `recipe_opt`. | phase-5 |
| 2026-09-25 | RECORDS: the records live in their own crate, `liquers-records`, over `liquers-core`; an integration may depend on it alone through the `RecordValue` adapter, or reach it through `liquers-lib`. | `design/record-streams/` |
| 2026-09-25 | RECORDS inventory: the mutable table (`RecordViewMut`, `RecordBatchMut`, `ColumnMut`) replaces the builder, rows carry an implicit `RowId` so the explicit `Id` is optional, and `to_record` / `to_record_source` are the conversions a binding can call to hand bytes, text or JSON to the record layer. | `design/record-streams/` |
| 2026-09-24 | RECORDS: a source's only byte form is its manifest, and its rows reach bytes through `materialize`, so a *language* wanting a source's data as CSV or Arrow materializes it first (route 3 named accordingly); `collect_view` renamed in the inventory. | `design/record-streams/` |
| 2026-09-24 | RECORDS revised for the trait form of `design/record-streams/`: values hold `RecordView` and `RecordSource` trait objects; a `RecordBatch` crosses as Arrow and any other view as a wrapper or materialized; a *language* may implement the traits, with a `RecordView` kept synchronous; type inventory rewritten; design question 5 and tests `RECORDS10`–`RECORDS11` added. | `design/record-streams/` |
| 2026-09-20 | VALUE gains a RECORDS subsection: the Arrow-hand-off versus wrappers decision with a recommended default, the full type inventory of `design/record-streams/`, the routes for traversing a stream from a *language* with no async model, and tests `RECORDS01`–`RECORDS09`. | `design/record-streams/` |
| 2026-09-20 | VALUE gains a third bridging category for values whose *buffers* are lent to the language in place, with its four obligations — reads only, views invalidated by host-heap growth, handle-owned lifetime, and a copy that is always available. RECIPE's “keep `contains`, `recipe`, and listing mutually consistent” is corrected: a generative provider legitimately has addressable ⊋ listed, and must override `contains` rather than inherit a default that enumerates. `RECIPE02` restated accordingly. | `design/record-streams/` |
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
