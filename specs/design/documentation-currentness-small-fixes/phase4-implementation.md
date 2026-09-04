# Phase 4: Implementation Plan

1. In `STORE_CONFIG_FSD.md`, replace the legacy filesystem implementation wording with
   `AsyncFileStore`; retain the YAML configuration unchanged. Add the required same-day History
   row and `reviewed:` update. Proof: focused text search. Containment: revert this reference only.
2. In `escape.rs`, `plan.rs`, and `store.rs`, remove rustdoc-link brackets around inaccessible
   identifiers without changing the explanatory prose. Proof: strict rustdoc build. Containment:
   revert only the affected doc comments.
3. In `CLAUDE.md`, remove the hard-coded matrix count and point readers to the script's emitted
   total. Proof: focused search. Containment: revert this guidance line only.
4. Close the three issue records with resolution evidence, set the design to documentation for its
   final record review, regenerate the indexes, run their check and inspect the final diff.

## Review

The steps exactly implement Phase 2, have no API or runtime effect, and keep each correction
independently reversible. Strict rustdoc is the meaningful regression check; no code test can add
coverage for a prose-only change.

