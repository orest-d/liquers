---
name: rust-best-practices
description: >-
  Validate Rust architecture and implementation for the Liquers codebase against
  idiomatic ownership, trait design, error handling, async, and serialization
  conventions. Use this whenever reviewing or writing Rust for liquers-core,
  liquers-macro, liquers-store, liquers-lib, liquers-axum, or liquers-py — before
  finalizing a design document, when checking function signatures and data
  structures, when adding traits or ExtValue variants, when feature-gating
  optional backends, or when someone asks "is this idiomatic / will this compile /
  does this follow our conventions". The liquers-designer workflow auto-invokes
  this skill at Phase 2 (architecture) and Phase 4 (implementation plan); apply it
  there without being asked. Not for non-Rust code or trivial one-line edits.
---

# Rust Best Practices (Liquers)

This skill is a **review lens**, not a code generator. Apply it to a design
document, a diff, or a set of signatures and produce a short list of concrete
findings: what violates a convention, *why the convention exists*, and the
minimal fix. Prefer a few high-confidence findings over an exhaustive dump.

The Liquers rules below are the enforced ones — they come from `CLAUDE.md` and
`.claude/skills/liquers-designer/references/liquers-patterns.md`. General Rust
idioms follow. When these two conflict, the Liquers rule wins (it encodes a
deliberate project decision).

## How to run a review

1. Identify what you're reviewing (data structures, trait impls, signatures,
   feature gating, error handling) and which crate it lands in.
2. Walk the checklist sections below in order. For each hit, record:
   `file:line (or doc section) → what → why it matters → minimal fix`.
