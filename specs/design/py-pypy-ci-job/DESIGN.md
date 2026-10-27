---
id: PY-PYPY-CI-JOB
kind: design
title: CI runs the liquers-py Python suite on PyPy 3.11
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py, build]
issues: [PY-PYPY-SUPPORT-UNVERIFIED]
created: 2026-10-27
---
# CI runs the liquers-py Python suite on PyPy 3.11

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

`liquers-py` says it supports PyPy 3.11 or newer, but nothing has ever built or tested it there.
This design appends a PyPy 3.11 row to whatever `liquers-py` CI matrix the owner approves in
`language-binding-tests-in-ci`, and makes `scripts/test-liquers-py.sh` skip the Rust unit tests on
PyPy, which cannot compile there, so that the claim is checked on every relevant PR.

### Problem Example

```bash
PYTHON=pypy3.11 bash scripts/test-liquers-py.sh
# today:    never run; the Rust half (`cargo test -p liquers-py --no-default-features --lib`)
#           does not compile for PyPy, because its tests call `Python::initialize()`, which PyO3
#           leaves out on PyPy
# expected: maturin builds the extension, pytest passes (polars tests skip), the Rust half is
#           skipped with a message, and CI runs this on PyPy 3.11
```

Verified at HEAD (`04667a80`) and in the dependencies:

- `scripts/test-liquers-py.sh` names the venv from `sys.implementation.name` (PyPy gets
  `liquers-py-venv-pypy3.11`) and ends with `cargo test -p liquers-py --no-default-features --lib`.
- `Python::initialize()` is called in the tests of `liquers-py/src/value.rs`, `environment.rs`,
  `error.rs` and `aio.rs`. In `pyo3-0.30.1/src/marker.rs` it is `#[cfg(not(any(PyPy, GraalPy)))]`,
  so on PyPy the test target fails **to compile**; it is not a runtime failure.
- `pyo3-ffi` 0.30.1 supports PyPy 3.11 or newer.
- polars cannot be installed binary-only on PyPy 3.11 (`polars-runtime-32` has no compatible
  wheel); pillow has a `pp311` wheel. The tests that need polars or PIL already skip without them
  (`pytest.importorskip` in `test_VALUE_value_bridge.py` and `test_GUIDE_examples.py`).
- `language-binding-tests-in-ci` Phase 2 rewrites the script's install line into a per-package
  `--only-binary :all:` loop that prints `no $pkg wheel for this interpreter; its tests will skip`
  and continues. Under it, polars is skipped on PyPy with no change here.
- The PyPy claims are in `liquers-py/README.md` "Supported Python", `PYTHON_GUIDE.md` §1,
  `CLAUDE.md` and the `pyproject.toml` classifiers.

**Unverified:** that `actions/setup-python` resolves `pypy3.11`; that `liquers-py` compiles and
imports on PyPy; how GC-timing tests (`RUNTIME05` counts released handles) behave under PyPy's
garbage collector.

### Scope and Acceptance Criteria

- **AC-1** PyPy runs in CI
  WHEN the `liquers-py` workflow runs
  THEN one more job of the approved matrix runs `scripts/test-liquers-py.sh` on PyPy 3.11,
  independent of the CPython jobs, and fails when a test in `liquers-py/tests/` fails
- **AC-2** The script skips what PyPy cannot run
  WHEN `scripts/test-liquers-py.sh` runs under an interpreter whose `sys.implementation.name` is
  `pypy`
  THEN polars is skipped by the sibling's no-wheel path, the Rust unit tests are skipped with a
  line saying why, and everything else runs unchanged
- **AC-3** CPython runs are unchanged
  WHEN the script runs under each CPython interpreter of the approved matrix
  THEN it installs and runs exactly what it does without this design
- **AC-4** The documents say what is verified
  WHEN the PyPy job is green on the implementing PR
  THEN `liquers-py/README.md` "Supported Python" says that CI runs the suite on PyPy 3.11 and that
  the Rust unit tests and the polars tests do not run there

Non-goals: fixing any failure the job finds (each is filed as its own issue); PyPy 3.12 or newer
before `setup-python` offers it; wheels or packaging; changing any test.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible (rule 5) until question 1 is decided. Once it is, the work is a
  tooling fix of size `S` (rules 1, 2): one matrix value and one interpreter guard in
  `scripts/test-liquers-py.sh`, no structure, interface, dependency or crate change (rules 3, 4, 6).
- **Leading issue:** question 1.
- **Explanation:** the change is fully specified against the sibling's matrix. Whether to spend a
  CI job on PyPy, and what the support claim says if that job is red, is the same kind of
  matrix-and-cost call as the sibling's own decision 1.
