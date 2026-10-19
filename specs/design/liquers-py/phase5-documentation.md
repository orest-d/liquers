# Phase 5: Documentation - liquers-py

## Completion Preconditions

- [x] Implementation is finished and validated — Phase 4 steps 1–12; see Validation
- [x] All user comments are answered or incorporated — Phase 1 readiness questions 1, 5 and 6
  were decided by the owner on 2026-10-19, and the documentation the owner then asked for is
  below
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

The owner asked on 2026-10-19 for three Python documents: a standard Python API doc, a Python
reference documenting the integration, and a Python guide to the modules and to configuring an
environment, registering a command and running a query. `LANGUAGE-INTEGRATION_GUIDE.md` requires
none of them; it asks only for a quick-start under the Optional `PACKAGE` feature. That gap was
already filed, as `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` (2026-08-30), which now carries this design's evidence.

### New Reference Documents
- **[`reference/PYTHON_INTEGRATION.md`](../../reference/PYTHON_INTEGRATION.md)** (audience
  `both`): the integration's contract at HEAD. It covers the layout and features, the exposed
  types, the environment's declarations, build, rebuild and shutdown, the command declaration and
  call path, both value-conversion directions and the pickle rule, error mapping and traceback
  concatenation, waits, threads and limitations. It also records the three deviations accepted
  in Phase 1 decision 6.

### New Guide Documents
- **[`guides/PYTHON_GUIDE.md`](../../guides/PYTHON_GUIDE.md)** (audience `user`): installing, the
  module overview, a first query, configuring an environment, registering commands (input,
  arguments, context, metadata, replacing, errors), running queries (metadata, cancelling,
  recipes), values, errors, testing, and known limitations. `test_GUIDE_examples.py` runs its
  thirteen examples.

### Deferred: the generated API reference
Most public names have no docstring (285 of 392 on 2026-10-19), so a generated reference would
be empty. Writing them, generating HTML with a standard tool and checking coverage in CI is
`PY-API-REFERENCE-NOT-GENERATED` (P2). It is tracked with the rest of the Python documentation as
DOC-12 in `reference/api/API_DOCS_GAP_ANALYSIS.md`.

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

- **`reference/api/API_DOCS_GAP_ANALYSIS.md`**: §12 Python bindings and the DOC-12 tracker row
  described `liquers-py` as having no README, guide or examples; they now point at the new
  documents and at what remains.

Each kept document has a History row with source phase-5 and a matching `reviewed:` date.
`LANGUAGE-INTEGRATION_GUIDE.md` §6 Python also links the two new documents (2026-10-19).

Also updated:
- `CLAUDE.md`: a "Python bindings" entry under "Where Code Goes", pointing at the reference and
  the guide;
- `liquers-py/README.md`, which links both.

### Links and Capability Map
`specs/README.md`: the "Python integration (liquers-py)" entry moves from *designing* to *built*. It
now links the Python guide, the Python integration reference, the integration guide's §6 and the
README matrix, keeping the design link.

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
- `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING` (P3): a traceback crossing several languages is one
  flat string. This is an open problem the owner raised; nested Python stacks are concatenated for
  now.
- `PY-API-REFERENCE-NOT-GENERATED` (P2): the deferred API reference, above.
- `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` (P3): `repr(Query)` and `repr(Key)` quote their text twice,
  and several classes print Rust debug output. Found while writing the guide.
- `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` (P2, pre-existing): the guide requires no user
  documentation from an integration. Updated with this design's evidence and the three-document
  split, instead of filing a duplicate.
- `COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED` (P2): a command named `q` or `v` registers and never
  runs. Found by the Phase 5 review.
- `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR` (P2): a missing key raises `GeneralError` without
  its key. Found by the Phase 5 review.
- `PY-METADATA-FIELDS-ARE-METHODS` (P3): `Metadata`'s fields are methods where `State` and
  `AssetRef` use properties.

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

What remains is in the issues above; nothing is left half-done in the design. No owner decision
is open: Phase 1 questions 1, 5 and 6 were decided on 2026-10-19 (M0 is PR #163).

## Validation

- The Python guide and reference had two independent reviews of their own (2026-10-19): one
  checked every claim against the code and by running probes, the other read them as a new user
  and against `DOCS_STRUCTURE_GUIDE.md`. Fixed from them:
  - `Context.error` *does* fail the asset, contrary to both documents and the `context.rs`
    docstring; all three now say so, and `test_EVAL_context_error_fails_the_result` pins it;
  - pickling of the classes, `validate()`'s scope, behaviour after `shutdown()`, `State` being a
    snapshot, `store_set`'s caller-thread serialization, unannotated arguments arriving as `str`,
    the container rule, the `filename` keyword and the OpenSSL requirement were corrected;
  - the guide gained the store-prefix rule, a YAML file example with `${VAR}`, argument escapes,
    reserved names, a metadata table, recipes, thread safety, the common errors, install notes
    and definitions of its terms; the reference gained a method table, reserved names and
    concurrency;
  - a duplicate issue was withdrawn in favour of `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`, and
    three issues were filed from what the reviews found;
  - `liquers-py/tests/test_GUIDE_examples.py` now runs every guide example (13), so they cannot
    rot.

  Not adopted: moving the guide's issue references out (they are plain text, not links, so a move
  to `docs/` breaks nothing) and removing the guide's user-level conversion and error tables in
  favour of the reference (a user needs them; the guide points at the reference for the exact
  rules).

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
  missing sections.

