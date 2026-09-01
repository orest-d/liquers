# Phase 4: Implementation Plan

1. In `specs/reference/POLARS_COMMAND_LIBRARY.md`, inventory and classify query-like examples before
   editing; record the runnable set in a temporary validation file under `/tmp`.
2. Add `ns-pl` to each complete Polars pipeline, repair the malformed resource-transform example,
   and leave command fragments/contextual suffixes untouched. Preserve variadic parameter spelling.
3. Build or run `liquers-validate` against every curated query using the committed registry. On a
   failure, fix only documentation syntax unless registry freshness proves the reference is stale.
4. Update the reference History row and source issue/design lifecycle records; regenerate the docs
   index and README blocks with the required script.
5. Run docs-index check and relevant link/reference checks. Review the final diff line by line for
   missed runnable examples, accidental command renames, runtime source edits, generated-file
   hand-edits, and unrelated prose. Rollback is the single reference edit plus lifecycle records.
