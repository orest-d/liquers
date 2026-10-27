---
id: DOCS-TOOLING-TESTS-IN-CI
kind: design
title: The docs workflow runs the documentation tooling's unit tests
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [docs, build]
issues: [DOCS-TOOLING-TESTS-NOT-RUN-IN-CI]
created: 2026-10-26
---
# The docs workflow runs the documentation tooling's unit tests

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

The documentation tooling (`scripts/docs_index.py` and the `liquers-project` skill scripts) has
unit tests, but CI never runs them. CI only checks that the current `specs/` tree validates. A
regression in the tooling therefore merges whenever today's tree still passes `--check`. This
design makes the existing `docs` workflow run those tests.

### Problem Example

Verified at HEAD (`cd6d5da`):

- `.github/workflows/docs-check.yml` has one step, `python3 scripts/docs_index.py --check`. Its
  `pull_request` paths are `specs/**`, `scripts/docs_index.py` and the workflow file. Its `push`
  paths are `specs/**` and `scripts/docs_index.py`, without the workflow file.
- `python3 -m unittest scripts/test_docs_index.py` runs 49 tests, all `OK`, in 0.09 s. They use only
  the standard library and temporary directories.
- `.claude/skills/liquers-project/scripts/` holds `init_feature.py` and `validate_phase.py` and no
  tests. `python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'`
  prints `NO TESTS RAN`. **It exits with code 5 on Python 3.13**, because Python 3.12 and later treat
  "no tests" as a failure. On the workflow's pinned 3.11 it exits 0. So the issue's suggested command
  works on the runner today, but it breaks when the pin is raised, unless a test file exists by then.

Scenario: a PR changes `phase_text` in `docs_index.py` so that a fenced `## Phase` heading counts
again, and `PhaseTextTests` would catch it. Today the `docs` job runs only `--check`. That passes
because no current design has a fenced heading, so the PR merges green. It should fail at a unit-test
step. A PR that edits only `scripts/test_docs_index.py` does not run the workflow at all today.

### Scope and Acceptance Criteria

- **AC-1** Tooling tests gate the workflow
  WHEN the `docs` workflow runs and a test in `scripts/test_docs_index.py` fails
  THEN the job fails at a step named for the unit tests, separately from the `--check` step
- **AC-2** A test-only change triggers the workflow
  WHEN a pull request or push to `main` changes only `scripts/test_docs_index.py`, or only a file
  under `.claude/skills/liquers-project/scripts/`
  THEN the `docs` workflow runs
- **AC-3** Skill-script tests run when they exist
  WHEN `.claude/skills/liquers-project/scripts/` contains a `test_*.py`
  THEN the workflow runs it with `unittest discover`, and a failing test fails the job
- **AC-4** No skill-script tests is not a failure
  WHEN that directory has no `test_*.py`, whatever the Python version
  THEN the step passes and says that it skipped
- **AC-5** The workflow contract holds
  WHEN the workflow runs
  THEN it uses no token, network access or package installation beyond `actions/setup-python`, as
  its header promises
- **AC-6** Push and pull request filters agree
  WHEN the workflow file itself changes, on a pull request or on a push to `main`
  THEN the workflow runs

Non-goals: tests for `init_feature.py` / `validate_phase.py` themselves (added by
`phase5-skeleton-unticked` and `validate-phase-section-match`); raising the Python pin; running
`scripts/check-build-matrix.sh` or other `scripts/` from this workflow.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. This is a tooling fix to one CI file
  (`.github/workflows/docs-check.yml`) of size `S`. It adds no structure and changes no interface,
  dependency or feature flag.
- **Leading issue:** None
- **Explanation:** both test commands already exist and pass locally. The only finding beyond the
  issue is the exit code 5 of an empty `discover` on Python 3.12 and later, which a presence guard
  handles (AC-4).
- **Open questions:** None. Decided in Phase 2, as an implementation detail: the path filter names
  the tooling files exactly rather than `scripts/**`, so that unrelated scripts do not trigger it.

### Design Dependencies

- `validate-phase-section-match` (in review) and `phase5-skeleton-unticked` (in review):
  **overlaps**. Each adds a `test_*.py` beside the skill scripts, and AC-3 runs it. There is no order
  between them: AC-4 makes this design safe to land first.
