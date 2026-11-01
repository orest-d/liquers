# Phase 4: Implementation Plan — liquers-py-stubs

## Overview

This design lands last in M5, after `liquers-py-package` and `liquers-py-records`, so every name
it declares already exists at run time. The work runs in this order:
1. The harness, so each later step is checked as it lands.
2. The declarations, surface by surface.
3. The annotations of the pure-Python modules.
4. The typed samples.
5. The drift tests.

Nothing has to be fixed first. If `py-command-decorator-declaration-keywords` lands before this,
Step 3 declares its `arguments=` and `realm=`; if after, that design updates the overloads (Phase
2 preflight).

## Progress

- [ ] Step 1: the typing harness and the pinned checkers
- [ ] Step 2: `py.typed`, `_native.pyi`, and the core surface of `__init__.pyi`
- [ ] Step 3: commands, decorators, stores, recipe providers, async and errors
- [ ] Step 4: the record classes and `State.record_view()`
- [ ] Step 5: annotations in `_commands.py`, `_env.py`, `_errors.py` and `_factories.py`
- [ ] Step 6: the good and bad samples, under both checkers
- [ ] Step 7: the drift tests

## Implementation Steps

### Step 1: the typing harness and the pinned checkers

- Files / symbols:
  - new `liquers-py/tests/test_STUBS_declarations.py`, with a module-level `skipif` naming the
    leg;
  - new `liquers-py/tests/typing/{mypy.ini,pyrightconfig.json,stubtest-allowlist.txt}`;
  - `scripts/test-liquers-py.sh`, which installs `mypy==2.4.1` and `pyright[nodejs]==1.1.414` on
    the typing leg only, with the comment about raising the pins.
- Change: Phase 2's configuration. The allowlist holds its two `debug-handles` entries, each
  with a `#` reason.
- Depends on: none.
- Proof: on 3.14, `bash scripts/test-liquers-py.sh -k STUBS` collects the module. On 3.14t and
  3.10, every STUBS test skips with the leg's name.
- Rollback: revert.

### Step 2: `py.typed`, `_native.pyi`, and the core surface of `__init__.pyi`

- Files / symbols: new `liquers-py/python/liquers/{py.typed,_native.pyi,__init__.pyi}`.
- Change: Phase 2's layout and rules:
  - native classes are `@final`;
  - values are `Any`;
  - dunders are declared;
  - enums are classes.

  This step covers queries and keys, parsing, `Environment` (with `liquers_version`), `State`,
  `Metadata`, `AssetRef`, `Context`, plans and command metadata. `_native.pyi` re-exports every
  name, and declares `__all__`.
- Depends on: Step 1.
- Proof: `test_declaration_files_exist`, and `stubtest liquers --allowlist …` reporting only the
  surfaces of Steps 3 and 4, listed and compared with Phase 2's measurement.
- Rollback: revert; the package returns to untyped.

### Step 3: commands, decorators, stores, recipe providers, async and errors

- Files / symbols: `__init__.pyi`:
  - the `_Decorator` protocol and the overloads of `command` / `first_command`, at module level
    and on `Environment`;
  - `Store`, `StoreArgumentInfo`, `RecipeProviders`;
  - every `_async` method as `Awaitable[T]`;
  - `__version__` and `require`;
  - the error classes, with bases in `ErrorType::categories` order.
- Change: Phase 2's typed forms.
- Depends on: Step 2.
- Proof:
  - `test_stubtest_passes` passes except for the record classes. That test is marked `xfail`
    until Step 4, and the mark names it;
  - `test_error_classes_are_declared` passes.
- Rollback: revert the step's part of `__init__.pyi`.

### Step 4: the record classes and `State.record_view()`

- Files / symbols: `__init__.pyi`: `RecordView`, `RecordColumn`, `RecordBuffer`, `RecordSource`,
  `RecordStream`, `RecordChunk`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`,
  `ChunkOrigin`, and `State.record_view()`.
- Change: the members of `liquers-py-records` Phase 2's table. The third-party exits and
  dataframe parameters are `Any`, with a comment naming each runtime type. `fields=` is typed as
  in Phase 2.
- Depends on: Step 3; `liquers-py-records` implemented.
- Proof: `test_stubtest_passes` passes with no `xfail`, on the `debug-handles` build, with the
  allowlist used in full (an unused entry fails).
- Rollback: revert the record declarations, and `xfail` again.

### Step 5: annotations in `_commands.py`, `_env.py`, `_errors.py` and `_factories.py`

- Files / symbols: the four modules. `_records.py` and `_extras.py` were annotated by their own
  designs.
- Change: parameter and return annotations, with no behaviour change.
- Depends on: Step 3.
- Proof:
  - `test_stubtest_passes` still passes; stubtest checks these modules too;
  - `mypy --strict liquers-py/python/liquers` reports nothing;
  - `bash scripts/test-liquers-py.sh` passes in full.
- Rollback: revert; annotations only.

### Step 6: the good and bad samples, under both checkers

- Files / symbols:
  - `tests/typing/good/{queries,environments,commands,stores,errors,records,async_api}.py`;
  - `tests/typing/bad/calls.py`;
  - `test_good_samples_pass[mypy|pyright]` and `test_bad_samples_fail_exactly[mypy|pyright]`.
- Change: Phase 3's Example 1 and samples. The bad-sample test's second run strips the ignore
  comments from a copy and compares the reported line numbers with the marked lines.
- Depends on: Steps 4 and 5.
- Proof: `bash scripts/test-liquers-py.sh -k "STUBS and samples"` passes. Deleting one bad line's
  ignore comment fails the first run, which shows the test can fail.
- Rollback: revert.

### Step 7: the drift tests

- Files / symbols: `test_native_names_are_declared`, `test_undeclared_names_the_class`,
  `test_stubtest_reports_drift[missing|extra]`, and the `undeclared(runtime, declared)` helper.
- Change: Phase 3's Example 2.
- Depends on: Step 4.
- Proof: `bash scripts/test-liquers-py.sh -k STUBS` passes in full. A scratch method added to a
  `#[pymethods]` block, rebuilt with `maturin develop`, fails `test_stubtest_passes` naming it.
  That is AC-3 seen once by hand, then reverted.
- Rollback: revert.

## Testing Plan

- Each step's proof as it lands, on the CPython 3.14 GIL leg.
- At the end:
  - `bash scripts/test-liquers-py.sh` on 3.10, 3.14 and 3.14t, where only 3.14 runs STUBS;
  - CI's `liquers-py-tests` on all three legs;
  - `liquers-py-package`'s `test_wheel_contents`, which finds the `.pyi` files and `py.typed`
    in the built wheel.
- No Rust, `cfg` or feature changes, so the build matrix is not rerun for this design.

## Rollback Plan

The declarations and samples are separate from run-time code. Reverting any suffix of the steps
leaves an untyped or partly typed package that behaves the same. Step 5's annotations are
behaviour-free, so they can stay if the stubs are reverted.

## Documentation Updates

Phase 2's Documentation Architecture, in Phase 5:
- `PYTHON_INTEGRATION.md`: §Type declarations, §1 Layout rows, and §8;
- `PYTHON_GUIDE.md`: "Type checking" and §10;
- `liquers-py/README.md`: the STUBS row and §Layout;
- `LANGUAGE-INTEGRATION_GUIDE.md` §6;
- `specs/README.md`.

Each gets a History row and a `reviewed:` bump, and `specs/index.csv` is regenerated.
`PY-NO-TYPE-DECLARATIONS` closes at Phase 5. `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` stays open,
and its note records the stub-only `_QuerySegment`.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
