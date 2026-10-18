# Phase 5: Documentation - liquers-py

## Completion Preconditions

- [x] Implementation is finished and validated — Phase 4 steps 1–12; see Validation
- [ ] All user comments are answered or incorporated — Phase 1 readiness questions 1, 5 and 6 await
  the owner (1 and 6 implemented as recommended; 5, the M0 split, not done)
- [x] All review comments are answered or incorporated — Phase 4 "Implementation Review"
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

`liquers-py` is now an integration of the Python baseline profile of
[`LANGUAGE-INTEGRATION_GUIDE.md`](../../guides/LANGUAGE-INTEGRATION_GUIDE.md), built as Phase 2
specified:

- **Value.** `liquers-lib`'s `Value` replaces the crate's own enum. A Python object with no
  structural mapping is a `PyObjectValue` (`py.Object`) in `ExtValue::Foreign`. Pickle is opt-in
  (`Environment(pickle=True)`); with it off, `py.Object` declares no formats, and core reads such
  stored bytes as degraded instead of decoding them.
- **Environment.** `lq.Environment` keeps its declarations, builds lazily, and rebuilds on a late
  registration while reusing the store. `lq.default_environment()` and `lq.evaluate` use a
  process default.
- **Commands.** `@lq.command` and `@lq.first_command` register closures that own the callable,
  with metadata from the shared declaration pipeline.
- **Evaluation and errors.** Evaluation is synchronous and releases the GIL. `lq.Context`
  supports nested evaluation, and `lq.AssetRef` gives status, metadata and cancellation. Every
  `ErrorType` is a `LiquersError` subclass.

**Conformance to the request and design.** All seven Essential features are implemented, and each
has its Appendix A suite (153 tests pass, 2 skip as NA, 2 are strict `xfail`s). Two features are
`BLOCKED` on `liquers-core` defects outside this design. The three that depend on them read
`PARTIAL` by the guide's dependency rule, although their own tests pass. AC-18 is met in part:
nothing is unpickled, but the error does not name the type. The matrix is in
`liquers-py/README.md`.

**Added beyond the design.**
- Core's formatless read rule went to `reference/VALUE_TYPE_SYSTEM.md` §Reading, not to
  `reference/ASSETS.md` as Phase 2 planned, because the rule is about types and formats. That is
  why `ASSETS.md` left `affects_docs`.
- `Metadata.log()`, `ActionRequest.from_arguments`, `ArgumentInfo.argument_type` and
  `module = "liquers"` on every class were added to pass `OBJECT` and `EVAL03`.
- The soundness review added interruptible, shutdown-aware waits, `block_in_place` around pickling,
  and restoring a built-in when its Python override is unregistered (Phase 4, "Implementation
  Review").

**Omitted.**
- Async evaluation and `async def` commands moved to
  [`liquers-py-async`](../liquers-py-async/DESIGN.md).
- `realm=` and `arguments=` on the decorators, and recipe providers other than the default, are
  filed as issues below.
- `RUNTIME02` (Pyodide) and `VALUE12` (a `Value` wrapper with operators) are `NA` with their
  reasons in the matrix.

## Documentation Delivered

### New Reference Documents
None. The Python API's current behaviour is in `liquers-py/README.md` (quick start and
conformance matrix) and in the package's docstrings, which ship with the code. The integration
patterns are guidance, so they went to the guide.

### New Guide Documents
None. `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python is the guide, rewritten (below).

### Existing Documents Reviewed or Updated
`affects_docs` (authoritative):
- **`guides/LANGUAGE-INTEGRATION_GUIDE.md`**, reviewed against the implementation.
  - §6 Python is rewritten: the value, environment, command and GIL choices, plus five pitfalls.
  - §5 VALUE no longer claims `liquers-py` owns a value type.
  - §1 no longer calls `liquers-py` a partial reference.
  - A History row was added and `reviewed:` set to 2026-10-18.
- **`reference/VALUE_TYPE_SYSTEM.md`.**
  - §Reading gained the foreign decoder (Phase 4 step 1) and the formatless read rule (step 2).
  - Each has a History row, and `reviewed:` is 2026-10-18.
  - The History table header now has the `Source` column its rows already used.
- **`reference/COMMAND_DECLARATION.md`** (a candidate by its `py` area), reviewed against
  `_commands.py` and `command.rs`. The stages match. §9 now says how `liquers-py` differs from the
  worked example: no `arguments=` on its decorators, and the context is passed positionally.
- **`guides/COMMAND_DESIGN_GUIDE.md`**: its alias "bridge" example described the removed
  `pycall` design, and is corrected.

Each kept document has a 2026-10-18 History row with source phase-5, and `reviewed:` set to
2026-10-18.

Also updated:
- `CLAUDE.md`: a "Python bindings" entry under "Where Code Goes";
- `liquers-py/README.md`.

### Links and Capability Map
`specs/README.md`: the "Python integration (liquers-py)" entry moves from *designing* to *built*. It
now links the guide's §6 and the README matrix, keeping the design link.

## Issues Filed

- `DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE` (P2): a refused degraded read reports
  "No recipe found". It blocks `VALUE` and the rest of AC-18.
- `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2, pre-existing, updated with this design's evidence):
  built environments never free. It blocks `RUNTIME05`, and retired environments accumulate;
  `shutdown()` stops them but cannot free them.
