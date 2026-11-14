# Phase 5: Documentation - liquers-py-ui

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #268)

## Implementation Summary

Built as approved, as the second commit group of PR #268:

- `PyEnvironment` is `DefaultEnvironment<Value, SimpleUIPayload>`; the `lui` commands are built-ins.
- `liquers-py/src/ui.rs`: `UiModel` holds an `AppState`, its channel and a `NativeDriver` ticked by
  `spawn_headless`; `lq.UI` (`show`, `submit`, `elements`, `on`, `wait_idle`, `wait_idle_async`,
  `process_events`, `asset`, `dispose`, a context manager), `lq.UIElementInfo`,
  `lq.UIStatusEvent`, `lq.UISubscription` and `lq.UIElement`.
- `Context.element`; the own-action mark in the command adapter; `env.shutdown()` and the exit
  hook dispose the models after the servers.

Changed from Phase 2, all recorded in Phase 3's Learning Log: `wait_idle` waits on a tick counter
(`watch`) instead of a `Notify`; `wait_idle_async` is a Python coroutine (`liquers/_ui.py`). From
the code review: `wait_idle` is refused inside **any** Python command, not only the model's own
action, because an element's own evaluation carries no payload and would otherwise wait for
itself (`test_UIUSE_wait_idle_inside_an_elements_command_raises`); the re-entrancy guard of
delivery is per model, so a callback of one UI may wait for another
(`test_UIUSE04_a_callback_of_one_ui_may_wait_for_another`); `on` after `dispose` raises
(`test_UIUSE05_on_after_dispose_raises`). `dispose` waits up to 5 s for the task without checking
signals, where Phase 2 said through `block_on`: a Ctrl-C there would drop the stop half-way.

## Documentation Delivered

### New Reference Documents
None: `PYTHON_INTEGRATION.md` gains §18 User interface.

### New Guide Documents
None: `PYTHON_GUIDE.md` gains §6 "Showing a Liquers UI".

### Existing Documents Reviewed or Updated
Each with a History row and `reviewed: 2026-11-11`:
- `specs/reference/PYTHON_INTEGRATION.md`: §18 (new), §1, §2, §3 (payload, `lui`,
  `Context.element`), §7 (exit order), §8 (limitations).
- `specs/reference/UI_INTERFACE_FSD.md`: Hosts names the Python host.
- `specs/reference/PAYLOAD_GUIDE.md`: Language bindings.
- `specs/guides/PYTHON_GUIDE.md`: §1, §2, §6, §10; its examples run (`test_GUIDE_examples.py`).
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Python: *A Liquers UI*.
- `liquers-py/README.md`: the `UIUSE` row (COMPLETE, with Appendix A deviations) and the `EVAL05`
  wording.
- `specs/README.md`: the Python UI line is built.

### Module Docs
`liquers-py/src/ui.rs` (new), `environment.rs`, `builtins.rs`, `value.rs`, `context.rs`,
`command.rs`, `exit.rs`, `lib.rs`, and `python/liquers/_ui.py`, `_env.py`, `__init__.py`:
`check_api_docs.py` reports 0 warnings and 0 errors on each.

### User-Guide Chapters
`docs/guides/python/11-ui.md` rewritten (the model, callbacks, actions, the window); `01`,
`15` and `d-sister-guides` updated; `03`-`10`, `12`, `14` and `c-glossary` reviewed, unchanged
(nothing in them is about a UI). Every example that imports `liquers` runs.

### Links and Capability Map
`specs/README.md`: "Liquers UI from Python" is built, linking both designs.

## Issues Filed
None for this design; `PY-CANNOT-USE-A-LIQUERS-UI` is closed with a resolution note.

## Important Learning

- A host that delivers callbacks only on threads that ask needs no delivery thread and keeps
  callbacks off Liquers' threads; it does need "waits that could never return" to raise.
- Any blocking wait for a UI from inside a command is a deadlock risk, because the command may be
  the evaluation the UI waits for; refusing it is cheaper than detecting the cycle.

## Conformance and Remaining Work
Requested, approved and built agree, with the deviations above. Nothing remains here; elements
written in Python are `UIDEF`, a later feature.

## Validation
`xvfb-run -a bash scripts/test-liquers-py.sh` (pytest, stubtest, the docs build, the Rust unit
tests); `check_api_docs.py` on the modules above; `docs_index.py --check`; `liquers-docs check`.
