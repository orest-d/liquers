---
id: LANGUAGE-BINDING-TESTS-IN-CI
kind: design
title: CI runs the liquers-py test loop and the liquers-web Node loop
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [build, py, web]
issues: [LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI, PY-TESTS-NOT-RUN-IN-CI]
merged: 2026-10-27
created: 2026-10-27
---
# CI runs the liquers-py test loop and the liquers-web Node loop

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

The two language bindings have test loops that only run by hand. `build-matrix` proves that
`liquers-py` and `liquers-web` compile, not that they behave. A core change can therefore make
their conformance tables untrue without failing any check. This design adds two workflows: one runs
`scripts/test-liquers-py.sh` on three interpreters, the other runs the `liquers-web` Node loop
with both feature sets.

### Problem Example

Verified at HEAD (`04667a80`):

- `.github/workflows/` holds `build-matrix.yml` (`cargo check` rows via
  `scripts/check-build-matrix.sh`) and `docs-check.yml`. Neither runs `pytest` or `cargo test`.
- `scripts/test-liquers-py.sh` builds into a venv per interpreter (`PYTHON`, default `python3`),
  installs `maturin pytest polars pillow` with pip, runs `maturin develop --features
  debug-handles`, `pytest liquers-py/tests`, then `cargo test -p liquers-py --no-default-features
  --lib`, adding the interpreter's `LIBDIR` to `LD_LIBRARY_PATH`.
- The Node loop is `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles` and the same with `debug-handles,records`. `.cargo/config.toml` sets the wasm32
  runner to `wasm-bindgen-test-runner`, which must come from a `wasm-bindgen-cli` of the
  lockfile's `wasm-bindgen` version (`Cargo.lock`: `0.2.129`). The browser-only files are gated
  behind `browser-tests`, so this loop needs only Node.
- `specs/reference/PYTHON_INTEGRATION.md` §8 and `liquers-py/README.md` "Supported Python" both
  say that the suite has passed on CPython 3.13 and 3.14t locally, "not in CI".

Scenario: a core PR adds an `ErrorType` variant. `build-matrix` passes. `test_ERROR01` (Python) and
`OBJECT06` (web, the exhaustive error-type round trip) would fail, but nothing runs them, so the
PR merges green. It should fail in a `liquers-py` or `liquers-web` check on that PR.

### Scope and Acceptance Criteria

- **AC-1** Python loop gates a PR
  WHEN a pull request changes a file that `liquers-py` builds from, and a test in
  `liquers-py/tests/` or a `liquers-py` Rust unit test fails
  THEN the `liquers-py` workflow fails
- **AC-2** Three interpreters
  WHEN the `liquers-py` workflow runs
  THEN it runs `scripts/test-liquers-py.sh` once on CPython 3.9, once on 3.14 and once on
  free-threaded 3.14t, each as its own job, and a failure on one does not cancel the others
- **AC-3** Web Node loop gates a PR
  WHEN a pull request changes a file that `liquers-web` builds from, and a Node-loop test fails
  with either feature set (`debug-handles`, `debug-handles,records`)
  THEN the `liquers-web` workflow fails
- **AC-4** The runner matches the lockfile
  WHEN the `liquers-web` workflow installs `wasm-bindgen-cli`
  THEN the version is read from `Cargo.lock` at run time, so a `wasm-bindgen` bump needs no
  workflow edit
- **AC-5** Path filters cover the dependency closure
  WHEN a change touches only `liquers-core/`, `liquers-lib/`, `liquers-records/`, `Cargo.lock`,
  the workflow file or the binding's own directory (for Python also `liquers-store/`,
  `liquers-template/`, `scripts/test-liquers-py.sh`)
  THEN that binding's workflow runs; a change only under `liquers-axum/`, `liquers-mcp/` or
  `liquers-agent/` runs neither; the `push` and `pull_request` lists are identical
- **AC-6** The disk suffices
  WHEN either workflow runs on a hosted `ubuntu-latest` runner
  THEN it does not fail with "No space left on device"

Non-goals: PyPy (supported by PyO3 but never yet verified here), the browser and Playwright loops,
`check-stubs.sh`, `test-guide-examples.sh`, any change to the test scripts or to the tests.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. This is a tooling fix: two new CI files and nothing else (rule 2).
  It adds no Rust structure (rule 3), changes no interface, dependency or feature flag (rule 4),
  and touches no crate (rule 6). Size `M`: two files, each under 60 lines.
- **Leading issue:** None
- **Explanation:** both loops already run locally, and Phase 2 resolves every remaining choice
  from repository evidence. The `liquers-py-async` preflight said that the CI matrix and its cost
  were the owner's call. `PY-TESTS-NOT-RUN-IN-CI` already specifies the matrix (oldest CPython,
  newest CPython, free-threaded). The repository is public, so hosted-runner minutes cost nothing;
  the only cost is wall time on PRs that touch the bindings' dependencies.
