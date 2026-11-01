# Phase 1: High-Level Design — liquers-py-stubs

## Purpose

Give the users of `liquers` completion, signature help and static checking in their editors and
CI, with a command keeping its own signature through the decorators: the `STUBS` feature of
`LANGUAGE-INTEGRATION_GUIDE.md`, for `VISION.md`'s Python data science use case and its "better
Python integration" priority. Milestone M5 of the liquers-py plan, with
[`liquers-py-records`](../liquers-py-records/DESIGN.md) and [`liquers-py-package`](../liquers-py-package/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible`: an `L` feature, new development dependencies.
- **Leading issue:** question 1.
- **Explanation:** the plan (§4 `STUBS`) already chose hand-written declarations verified by
  `stubtest` and two checkers; question 1 asks Orest to confirm that against the generated
  alternative, which has moved since the plan was written. The rest are proposed resolutions.
- **Open questions:**
  1. **Open design - written or generated.** Recommended: hand-written `.pyi` for the native
     extension, inline annotations in the pure-Python modules, and `mypy.stubtest` failing the test
     loop on any mismatch with the runtime, so a declaration cannot drift silently (the guide's
     rule). Alternative: generate `_native.pyi` from PyO3's introspection data (PyO3's
     `experimental-inspect` feature and maturin's stub output). It covers only the extension, not
     the decorators or the error classes made at import, is marked experimental, and its output
     would still need the same checks; revisit when it is stable (an issue is filed then, not now).
  2. **Proposed resolution - layout.** `liquers/_native.pyi` declares every extension class and
     function; `liquers/__init__.pyi` declares the package as checkers should see it, including
     the `LiquersError` subclasses that `__init__.py` creates with `globals().update` (invisible to
     a checker otherwise); `_commands.py`, `_env.py`, `_errors.py`, `_factories.py` carry inline
     annotations; `liquers/py.typed` marks the package. Private modules are not part of the surface
     (`STUBS01` lists `liquers` and `liquers._native`).
  3. **Proposed resolution - values are `Any`.** `evaluate` and `State.value` return `Any`, because
     the result depends on the query and may be any retained Python object; a `Query | str`
     argument, a wrapper class or an error is declared exactly. Unions such as `str | bytes |
     RecordView | …` would claim a structure the bridge does not promise (guide: "be honest about
     what is not statically known").
  4. **Proposed resolution - decorators keep signatures.** `command`, `first_command` and the
     environment-bound forms are overloaded for the bare and the keyword form and typed with
     `ParamSpec`, returning the decorated callable's own type (they return the function unchanged
     at run time, `_commands._register`), so an `async def` command stays a coroutine function.
  5. **Proposed resolution - the checks.** `scripts/test-liquers-py.sh` runs `stubtest liquers`
     (allowlist checked in, each entry justified), then mypy `--strict` and pyright `strict` on
     `liquers-py/tests/typing/good/` (must pass) and `bad/` (each intended error carries a
     `# type: ignore[<code>]` / `# pyright: ignore[<rule>]`, and unused-ignore reporting makes an
     accepted bad line fail). Checker versions are pinned in the script; pyright comes from its
     PyPI wrapper. A pytest module runs the same commands so each `STUBSnn` has a named test.
  6. **Proposed resolution - test-only surface.** `debug-handles` methods (`live_handle_count`, the
     record-handle count) are not declared; stubtest's allowlist names them, since the test loop
     builds with that feature and a wheel never does.
  7. **Proposed resolution - `NA` and `NS` features.** Nothing is declared for a feature liquers-py
     does not implement (`WEBSERV`, `UIUSE`, `TEMPLATE` rendering, …); declarations grow with the
     features, in the same change (a reviewed rule in `PYTHON_INTEGRATION.md`).
  8. **Proposed resolution - order.** Written after `liquers-py-records` Phase 2 fixes the record
     classes' API, so the stubs land with them; `STUBS07`'s wheel half is tested by
     `liquers-py-package` against the built wheel.

## Problem Example

```python
import liquers as lq

@lq.command
def repeat(text: str, count: int = 2) -> str:
    return text * count

reveal_type(repeat)      # should be: def (text: str, count: int = 2) -> str
lq.parse(42)             # should be: a checker error (argument type)
q = lq.parse("a/b")
reveal_type(q)           # should be: liquers._native.Query
```

