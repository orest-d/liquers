# Phase 4: Implementation Plan

1. Add `blank_code` and use it in `relative_link_errors`. Proof: T7 (`python3 scripts/docs_index.py --check`).
   Agent: haiku tier.
2. Add `scripts/test_docs_index.py` with T1–T6. Proof: `python3 -m unittest scripts/test_docs_index.py`.
3. Check other raw-text regex passes in the script, and apply `blank_code` where they validate
   links.
4. Docs contract sentence, issue resolution, index. Diff review.
