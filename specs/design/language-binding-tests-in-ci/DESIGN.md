---
id: LANGUAGE-BINDING-TESTS-IN-CI
kind: design
title: CI runs the liquers-py test loop and the liquers-web Node loop
form: compact
status: complete
readiness: ready
autofix: not-eligible
area: [build, py, web]
issues: [LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI, PY-TESTS-NOT-RUN-IN-CI]
merged: 2026-10-27
created: 2026-10-27
---
# CI runs the liquers-py test loop and the liquers-web Node loop

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

Both language bindings have test loops that run only by hand. `build-matrix` proves that
`liquers-py` and `liquers-web` compile, not that they behave, so a core change can make their
conformance tables untrue without any check failing. This design adds two workflows. One runs
`scripts/test-liquers-py.sh` on three interpreters. The other runs the `liquers-web` Node loop
with both feature sets.

### Problem Example

Verified at HEAD (`04667a80`):

- `.github/workflows/` holds `build-matrix.yml` (`cargo check` only) and `docs-check.yml`.
- `scripts/test-liquers-py.sh` makes a venv per interpreter (`PYTHON`, default `python3`) under
  `target/` and pip-installs `maturin pytest polars pillow` (line 42, under `set -euo pipefail`).
  It then runs `maturin develop --features debug-handles`, `pytest liquers-py/tests` and
  `cargo test -p liquers-py --no-default-features --lib`.
- The Node loop is `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles`, and the same with `,records`. The runner is `wasm-bindgen-test-runner`
  (`.cargo/config.toml`), whose version must match the `wasm-bindgen` entry in `Cargo.lock`
  (`0.2.129`).
- `PYTHON_INTEGRATION.md` §8 and `liquers-py/README.md` both say that the suite has passed on 3.13
  and 3.14t locally, "not in CI".

Scenario: a core PR adds an `ErrorType` variant. `build-matrix` passes. `test_ERROR01` (Python) and
`OBJECT06` (web) would fail, but nothing runs them, so the PR merges green. It should fail on that
PR.

### Scope and Acceptance Criteria

- **AC-1** Python loop gates a PR
  WHEN a PR changes a file that `liquers-py` builds from, and a Python or Rust test of
  `liquers-py` fails
  THEN the `liquers-py` workflow fails
- **AC-2** Three interpreters, independent
  WHEN the `liquers-py` workflow runs
  THEN `scripts/test-liquers-py.sh` runs as separate jobs on CPython 3.10, 3.14 and free-threaded
  3.14t, and each job runs to its own end (failed or passed, never cancelled by another)
- **AC-3** Web Node loop gates a PR
  WHEN a PR changes a file that `liquers-web` builds from, and a Node-loop test fails with
  either feature set
  THEN the `liquers-web` workflow fails
- **AC-4** The runner matches the lockfile
  WHEN the web workflow installs `wasm-bindgen-cli`
  THEN its version is read from `Cargo.lock` at run time
- **AC-5** Path filters cover the dependency closure
  WHEN a change touches only a `liquers-*` crate that the binding builds from, `Cargo.toml`,
  `Cargo.lock`, `.cargo/**` or the workflow file (for Python also `scripts/test-liquers-py.sh`)
  THEN that binding's workflow runs; a change only under `liquers-axum/`, `liquers-mcp/` or
  `liquers-agent/` runs neither, and the `push` and `pull_request` lists are identical
- **AC-6** Runner resources suffice
  WHEN either workflow runs on `ubuntu-latest`
  THEN it does not fail for lack of disk, and it does not evict `build-matrix`'s cache
- **AC-7** A missing wheel is not a failure
  WHEN no binary wheel of `polars` or `pillow` exists for the interpreter (polars on 3.14t)
  THEN the script reports this and continues, and the tests that need the package skip

Non-goals: PyPy (not yet verified anywhere; `py-pypy-ci-job`); the browser, Playwright, `check-stubs.sh` and
guide-example loops; changes to tests.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible (rule 5) until the owner approves the CI matrix and its cost.
  `design/liquers-py-async/` Phase 2 recorded that "the CI matrix and its cost are the owner's
  call". Once that is approved, the design is eligible. It is a tooling fix to two new workflows
  and one script line (rule 2). It adds no structure (rule 3), changes no interface, dependency or
  feature flag (rule 4), and touches no crate (rule 6). Size `M`.
