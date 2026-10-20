# Phase 1: High-Level Design — liquers-py

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible`. The work is `L`, crosses crates, and changes a Python API.
- **Leading issue:** `FOREIGN-VALUE-CANNOT-BE-DESERIALIZED`, which the read-path change (question 1)
  resolves.
- **Explanation:** every question below has a recommended answer that a working design is built on.
  Items 1, 5 and 6 are the owner's. On 2026-10-18 the owner said to run every phase without waiting
  for approval, so Phases 2–5 and the implementation proceeded on the recommended answer to each;
  all three are now settled except 6: 1 was accepted on 2026-10-19 as implemented, and 5 was
  decided the same day (the M0 split, PR #163). The readiness stays `needs-decision` until the
  owner confirms 6:
  proceeding was authorized, the choices themselves were not made by the owner.
- **Open questions:**
  1. **Decided (2026-10-19): read path for formatless types, as implemented.** Orest accepted the
     empty-format-list rule below; a stricter per-format rule was offered and not chosen.
     - Change: a stored value whose registered type declares *no* data formats is read as degraded
       (`Ok(None)`), not deserialized. Without this, a pickle-off environment unpickles whatever a
       shared store holds.
     - Who it affects: only `py.Object` in a pickle-off environment. Every built-in type with a
       byte form declares formats.
     - Recommended: make the change.
  2. **Proposed resolution: DataFrames.** They cross the boundary as Parquet bytes, which
     `liquers-lib` already reads and writes. IPC is refused by its codec (`POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE`).
     Zero-copy Arrow can come later without an API change.
  3. **Proposed resolution: state parameter.** A first parameter named `state` receives a
     `liquers.State`, which carries the value and its metadata. `value` and `text` receive the
     plain value. This is the shared declaration convention (`StateDelivery`), not a Python exception.
  4. **Proposed resolution: `VALUE12` is NA.** `evaluate` returns native Python objects. There is
     no `liquers.Value` wrapper whose operators could be specified. This reverses if a wrapper is
     exposed.
  5. **Decided (2026-10-19): M0 in its own PR.** Orest chose the split. The M0 code fixes (PyO3 0.30,
     the `liquers` import name, orphan removal, the test runner) are PR #163, cherry-picked onto
     `main`. It merges first, and this PR picks it up with a merge of `main`.
  6. **Decided (2026-10-19): the three deviations from the owner's plan are accepted.** Orest
     accepted all three and asked that they be documented; they are in
     `reference/PYTHON_INTEGRATION.md` §3 and §6 and in `guides/PYTHON_GUIDE.md` §4.
     - The pickle codec is a `pickle=` keyword, not `config={"codecs": …}`, because the core
       `EnvironmentConfig` has no codec section.
     - `reset()` is dropped: a new `Environment()` is the reset.
     - `language_context` is not added to core, since `ErrorPayload.traceback` already carries the
       stack and the class rides in the message.
       Accepted with the owner's note: a traceback may have to carry stacks from
       several languages (Rust calling Python calling … Rhai). For now it is a concatenation of
       those stacks, and what matters most is that Python tracebacks are passed along. Nested
       Python stacks are now concatenated innermost first; the open problem is
       `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`.
     - The owner also asked for user documentation: a Python guide and a Python integration
       reference, written in Phase 5, and a generated API reference, filed as
       `PY-API-REFERENCE-NOT-GENERATED`. The integration guide does not require any of them
       (`LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`).
  7. **Proposed resolution: configured stores are in scope** to the extent of `EnvironmentConfig`'s
     `store` section, using core's existing factories (AC-18's shared file store). Python-defined
     stores stay a non-goal.
  8. **Deferred to `liquers-py-async`: sync evaluation on a running loop.** This design's
     `lq.evaluate` works inside a Jupyter cell, because it blocks with the GIL released. The async
     design must not make it raise there.

## Purpose

Make Liquers usable from Python as a conformant integration
([`LANGUAGE-INTEGRATION_GUIDE.md`](../../guides/LANGUAGE-INTEGRATION_GUIDE.md)). A Python program
builds an environment, evaluates queries, and registers functions as commands that compose with
the Rust command library. This is the library mode of
[`python-wrapper`](../python-wrapper/phase1-high-level-design.md), which this design supersedes,
and the first step of [`VISION.md`](../../reference/VISION.md) strategy 5.

## Problem Example

```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

