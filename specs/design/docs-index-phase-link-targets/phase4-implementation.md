# Phase 4: Implementation Plan - Resolvable Design-Phase Links in `index.md`

1. **Reproduce.** In `scripts/docs_index.py` `tracked_markdown_paths`, add `specs / "index.md"`;
   run `python3 scripts/docs_index.py --check` and confirm it now reports the phase links as dead
   links in `specs/index.md`. This is the failing proof.
2. **Fix the link base.** `render_index_markdown`: `x.relative_to(REPO)` → `x.relative_to(SPECS)`.
   Run `python3 scripts/docs_index.py` (regenerates `specs/index.md`, `index.csv`, README blocks)
   and `python3 scripts/docs_index.py --check` → 0 errors. Depends on 1.
3. **Tests.** Add the two Phase 3 tests to `scripts/test_docs_index.py`;
   `python3 -m unittest scripts/test_docs_index.py`.
4. **Docs and records.** `specs/DOCS_STRUCTURE_GUIDE.md` §7.2: say the dead-link check also
   covers the generated `index.md`; History row (no `reviewed:` field exists). Close
   `specs/issues/DOCS-INDEX-EMITS-MACHINE-LOCAL-PATHS.md` with a resolution naming both tests.
   Regenerate and check again.
5. **Review.** Diff: two script lines, tests, the guide, the regenerated files (phase-link
   rewrite only — confirm no other row changed with `git diff specs/index.md | grep '^[-+]' |
   grep -v 'design/'`), and the issue.

## Final Review

Consistent across phases; local to the docs tooling. Rollback: revert the two script lines and
regenerate.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. `render_index_markdown` still emits
  `x.relative_to(REPO)` (`scripts/docs_index.py` ≈455), and `specs/index.md` contains links of
  the form `specs/design/…`, which resolve to `specs/specs/…`. `tracked_markdown_paths` still
  leaves out `index.md`.
- **Solution correct:** yes. It is a two-line change, and putting the generator's output under
  the existing link check is the right guard.
- **Unnecessary abstractions:** none. `test_index_markdown_phase_links_resolve` largely repeats
  what `--check` does once `index.md` is tracked. It is harmless; keep it if a fast unit-level
  signal is wanted.
- **Detail / tests:** sufficient.
- **Interactions:** every other design's implementation regenerates `specs/index.md`. Land this
  one **first**, so later PRs regenerate correct links and the new check guards them.
- **Verdict:** ready.

**Resolution (2026-10-05):** the findings above are incorporated into Phases 1-4.
`phase5-documentation.md` holds the documentation plan; where a Phase 4 step names documentation
work, that plan is the authoritative list.