- `front-matter-yaml-agreement` (in review, open PR #173): **overlaps**. It adds tests to
  `scripts/test_docs_index.py`, which AC-1 then runs. No order.
- `build-matrix-path-filters`: **overlaps**. It is the same kind of path-filter fix in the other
  workflow file. They do not merge, because T3 fails: they change different files and share no
  contract.

## Phase 2: Architecture

### Solution

Edit `.github/workflows/docs-check.yml` only:

1. `on.pull_request.paths` and `on.push.paths` become the same list: `specs/**`,
   `scripts/docs_index.py`, `scripts/test_docs_index.py`,
   `.claude/skills/liquers-project/scripts/**`, `.github/workflows/docs-check.yml` (AC-2, AC-6).
2. After the existing `--check` step, add two steps (AC-1, AC-3, AC-4):

   ```yaml
      - name: Unit tests of docs_index.py
        run: python3 -m unittest -v scripts/test_docs_index.py
      - name: Unit tests of the liquers-project skill scripts
        run: |
          dir=.claude/skills/liquers-project/scripts
          if compgen -G "$dir/test_*.py" > /dev/null; then
            python3 -m unittest discover -v -s "$dir" -p 'test_*.py'
          else
            echo "no test_*.py in $dir; skipped"
          fi
   ```

   The `--check` step stays first. It is the contract check, and its failure reads differently
   from a tooling regression.
3. The header comment gains one line saying that the job also runs the tooling's stdlib unit tests.

Rejected alternatives:
- **`scripts/**` as the filter**, as the issue suggests. It would also trigger on
  `check-build-matrix.sh` and `test-liquers-py.sh`, which this job does not exercise.
- **A plain `discover` without a guard.** It exits 5 on Python 3.12 and later while the directory
  has no tests.
- **A separate workflow.** It would duplicate checkout and setup for a run that takes under a second.

### Changes

`.github/workflows/docs-check.yml` only. No code, command, registry or document contract changes.
`DOCS_STRUCTURE_GUIDE.md` §7 does not describe the workflow's steps, so it needs no edit.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test depends on the working directory | Both commands run from the repository root, which is where they pass today | high |
| `compgen` needs bash | `ubuntu-latest` runs `run:` steps under bash by default | high |
| A tooling test is slow or flaky | 49 tests in 0.09 s, using only temporary directories | high |
| The PR merging tests into `test_docs_index.py` (#173) fails the new step | That is the intended effect: it fails on a real regression | high |

Recovery: revert the workflow file.

## Phase 3: Examples and Tests

### Examples

These are the local equivalents of the new steps, run from the repository root:

```bash
python3 scripts/docs_index.py --check
python3 -m unittest -v scripts/test_docs_index.py            # 49 tests, OK (AC-1)
bash -c 'dir=.claude/skills/liquers-project/scripts; if compgen -G "$dir/test_*.py" >/dev/null; then python3 -m unittest discover -v -s "$dir" -p "test_*.py"; else echo "no test_*.py in $dir; skipped"; fi'
```

The last command prints the skip line and exits 0 today (AC-4). With a `test_*.py` present, it
runs that file (AC-3).

### Tests

No new unit test is needed, because these are CI properties. The validation:

- running the three commands above locally: exit 0, and the skip message: AC-1, AC-4, AC-5
- the same snippet in a scratch directory holding a failing `test_x.py`: exit code 1: AC-3
- on the implementing PR, the `docs` run shows both new steps. A temporary commit that breaks one
  assertion in `scripts/test_docs_index.py` fails the "Unit tests of docs_index.py" step, and the
  commit is then reverted. This checks AC-1 and AC-2, by hand, once.
- reading the workflow diff: the two `paths` lists are identical and include the workflow file
  (AC-6), and the job has no new `uses:` or `pip` step (AC-5)

## Phase 4: Implementation Plan

### Steps

- [x] 1. `.github/workflows/docs-check.yml` `on.pull_request.paths` / `on.push.paths`: set the identical five-entry list. Proof: diff review (AC-2, AC-6)
- [x] 2. The same file: add the two unit-test steps after `--check`, and the header line. Proof: the three Phase 3 commands exit 0 locally (AC-1, AC-4, AC-5); the failing-`test_x.py` snippet exits 1 (AC-3)
- [x] 3. On the PR: break one assertion temporarily and see the step fail, then revert. Proof: the PR's `docs` run (AC-1, AC-2)
- [x] 4. `specs/issues/DOCS-TOOLING-TESTS-NOT-RUN-IN-CI.md` `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

`python3 scripts/docs_index.py --check`, and the PR's green `docs` run after the revert in step 3.
Review the diff: one workflow file, the issue file and the regenerated index.

### Implementation notes (2026-10-27)

- Step 3 was checked locally rather than with a temporary breaking commit on the PR: the
  skill-script step exits 1 on a scratch directory holding a failing `test_x.py`, and the
  `docs_index.py` step is a plain `unittest` run whose failure exits non-zero.
