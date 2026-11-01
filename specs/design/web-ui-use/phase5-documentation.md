# Phase 5: Documentation - web-ui-use

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none on the PR yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

A page mounts a Liquers UI with `mountUI(container, { query | spec })` and drives it through the
returned `UIMount`: `submit` and `show` queries, subscribe to element status with `on("status")`,
list the tree with `elements()`, and `dispose()`. A UI element value crosses into JavaScript as a
`UIElement` handle and back as the same `Arc`. All ten acceptance criteria (`UIUSE01`–`10`) are
met as approved.

The `liquers-lib` changes are reusable by any integration: `AppRunner` keeps a capped outbox of
`RunnerEvent`s drained after each tick (`take_events`), `replace_envref` lets a mount follow a
rebuilt environment, the runner drops a result whose node changed source while it was evaluated,
and `MountHandle` is stoppable (`stop`, `Drop`) through `mount_web_with` and `MountOptions`. The
web environment's payload became `SimpleUIPayload`, and the `lui` commands are built in.

Added beyond the plan: `MountOptions`, `UIStatusEvent` and `UIElementInfo` are declared in the
`.d.ts`, and the quickstart gained `ui.html` with a Playwright spec. Nothing approved was omitted.
Details: [`reference/JAVASCRIPT_INTEGRATION.md`](../../reference/JAVASCRIPT_INTEGRATION.md) §10.

## Documentation Delivered

### New Reference Documents
None: the contract is a section of the existing JavaScript integration reference.

### New Guide Documents
None: [`guides/JAVASCRIPT_GUIDE.md`](../../guides/JAVASCRIPT_GUIDE.md) §11 "Showing a Liquers UI".

### Existing Documents Reviewed or Updated
- `specs/reference/JAVASCRIPT_INTEGRATION.md`: §10 "User interface"; Limitations renumbered; History row, `reviewed:` 2026-10-31.
- `specs/guides/JAVASCRIPT_GUIDE.md`: §11; later sections renumbered; History row, `reviewed:` 2026-10-31.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: §6 Browser JavaScript `UIUSE` paragraph; History row.
- `liquers-web/README.md`: `UIUSE` row complete.

### Links and Capability Map
`specs/README.md`: "Liquers UI from JavaScript — built".

## Issues Filed

- `WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`: with two mounts, an unfocused menu shortcut runs in both.
- Closed: `WEB-CANNOT-USE-A-LIQUERS-UI`, `WEBUI-MOUNT-HANDLE-DROP-LEAVES-LISTENERS-ATTACHED`.
- Remains open and documented: `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`.

## Important Learning

The status a page sees has to be delivered after the DOM update and with no lock held, or a
callback that calls back into the mount deadlocks; an outbox drained by the driver gives that
ordering without the runner knowing about callbacks. On wasm the inline asset manager evaluates
inside `run`, so a node replaced during an evaluation is the normal case, not a race: the result
has to be checked against the node's current source before it lands.

## Conformance and Remaining Work

Requested (the guide's `UIUSE`), approved (AC-1–AC-10) and implemented scope agree. Nothing
remains except the freeze, which has its own issue.

## Validation

`cargo test -p liquers-lib --lib --tests`; the liquers-web Node loops with and without `records`;
`ui_browser_UIUSE` 13/13 (browser loop); `tests/e2e` 17/17; `check-stubs.sh --build` (and
`--records`); `test-guide-examples.sh`; `docs_index.py --check`. Not run: the
`liquers-lib/examples-web` trunk build, because `trunk` is not installed in this environment. The
browser loop ran against the Playwright Chromium (`NO_HEADLESS` route in `liquers-web/README.md`),
since the installed chromedriver does not match the browser.