3. Separate **blocking** findings (won't compile, breaks a hard rule, unsound)
   from **advisory** ones (style, minor over-constraint). Say which is which.
4. If a finding needs a human design decision (not resolvable from context),
   surface it as a question rather than silently "fixing" it.

Keep the output scannable. Explain the *why* — the model reading your review is
smart and acts better on reasons than on bare imperatives.

## Liquers hard rules (blocking)

These are enforced project-wide. A violation is a blocking finding.

- **No `unwrap()` / `expect()` in library code.** They panic; library code must
  return `Result<_, Error>` and propagate with `?`. Tests are the only exception.
- **All errors are `liquers_core::error::Error`.** No new error types. Construct
  with typed constructors — `Error::general_error(msg)`, `Error::key_not_found(&key)`,
  `Error::from_error(ErrorType::General, source)`, `Error::conversion_error(...)`.
  Never `Error::new(...)` directly (it bypasses the typed-constructor convention).
- **No default match arm (`_ =>`) on Liquers-owned enums.** Enumerate every
  variant so adding a variant later is a compile error, not a silent fallthrough.
  Exception: matching on *external* enums you don't own (document why).
- **Async is the default.** I/O and anything reachable from an async context is
  async (`#[async_trait]`, `AsyncStore`). Sync exists only as a deliberate wrapper
  (`AsyncStoreWrapper`) or for genuinely CPU-bound, I/O-free, sync-called code
  (e.g. a render pass, Python bindings). No blocking I/O inside async.
- **Respect the one-way crate dependency flow:**
  `liquers-core ← liquers-macro ← liquers-store ← liquers-lib ← liquers-axum ← liquers-py`.
  A `use` that points backward (e.g. `liquers-core` importing `liquers-lib`) is a
  blocking finding. Rich value types / UI go in `liquers-lib`, not core.
- **New value types are `ExtValue` variants in `liquers-lib/src/value/mod.rs`.**
  `ExtValue` derives only `Debug + Clone` (no `Serialize`); use `Arc<T>` for shared
  payloads; serialize via `DefaultValueSerializer`, not a `Serialize` derive.
- **Commands via the `register_command!` macro.** Sync command fns take
  `&State<Value>`; async command fns take **owned** `State<Value>`. A `context`
  parameter must be **last**. Namespace goes in metadata, not the fn name.

## Ownership & types (mostly blocking when wrong, some advisory)

- **Pick the pointer for the job.** `Arc<T>` for shared, cheaply-cloned ownership
  (the Liquers default for value payloads and trait objects); `Box<dyn Trait>` for
  a single owner of a trait object; borrowed `&T` / `&mut T` for transient access
  that doesn't outlive the call. Prefer `Arc<T>` over `Box<T>` when the type is
  cloned often — that's why `ExtValue` payloads are `Arc`.
- **Borrow in signatures; own in returns.** Take `&State<Value>`, `&DataFrame`,
  `&str` to avoid copying large inputs; return owned `Value` / `Vec<u8>` /
  `DataFrame` when the caller needs ownership.
- **Shared-mutable state:** `Arc<std::sync::RwLock<T>>` / `Arc<std::sync::Mutex<T>>`
  for data touched by a synchronous render thread (egui/web); `tokio::sync::Mutex`
  for state held across `.await`. Don't hold a lock across an `.await` unless you
  mean to. Mark truly immutable-after-construction data without a lock.
- **`#[serde(skip)]` for transient / non-serializable fields** (trait objects,
  runtime handles, cached DOM/GPU state, notification receivers). Reconstruct them
  on load (`init`, refresh) rather than serializing them.

## Traits & generics (advisory unless it blocks compilation)

- **Minimal bounds.** Add a bound only when a call site needs it. Over-constraining
  (`T: Clone + Send + Sync + 'static` when only `Send` is used) leaks into every
  caller. Justify each bound in one phrase.
- **Extend, don't mutate, established traits.** Prefer new methods with default
  implementations (often feature-gated) over changing existing signatures —
  changing a trait method breaks every implementor, including `liquers-py`.
- **Keep `liquers-core` traits minimal;** rich behavior belongs in `liquers-lib`.
- **Object safety:** if a trait is used as `dyn Trait` (like `UIElement`,
  `AsyncStore`), keep it object-safe — no generic methods, no `Self`-by-value in
  methods, associated types handled with care.

## Feature gating (blocking when it breaks a build config)

Relevant when making a dependency/backend optional (e.g. egui vs webui):

- **Gate the dependency *and* every use of it.** An `optional = true` dep needs a
  matching `feature`, and every `use`, type, field, method, and match arm that
  touches it needs `#[cfg(feature = "...")]`. A cfg'd-out enum variant means every
  exhaustive `match` on that enum needs a cfg'd arm too — otherwise the build with
  the feature off fails on a non-exhaustive match.
- **Each feature must build alone and in combination.** Check the real matrix:
  default, each feature solo (`--no-default-features --features X`), and together.
  Don't assume; the cross-product is where cfg bugs hide.
- **Prefer `#[cfg(feature = "x")]` over `cfg!(...)`** for code that must not
  *compile* when the feature is off (not just not run).
- **`target_arch` gating** (e.g. `wasm32`) is orthogonal to feature gating —
  a symbol may need both. wasm is single-threaded: `Send` bounds and
  `tokio::spawn` don't apply; use the project's `spawn_ui_task`-style helper.

## General Rust idioms (advisory)

- Iterators/combinators over manual index loops where it reads clearer.
- `?` and `map_err` over nested `match` on `Result`.
- `impl Trait` / generics over `dyn` in hot paths (dynamic dispatch has a cost);
  `dyn` where you need heterogeneous storage or object safety.
- Derive `Debug`; derive `Clone`/`PartialEq`/`Default` when semantically sound.
- Avoid needless `.clone()` — clone an `Arc` (cheap) not the inner data; borrow
  when the value outlives the use.
- Name by convention: traits `PascalCase` (`AsyncStore`), async variants prefixed
  `Async`, builders `...Builder`, snake_case fns without namespace prefixes.

## Output shape

Return findings as a short list, blocking first. For a design-document review,
anchor each to a doc section; for a diff, to `file:line`. Example:

```
BLOCKING
- value/mod.rs (ExtValue match in `identifier`): egui-only variant `Widget` is
  cfg-gated but this match arm is not → build with `--no-default-features
  --features webui` fails (non-exhaustive match). Fix: add `#[cfg(feature="egui")]`
  to the UiCommand/Widget arms.

ADVISORY
- WebUi.ctx is cloned per call; UIContext is Clone-cheap (Arc + sender) so this is
  fine, but note it so nobody "optimizes" it into a borrow that fights the lifetime.

QUESTIONS (need a human decision)
- Should render methods be infallible (like egui::Ui) or return Result? Affects
  every call site — pick before Phase 4.
```

For deeper, less-frequently-needed material (a fuller anti-pattern catalog with
worked before/after examples), see `references/anti-patterns.md`.
