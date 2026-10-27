---
id: PY-API-REFERENCE
kind: design
title: A script generates the Python package's API reference from its docstrings
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py, docs]
issues: [PY-API-REFERENCE-NOT-GENERATED]
created: 2026-10-27
---
# A script generates the Python package's API reference from its docstrings

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written, not
approved, nothing implemented. The docstrings and their checks were split off the same day into
`design/py-docstrings/`; see Scope Changes.

## Phase 1: High-Level Design

### Purpose

Nothing generates a per-method reference for `liquers`. This design adds a script that builds an
HTML API reference from the package's docstrings and runs it in the Python test loop, which is the
second half of the language-integration guide's `DOCS04`. The docstrings themselves, and the test
that every public name has one, are `py-docstrings`, which this design requires. It serves
`VISION.md`'s "Python data science" use case and its "Better Python integration" priority.

### Problem Example

```bash
ls scripts/                 # check-build-matrix.sh docs_index.py test-liquers-py.sh test_docs_index.py
python3 -m pdoc liquers     # no such step anywhere; no HTML reference exists to read or link
```

Expected: `bash scripts/build-liquers-py-docs.sh` writes `target/liquers-py-docs/liquers.html`
with a section per class, built from the docstrings. Verified at HEAD (`04667a80`): most public
names have no docstring yet (311 of 369 by a source scan; the counts are `py-docstrings`' Problem
Example). No generator configuration or docs script exists; `scripts/` has
`test-liquers-py.sh`, which already pip-installs `maturin pytest polars pillow`.

### Scope and Acceptance Criteria

- **AC-1** ~~Every public name is documented~~ Retired 2026-10-27: moved to `py-docstrings` (its AC-1).
  WHEN ~~the coverage test walks `lq.__all__` and each listed class's own public attributes~~
  THEN ~~every one has a non-empty `__doc__`~~
- **AC-2** ~~Coverage cannot regress~~ Retired 2026-10-27: moved to `py-docstrings` (its AC-2).
  WHEN ~~a public method or class without a docstring is added~~
  THEN ~~the coverage test fails and names it~~
- **AC-3** ~~Every public callable has a signature~~ Retired 2026-10-27: moved to `py-docstrings` (its AC-3).
  WHEN ~~`inspect.signature` is applied to each public function and method~~
  THEN ~~it does not raise~~
- **AC-4** An HTML API reference is generated
  WHEN `bash scripts/build-liquers-py-docs.sh` runs
  THEN it writes an HTML reference under `target/liquers-py-docs/` that lists every name in
  `lq.__all__`
- **AC-5** The test loop builds it
  WHEN `bash scripts/test-liquers-py.sh` runs and the generator fails, meaning `pdoc` exits
  non-zero (it does when the module cannot be imported; it renders a malformed docstring
  leniently, so that is not a failure) or the written page lacks a name of `lq.__all__`
  THEN the script fails