- **Open questions:** None. Resolved as implementation details in Phase 2: "newest" means newest
  stable (3.14, not the 3.15 beta); two workflows rather than one; `cargo install` rather than a
  third-party installer action.

### Design Dependencies

- `build-matrix-path-filters` (complete): **overlaps**. These workflows reuse its `liquers-*/**`
  pattern, with negations, so that a new crate is covered by default.
- `liquers-py-async` (complete): **overlaps**. Its AC-8 (free-threaded) was proved only by local
  runs; the 3.14t job makes it a CI property.
- `build-matrix-disk-budget` (complete): **overlaps**. It is the same disk constraint, handled
  in another workflow.

### Overlap Record

The two sources overlap strongly, and they form one design. **T4:** the Python half of
`LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI` ("`scripts/test-liquers-py.sh` on one CPython") is covered
by `PY-TESTS-NOT-RUN-IN-CI`'s wider matrix. **T3:** both sources would add the same workflow.
**E1** does not apply, because the merged design is eligible (the web half adds no
interface or structure), so the Python half need not stay separate. The leading source is the
wider issue, `LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI`.

### Consolidated Findings

- `test-liquers-py.sh` takes the interpreter from `PYTHON`. Under `setup-python` a free-threaded
  build's binary is `python3.14t`, so the job passes `PYTHON` from the step's `python-path`
  output rather than relying on `python3`.
- Each Python job builds `liquers-lib` with `polars` twice (the `dev` profile for maturin, the
  `test` profile for `cargo test`), estimated at 8-10 GB each (`design/liquers-py/` Phase 2). The
  job therefore frees preinstalled toolchains first and sets `CARGO_INCREMENTAL=0` (AC-6).
- A loop that is red at HEAD would land the workflow red. Step 3 runs the workflow first and stops
  on a real failure (see Risks).

## Phase 2: Architecture

### Solution

Two new files, so that each binding has its own path filter and its own red mark:

**`.github/workflows/liquers-py-tests.yml`**: `on.pull_request.paths` and `on.push.paths`
(branch `main`) are identical, plus `workflow_dispatch`:

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

The job is `test`, with `strategy.fail-fast: false`, `matrix.python: ['3.9', '3.14', '3.14t']`,
and `timeout-minutes: 75`. Its environment is `CARGO_INCREMENTAL: 0`. The steps are:

1. `actions/checkout@v4`;
2. "Free runner disk":
   `sudo rm -rf /usr/share/dotnet /usr/local/lib/android /opt/ghc /opt/hostedtoolcache/CodeQL; df -h /`;
3. `dtolnay/rust-toolchain@stable` and `Swatinem/rust-cache@v2` with `key: py-${{ matrix.python }}`;
4. `actions/setup-python@v5` with `id: py` and `python-version: ${{ matrix.python }}`;
5. `bash scripts/test-liquers-py.sh` with `env: PYTHON: ${{ steps.py.outputs.python-path }}`, then
   `df -h /`.

**`.github/workflows/liquers-web-tests.yml`** uses the same filter shape, with `!liquers-py/**` in
place of `!liquers-web/**`, no `scripts/test-liquers-py.sh` entry, and its own file name. Its job is
`node`, with `timeout-minutes: 60` and `CARGO_INCREMENTAL: 0`. The steps are: checkout; free disk;
`dtolnay/rust-toolchain@stable` with `targets: wasm32-unknown-unknown`; rust-cache;
`actions/setup-node@v4` with `node-version: '20'`; then:

```yaml
      - name: Install wasm-bindgen-test-runner matching Cargo.lock
        run: |
          ver=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
          test -n "$ver"
          cargo install wasm-bindgen-cli --version "$ver" --locked
      - name: Node loop (debug-handles)
        run: cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
      - name: Node loop (debug-handles, records)
        run: cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records
```

Each header comment says what the workflow runs and names this design.

Rejected alternatives:
- **One workflow with both jobs.** A path filter is per workflow, so a Python-only change would also
  run the web job, and the reverse. Splitting jobs inside one file would need a third-party filter
  action.
- **Adding jobs to `build-matrix.yml`.** It would mix compile checks with test runs, and its runner
  disk is already sized for `check-build-matrix.sh`.
- **The 3.15 beta as "newest".** Wheels for `pillow` do not exist for a beta, so the job would
  build from source or fail for reasons outside this repository. 3.15 replaces 3.14 when it is
  released.
- **`taiki-e/install-action` for `wasm-bindgen-cli`.** It is faster, but it is one more
  third-party action. `cargo install --locked` costs a few minutes.
- **Listing crates by name.** That is the defect `build-matrix-path-filters` removed.