@lq.command
def repeat(text: str, count: int = 2) -> str:
    """Repeat the input text."""
    return text * count

assert lq.evaluate("hello/repeat-3") == "hellohellohello"
```

**Today** M0 has landed: PyO3 0.30, the `liquers` import name, and the orphan files removed.
`liquers-py` still exposes parser and metadata wrappers only. There is no evaluation entry point,
`Environment` has `todo!()` for its asset manager and recipe provider (`liquers-py/src/context.rs`),
and there is no way to register a Python command (`PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND`).

**After** this design the snippet runs. `repeat`'s metadata comes from its signature and
docstring, and it composes with built-ins (`hello/to_text`, `make_frame/ns-pl/head-2`).

## Scope and Acceptance Criteria

Features: `OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND` (the guide's Python baseline).

- **AC-1** Python command in a query
  WHEN the problem example runs
  THEN `evaluate("hello/repeat-3")` returns `"hellohellohello"`
- **AC-2** Built-ins
  WHEN a fresh `lq.Environment()` evaluates a query of `liquers-lib` commands only
  THEN it succeeds with no registration
- **AC-3** Python and Rust compose
  WHEN a Python command's `polars.DataFrame` is followed by `ns-pl/head-2`
  THEN a two-row `polars.DataFrame` returns
- **AC-4** Structural round trip
  WHEN `None`, `bool`, `int` (±2⁶³ boundaries), `float`, `str`, `bytes`, `list`, `dict[str, …]`
  pass through a command
  THEN each returns as the same type and value; `bytes` never becomes `str`
- **AC-5** Conversion mode
  WHEN a user-class instance passes between two Python commands
  THEN permissive mode (default) delivers the same object; strict mode raises `ConversionError`
  naming the type
- **AC-6** Opaque values persist only when opted in
  WHEN `env.store_set(key, obj)` writes a user-class object
  THEN it raises `SerializationError` without the codec; with `pickle=True`, `-R/<key>` returns an
  equal object
- **AC-7** Exceptions on the evaluation path
  WHEN a command raises `ValueError("bad")`
  THEN `evaluate` raises `lq.LiquersError`'s `ExecutionError` subclass, naming `ValueError` and
  `bad`, with `traceback` set
- **AC-8** Typed errors from Python
  WHEN a command raises `lq.LiquersError(lq.ErrorType.KeyNotFound, "…")`
  THEN the caller receives the `KeyNotFound` subclass
- **AC-9** Metadata from the signature
  WHEN a command has a bare `@lq.command`
  THEN its metadata has name, derived label, docstring, and each parameter's type and default; a
  keyword-only parameter or `**kwargs` is refused by name
- **AC-10** Late registration and replacement
  WHEN a command is registered after an evaluation, or replaces one
  THEN the next evaluation uses it, and a warning says whether a built-in or Python command went
- **AC-11** Isolation
  WHEN two `lq.Environment()`s each register a command
  THEN neither sees the other's; `lq.evaluate` uses one shared default
- **AC-12** Validation report
  WHEN `env.import_metadata(yaml)` imports invalid command metadata
  THEN `env.validate()` lists every issue (severity, message, key) before the first evaluation
  raises `ConfigurationError`
- **AC-13** Nested evaluation
  WHEN a Python command's `context.evaluate(...)` runs another Python command
  THEN both complete without deadlock
- ~~**AC-14**~~ ~~**AC-15**~~ ~~**AC-16**~~ moved to [`liquers-py-async`](../liquers-py-async/DESIGN.md)
- **AC-17** Store path
  WHEN `env.store_set("d/in.txt", "abc")` precedes `-R/d/in.txt/-/repeat-2`
  THEN the result is the `str` `"abcabc"`
- **AC-18** Pickle bytes untrusted by default
  WHEN a pickle-off environment evaluates `-R/<key>` holding bytes a pickle-on one stored
  THEN nothing is unpickled, and the error names `py.Object` and says it has no byte form here
- **AC-19** Results and cancellation
  WHEN `env.get_asset(query)` completes, or is cancelled before its command starts
  THEN completion exposes value, metadata and logs; cancellation ends `Cancelled`, command never run
- **AC-20** Shutdown
  WHEN `env.shutdown()` is called twice and evaluation is then attempted
  THEN the second call is a no-op and evaluation raises a typed error
- **AC-21** Conformance suites
  WHEN `bash scripts/test-liquers-py.sh` runs
  THEN the Appendix A suites of the seven features pass, each `NA` with its reversing condition in
  `liquers-py/README.md` (`RUNTIME02` until a Pyodide target, `VALUE12` until a `Value` wrapper)

Non-goals, each a follow-up design that this must not preclude: async (`liquers-py-async`);
Python-defined stores and recipe providers; records and Arrow; stubs and wheels; server,
store-backed imports, UI.

## Core Interactions

- Commands: Python callables registered as closures; metadata via `CommandDeclaration`.
- Value types: `liquers-lib` `Value`, Python objects in `ExtValue::Foreign`; a foreign decoder hook.
- Assets: one read rule for formatless types (question 1); otherwise unchanged, as are Query and Store.

## Crate Placement

`liquers-py`, now depending on `liquers-lib` (default features, `egui` off). Additive changes in
`liquers-lib` (decoder hook) and `liquers-core` (the read rule).

## Documentation Intent

- Reference:
  - `reference/VALUE_TYPE_SYSTEM.md`: the foreign decoder.
  - `reference/ASSETS.md`: the formatless read rule.
- Guide: `guides/LANGUAGE-INTEGRATION_GUIDE.md`.
  - §6 Python is rewritten.
  - The §5 VALUE paragraph that describes the removed own-enum design is corrected.
- Other documents: `liquers-py/README.md` (quick start, conformance matrix, NA list).
- Documents to update: `specs/README.md`, `CLAUDE.md` (liquers-py notes).

## Design Dependencies

- **requires:** M0, done in this PR. `PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13` and
  `PY-MODULES-NOT-DECLARED-IN-LIB` are closed.
- **supersedes:** `python-wrapper` (salvage: plan §2).
- **required-by:**
  - `liquers-py-async` (ASYNCQ, ASYNCCMD);
  - designs for STORE/RECIPE, RECORDS/STUBS/PACKAGE, and WEBSERV/MODULE/UIUSE.
- **overlaps (T2):** `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`, `ENVIRONMENT-ORDERLY-SHUTDOWN`.

## Scope Changes

2026-10-18, after the Phase 1-2 reviews (all phases pre-approved that day):
- Async (AC-14..16, ASYNCQ, ASYNCCMD) moved to `liquers-py-async` to fit the size limit.
- Added AC-18 (codec off on read), AC-19 (EVAL06's named result), AC-20 (ENVIRON06), AC-21
  (remaining Appendix A). Reworded AC-1 (`text`, per `StateDelivery`), AC-6 (an explicit write: an
  unwritten opaque value persists as metadata only) and AC-12 (`import_metadata`: the core
  `EnvironmentConfig` has no command-metadata section).
- Placement extended to the `liquers-lib` hook and core read rule (`FOREIGN-VALUE-CANNOT-BE-DESERIALIZED`).
- M0 shipped in this PR; it also fixed the plan's unfiled `PY-WRAPPER-DESIGN-MARKED-COMPLETE` and
  `PY-TESTS-HAVE-NO-RUNNER`. Splitting it out is put to the owner (question 5).
- After the Phase 4 review: configured stores via `EnvironmentConfig.store` brought into scope
  (question 7), since AC-18 needs two environments on one store.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §4–§7, Appendix A
- [`archive/2026-10-17-liquers-py-implementation-plan.md`](../../archive/2026-10-17-liquers-py-implementation-plan.md)
- `specs/design/liquers-web/`, `specs/design/foreign-value-type-registration/`, `specs/reference/COMMAND_DECLARATION.md`
