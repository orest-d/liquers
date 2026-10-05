# Phase 2: Solution and Architecture - Resolvable Design-Phase Links in `index.md`

## Chosen Solution

`scripts/docs_index.py`:

1. In `render_index_markdown` (≈426), the phase-link expression:

   the link target `x.relative_to(REPO).as_posix()` becomes `x.relative_to(SPECS).as_posix()`;
   the label, the `stable_paths(...iterdir())` iteration and the filter are unchanged. (Not shown
   as a code block: the dead-link checker reads link syntax inside code blocks too.) `designs[design]["_path"]` is an absolute
   path under `SPECS`, so `relative_to(SPECS)` cannot raise.

2. In `tracked_markdown_paths(specs)` (≈160), include the generated index:

   ```python
   paths = [specs / "README.md", specs / "index.md"]
   ```

   The existing `path.is_file()` filter keeps it harmless where the file is absent (temporary
   test trees).

## Rejected Alternatives

- **Absolute repository URLs** (`https://github.com/.../blob/main/specs/...`) — branch-specific
  and break in forks and local viewers.
- **Reuse `relative_specs_path`** — takes a row dict with a `file` key; building fake rows is
  worse than one `relative_to(SPECS)`.
- **A separate assertion just for phase links** — the general dead-link check over `index.md`
  covers this and any future link column.

## Files and Symbols

| File | Symbol | Change |
|---|---|---|
| `scripts/docs_index.py` | `render_index_markdown` | link base `SPECS` |
| `scripts/docs_index.py` | `tracked_markdown_paths` | add `index.md` |
| `scripts/test_docs_index.py` | new tests | Phase 3 |
| `specs/index.md` | generated | regenerated |
| `specs/DOCS_STRUCTURE_GUIDE.md` | §7.2 | scope sentence |

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | as above |
| Affected workflows | docs regeneration; `docs-check.yml` CI (runs `--check`) |
| Existing-test impact | none; link tests use temporary trees without `index.md` |
| New validation | rendered-link test; checker-scope test; full `--check` on the repo |
| Compatibility/data | one-time rewrite of all phase links in `index.md` |
| Concurrency/performance/security | none |
| Recovery | revert two lines |
| Certainty | high |

## Review

Against Phase 1: criteria 1 → change 1; 2, 4 unchanged code; 3 → change 2. Against code:
`render_index_markdown`, `render_index_html`, `tracked_markdown_paths`, `relative_link_errors`,
`SPECS`/`REPO` definitions and the CI workflow were read at HEAD.