Non-goals: writing docstrings or checking their coverage (`py-docstrings`); type stubs (`.pyi`, `py.typed`, the guide's `STUBS` feature); publishing the reference
(`PACKAGE`); renaming or reshaping any API (`py-metadata-fields-as-properties`); moving the guide
to `docs/`.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — the generator is a new (development) dependency installed
  by a script, chosen between two tools (rule 4, and rule 5 until question 1 is decided). The
  eligible docstring part was split off into `py-docstrings`.
- **Leading issue:** question 1.
- **Explanation:** only the generator needs a decision, and either tool reads the same docstrings.
  Without `py-docstrings` the generated reference would list bare names, so it lands first.
- **Open questions:**
  1. **Open design - generator:** `pdoc` (recommended): no configuration file, reads `__doc__` and
     `__text_signature__` of an extension module, renders Markdown (the backtick style the
     existing docstrings use), one command. Sphinx `autodoc`: needs `conf.py` and reStructuredText,
     worth it only if the reference is to join other Sphinx documents, which none are.
  2. **Proposed resolution - output and publication:** generated into `target/liquers-py-docs/`,
     never committed (a generated copy drifts); publishing waits for `PACKAGE`.
  3. ~~Proposed resolution - split~~: done on 2026-10-27 (`py-docstrings`, Scope Changes).
  4. ~~Implementation detail - docstring style~~: moved to `py-docstrings`.

### Design Dependencies

- `py-docstrings` (ready, eligible): **requires**. It writes the docstrings, `text_signature`s
  and the coverage test; this design renders them. Implement it first.
- `py-pypy-ci-job` (needs decision) — **overlaps** (weak): whether `pdoc` runs on PyPy is this
  design's concern if that job lands.
- `language-binding-tests-in-ci` — **overlaps**: once CI runs `scripts/test-liquers-py.sh`, AC-5
  runs there too and CI needs `pdoc` installed by that script. No order.
- The overlaps with `py-metadata-fields-as-properties`, `py-metadata-typed-errors`,
  `py-python-style-reprs` and `py-command-decorator-declaration-keywords` (same `#[pymethods]`
  blocks) moved to `py-docstrings` with the docstrings; this design no longer edits
  `liquers-py/src/`.

### Scope Changes

- **2026-10-27, docstrings split off.** The docstrings, the `text_signature` attributes, the
  generated error-class docstrings and the coverage and signature tests (former AC-1 to AC-3,
  Phase 2 items 1-3, Phase 4 steps 1-6, question 4) moved to `py-docstrings` and its issue
  `PY-PUBLIC-NAMES-HAVE-NO-DOCSTRINGS`, so the eligible part is not held by question 1 (overlap
  exclusion E1). AC-1 to AC-3 are retired, not renumbered. This design keeps the generator (AC-4,
  AC-5) and its documents; size drops from `M` to `S`. Readiness stays `needs-decision`
  (question 1).

### Consolidated Findings

- The findings on PyO3 docstrings, `_install` and the coverage set moved to `py-docstrings`.
- `pdoc` reads `__doc__` and `__text_signature__` of an extension module, so it needs nothing
  beyond what `py-docstrings` adds.
- The Python suite is not in CI yet (`PY-TESTS-NOT-RUN-IN-CI`).

## Phase 2: Architecture

### Solution

Former items 1-3 (docstrings, generated error-class docstrings, coverage test) are
`py-docstrings`.

4. **Generator.** `scripts/build-liquers-py-docs.sh [module]` (default `liquers`): reuses the
   venv layout of `test-liquers-py.sh` (or takes `VIRTUAL_ENV`), installs `pdoc` if absent, runs
   `python3 -m pdoc "$module" -o target/liquers-py-docs` under `set -e`, then checks that the
   page names every entry of the module's `__all__` and exits non-zero otherwise. The `module`
   argument exists for the AC-5 check. `scripts/test-liquers-py.sh` installs `pdoc` with its other
   test packages, before pytest, so `test_DOCS04_api_reference_lists_every_public_name`
   (`importorskip("pdoc")`) runs on a fresh venv, and calls the script after pytest. pip resolves
   `pdoc` 16 on Python 3.10 or newer and 15.0.4 on 3.9 (16 needs 3.10); the check reads names
   only, so the differing output does not matter.

Rejected: a hand-written per-method reference in `specs/` (the issue rules it out; it drifts).

### Changes

- New `liquers-py/tests/test_DOCS_api_reference.py` (the reference test only), new
  `scripts/build-liquers-py-docs.sh`,
  `scripts/test-liquers-py.sh` (one call and its header comment).
- Documents: `liquers-py/README.md` "Building and testing" (the docs script; `pdoc` among the
  installed packages) and `DOCS` row (`DOCS04` passes); `specs/reference/PYTHON_INTEGRATION.md` §8
  limitation removed; `specs/guides/PYTHON_GUIDE.md` §1 (how to build the reference) and §10;
  `specs/reference/api/API_DOCS_GAP_ANALYSIS.md` DOC-12 row. History rows and `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `pdoc` fails on an extension module or a free-threaded build | runs on every interpreter of the approved CI matrix (planned 3.9, 3.14, 3.14t; 3.9 gets `pdoc` 15.0.4); Sphinx is the fallback | medium |
| Network needed for `pip install pdoc` | the test script already installs from pip | high |

Recovery: revert; a script and a test file change no behaviour.

## Phase 3: Examples and Tests

### Examples

After the change, `bash scripts/build-liquers-py-docs.sh` writes
`target/liquers-py-docs/liquers.html`, with a section per class showing the signatures and
docstrings that `py-docstrings` added.

### Tests

In `liquers-py/tests/test_DOCS_api_reference.py`:

- AC-1, AC-2 and AC-3 are retired here; their tests are `py-docstrings`'
  (`liquers-py/tests/test_DOCS_docstrings.py`)
- `test_DOCS04_api_reference_lists_every_public_name` — `pytest.importorskip("pdoc")`, runs
  `python3 -m pdoc liquers -o <tmp_path>`, and checks the output names each `__all__` entry: AC-4
- `docs_script_fails_on_generator_error`: `bash scripts/build-liquers-py-docs.sh liquers_missing`
  exits non-zero (pdoc cannot import the module), and so does `test-liquers-py.sh` with that
  call temporarily pointed at `liquers_missing` (reverted): AC-5

Run: `bash scripts/test-liquers-py.sh -k DOCS04`, and
`bash scripts/build-liquers-py-docs.sh`.

## Phase 4: Implementation Plan

### Steps

Precondition: `py-docstrings` has landed. Former steps 1-6 are its Phase 4.

- [ ] 7. `scripts/build-liquers-py-docs.sh` (new), the reference test, `scripts/test-liquers-py.sh` call — proof: `bash scripts/build-liquers-py-docs.sh`; `bash scripts/test-liquers-py.sh -k DOCS04` (AC-4)
- [ ] 8. Documents listed in Phase 2 §Changes, History rows, `reviewed:` — proof: `python3 scripts/docs_index.py --check`
- [ ] 9. `docs_script_fails_on_generator_error` (AC-5), the temporary change reverted; issue `status: closed` with a resolution note; `python3 scripts/docs_index.py` — proof: `python3 scripts/docs_index.py --check`

Steps are numbered from 7 so that references to them stay valid. Rollback: revert per file; nothing depends on the script.

### Validation

`bash scripts/test-liquers-py.sh` (whole suite, generator included),
`python3 scripts/docs_index.py --check`. Review the diff: no change under `liquers-py/src/`, no
`specs/command_registry.yaml` change.
