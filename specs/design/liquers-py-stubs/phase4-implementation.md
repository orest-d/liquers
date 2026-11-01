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
2 preflight). The steps run in order; Step 7 needs only Step 4.

## Progress

- [x] Step 1: the typing harness and the pinned checkers
- [x] Step 2: `py.typed`, `_native.pyi`, and the core surface of `__init__.pyi`
- [x] Step 3: commands, decorators, stores, recipe providers, async and errors
- [x] Step 4: the record classes and `State.record_view()`
- [x] Step 5: annotations in the six pure-Python modules
- [x] Step 6: the good and bad samples, under both checkers
- [x] Step 7: the drift tests and the wheel's stub files

Deviations, found while implementing:
- `fields=` is typed `dict[str, FieldType | str | FieldSchema] | RecordSchema | None`, not
  `Mapping[...]`: the runtime accepts only a `dict` (`build.rs` casts to `PyDict`), and the stubs
  declare what the runtime takes. The same holds for `from_records`' rows and `from_dict`'s columns.
- The stubtest allowlist has a third entry, `liquers._native.__all__`: in a `debug-handles` build the
  runtime `__all__` lists `live_record_handle_count`, which the stub does not declare.
  `test_native_names_are_declared` compares the names themselves.
- `mypy.ini` ignores missing imports of `pyarrow.*` and `pandas.*`, which `_records.py` imports
  lazily and which ship no types; polars ships its own.
- `test_package_sources_pass_mypy` runs `mypy -p liquers`: given file paths, mypy checks the
  modules outside their package and misreads `import liquers`.
- pyright is given the interpreter (`--pythonpath sys.executable`); without it, it does not find the
  installed package and reports every name as unknown.
- The pyright wrapper fetched its npm package on the first run, as Phase 2 expected.
- The declarations were drafted from the Rust `#[pymethods]` signatures and the runtime's
  `__text_signature__`s by a one-off script, then typed by hand; stubtest passed with no finding
  beyond the allowlist.

## Implementation Steps

### Step 1: the typing harness and the pinned checkers

- Files / symbols:
  - new `liquers-py/tests/test_STUBS_declarations.py`, with a module-level `skipif` naming the
    leg, and its first test `test_checkers_are_pinned`;
  - new `liquers-py/tests/typing/{mypy.ini,pyrightconfig.json,stubtest-allowlist.txt}`;
  - `scripts/test-liquers-py.sh`, which installs `mypy==2.4.1` and `pyright[nodejs]==1.1.414` on
    the typing leg only, with the comment about raising the pins;
  - `.github/workflows/liquers-py-tests.yml`, which caches `~/.cache/pyright-python`.
- Change: Phase 2's configuration. The allowlist holds its two `debug-handles` entries, each
  with a `#` reason.
- Depends on: none.
- Proof: on 3.14, `bash scripts/test-liquers-py.sh -k STUBS` runs `test_checkers_are_pinned`. On
  3.14t and 3.10 it skips with the leg's name, and pytest does not exit 5. The first pyright run
  shows whether it fetched the npm package; Phase 2's sentence is corrected if not.
- Rollback: revert.

### Step 2: `py.typed`, `_native.pyi`, and the core surface of `__init__.pyi`

- Files / symbols: new `liquers-py/python/liquers/{py.typed,_native.pyi,__init__.pyi}`; tests
  `test_declaration_files_exist`, `test_stubtest_passes` and the helper
  `remaining(stubtest_output) -> set[str]`, which returns the class or function each error names.
- Change: Phase 2's layout and rules: native classes `@final`, values `Any`, dunders declared,
  enums as classes. This step declares every name in `__init__.py`'s `__all__` except those
  Steps 3 and 4 name, among them `default_environment`, `evaluate`, `evaluate_async`, `Error`,
  `ErrorType`, `Recipe`, `RecipeList`, `Issue`, `Expires`, `ExpirationTime`,
  `DependencyRelation`, `PlanDependency`, `DependencyKey`, `DependencyRecord`, `MetadataRecord`,
  `AssetInfo`, `Status`, `LogEntry`, `LogEntryKind`, `Version`, and the stub-only
  `_QuerySegment`. `_native.pyi` re-exports what is declared so far, and declares `__all__` and
  `__version__`.
- Depends on: Step 1.
- Proof: `test_declaration_files_exist` passes. `test_stubtest_passes` asserts that
  `remaining(...)` is exactly the set of names Steps 3 and 4 declare; any other name fails it.