- `WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL` (P2): the same bug `liquers-py` had, in
  `liquers-web`, found by reading the code.
- `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` (P3) and `POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE`
  (P3): found in the value bridge.
- `PHASE5-SKELETON-CREATED-WITH-PRECONDITIONS-TICKED` (P3): a workflow defect in the skill's
  skeleton.
- `PY-COMMAND-DECORATOR-CANNOT-SET-REALM`, `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`
  and `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` (all P3): omitted scope.

**Closed by this work:**
- `FOREIGN-VALUE-CANNOT-BE-DESERIALIZED`;
- `PY-VALUE-ENUM-OVERSIZED`;
- `PY-VALUE-SERIALIZER-IS-A-STUB`;
- `PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND`;
- `PY-MODULES-NOT-DECLARED-IN-LIB` and `PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13`, both in M0.

Each has a resolution note.

## Important Learning

- **Check `state.value()`, not `asset.get()`.** A failed asset resolves `Ok` with an error state.
  Both bindings were written as if it rejected, so a failed command looked like a `None` result.
- **The dependency rule changes what a green suite means.** Every `ENVIRON`, `EVAL` and `COMMAND`
  test passes, yet those features are not `COMPLETE` while `RUNTIME` and `VALUE` are blocked. The
  first matrix claimed otherwise, and the conformity review caught it.
- **Releasing the GIL is not enough.** A wait must also come back to Python periodically, or
  Ctrl-C and shutdown cannot end it.
- **A reviewer pass per phase found real defects.** The Phase 4 soundness review found thirteen,
  including two hangs, and twelve were fixed. The conformance suites passed with every one of them
  present.

These are in the guide's §6 Python, which is the place to update them.

## Conformance and Remaining Work

| | Requested / approved | Implemented |
|---|---|---|
| Essential features | all seven (Phase 1) | all seven: 2 `COMPLETE`, 3 `PARTIAL` by dependency, 2 `BLOCKED` |
| AC-1–AC-21 | all | all except AC-18's message half (`DEGRADED-…`) and AC-21's `RUNTIME05`, built half (`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`) |
| Async | split to `liquers-py-async` (2026-10-18) | not here, by decision |

What remains is in the issues above; nothing is left half-done in the design. Two owner decisions
are still open in Phase 1 readiness: question 5 (the M0 split) and confirmation of 1 and 6.

## Validation

- `bash scripts/test-liquers-py.sh`: 153 passed, 2 skipped (NA), 2 xfailed (BLOCKED). Rust unit
  tests: 15 passed, stable over 12 consecutive runs after the race fix.
- `cargo test -p liquers-lib --lib --tests`: 39 suites, 677 passed, 0 failed, `registry_export`
  included. `liquers-core` passed in full at step 2 (1201 lib tests).
- `bash scripts/check-build-matrix.sh`: every native and wasm32 row passes (Phase 4 Progress, step 12).
- `python3 scripts/docs_index.py --check`: 0 errors.
- Two independent Phase 5 reviews ran: one against the skill's Phase 5 checklist, one checking every
  documentation claim against the code. Their blocking findings were all fixed. The fixes:
  - `COMMAND_DECLARATION.md` and `COMMAND_DESIGN_GUIDE.md` were added to `affects_docs`, and the
    latter's stale `pycall` example was corrected;
  - a phase-5 History row was added to `VALUE_TYPE_SYSTEM.md`;
  - the guide no longer says "conforming";
  - a resolution citation was corrected.

  Their optional findings were also applied: the guide gained the shutdown, Ctrl-C and decoder
  notes, the README gained a waiting and shutdown paragraph, and the filed issues gained their
  missing sections. Not resolvable here: the entry gate's "all user comments answered", which
  waits on the owner's Phase 1 decisions.

