# Phase 1: High-Level Design — liquers-py-stubs

## Purpose

Give the users of `liquers` completion, signature help and static checking in their editors and
CI, with a command keeping its own signature through the decorators: the `STUBS` feature of
`LANGUAGE-INTEGRATION_GUIDE.md`, for `VISION.md`'s Python data science use case and its "better
Python integration" priority. Milestone M5 of the liquers-py plan, with
[`liquers-py-records`](../liquers-py-records/DESIGN.md) and [`liquers-py-package`](../liquers-py-package/DESIGN.md).

## Design Readiness

- **Readiness:** `ready` (Phase 1 approved; decision log below)
- **Automatic fixing:** `not-eligible`: an `L` feature, new development dependencies.
- **Leading issue:** `PY-NO-TYPE-DECLARATIONS`, which this design implements.
- **Explanation:** Orest decided questions 1, 3 and 5 (Decision 1) and approved Phase 1 with every
  later phase pre-approved (Decision 2); the proposed resolutions 2, 4 and 6-8 stand.
- **Open questions:**
  1. **Decided (Decision 1) - written or generated.** As recommended: hand-written `.pyi` for the native
     extension, inline annotations in the pure-Python modules, and `mypy.stubtest` failing the test
     loop on any mismatch with the runtime, so a declaration cannot drift silently (the guide's
     rule). Alternative: generate `_native.pyi` from PyO3's introspection data (PyO3's
     `experimental-inspect` feature and maturin's stub output). It covers only the extension, not
     the decorators or the error classes made at import, is marked experimental, and its output
     would still need the same checks. PyO3 0.30's guide calls it "still in active development"
     and does not support a module declared as a function, which `_native` is
     (`liquers-py/src/lib.rs`), so generating would first need the module rewritten in the
     declarative form. Revisit when it is stable (an issue is filed then, not now). A cheap step
     toward it is allowed meanwhile: return annotations in `#[pyo3(signature = …)]`.
  2. **Accepted at the gate - layout.** `liquers/_native.pyi` declares every extension class and
     function; `liquers/__init__.pyi` declares the package as checkers should see it, including
     the `LiquersError` subclasses that `__init__.py` creates with `globals().update` (invisible to
     a checker otherwise); `_commands.py`, `_env.py`, `_errors.py`, `_factories.py` carry inline
     annotations; `liquers/py.typed` marks the package. Private modules are not part of the surface
     (`STUBS01` lists `liquers` and `liquers._native`). Python has a static type system and a
     stub format (PEP 484, PEP 561). No generic Rust type is exposed: every wrapper is concrete.
     Every class declares `module = "liquers"` (`liquers-py/src/environment.rs` and the others), and
     `_native` has no `__all__`, so stubtest alone does not report a class missing from
     `_native.pyi`; the classes are therefore declared in `liquers/__init__.pyi` under their
     runtime home `liquers`, and a pytest compares `dir(liquers._native)` with the declared names
     (AC-3).
  3. **Decided (Decision 1) - values are `Any`.** `evaluate` and `State.value` return `Any`, because
     the result depends on the query and may be any retained Python object; a `Query | str`
     argument, a wrapper class or an error is declared exactly. Unions such as `str | bytes |
     RecordView | …` would claim a structure the bridge does not promise (guide: "be honest about
     what is not statically known").
  4. **Accepted at the gate - decorators keep signatures.** `command`, `first_command` and the
     environment-bound forms are overloaded for the bare and the keyword form and typed with
     `ParamSpec`, returning the decorated callable's own type (they return the function unchanged
     at run time, `_commands._register`), so an `async def` command stays a coroutine function.
     The overloads mirror the runtime signature so stubtest accepts them: `(fn: Callable[_P, _R])`
     and `(fn: None = None, *, name, namespace, label, doc, filename, volatile, cached, env)`,
     with private type variables. They live in `__init__.pyi`, since `command` is built by a
     factory (`_commands._decorator`) that inline annotations cannot overload.
  5. **Decided (Decision 1) - the checks.** `scripts/test-liquers-py.sh` runs `stubtest liquers`
     (allowlist checked in, each entry justified), then mypy `--strict` and pyright `strict` on
     `liquers-py/tests/typing/good/` (must pass) and `bad/` (each intended error carries a
     `# type: ignore[<code>]` / `# pyright: ignore[<rule>]`; mypy `--strict` reports an unused
     ignore, and a pyright configuration sets `enableTypeIgnoreComments: false` and
     `reportUnnecessaryTypeIgnoreComment: error`, so an accepted bad line fails both). Signatures
     are asserted with `assert_type`, not by matching `reveal_type` text, which differs between
     checkers. Checker versions are pinned in the script; pyright comes from its
     PyPI wrapper, pinned as `pyright[nodejs]` so it needs no separate Node. The checks run on one CI leg, CPython 3.14 (mypy
     has no compiled wheels for 3.14t or PyPy, and stubtest's answers can differ by interpreter);
     the other legs skip them with a message. The checks run once, from a pytest module
     (`test_STUBS_declarations.py`), so each `STUBSnn` has a named test and the script does not run
     them a second time. Phase 2 builds `_native` once and records stubtest's raw output (PyO3's
     comparison, hash and `__new__` signatures) before sizing the allowlist.
  6. **Accepted at the gate - test-only surface.** `debug-handles` methods (`live_handle_count`, the
     record-handle count) are not declared; stubtest's allowlist names them, and stubtest runs only
     against the test build (with that feature), never against a wheel.
  7. **Accepted at the gate - `NA` and `NS` features.** Nothing is declared for a feature liquers-py
     does not implement (`WEBSERV`, `UIUSE`, `TEMPLATE` rendering, …); declarations grow with the
     features, in the same change (a reviewed rule in `PYTHON_INTEGRATION.md`).
  8. **Accepted at the gate - order.** `liquers-py-records` chose wrappers (its Decision 1), so the
     stubs declare `lq.RecordView`, `lq.RecordSource`, `lq.RecordColumn` and the schema classes,
     with the constructors (`from_records`, `from_dict`, `from_pandas`, `from_polars`,
     `from_arrow`, `fields=`) and `to_list` / `to_dict`, once that design's Phase 2 fixes their API.
     pandas, polars and pyarrow types appear only under `TYPE_CHECKING`, so a checker without
     them installed sees `Any` there, not an error. *(Superseded by the decision log: they are
     declared `Any`, and the stubs import no third-party package.)* `STUBS07`'s wheel half is tested by
     `liquers-py-package` against the built wheel.

