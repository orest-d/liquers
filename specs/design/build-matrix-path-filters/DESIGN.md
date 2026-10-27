---
id: BUILD-MATRIX-PATH-FILTERS
kind: design
title: The build-matrix workflow runs for a change to any liquers-* crate
form: compact
status: complete
gh_pr: [185]
readiness: ready
autofix: eligible
area: [build]
issues: [BUILD-MATRIX-CI-SKIPS-NEWER-CRATES]
created: 2026-10-26
---
# The build-matrix workflow runs for a change to any liquers-* crate

> **Complete on 2026-10-27**: implemented by automatic fix in #185.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`scripts/check-build-matrix.sh` is the only CI that builds the workspace. It should run for every
change that can break a configuration it checks. Its trigger lists crates by name, so each new
crate is missed until someone notices. This has happened twice: `liquers-records`, filed as
`BUILD-MATRIX-CI-SKIPS-LIQUERS-RECORDS-CHANGES` and now closed, and now three crates at once.

### Problem Example

Verified at HEAD (`cd6d5da`):

- `.github/workflows/build-matrix.yml` `on.pull_request.paths` and `on.push.paths` both name
  `liquers-core/**`, `-macro`, `-store`, `-lib`, `-axum`, `-web`, `-py` and `-records`, plus
  `Cargo.toml`, `Cargo.lock`, `.cargo/**`, the script and the workflow file.
- The workspace (`Cargo.toml` `members`) also has `liquers-template`, `liquers-mcp` and
  `liquers-agent`. The script checks all three: `TEMPLATE_CONFIGS`, which includes
  `--target wasm32-unknown-unknown --tests`, `check liquers-mcp "--tests"` and
  `check liquers-agent "--tests"`.
- All eleven crate directories at the root are named `liquers-*`, and nothing else at the root
  has that name.

Scenario: a PR changes only `liquers-template/src/render.rs` and the code no longer builds for
wasm32. Today the `build-matrix` workflow does not run, so the PR merges with only the `docs` check.
The next PR that touches `liquers-lib` then fails, and the break is blamed on that PR. The workflow
should run on the first PR and fail there.

### Scope and Acceptance Criteria

- **AC-1** Newer crates trigger the matrix
  WHEN a pull request, or a push to `main`, changes only files under `liquers-template/`,
  `liquers-mcp/` or `liquers-agent/`
  THEN the `build-matrix` workflow runs
- **AC-2** A future crate triggers it without a workflow edit
  WHEN a crate directory `liquers-<name>/` is added and a later change touches only it
  THEN the workflow runs, with no change to `build-matrix.yml`
- **AC-3** Existing triggers are kept
  WHEN a change touches only `Cargo.toml`, `Cargo.lock`, `.cargo/**`,
  `scripts/check-build-matrix.sh` or `.github/workflows/build-matrix.yml`
  THEN the workflow runs, as today
- **AC-4** Unrelated changes do not trigger it
  WHEN a change touches only `specs/**`, `memory/**` or the root `README.md`
  THEN the workflow does not run

Non-goals: adding matrix rows (every member already has one); making the matrix cheaper; the
`docs` workflow's own filter (`docs-tooling-tests-in-ci`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. This is a tooling fix to one CI file, of size `S`, with no
  structure, interface, dependency or feature change.
- **Leading issue:** None
- **Explanation:** one glob, `liquers-*/**`, covers every member today and every crate that keeps
  the naming convention. Of the issue's options, it is the only one that needs no new mechanism and
  misses no input to a build (see Phase 2).
- **Open questions:** None. Phase 2 resolved the issue's choice of route with repository evidence.

### Design Dependencies

- `build-matrix-ci` (complete; frozen under E4): **requires**. It created the workflow. Its
  per-crate list is what this design replaces.
- `library-no-panic-lint` (in review): **overlaps**. It edits the same workflow's toolchain step
  (`components: clippy`), not its triggers, so the change is only a textual rebase with no order.
- `docs-tooling-tests-in-ci`: **overlaps**. It is the same kind of fix in `docs-check.yml`. They do
  not merge, because T3 fails: the two designs change different files.

## Phase 2: Architecture

### Solution

