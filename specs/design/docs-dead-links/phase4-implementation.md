# Phase 4: Implementation plan

1. Extract the tracked-document scan and per-document relative-link resolution in `scripts/docs_index.py`; preserve current README ID checks. Proof: Phase 3 Python tests. Roll back by restoring check-eight scope if the repaired corpus cannot pass.
2. Repair every non-archive reported link, prioritizing nested API source paths, machine-local paths, and moved design targets. Proof: `python3 scripts/docs_index.py --check`; never edit `specs/archive/`.
3. Update `specs/DOCS_STRUCTURE_GUIDE.md` to describe check-eight coverage and the restricted scope; add tests for anchors, URLs, fragments, and archive exclusion.
4. Regenerate index/README blocks, run the docs check and Python tests, then review the diff for generated-only changes, accidental archive edits, and links whose target was made to pass at the wrong relative depth.