- **Leading issue:** decision 1.
- **Explanation:** both loops pass locally, and every runner choice below is settled with
  evidence. Only the policy question is open.
- **Open questions:**
  1. **Resolved 2026-10-31 (Orest): CPython 3.10, 3.14 and 3.14t,** with the supported floor raised
     from 3.9 to 3.10, since 3.9 is past its end of life (Scope Changes). Originally proposed: the
     matrix is CPython 3.9, 3.14 and 3.14t, plus the
     web Node loop with two feature sets. That makes four jobs, triggered only by changes in the
     bindings' dependency closure. The repository is public, so hosted minutes are free; the cost is
     wall time. That time is estimated, not measured: a cold run takes about 25-35 minutes per
     Python job and 20 for web, and a warm cached run about 10-15. The jobs run in parallel, so a PR
     waits about as long as the slowest job. Recommended: approve.

### Design Dependencies

- `build-matrix-path-filters` (complete): **overlaps**. These workflows reuse its `liquers-*/**`
  pattern, so that a new crate is covered without a workflow edit.
- `liquers-py-async` (complete): **overlaps**. Its AC-8 (free-threaded) becomes a CI property.
- `build-matrix-disk-budget` (complete): **overlaps**. It is the same disk constraint.
- `py-pypy-ci-job`: **required-by**. It appends a PyPy row to the approved matrix.

### Overlap Record

**T4:** `LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI`'s Python half is `PY-TESTS-NOT-RUN-IN-CI` on one
interpreter. **T3:** both would add the same workflow. E1 does not separate them, because the merged
design is labelled as one item. The Python half on its own would raise the same matrix decision.
The wider issue leads.

### Scope Changes

- **2026-10-31**: at the decision Orest raised the supported CPython floor to 3.10, so the oldest
  job is 3.10 and the floor change rides this PR: `liquers-py/pyproject.toml`
  (`requires-python = ">=3.10,<3.16"`), `liquers-py/README.md`, `specs/guides/PYTHON_GUIDE.md` §1
  and `CLAUDE.md`. The workflow also installs `libssl-dev`, as `build-matrix.yml` does, because
  `liquers-lib` links the system OpenSSL.
- **2026-10-31**: running the suite on 3.10 found a defect: a Python traceback carried no frames
  before CPython 3.12, because `format_traceback` (`liquers-py/src/error.rs`) passed only the
  exception value, whose `__traceback__` PyO3 does not set there. It now passes type, value and
  traceback. `test_ERROR03_language_exception_includes_class_and_stack` and
  `test_ERROR_nested_python_tracebacks_are_concatenated` failed on 3.10 before the fix; the whole
  suite passes on 3.10 and 3.13 after it.
- **2026-10-31**: `Cargo.lock` is not committed (`.gitignore`), so the web job's first CI run failed
  reading it. The job now runs `cargo generate-lockfile` before reading the `wasm-bindgen` version.
- **2026-10-31**: on CI, 3.10 passed every test and then aborted as the interpreter exited
  ("Fatal Python error: PyGILState_Release", or a segfault). Cause: a Python command still running
  on a runtime blocking thread when the interpreter finalized. Before CPython 3.14 such a thread is
  ended as it takes the GIL back, and the unwind through liquers-py's frames releases a thread
  state that is no longer current. A script exiting with a running command crashed 3.10 every time
  and 3.13 never. Fixed with an `atexit` hook (`liquers-py/src/exit.rs`) that refuses new command
  calls, cancels the running ones and waits up to 5 s for every liquers-thread attach; covered by
  `test_RUNTIME_interpreter_exit_with_a_running_command`. Also: RUNTIME05's handle baseline is
  taken after `gc.collect()`, since a collection during its wait took the count under a baseline
  that still held earlier tests' garbage (seen once in 25 local 3.10 runs).

- **2026-10-27**: one line of `scripts/test-liquers-py.sh` is in scope (AC-7). polars ships only
  `abi3` wheels, which a free-threaded interpreter cannot load, so on 3.14t pip falls back to the
  sdist and would build polars from source.

### Consolidated Findings

