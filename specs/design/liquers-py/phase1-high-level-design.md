# Phase 1: High-Level Design — liquers-py

## Purpose

Make Liquers usable from Python as a conformant integration
([`LANGUAGE-INTEGRATION_GUIDE.md`](../../guides/LANGUAGE-INTEGRATION_GUIDE.md)): a Python program
builds an environment, evaluates queries, and registers functions as commands that compose with the
Rust command library. This is the library mode of the three usage modes the owner set out in
[`python-wrapper`](../python-wrapper/phase1-high-level-design.md), which this design supersedes, and the first step of
[`VISION.md`](../../reference/VISION.md) strategy 5 (supersede the Python LiQuer through bindings).

## Problem Example

A data scientist wants a Python step in a Liquers pipeline:

```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

@lq.command
def repeat(state: str, count: int = 2) -> str:
    """Repeat the input text."""
    return state * count

assert lq.evaluate("hello/repeat-3") == "hellohellohello"
```

**Today** this cannot be written. `liquers-py` exposes parser and metadata wrappers only: there is
no evaluation entry point, its `Environment` has `todo!()` for the asset manager and recipe provider
(`liquers-py/src/context.rs`), and the Python command path (`pycall`) is in a file the crate does not
compile (`PY-MODULES-NOT-DECLARED-IN-LIB`). **After** this design the snippet runs, metadata for
`repeat` is derived from its signature and docstring, and `repeat` composes with built-in commands
(`hello/to_text`, `make_frame/ns-pl/head-2`).

## Scope and Acceptance Criteria