- **Decision log (pre-approved phases; none needs Orest):**
  - Phase 2: question 2 is read as "classes declared in `__init__.pyi` under `liquers`;
    `_native.pyi` re-exports them" (its first sentence and its last two disagreed).
  - Phase 2/3: question 8 is met by declaring the exits and dataframe parameters `Any` (pyarrow
    ships no type information, so a precise type would be *Unknown* under pyright strict); the
    stubs import no third-party package.
  - Phase 3: M5 lands package, records, then stubs, in one branch, so every declared name exists
    when stubtest first runs.
  - Phase 2: `liquers-py-package`'s `__version__`, `require` and `Environment.liquers_version`
    are declared here; each design declares its own names when it adds them.
  - Phase 2 measurement: every native class is `@final` (stubtest requires it); the allowlist is
    re-measured once the record classes exist (Phase 4 Step 4).
  - Phase 4 review: this design annotates all six pure-Python modules, `_records.py` and
    `_extras.py` included; it adds the `.pyi` / `py.typed` half of `liquers-py-package`'s
    `test_wheel_contents`. None needs Orest.

## Decisions

1. Orest, 2026-11-01, on gate items 5-7 (this design's questions 1, 3 and 5): "5. ok 6. ok 7. ok"
   — hand-written and checked declarations, values as `Any`, and the checks on one CI leg
   (CPython 3.14), as recommended.
2. Orest, 2026-11-01, round 2: "Agreed with 1 and 2, proceed all" — Phase 1 approved with the
   proposed resolutions, and every later phase pre-approved.

## Problem Example

```python
import liquers as lq

@lq.command
def repeat(text: str, count: int = 2) -> str:
    return text * count

reveal_type(repeat)      # should be: (text: str, count: int = 2) -> str
lq.parse(42)             # should be: a checker error (argument type)
q = lq.parse("a/b")
reveal_type(q)           # should be: liquers.Query (its runtime home)
```

**Today** the package has no `py.typed` and no `.pyi`, so mypy stops at `import liquers` with
`import-untyped` ("module is installed, but missing library stubs or py.typed marker", its
documented behaviour), every name is `Any`, and `lq.parse(42)` is accepted until it raises at run
time. **After** the three comments hold under mypy and pyright (each prints the types in its own
words: mypy omits default values), and a renamed or removed method
fails the test loop before it reaches a user.

## Scope and Acceptance Criteria

- **AC-1** Every exposed module is declared
  WHEN the installed package is listed
  THEN `liquers/__init__.pyi`, `liquers/_native.pyi` and `liquers/py.typed` exist (STUBS01)
- **AC-2** Declarations match the runtime
  WHEN `stubtest liquers` runs against the built extension
  THEN it reports no missing, extra or mismatched name beyond the justified allowlist (STUBS03)
- **AC-3** A drift fails the loop
  WHEN a class is registered in `lib.rs`, or a method added to a `#[pymethods]` block, without a declaration, or a declared one is removed
  THEN `scripts/test-liquers-py.sh` fails naming it (STUBS03)
- **AC-4** Representative use type-checks
  WHEN mypy `--strict` and pyright `strict` check `tests/typing/good/` (queries, environments, commands, stores, recipe providers, errors, records and their constructors, async)
  THEN both report no error (STUBS02)
- **AC-5** Wrong use is rejected
  WHEN they check `tests/typing/bad/` (`lq.parse(42)`, a wrong keyword, awaiting a typed sync result such as `env.evaluate_state(q)`, a missing argument)
  THEN each intended line is an error and no other line is (STUBS06)
- **AC-6** Commands keep their signatures
  WHEN `repeat` above, a `first_command`, a bare `@env.command`, an `env.command(name=…)` and an `async def` command are revealed
  THEN each reveals its own parameters and return type (STUBS04)
- **AC-7** Async entry points are awaitable
  WHEN `evaluate_async`, `AssetRef.get_async`, `Store.get_async` and `RecordSource.materialize_async` are checked
  THEN each is declared as returning an awaitable of its result (STUBS05)
- **AC-8** Errors are declared
  WHEN every `LiquersError` subclass `_errors._install` creates is compared with the declarations
  THEN each is declared with its base classes, and `except lq.KeyNotFoundError` type-checks (the guide's error declarations)

`STUBS07` (declarations and `py.typed` in the distributed artifact) is `liquers-py-package` AC-6;
AC-1 checks the same files in the development install.

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
- Documents to update: the "no type stubs" limitation removed from `PYTHON_INTEGRATION.md` §8 and
  `PYTHON_GUIDE.md` §10; `specs/README.md` status.
- Open problems documented: hand-written upkeep; values are `Any`; generated stubs revisited later.

## Design Dependencies

- `PY-NO-TYPE-DECLARATIONS` — **implemented** by this design.
- `liquers-py-records` — **requires**: its classes are declared here (question 8).
- `liquers-py-package` — **required-by**: ships the files (STUBS07); and **requires**-side
  overlap: its `__version__`, `require` and `liquers_version` are declared here (decision log).
- `py-record-values-to-python` — **superseded** by `liquers-py-records` (Orest's gate answer on
  records question 1); records are declared (question 8).
- `py-pypy-ci-job`, `language-binding-tests-in-ci` (complete) — **overlap**: both run or rewrite
  `scripts/test-liquers-py.sh`, which the checks join; the checks skip on PyPy (question 5).
- `py-api-reference` — **overlaps**: pdoc reads the stubs; no order.
- `py-metadata-fields-as-properties`, `py-python-style-reprs`, `py-query-segment-registration`,
  `py-command-decorator-declaration-keywords` — **overlap**: each changes the surface; whichever
  lands second updates the declarations (AC-3 enforces it).

## Scope Changes

- **2026-11-01** — `liquers-py-records` chose wrappers and added constructors, `to_list` /
  `to_dict` and `fields=` (its Decisions 1, 2), so question 8 and AC-4 name the record classes and
  their constructors (for example `lq.RecordView.from_dict({"a": [1, 2]}, fields={"a": "int"})` must
  type-check). Approved with that design's gate decisions by Orest. Size: still `L`. Phases: 1.

- **2026-11-01** — Phase 2 (pre-approved): the layout reading of question 2, third-party
  types declared `Any` with no imports, and the package design's names (decision log), so the declarations have
  one home and stay checkable; in Phase 3, `Any` for third-party types and the landing order.
  Approved: pre-approval of 2026-11-01. Size: still `L`. Phases: 1, 2, 3.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STUBS, Appendix A STUBS
- `liquers-py/python/liquers/__init__.py`, `_commands.py`; `liquers-py/src/lib.rs`