- **Wheels.** These are what `pip --dry-run --only-binary :all:` resolves:
  - 3.10 (checked 2026-10-31): `polars 2.0.0` with `polars-runtime-32 2.0.0` (`cp310-abi3`), and
    `pillow 12.3.0` (`cp310`);
  - 3.14: `polars 2.1.0` (`cp311-abi3`) and `pillow 13.0.0`;
  - 3.14t (run on 3.14.6t): `pillow 13.0.0` (`cp314t`); for polars there is no wheel, only
    `polars_runtime_32-2.1.0.tar.gz`.
  The 3.14t job therefore runs without polars. The polars tests skip (`test_VALUE_value_bridge.py`
  and `test_GUIDE_examples.py` use `importorskip`).
- **3.14t binary.** It is `python3.14t`, so `PYTHON` comes from setup-python's `python-path`.
- **Disk.** Each Python job builds `liquers-lib` with polars under two profiles. That is estimated at
  8-10 GB each (`design/liquers-py/` Phase 2), so the job frees preinstalled toolchains first.
- **Cache.** The 10 GB repository quota is shared with `build-matrix`. Only `main` saves, and
  only one Python job does.

## Phase 2: Architecture

### Solution

**`scripts/test-liquers-py.sh` line 42** becomes:

```bash
python3 -m pip install --quiet --disable-pip-version-check maturin pytest
for pkg in polars pillow; do
  python3 -m pip install --quiet --disable-pip-version-check --only-binary :all: "$pkg" \
    || echo "==> no $pkg wheel for this interpreter; its tests will skip"
done
```

**`.github/workflows/liquers-py-tests.yml`.** `on.pull_request.paths` and `on.push.paths` (branch
`main`) are identical, and `workflow_dispatch` is also a trigger:

```yaml
      - 'liquers-*/**'
      - '!liquers-web/**'
      - '!liquers-axum/**'
      - '!liquers-mcp/**'
      - '!liquers-agent/**'
      - 'Cargo.toml'
      - 'Cargo.lock'
      - '.cargo/**'
      - 'scripts/test-liquers-py.sh'
      - '.github/workflows/liquers-py-tests.yml'
```

The job `test` has `strategy.fail-fast: false`, `matrix.python: ['3.10', '3.14', '3.14t']`,
`timeout-minutes: 75` and `env: CARGO_INCREMENTAL: 0`. Its steps are:

1. `actions/checkout@v4`.
2. "Free runner disk":
   `sudo rm -rf /usr/share/dotnet /usr/local/lib/android /opt/ghc /opt/hostedtoolcache/CodeQL; df -h /`.
3. `dtolnay/rust-toolchain@stable`.
4. `Swatinem/rust-cache@v2` with `shared-key: liquers-py` and
   `save-if: ${{ github.ref == 'refs/heads/main' && matrix.python == '3.14' }}`.
5. `actions/setup-python@v5` with `id: py` and `python-version: ${{ matrix.python }}`.
6. `rm -rf target/liquers-py-venv-*`. A restored venv would point at a toolcache interpreter that
   no longer exists.
7. `bash scripts/test-liquers-py.sh` with `env: PYTHON: ${{ steps.py.outputs.python-path }}`, then
   `df -h /`.

**`.github/workflows/liquers-web-tests.yml`** uses the same filter, with `!liquers-py/**` in place
of `!liquers-web/**` and without the script entry. The job `node` has `timeout-minutes: 60` and
`CARGO_INCREMENTAL: 0`. Its steps are checkout, free disk, the toolchain with
`targets: wasm32-unknown-unknown`, and rust-cache with
`save-if: ${{ github.ref == 'refs/heads/main' }}`. Then `actions/setup-node@v4` with
`node-version: '22'`, and:

```yaml
      - name: Install wasm-bindgen-test-runner matching Cargo.lock
        run: |
          ver=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
          test -n "$ver"
          cargo install wasm-bindgen-cli --version "$ver" --locked
      - run: cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
      - run: cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records
```

Each header comment names what the workflow runs and names this design.

**Free-threaded interpreter: `actions/setup-python` with `3.14t`.** This is documented in
setup-python's `docs/advanced-usage.md`: the `t` suffix, from 3.13 on. `actions/python-versions`'
manifest lists `python-3.14.10-linux-24.04-x64-freethreaded.tar.gz`, and also
`python-3.10.22-linux-24.04-x64` (checked 2026-10-31). The `v5` tag floats to the newest 5.x release.

Rejected alternatives:
- **`astral-sh/setup-uv` with `uv python find 3.14t`**, as `liquers-py/README.md` does locally. It
  is one more third-party action and a second way to find interpreters, and setup-python already
  serves all three versions.