Selected features: `OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND` (the guide's Python baseline)
plus `ASYNCQ` and `ASYNCCMD`.

- **AC-1** Python command in a query
  WHEN the problem example runs
  THEN `evaluate("hello/repeat-3")` returns the Python `str` `"hellohellohello"`
- **AC-2** Built-in commands in the default environment
  WHEN a fresh `lq.Environment()` evaluates a query that uses only `liquers-lib` commands
  THEN it succeeds without any registration by the caller
- **AC-3** Python and Rust commands compose
  WHEN a Python command returning a `polars.DataFrame` is followed by `ns-pl/head-2`
  THEN the result is a `polars.DataFrame` with two rows
- **AC-4** Structural round trip
  WHEN `None`, `bool`, `int` (including ±2⁶³ boundaries), `float`, `str`, `bytes`, `list` and
  `dict[str, …]` pass through a command and back
  THEN each returns as the same Python type and value, and `bytes` never returns as `str`
- **AC-5** Unknown objects follow the conversion mode
  WHEN a command returns an instance of a user class and the next Python command receives it
  THEN in the default permissive mode it receives the same object, and in strict mode evaluation
  raises a conversion error naming the type
- **AC-6** Opaque values do not persist by accident
  WHEN an evaluation yields a user-class object and the environment has no pickle codec enabled
  THEN serializing it fails with a serialization error, while evaluation itself succeeds; with the
  codec enabled the value round-trips through bytes
- **AC-7** Exceptions cross on the evaluation path
  WHEN a Python command raises `ValueError("bad")`
  THEN `evaluate` raises a `lq.LiquersError` subclass for `ExecutionError` whose message names
  `ValueError`, `bad` and the traceback, and no panic or Rust abort occurs
- **AC-8** Python can raise a typed Liquers error
  WHEN a command raises `lq.LiquersError(lq.ErrorKind.KeyNotFound, "…")`
  THEN the caller receives the `KeyNotFound` subclass, not `ExecutionError`
- **AC-9** Metadata from the signature
  WHEN a command is declared with only `@lq.command`
  THEN its metadata has the function's name, a derived label, the docstring, and one argument per
  parameter with type and default; a keyword-only parameter or `**kwargs` is refused with an error
  naming the parameter
- **AC-10** Late registration and replacement
  WHEN a command is registered after the environment has evaluated a query, or replaces an existing
  command
  THEN the next evaluation uses the new command, and a replacement emits a warning that says
  whether a built-in or a Python command was replaced
- **AC-11** Isolated environments
  WHEN two `lq.Environment()` instances each register a command
  THEN neither sees the other's command, while module-level `lq.evaluate` uses one shared default
  environment
- **AC-12** Validation report before failure
  WHEN an environment is configured with invalid command metadata (for example from YAML)
  THEN `env.validate()` returns every issue with severity, message and command key before building
  raises its summary error
- **AC-13** Nested evaluation
  WHEN a Python command calls `context.evaluate(...)` for another query that itself runs a Python
  command
  THEN both complete without deadlock under the default asset manager
- **AC-14** Await an evaluation
  WHEN `await lq.evaluate_async("hello/repeat-3")` runs inside an asyncio program
  THEN it returns the same result as AC-1 and does not block the event loop; cancelling the task
  cancels the evaluation
- **AC-15** Async Python commands
  WHEN a command is declared as `async def`
  THEN it is awaited, its metadata marks it async, and its exceptions map as in AC-7
- **AC-16** No blocking inside the event loop
  WHEN the synchronous `lq.evaluate` is called from a coroutine running on an asyncio loop
  THEN it raises `RuntimeError` pointing at `evaluate_async`
- **AC-17** Thin store path
  WHEN text is written to the default environment's store at `d/in.txt` and
  `-R/d/in.txt/-/repeat-2` is evaluated
  THEN the result is the text repeated twice. This proves the keyed path that the later
  store design depends on.

Non-goals: Python-defined stores and recipe providers, configured stores beyond the default
memory/file stores, record-stream wrappers and Arrow export, `.pyi` stubs, published wheels,
`liquers-axum` from Python, store-backed Python imports, and UI. Each goes into a follow-up design
(Design Dependencies). This design must not preclude any of them.

## Core Interactions

- **Commands:** Python callables become registered commands; metadata via `CommandDeclaration`.
- **Value types:** `liquers_lib::value::Value`, Python objects in `ExtValue::Foreign`; the own enum goes.
- **Assets, Query, Store:** reused unchanged (`liquers-lib` default environment).
- **Bindings:** `liquers-py` rewritten above its existing object wrappers.

## Crate Placement

`liquers-py`, now also depending on `liquers-lib` (default features, egui off). No new crate;
additive `liquers-core` changes only (guide §3).

## Documentation Intent

- Reference: none new. The conformance matrix lives in `liquers-py/README.md` and this design.
- Guide: extend `guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Python and the "A Python-style
  integration needs none of this" paragraph in §5 VALUE, which describes the removed own-enum design.
- Other documents: `liquers-py/README.md` (quick start, supported Python, conformance matrix,
  test commands).
- Documents to update: `specs/README.md` (Bindings and servers), `CLAUDE.md` (liquers-py build
  notes), `design/python-wrapper/DESIGN.md` (marked superseded).

## Design Dependencies

- **requires:** M0 (plain issue fixes): `PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13`,
  `PY-MODULES-NOT-DECLARED-IN-LIB`, the `liquers` import name.
- **supersedes:** `python-wrapper` (salvage list: plan §2).
- **required-by:** follow-up designs for `STORE`/`RECIPE`; `RECORDS`/`STUBS`/`PACKAGE`;
  `WEBSERV`/`MODULE`/`UIUSE`.
- **overlaps (T2):** `JS-COMMAND-CANNOT-ACCESS-CONTEXT`, `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`;
  soft prerequisite `ENVIRONMENT-ORDERLY-SHUTDOWN`. Phase 2 preflights each.

## Open Questions

Already decided (owner, 2026-10-17): `liquers_lib::value::Value` + `ExtValue::Foreign`; import name
`liquers`; closures, not `pycall` aliases; pickle codec present but off. Carried from
`python-wrapper`: modernize in place, permissive conversion, Pillow/polars/numpy supported.

1. **Where does an `async def` command run?** Recommended: on the caller's running asyncio loop
   when one exists, otherwise on a background loop thread that liquers-py owns.
2. **Python versions.** Recommended: every CPython the upgraded PyO3 supports, with free-threaded
   builds not claimed in this design.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §4–§7, Appendix A (test contracts)
- [`archive/2026-10-17-liquers-py-implementation-plan.md`](../../archive/2026-10-17-liquers-py-implementation-plan.md) (the plan this design implements; §2 salvage, §7 issue map)
- `specs/design/liquers-web/` (worked integration), `specs/reference/COMMAND_DECLARATION.md`
- `specs/design/python-wrapper/` (superseded)
