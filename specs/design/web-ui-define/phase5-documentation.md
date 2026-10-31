# Phase 5: Documentation - web-ui-define

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none on the PR yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

A page defines element types with `defineElement({ type, render, onEvent?, mount?, title? })`,
makes elements with `element(type, props)`, and either shows them in a `mountUI` container or
renders the whole tree itself with `mountRenderer(renderer, options)`, forwarding user events
with `UIMount.dispatchEvent`. All eight acceptance criteria are met; `UIDEF04` is claimed for the
web backend only, as approved (egui shows a label).

`liquers-lib` gained `DelegatedElement` (one typetag type for every language-defined element,
holding a kind and JSON props) with its `ElementDelegate` registry, `UIElement::as_any`, and
`run_headless`, which shares one driver loop with `mount_web_with`. `liquers-web` gained
`ui_define.rs` and, in `ui.rs`, the `mount` reconciliation, the renderer mount and
`dispatchEvent`. The plan's differences, none of which changes the interface, are listed in
[Phase 4](./phase4-implementation.md) "As built". Details:
[`reference/JAVASCRIPT_INTEGRATION.md`](../../reference/JAVASCRIPT_INTEGRATION.md) §10
"Elements and renderers".

## Documentation Delivered

### New Reference Documents
None: a subsection of the JavaScript integration reference.

### New Guide Documents
None: [`guides/JAVASCRIPT_GUIDE.md`](../../guides/JAVASCRIPT_GUIDE.md) §11 "Define an element" and
"Render the tree yourself", whose two runnable examples pass under Node.

### Existing Documents Reviewed or Updated
- `specs/reference/JAVASCRIPT_INTEGRATION.md`: §2 table, §10 "Elements and renderers", Limitations; History row.
- `specs/guides/JAVASCRIPT_GUIDE.md`: §11 subsections, §13 limitation; History row.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: §6 Browser JavaScript `UIDEF` paragraph; History row.
- `liquers-web/README.md`: `UIDEF` row complete.
- `specs/reference/UI_INTERFACE_FSD.md`: not edited, as planned (being rewritten under its own issue).

### Links and Capability Map
`specs/README.md`: "UI elements and renderers defined in JavaScript — built".

## Issues Filed

- Fixed in place, not filed: a definition made from inside a render did not re-render a mount
  whose lock was held; the mount now applies it after its tick (`uidef01e`).
- `WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT`: the guard against replacing the element being updated
  records only the handle, so a cross-mount `show` with the same handle number is refused.
- Closed: `WEB-CANNOT-DEFINE-UI-ELEMENTS`.

## Important Learning

A language cannot add a `typetag` type at run time, so the element is data (kind and JSON props)
plus behaviour registered per thread; only the data serializes, which is why a restored tree needs
a placeholder path. The incremental driver replaces markup with `set_outer_html`, so a DOM
lifecycle belongs after the tick, keyed by node identity, not in the element trait. A definition
made while the UI lock is held cannot mark the tree at once; the mount applies it after its tick.

## Conformance and Remaining Work

Requested (the guide's `UIDEF`), approved (AC-1–AC-8) and implemented scope agree. Nothing remains
beyond the filed issue.

## Validation

`cargo test -p liquers-lib --lib ui::widgets::delegated_element` (default and `--features webui`);
the liquers-web Node loops with and without `records` (`ui_define_UIDEF` 9/9); `ui_browser_UIDEF` 9/9
(browser loop) and `ui_browser_UIUSE` 13/13 after the driver refactor; `check-stubs.sh --build`
(and `--records`); `test-guide-examples.sh` 24 passed, 4 not run (DOM-only); `docs_index.py
--check`; `scripts/check-build-matrix.sh`.
