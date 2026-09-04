# Phase 4: Implementation Plan

1. In `liquers-lib/src/egui/widgets.rs`, add
   `query_to_layout_job_with_position(q, Option<&Position>)`; make the existing helper delegate with
   `None`, and add conversion tests. Rollback leaves the old wrapper untouched.
2. In `liquers-lib/src/ui/widgets/query_console_element.rs::show_toolbar`, capture the current
   error position alongside query text and call the new helper from the layouter. Prefer borrowing;
   clone the small position only when the closure lifetime requires ownership.
3. Add a console update-state test for a known error position and a layout conversion test covering
   both known and absent positions. Keep HTML rendering explicitly out of scope.
4. Review `UI_INTERFACE_FSD.md` for an error-presentation claim and update only if needed; update
   source issue/design lifecycle records during implementation.
5. Run formatting, `cargo test -p liquers-core --lib query`, relevant `liquers-lib` tests,
   the egui-only feature build, the build matrix, and docs-index checks. Review for query-text
   mutation, layout regressions, feature-gate leakage, unrelated UI edits, and debug output.