Known-issue preflight: no open issue blocks. `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`
(P3) touches both bindings, but it is a performance issue and does not affect a pass or a fail.

### Changes

New: `.github/workflows/liquers-py-tests.yml` and `.github/workflows/liquers-web-tests.yml`.
Nothing else changes: no code, no script, no test, no command, and so no registry change. Two
documents have their "not in CI" lines updated at Phase 4 step 4. These are
`specs/reference/PYTHON_INTEGRATION.md` §8 (with a History row and a `reviewed:` bump) and
`liquers-py/README.md` "Supported Python".

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A loop is red at HEAD (a real defect) | Step 3 runs the workflow on the PR before merge. A real failure stops the fix: the defect is filed and the workflow is not merged red, so a human decides | medium |
| `polars` / `pillow` have no wheel for 3.9 or 3.14t | pip resolves an older wheel for 3.9 (`polars 1.36.1`, `pillow 11.3.0`, checked with `pip download`). 3.14t passed locally with both installed (`liquers-py-async` Phase 2 probe) | medium |
| `setup-python` does not accept `3.14t` | v5 supports the `t` suffix for free-threaded builds. If it does not, the fallback is `astral-sh/setup-uv` and `uv python find 3.14t`, as `liquers-py/README.md` documents | medium |
| Disk runs out (AC-6) | Free about 20 GB of preinstalled toolchains, `CARGO_INCREMENTAL=0`, and `df -h /` in the log | medium |
| `pip install` without pinned versions drifts | It uses the same unpinned set that developers run locally, which is the intent | high |
| Wall time (cold runs of 20 min or more) | The jobs run in parallel and are path-filtered; rust-cache cuts warm runs | high |

Recovery: delete either workflow file. Nothing else depends on it.

## Phase 3: Examples and Tests

### Examples

These are the local equivalents, run from the repository root:

```bash
bash scripts/test-liquers-py.sh                                   # 3.13 here; CI: 3.9, 3.14, 3.14t
PYTHON=$(uv python find 3.14t) bash scripts/test-liquers-py.sh
grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p'   # 0.2.129
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

### Tests

No unit test applies, because these are CI properties. The validation:

- `wf_py_matrix_green`: on the implementing PR, all three `liquers-py` jobs pass, each log shows
  its interpreter in the venv path (`liquers-py-venv-cpython3.14t` for the free-threaded job), and
  `df -h /` shows space remaining: AC-2, AC-6
- `wf_web_node_green`: on the same PR, the `liquers-web` job passes both Node steps and its install
  step prints the lockfile version: AC-3, AC-4, AC-6
- `wf_red_on_failure`: a temporary commit breaks one assertion in
  `liquers-py/tests/test_ERROR_errors.py` and one in `liquers-web/tests/objects_OBJECT.rs`. Both
  workflows fail at their test step, and the commit is then reverted: AC-1, AC-3
- `wf_filters_review`: reading the diff shows that each file's two `paths` lists are identical and
  match Phase 2, and that the negations come after `liquers-*/**`, as GitHub's
  last-match-wins rule requires: AC-5
- `wf_version_extraction`: the `ver=` line from Phase 3's examples prints `0.2.129` locally: AC-4

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `.github/workflows/liquers-py-tests.yml`: write the workflow of Phase 2. Proof: `wf_filters_review`; `python3 -c 'import yaml,sys; yaml.safe_load(open(sys.argv[1]))' <file>` parses it
- [ ] 2. `.github/workflows/liquers-web-tests.yml`: write the workflow of Phase 2. Proof: `wf_filters_review`, `wf_version_extraction`, and the YAML parse
- [ ] 3. Open the PR and watch both workflows. Proof: `wf_py_matrix_green`, `wf_web_node_green`, then `wf_red_on_failure` (a temporary commit, then a revert). If a job fails at HEAD for a reason in code, stop: file the defect (`DOCS_STRUCTURE_GUIDE.md` §4.8), leave the PR unmerged for the maintainer, and do not use `continue-on-error`. If it fails for a runner reason (disk, wheel, `3.14t`), apply the Risks fallback and re-run; if the fallback does not help, stop the automatic fix
- [ ] 4. `specs/reference/PYTHON_INTEGRATION.md` §8: drop "but not in CI" (add a History row and bump `reviewed:`); `liquers-py/README.md` "Supported Python": "Neither run is in CI yet" becomes "CI runs 3.9, 3.14 and 3.14t". Proof: `python3 scripts/docs_index.py --check`
- [ ] 5. Both source issues: `status: closed`, each with a resolution note naming the PR; then `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

Both workflows are green on the PR after the revert in step 3, and `python3 scripts/docs_index.py
--check` passes. Review the diff: it should contain two workflow files, two document edits, two
issue files and the regenerated index, and no change under any `liquers-*` crate.