**Today** the package has no `py.typed` and no `.pyi`, so mypy stops at `import liquers` with
`import-untyped` ("module is installed, but missing library stubs or py.typed marker", its
documented behaviour), every name is `Any`, and `lq.parse(42)` is accepted until it raises at run
time. **After** the three comments hold under mypy and pyright, and a renamed or removed method
fails the test loop before it reaches a user.

## Scope and Acceptance Criteria

- **AC-1** Every exposed module is declared
  WHEN the installed package is listed
  THEN `liquers/__init__.pyi`, `liquers/_native.pyi` and `liquers/py.typed` exist (STUBS01)
- **AC-2** Declarations match the runtime
  WHEN `stubtest liquers` runs against the built extension
  THEN it reports no missing, extra or mismatched name beyond the justified allowlist (STUBS03)
- **AC-3** A drift fails the loop
  WHEN a method is added to a `#[pymethods]` block without a declaration, or a declared one is removed
  THEN `scripts/test-liquers-py.sh` fails naming it
- **AC-4** Representative use type-checks
  WHEN mypy `--strict` and pyright `strict` check `tests/typing/good/` (queries, environments, commands, stores, recipe providers, errors, records, async)
  THEN both report no error (STUBS02)
- **AC-5** Wrong use is rejected
  WHEN they check `tests/typing/bad/` (`lq.parse(42)`, a wrong keyword, awaiting a sync result, a missing argument)
  THEN each intended line is an error and no other line is (STUBS06)
- **AC-6** Commands keep their signatures
  WHEN `repeat` above, a `first_command`, an `env.command(name=…)` and an `async def` command are revealed
  THEN each reveals its own parameters and return type (STUBS04)
- **AC-7** Async entry points are awaitable
  WHEN `evaluate_async`, `AssetRef.get_async`, `Store.get_async` and `RecordSource.materialize_async` are checked
  THEN each is declared as returning an awaitable of its result (STUBS05)
- **AC-8** Errors are declared
  WHEN every `LiquersError` subclass `_errors._install` creates is compared with the declarations
  THEN each is declared with its base classes, and `except lq.KeyNotFoundError` type-checks

Non-goals: generated stubs (question 1); a typed `Value` union (question 3); stubs for liquers-web;
publishing the API reference (`py-api-reference`).

## Core Interactions

Bindings only: declarations for every class registered in `liquers-py/src/lib.rs` and every name
in `liquers/__init__.py`'s `__all__`. No Rust change.

## Crate Placement

`liquers-py/python/liquers/` (the `.pyi` files and `py.typed`); `liquers-py/tests/typing/` and a
`test_STUBS_declarations.py`; `scripts/test-liquers-py.sh`. maturin includes `.pyi` and `py.typed`
under `python-source` in the wheel.

## Documentation Intent

- Reference: a §Type declarations in `PYTHON_INTEGRATION.md` (layout, what is `Any` and why, the
  decorator typing, the checks and the allowlist, the rule that a surface change updates them).
- Guide: a "Type checking" section in `PYTHON_GUIDE.md` (what an editor shows, a typed command,
  what stays `Any`). API: the `.pyi` files feed `pdoc`'s signatures (`py-api-reference`).
- Other: `liquers-py/README.md` (a `STUBS` row; adding a class now means its declaration too, in
  §Layout); `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python (what cost time).
- Open problems documented: hand-written upkeep; values are `Any`; generated stubs revisited later.

## Design Dependencies

- `PY-NO-TYPE-DECLARATIONS` — **implemented** by this design.
- `liquers-py-records` — **requires**: its classes are declared here (question 8).
- `liquers-py-package` — **required-by**: ships the files (STUBS07).
- `py-api-reference` — **overlaps**: pdoc reads the stubs; no order.
- `py-metadata-fields-as-properties`, `py-python-style-reprs`, `py-query-segment-registration`,
  `py-command-decorator-declaration-keywords` — **overlap**: each changes the surface; whichever
  lands second updates the declarations (AC-3 enforces it).

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STUBS, Appendix A STUBS
- `liquers-py/python/liquers/__init__.py`, `_commands.py`; `liquers-py/src/lib.rs`
