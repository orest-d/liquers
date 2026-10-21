---
id: FRONT-MATTER-YAML-AGREEMENT
kind: design
title: docs_index.py accepts only front-matter that a YAML parser reads the same way
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [docs]
issues: [SPECS-FRONT-MATTER-IS-NOT-VALID-YAML]
created: 2026-10-20
---
# docs_index.py accepts only front-matter that a YAML parser reads the same way

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented. The issue was found by the completed design
`agent-memory-mvp` (Step 11); this design replaces that link.

## Phase 1: High-Level Design

### Purpose

`scripts/docs_index.py` and YAML readers (the store's `front_matter` handler, `ns-mem`'s
`FrontMatter::fields` in `liquers-lib/src/mem/front_matter.rs`) must agree on every indexed
document's fields. Fix the nine documents, and make `--check` refuse the next one.

### Problem Example

At HEAD (18732dd), `yaml.safe_load` rejects the front-matter of exactly the nine files the issue
lists, for example `specs/design/store-guide-status-table/DESIGN.md`:

```yaml
title: Store guide status table: generator or honest hand maintenance
```

(`mapping values are not allowed here`, because of the second `: `), and
`specs/issues/CORE-STORE-OPENBIN-MISSING.md`, whose title starts with a backtick (a reserved YAML
indicator). `python3 scripts/docs_index.py --check` passes on all nine. Should be: the titles are
quoted, and `--check` fails on an unquoted value that YAML would reject or misread.

A trap the fix must avoid: `parse_front_matter` (`scripts/docs_index.py:304`) does not unquote.
Quoting a title today would put the quote characters into `index.csv`, `index.md` and the
near-duplicate search. No indexed document has a quoted title yet, so nobody has hit it.

### Scope and Acceptance Criteria

- **AC-1** Quoted values read as their content
  WHEN a front-matter value is `"…"` (JSON-style escapes) or `'…'` (`''` for a quote)
  THEN `parse_front_matter` returns the unquoted string, and `index.csv` shows the title without
  quotes
- **AC-2** Unsafe plain values are refused
  WHEN an unquoted scalar or list item starts with one of `` ` @ % * & ! | > ' " { # `` or `? `,
  `- `, or contains `: ` or ` #`, or ends with `:`
  THEN `--check` reports `<file>: front-matter '<key>' must be quoted (YAML would misread it)`
- **AC-3** The repository is clean
  WHEN `--check` runs after the change
  THEN it passes, and `yaml.safe_load` accepts every indexed document's front-matter
- **AC-4** `new` writes valid YAML
  WHEN `docs_index.py new` scaffolds an issue whose title starts with a backtick or contains `: `
  THEN its `title:` is double-quoted and passes AC-2 and `yaml.safe_load`
- **AC-5** The readers agree
  WHEN PyYAML is importable
  THEN a test parses every indexed document both ways and finds the same `id`, `kind`, `title`
  and `status` strings (skipped, not failed, without PyYAML)

Non-goals: making `docs_index.py` a YAML parser; changing the `ns-mem` reader.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — tooling fix in `scripts/docs_index.py` with tests, quoting in
  nine documents and a guide sentence; stdlib only, no product interface change.
- **Leading issue:** None
- **Explanation:** The issue's "have `--check` reject what YAML cannot parse" cannot use PyYAML:
  the docs workflow and `parse_front_matter`'s docstring require a dependency-free check. A
  conservative stdlib rule (AC-2) that is stricter than YAML, plus an agreement test that runs where
  PyYAML exists, gives the same guarantee.
- **Open questions:** None. *Resolved implementation detail:* double quotes are the house style
  (`json.dumps(value, ensure_ascii=False)` is valid YAML).

### Design Dependencies

- `validate-phase-section-match` — overlaps (same script, different function); can share a PR.
- `DOCS-TOOLING-TESTS-NOT-RUN-IN-CI` — overlaps: AC-5's test runs locally only until fixed.

### Consolidated Findings

Unquoting must land in the same commit as the quoting, or the index gains literal quotes. The rule
in AC-2 is deliberately stricter than YAML; it rejects nothing in the repository except the nine.

## Phase 2: Architecture

### Solution

In `scripts/docs_index.py`:

1. `_scalar(raw: str) -> str` (private): `"…"` → `json.loads` (an escape JSON lacks is an error
   reported by the check); `'…'` → inner text with `''` → `'`; otherwise unchanged. Used by
   `parse_front_matter` for scalars and list items.
2. `front_matter_yaml_errors(path_label, text) -> list[str]`: walks the same lines as
   `parse_front_matter` and applies AC-2 to unquoted scalars and to each list item; reports an
   unterminated quote too. `check()` calls it for every row (rows carry `_path`).
3. `new`: writes `title: {json.dumps(title, ensure_ascii=False)}` when the title fails the AC-2
   predicate, plain otherwise.

Quote the nine titles with double quotes. Update `specs/DOCS_STRUCTURE_GUIDE.md` §4.6 (front-matter
is YAML; quote a value that starts with an indicator or contains `: `) and §7.2 (the new check),
with a History row and `reviewed:` bump.

Rejected: PyYAML in `--check` (breaks the no-dependency CI job and local runs); unquoting only
titles (lists and other scalars would diverge later).

### Changes

`scripts/docs_index.py` (`parse_front_matter`, `check`, `new`, two private helpers),
`scripts/test_docs_index.py`, the nine documents, `specs/DOCS_STRUCTURE_GUIDE.md`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Rule rejects a value YAML accepts (e.g. a title with ` #`) | Intended: quoting is always safe | high |
| Another agent adds an unsafe title concurrently | `--check` reports it with the fix in the message | high |
| `index.csv` titles change | Only by losing quotes nobody wrote; regenerate the index | high |

## Phase 3: Examples and Tests

### Examples

`title: "Store guide status table: generator or honest hand maintenance"` → index title without
quotes; an unquoted title that starts with a backtick → `--check` error naming `title`.

### Tests

In `scripts/test_docs_index.py`, class `FrontMatterYamlTests`:

- `test_double_and_single_quoted_values_unquote` — AC-1
- `test_unsafe_plain_values_are_reported` — one case per indicator, `: `, ` #`, trailing `:`,
  a list item with `: ` — AC-2
- `test_safe_plain_values_pass` — `area: [build, docs]`, `issues: []`, dates, IDs — AC-2
- `test_repository_front_matter_is_clean` — runs the check over the real `specs/` — AC-3
- `test_new_quotes_a_title_that_needs_it` — scaffolds into a temporary tree — AC-4
- `test_readers_agree_with_yaml` — `skipUnless(yaml importable)` — AC-5

Run: `python3 -m unittest scripts/test_docs_index.py`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Tests for AC-1, AC-2, AC-4 (fail at HEAD) — `python3 -m unittest scripts/test_docs_index.py`
- [ ] 2. `_scalar` in `parse_front_matter` — AC-1 test passes
- [ ] 3. `front_matter_yaml_errors` wired into `check()`; `new` quoting — AC-2, AC-4 tests pass; `--check` now lists exactly the nine
- [ ] 4. Quote the nine titles; regenerate with `python3 scripts/docs_index.py` — `--check` passes; AC-3, AC-5 tests pass
- [ ] 5. `DOCS_STRUCTURE_GUIDE.md` §4.6 and §7.2, History row, `reviewed:` — `--check`
- [ ] 6. Close the source issue with a resolution note — `--check`

### Validation

`python3 -m unittest scripts/test_docs_index.py`; `python3 scripts/docs_index.py --check`;
`git diff specs/index.csv` shows only quote-free titles and the closed issue.