- **One workflow with both jobs.** A path filter applies per workflow.
- **Jobs in `build-matrix.yml`.** That would mix compile checks with test runs.
- **The 3.15 beta as "newest".** There are no stable wheels for a beta. 3.15 replaces 3.14 when it
  is released.
- **`taiki-e/install-action`.** Another third-party action; `cargo install --locked` costs minutes.
- **Building polars from its sdist on 3.14t.** That is a Rust build of polars inside the job.
- **Crates listed by name.** That is the defect `build-matrix-path-filters` removed.

Known-issue preflight: nothing blocks.

### Changes

- New: the two workflow files.
- Changed: line 42 of `scripts/test-liquers-py.sh` and its header comment, step 2.
- Documents (step 4):
  - `PYTHON_INTEGRATION.md` §8: the bullet "The Python suite runs locally, on CPython 3.13 and
    3.14t, but not in CI (`PY-TESTS-NOT-RUN-IN-CI`)" becomes "CI runs the Python suite on CPython
    3.10, 3.14 and 3.14t (3.14t without polars, which has no free-threaded wheel); PyPy is not
    tested". Add a History row and bump `reviewed:`. (With the 2026-10-31 decision: 3.10.)
  - `liquers-py/README.md` "Supported Python": the same facts.

No code, command or registry change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A loop is red at HEAD | Step 3 handles it | medium |
| Disk runs out | About 20 GB freed, `CARGO_INCREMENTAL=0`, `df -h /` logged | medium |
| The cache evicts `build-matrix`'s | Only `main` saves; the three Python jobs share one key saved by 3.14 | high |
| Unpinned pip packages drift | It is the same unpinned set developers use; AC-7 covers a missing wheel | high |
| Wall time | Parallel jobs, path filters, warm cache (decision 1) | high |

Recovery: revert the two workflow files and the script line.

## Phase 3: Examples and Tests

### Examples

```bash
PYTHON=$(uv python find 3.14t) bash scripts/test-liquers-py.sh   # prints "no polars wheel…", passes
grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p'   # 0.2.129
```

### Tests

These are CI properties, so there is no new unit test.

- `wf_py_matrix_green`: all three Python jobs pass, and each log shows its venv tag
  (`cpython3.14t` for the free-threaded job) and `df -h /`: AC-2, AC-6
- `wf_web_node_green`: both Node steps pass, and the install step prints the lockfile version:
  AC-3, AC-4, AC-6
- `wf_red_on_failure`: a temporary commit breaks one assertion in `test_ERROR_errors.py` and one in
  `liquers-web/tests/objects_OBJECT.rs`. Both workflows fail at their test step, and all three
  Python jobs end `failure` rather than `cancelled`. The commit is then reverted: AC-1, AC-2, AC-3
- `local_missing_wheel`: the Example run on 3.14t prints the skip line and passes: AC-7
- `wf_filters_review`, a static diff check: each file's two `paths` lists are identical and match
  Phase 2, and the negations come after `liquers-*/**`; the `save-if` lines are as in Phase 2: AC-5,
  AC-6

## Phase 4: Implementation Plan

### Steps

- [x] 1. `.github/workflows/liquers-py-tests.yml` and `liquers-web-tests.yml` as in Phase 2. Proof: `wf_filters_review`; `python3 -c 'import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]' <files>`
- [x] 2. `scripts/test-liquers-py.sh` line 42 and its header comment. Proof: `local_missing_wheel`; `bash scripts/test-liquers-py.sh` passes on 3.13
- [x] 3. Open the PR. Proof: `wf_py_matrix_green`, `wf_web_node_green`, then `wf_red_on_failure` (revert afterwards). A job that is red because of a defect in the code is filed under `DOCS_STRUCTURE_GUIDE.md` §4.8 and the PR is left unmerged for the maintainer, never with `continue-on-error`. A red job caused by the runner is not expected: every runner choice is settled above. If one occurs, the automatic fix stops and the design returns for review
- [x] 4. `PYTHON_INTEGRATION.md` §8 bullet, History row and `reviewed:`; `liquers-py/README.md` "Supported Python". Proof: `python3 scripts/docs_index.py --check`
- [x] 5. Both source issues `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

Both workflows are green after the step 3 revert, and `docs_index.py --check` passes. The diff holds
two workflows, the script, two documents, two issues and the index.
