# Phase 3: Examples and Tests - Resolvable Design-Phase Links in `index.md`

## Example

Row for `METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ` in `specs/index.md`:

| | Design cell |
|---|---|
| Before | label `phase1`, target `specs/design/metadata-serde-partialeq/phase1-high-level-design.md` → resolves to `specs/specs/…` (dead) |
| After | label `phase1`, target `design/metadata-serde-partialeq/phase1-high-level-design.md` → resolves |

## Tests to Add (`scripts/test_docs_index.py`)

| Test | Steps | Asserts | Criterion |
|---|---|---|---|
| `test_index_markdown_phase_links_resolve` | `rows = docs_index.collect()` (the row builder `main` uses), `text = docs_index.render_index_markdown(rows)`; for every link target matched by `RELATIVE_LINK_RE` in the text, `(docs_index.SPECS / target).exists()` | no dead targets; at least one `design/` target seen | 1 |
| `test_index_md_is_link_checked` | temporary tree: `specs/index.md` containing one Markdown link with target `design/missing.md` | `relative_link_errors(specs) == ["specs/index.md: dead link design/missing.md (§8.4)"]` | 3 |

`collect()` reads the real repository, so the first test is an integration test over the
committed tree, which is the intent.

## Repository Check

`python3 scripts/docs_index.py && python3 scripts/docs_index.py --check` — 0 errors; before the
fix, with change 2 alone, `--check` reports every phase link of `index.md` as dead — 334 on
2026-10-04, measured by running `relative_link_errors` with `index.md` added (the reproduction).

## Run

```bash
python3 -m unittest scripts/test_docs_index.py
```

## Coverage Review

Criteria 1 and 3 by tests; 2 and 4 by the existing `stable_paths` test and `--check`'s
regenerate-and-compare.

(Link examples on this page are written as label/target pairs rather than Markdown links,
because the dead-link checker does not skip code spans.)
