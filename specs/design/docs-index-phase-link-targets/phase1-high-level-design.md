# Phase 1: High-Level Design - Resolvable Design-Phase Links in `index.md`

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — tooling fix in `scripts/docs_index.py` (already implemented)
- **Leading issue:** None
- **Explanation:** The correct link form is fixed by the rest of the same table (links relative
  to `specs/`, via `relative_specs_path`), and the check that would have caught the defect
  already exists but skips this one generated file; both changes are local to
  `scripts/docs_index.py`.
- **Open questions:** None

## Problem and Evidence

The issue tracked two defects in the "Design" column of `specs/index.md`. Re-verified at HEAD
(2026-10-04):

- **Ordering — fixed.** The phase listing is `stable_paths(...iterdir())`
  (`scripts/docs_index.py` ≈456), the fix recorded in the closed
  `DOCS-INDEX-GENERATION-DIFFERS-BY-HOST`, with `test_stable_paths_ignore_host_case_ordering`.
- **Link target — still wrong.** `e326a45` replaced the absolute path with
  `x.relative_to(REPO).as_posix()` (≈455), i.e. `specs/design/<slug>/phase1-high-level-design.md`.
  `specs/index.md` is *in* `specs/`, so Markdown resolves that to
  `specs/specs/design/<slug>/…`, which does not exist: **every phase link in the file is dead** (334 on 2026-10-04).
  The issue column of the same row uses `relative_specs_path(r)` (`issues/<ID>.md`) and works; the
  HTML index (`render_index_html`) uses `relative_specs_path` for its design cell and works.
- **Why `--check` is silent:** `relative_link_errors` validates `tracked_markdown_paths(specs)`,
  which is `README.md` plus `issues/`, `design/`, `reference/`, `guides/` — not `index.md`.

## Expected Behaviour and Acceptance Criteria

1. Every phase link in `specs/index.md` is relative to `specs/` (`design/<slug>/<file>.md`) and
   resolves to an existing file.
2. Phase links stay in `stable_paths` order (unchanged).
3. `python3 scripts/docs_index.py --check` reports a dead link in `specs/index.md` if one is ever
   generated again.
4. Regeneration on any host produces byte-identical output (unchanged).

## Affected Systems

`scripts/docs_index.py`, `scripts/test_docs_index.py`, the generated `specs/index.md`. No library
code.

## Scope and Non-Goals

Non-goals: changing the table's columns, the HTML index, or `index.csv`'s `file` column (which is
repository-relative by design and is not a link).

## Compatibility

Regenerating rewrites every phase link in `specs/index.md` once (a large, mechanical diff in the
implementing PR). No consumer parses those links.

## Documentation Assessment

`specs/DOCS_STRUCTURE_GUIDE.md` §7.2 ("What `--check` validates") — add `index.md` to the link
check's scope (History row; the guide has no `reviewed:` field). Close the issue.

## Design Dependencies

- `covered-by` (ordering half only): the fix recorded in the closed
  `DOCS-INDEX-GENERATION-DIFFERS-BY-HOST` (no design folder).
- `overlaps` `docs-current-link-validation`, `docs-dead-links` (both complete): built the link
  checker this design extends to one more file.
- `required-by` (ordering, not content): every other design's implementation regenerates
  `specs/index.md`. Landing this one **first** means later PRs regenerate correct links and are
  guarded by the new check (post-Phase-4 review, 2026-10-05).

## Consolidated Findings

- The issue's prose was right that "every other link in the file goes through
  `relative_specs_path()`"; the partial fix chose a different base (repository) and created a
  second defect of the same kind.
- `relative_specs_path` takes a row dict; the phase link needs a `Path` — use
  `x.relative_to(SPECS).as_posix()` (`SPECS = REPO / "specs"` is already defined), which is the
  same base.
- Adding `SPECS / "index.md"` to `tracked_markdown_paths` makes the generator's output self-
  checking; `--check` already regenerates-and-compares, so it validates the committed file.
- Validation: unit tests for the rendered link form and for the checker's coverage of
  `index.md`, plus a full `--check`.

## Review

Small, two-line fix plus a checker scope change, both proven by tests.
