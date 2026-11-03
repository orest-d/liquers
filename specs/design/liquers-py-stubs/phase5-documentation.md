# Phase 5: Documentation - liquers-py-stubs

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #240)

## Implementation Summary

`liquers` is a typed package, implemented as approved: Phase 4 Steps 1-7, commit `12e0254` and
the setter follow-up.

**What ships.**
- `py.typed`.
- `__init__.pyi` declares:
  - every native class under `liquers`, `@final`, with its methods, properties and setters;
  - the module functions;
  - the `command` / `first_command` overloads with `ParamSpec`;
  - `LiquersError` and the thirty generated error classes, whose bases follow the categories.
- `_native.pyi` re-exports the native names.
- `_commands.py`, `_env.py`, `_errors.py`, `_factories.py`, `_records.py` and `_extras.py` are
  annotated, with no behaviour change.

**The checks.** `tests/test_STUBS_declarations.py` holds 13 tests. They run on CPython 3.14 with
the GIL, with mypy 2.4.1 and pyright 1.1.414 pinned:
- stubtest, with an allowlist of three `debug-handles` entries;
- `mypy -p liquers`;
- the native-name and error-class comparisons;
- the drift cases;
- seven good samples and one bad sample under both checkers.

The bad sample has eleven marked lines. Its second run strips the ignore comments and expects
errors on exactly those lines. `test_wheel_contents` asserts the three files are in the wheel.

**Deviations** (Phase 4's Progress):
- `fields=` and the row and column parameters are typed `dict`, not `Mapping`, because the
  runtime casts to `dict`;
- a third allowlist entry, `liquers._native.__all__`;
- mypy ignores pyarrow's and pandas' missing types;
- pyright is given the interpreter;
- the declarations were drafted from the Rust signatures by a one-off script, then typed by hand.

Settable properties needed setters, which stubtest does not check. They were added from the Rust
`#[setter]`s and covered by a sample.

## Documentation Delivered

### New Reference Documents
None. The contract is §13 of the Python reference.

### New Guide Documents
None.

### Existing Documents Reviewed or Updated
Each with a History row and `reviewed: 2026-11-01`:
- `specs/reference/PYTHON_INTEGRATION.md`:
  - new §13 Type declarations, with its open problems;
  - §1 Layout gains the `.pyi` / `py.typed` row;
  - §8 drops "no type stubs".
- `specs/guides/PYTHON_GUIDE.md`:
  - "Type checking" is a subsection of §9 Testing your commands, so the § numbers cited from the
    code stay valid;
  - §10 drops the limitation.
- `liquers-py/README.md`: the `STUBS` row; §Layout says a new class is declared too; the test loop
  installs the checkers on the typing leg.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Python: the decorator overloads, and what cost
  time (stubtest's submodule discovery, pyright's `--pythonpath` and its narrowing on assignment,
  pyarrow's missing types).
- `specs/README.md`: the M5 row.

### Links and Capability Map
`specs/README.md`'s M5 row links PYTHON_INTEGRATION §13 and PYTHON_GUIDE §9 Type checking.

## Issues Filed

None new.
- Closed: `PY-NO-TYPE-DECLARATIONS`.
- `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` stays open, with a note on the stub-only
  `_QuerySegment`.

## Important Learning

- stubtest verifies names, signatures and kinds, but not whether a property is settable. Derive
  setters from the Rust `#[setter]`s, or the stubs silently forbid valid assignments.
- A decorated function keeps its signature only through `ParamSpec` overloads. A
  `Callable[..., R]` stub would make the bad sample's ignores unused, which the second run of
  `test_bad_samples_fail_exactly` would catch.
- PyO3's `__text_signature__` gives exact parameter names and kinds, so a draft from it plus the
  Rust types leaves only the semantic typing (`Query | str`, awaitables, `Any` values) to do by
  hand.

## Conformance and Remaining Work

Requested, approved and implemented scope match. Nothing remains. Generated stubs are an open
problem recorded in §13, not deferred work.

## Validation

- `bash scripts/test-liquers-py.sh` on 3.14: 416 passed; the STUBS tests skip elsewhere by design.
- stubtest: no finding beyond the allowlist.
- mypy `--strict` and pyright `strict`: clean on the good samples.
- `docs_index.py --check`: clean after regeneration.