- **Open questions:**
  1. **Open design question: the PyPy row, and the claim if it is red.** Recommended: append
     `pypy3.11` to whatever matrix is approved for `language-binding-tests-in-ci`, as a required
     job. Land it only green. If it is red at HEAD, file each failure, and keep the support claim
     until the owner chooses between fixing the failures and narrowing the claim ("builds; suite
     not verified"). The alternative, no PyPy job, should come with narrowing the claim now.

### Design Dependencies

- `language-binding-tests-in-ci` (needs decision): **requires**. It creates
  `.github/workflows/liquers-py-tests.yml` and the no-wheel install loop that this design relies
  on. The precondition is that it has landed.
- `py-docstrings`, `py-api-reference`: **overlaps** (weak). Their tests run in this job too;
  `pdoc` on PyPy is `py-api-reference`'s concern.

### Overlap Record

This was a close call (`overlap.md`). Against `language-binding-tests-in-ci`, **T3** holds (the
same matrix in the same workflow) and **T2** holds (both depend on the matrix-and-cost decision).
They are kept apart, not merged, because the owner's pending decision card for that design
explicitly scopes it without PyPy and says PyPy gets its own design, so the PyPy question can be
decided separately (E5 in spirit). E1 does not apply: neither design is eligible today.

## Phase 2: Architecture

### Solution

1. **`scripts/test-liquers-py.sh`**, as rewritten by the sibling. Before the Rust half, read
   `impl="$(python3 -c 'import sys; print(sys.implementation.name)')"`; when it is `pypy`, print
   `==> skipping cargo test -p liquers-py: PyO3 has no Python::initialize on PyPy` and exit 0. The
   header comment gains one line. The install loop is not touched: it already skips polars.
2. **`.github/workflows/liquers-py-tests.yml`.** Append `pypy3.11` to `matrix.python` of the
   approved matrix. The job already passes `PYTHON` from setup-python's `python-path` and uses
   `fail-fast: false`. No other step changes.

Rejected: a separate PyPy workflow (duplicates the path filter and disk steps); skipping by a
workflow environment variable (a developer running the script under PyPy locally would hit the same
compile failure); building polars from source on PyPy (a long Rust build, not what the claim is
about); `continue-on-error` on the PyPy job (a check nobody must look at verifies nothing).

Known-issue preflight: no open issue blocks. `LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI` must be fixed
first.

### Changes

`scripts/test-liquers-py.sh` (one guard and a header line) and
`.github/workflows/liquers-py-tests.yml` (one matrix value). Documents: `liquers-py/README.md`
"Supported Python" (AC-4); `PYTHON_GUIDE.md` §1 keeps its line, which stays true. No code,
command or registry change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| The suite fails on PyPy (GC-timing tests such as `RUNTIME05`, or a real defect) | Step 3 stop rule: file each failure, leave the PR for the maintainer | medium |
| `liquers-py` does not compile or import on PyPy | Same stop rule; question 1 then decides the claim | medium |
| `setup-python` does not resolve `pypy3.11` | Same stop rule (unverified) | medium |
| The guard changes CPython runs | AC-3: the guard is inside `if [ "$impl" = pypy ]` | high |

Recovery: drop the matrix value; the script guard is harmless on CPython.

## Phase 3: Examples and Tests

### Examples

```bash
PYTHON=$(uv python find pypy3.11) bash scripts/test-liquers-py.sh   # prints "no polars wheel…" and the skip line
bash scripts/test-liquers-py.sh                                      # CPython: unchanged
```

### Tests

No unit test applies; these are script and CI properties.

- `wf_pypy_job_green`: on the implementing PR the `pypy3.11` job passes, its log shows
  `liquers-py-venv-pypy3.11`, the no-polars-wheel line and the skip line: AC-1, AC-2
- `wf_pypy_red_on_failure`: a temporary commit breaks one assertion in
  `liquers-py/tests/test_ERROR_errors.py`; the PyPy job fails, and the commit is reverted: AC-1
- `script_cpython_unchanged`: the CPython job logs run the Rust half, and
  `bash -n scripts/test-liquers-py.sh` passes: AC-3
- `docs_review`: the README diff states the PyPy coverage and its two gaps: AC-4

## Phase 4: Implementation Plan

### Steps

Precondition: `language-binding-tests-in-ci` has landed, and question 1 is decided.

- [ ] 1. `scripts/test-liquers-py.sh` — the `impl` guard before the Rust half, header line — `bash -n scripts/test-liquers-py.sh`; `bash scripts/test-liquers-py.sh -k parse` on CPython runs as before (AC-3)
- [ ] 2. `.github/workflows/liquers-py-tests.yml` — append `pypy3.11` to `matrix.python` — `python3 -c 'import yaml,sys; yaml.safe_load(open(sys.argv[1]))' .github/workflows/liquers-py-tests.yml`
- [ ] 3. Open the PR and watch the jobs — `wf_pypy_job_green`, `script_cpython_unchanged`, then `wf_pypy_red_on_failure` (temporary commit, revert). If the PyPy job fails at HEAD for any reason other than a runner fault, stop: file each failure (`DOCS_STRUCTURE_GUIDE.md` §4.8), do not edit tests, do not add `continue-on-error`, and leave the PR unmerged for the maintainer
- [ ] 4. `liquers-py/README.md` "Supported Python" (AC-4); `specs/issues/PY-PYPY-SUPPORT-UNVERIFIED.md` `status: closed` with a resolution note; this design `complete`; `specs/README.md` row; `python3 scripts/docs_index.py` — `python3 scripts/docs_index.py --check`

Rollback: revert per step.

### Validation

All `liquers-py` jobs green after the revert in step 3, and `python3 scripts/docs_index.py
--check`. The diff holds the script, the workflow, the README, the issue, the design and the
index, and nothing under `liquers-py/src/` or `liquers-py/tests/`.