- Rollback: revert; the package returns to untyped.

### Step 3: commands, decorators, stores, recipe providers, async and errors

- Files / symbols: `__init__.pyi`:
  - the `_Decorator` protocol and the overloads of `command` / `first_command`, at module level
    and on `Environment`;
  - `Store`, `StoreArgumentInfo`, `RecipeProviders`;
  - every `_async` method as `Awaitable[T]`;
  - `__version__` and `require`;
  - `LiquersError` and the generated error classes, with bases in `ErrorType::categories` order.

  Also the test `test_error_classes_are_declared`.
- Change: Phase 2's typed forms. If `py-command-decorator-declaration-keywords` has landed, its
  `arguments=` and `realm=` too.
- Depends on: Step 2.
- Proof: `test_error_classes_are_declared` passes, and `test_stubtest_passes`' expected set
  shrinks to the Step 4 names.
- Rollback: revert the step's part of `__init__.pyi`.

### Step 4: the record classes and `State.record_view()`

- Files / symbols: `__init__.pyi`: `RecordView`, `RecordColumn`, `RecordBuffer`, `RecordSource`,
  `RecordStream`, `RecordChunk`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`,
  `ChunkOrigin`, and `State.record_view()`; `stubtest-allowlist.txt`.
- Change: the members of `liquers-py-records` Phase 2's table, with `__buffer__` and
  `__release_buffer__` on `RecordColumn` and `RecordBuffer`. The third-party exits and
  dataframe parameters are `Any`, each with a comment naming its runtime type. `fields=` is typed
  as in Phase 2. stubtest is re-measured on the `debug-handles` build with records. A new entry
  gets a reason in the allowlist and a line in Phase 1's decision log.
- Depends on: Step 3; `liquers-py-records` implemented.
- Proof: `test_stubtest_passes` passes with an empty expected set, and every allowlist entry is
  used (an unused entry fails).
- Rollback: revert the record declarations.

### Step 5: annotations in the six pure-Python modules

- Files / symbols: `_commands.py`, `_env.py`, `_errors.py`, `_factories.py`, `_records.py`,
  `_extras.py`; the test `test_package_sources_pass_mypy`.
- Change: parameter and return annotations, with no behaviour change.
- Depends on: Step 4.
- Proof: `test_package_sources_pass_mypy` and `test_stubtest_passes` pass, and so does the whole of
  `bash scripts/test-liquers-py.sh`.
- Rollback: revert; annotations only.

### Step 6: the good and bad samples, under both checkers

- Files / symbols:
  - `tests/typing/good/{queries,environments,commands,stores,errors,records,async_api}.py`;
  - `tests/typing/bad/calls.py`;
  - `test_good_samples_pass[mypy|pyright]` and `test_bad_samples_fail_exactly[mypy|pyright]`.
- Change: Phase 3's Example 1 and samples. The good-sample test asserts the number of checked
  files. The bad-sample test's second run strips the ignore comments from a copy and compares
  the reported line numbers with the marked lines.
- Depends on: Step 5.
- Proof: `bash scripts/test-liquers-py.sh -k "STUBS and samples"` passes. Deleting one bad line's
  ignore comment fails the first run, which shows the test can fail.
- Rollback: revert.

### Step 7: the drift tests and the wheel's stub files

- Files / symbols: `test_native_names_are_declared`, `test_undeclared_names_the_class`,
  `test_stubtest_reports_drift[missing|extra]`, the `undeclared(runtime, declared)` helper; and
  `liquers-py/tests/package/test_PACKAGE_wheel.py::test_wheel_contents`, which gains the three
  stub files.
- Change: Phase 3's Example 2.
- Depends on: Step 4.
- Proof: `bash scripts/test-liquers-py.sh -k STUBS` passes in full. A scratch method added to a
  `#[pymethods]` block, rebuilt with `maturin develop`, fails `test_stubtest_passes` naming it.
  That is AC-3 seen once by hand, then reverted. `liquers-py-package.yml`'s `test` job passes
  `test_wheel_contents`.
- Rollback: revert.

## Testing Plan

- Each step's proof as it lands, on the CPython 3.14 GIL leg.
- At the end:
  - `bash scripts/test-liquers-py.sh` on 3.10, 3.14 and 3.14t, where only 3.14 runs STUBS;
  - CI's `liquers-py-tests` on all three legs;
  - `liquers-py-package.yml`'s `test` job, where `test_wheel_contents` finds the `.pyi` files
    and `py.typed` in the built wheel.
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