In `.github/workflows/build-matrix.yml`, replace the eight `liquers-<crate>/**` entries in both
`on.pull_request.paths` and `on.push.paths` with one entry, `'liquers-*/**'`. In GitHub path
filters, `*` matches any characters except `/`, so this entry matches every file under every
`liquers-*` directory. The other five entries stay. Add a comment above each list:
`# every workspace crate is a liquers-* directory; a member named otherwise needs its own entry`.

Rejected alternatives (the issue's other two routes):
- **`'**/*.rs'` plus `'**/Cargo.toml'`.** This misses build inputs that are not Rust files and are
  read by `include_str!` / `include_bytes!`. Examples are `liquers-agent/src/lib.rs`, which reads
  `../tools/memory.yaml` and `../tools/query-guide.md`, and `liquers-records` test fixtures
  (`.ipc`, `.json`). It would also run on `.rs` files outside the crates.
- **A guard in `check-build-matrix.sh` that compares workspace members with the path filters.**
  A bash script would have to parse YAML and TOML. It also runs only when the workflow is already
  triggered, so it cannot see the case it is meant to catch.

### Changes

`.github/workflows/build-matrix.yml`: the two `paths` lists and their comment. Nothing else
changes: no script, code or document. `CLAUDE.md` does not describe the triggers.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A future member not named `liquers-*` | The comment says it needs an entry. Every member so far follows the convention | medium |
| More runs than before (for example a change to `liquers-py/python/**` only) | `liquers-py/**` already matched these files; the set of matched files is the old set plus the three crates | high |
| Glob semantics misread | `liquers-*/**` is the documented form for "any file under a matching directory" | high |

Recovery: revert the workflow file.

## Phase 3: Examples and Tests

### Examples

| Changed path | Today | After |
|---|---|---|
| `liquers-template/src/render.rs` | not run | runs (AC-1) |
| `liquers-mcp/src/lib.rs` | not run | runs (AC-1) |
| `liquers-agent/tools/query-guide.md` | not run | runs (AC-1) |
| `liquers-lib/src/commands.rs` | runs | runs (AC-3) |
| `Cargo.lock` | runs | runs (AC-3) |
| `specs/issues/X.md` | not run | not run (AC-4) |

### Tests

No unit test is needed, because this is a CI property. The validation:

- this local check, which matches each path in the table above against the new patterns with
  `fnmatch`-style matching (`*` does not cross `/`). It prints the expected run or no-run for every
  row: AC-1, AC-3, AC-4

  ```bash
  python3 - <<'PY'
  import re
  pats = ['liquers-*/**', 'Cargo.toml', 'Cargo.lock', '.cargo/**',
          'scripts/check-build-matrix.sh', '.github/workflows/build-matrix.yml']
  def rx(p): return re.compile('^' + re.escape(p).replace(r'\*\*', '.*').replace(r'\*', '[^/]*') + '$')
  for path in ['liquers-template/src/render.rs', 'liquers-mcp/src/lib.rs',
               'liquers-agent/tools/query-guide.md', 'liquers-lib/src/commands.rs',
               'Cargo.lock', 'specs/issues/X.md', 'memory/liquers-specs.yaml', 'README.md']:
      print(path, any(rx(p).match(path) for p in pats))
  PY
  ```
- `ls -d liquers-*/` lists exactly the `members` of `Cargo.toml`: AC-2 holds for all of today's
  crates
- on the implementing PR, which touches only the workflow file, `build-matrix` runs (AC-3). A
  later PR that touches only one of the three crates shows the run: AC-1, observed once

## Phase 4: Implementation Plan

### Steps

- [x] 1. `.github/workflows/build-matrix.yml` `on.pull_request.paths` and `on.push.paths`: replace the eight crate entries with `'liquers-*/**'` and add the comment. Proof: the Phase 3 local check (AC-1, AC-3, AC-4) and `ls -d liquers-*/` against `members` (AC-2)
- [x] 2. `specs/issues/BUILD-MATRIX-CI-SKIPS-NEWER-CRATES.md` `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

The PR's own `build-matrix` run (triggered by the workflow file) is green. Review the diff: one
workflow file, the issue file and the regenerated index.
