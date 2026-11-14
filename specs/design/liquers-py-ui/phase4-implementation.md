# Phase 4: Implementation Plan — liquers-py-ui

## Overview

The payload and the `lui` built-ins first, since every later step needs `PyEnvironment`'s new type;
then the value bridge, the model, the command context, the Python names, and the documents.
Prerequisite: [`egui-native-app`](../egui-native-app/DESIGN.md) Steps 1 and 2 (`NativeDriver`,
`spawn_headless`), which land as the first commit group of the same PR. The window
([`liquers-py-ui-window`](../liquers-py-ui-window/DESIGN.md)) follows as the third group.

## Progress

- [x] Step 1: the UI payload and the `lui` built-ins (8465894)
- [x] Step 2: UI elements cross the bridge (8465894)
- [x] Step 3: the model and `lq.UI` (8465894)
- [x] Step 4: `context.element` and the own-action mark (8465894)
- [x] Step 5: the Python names, stubs and docstrings (8465894)
- [x] Step 6: documents and module docs (the docs commit of PR #268)

## Implementation Steps

### Step 1: the UI payload and the `lui` built-ins
- Files / symbols: `liquers-py/src/environment.rs` `PyEnvironment`, `PyBuilder`, the
  `default_environment_builder` call; `liquers-py/src/builtins.rs` (`register_lui_commands!`).
- Change: `PyEnvironment = DefaultEnvironment<Value, SimpleUIPayload>`; nothing else changes type.
- Depends on: `egui-native-app` Step 1.
- Proof: `maturin develop` and the existing suite unchanged (`bash scripts/test-liquers-py.sh`).
- Rollback: revert the two files.

### Step 2: UI elements cross the bridge
- Files / symbols: `liquers-py/src/value.rs` `ext_to_py`, `py_to_ext`; `ui.rs` `UIElement`.
- Change: `ExtValue::UIElement` ↔ `lq.UIElement`, the same `Arc` both ways.
- Depends on: Step 1.
- Proof: test 4 (`02_a_ui_value_reaches_python`); test 5 needs `show` and passes with Step 3.
- Rollback: the variant falls back to the conversion error it raised before.

### Step 3: the model and `lq.UI`
- Files / symbols: `ui.rs` `UiModel`, `DriverSlot`, `UI`, `UIElementInfo`, `UIStatusEvent`,
  `UISubscription`, `UiList`, `stop_all_uis`; `environment.rs` `EnvCell::uis`, `add_ui`, the order
  in `shutdown`, `Environment.ui`; `exit.rs` (after the servers); `lib.rs` (classes).
- Change: Phase 2's interfaces, with the tick counter of Phase 3's learning log.
- Depends on: Steps 1, 2 and `egui-native-app` Step 2.
- Proof: tests 3, 5, 8, 9 and 11 to 21, 23 (tests 10 and 24 need Step 5's Python names).
- Rollback: remove `ui.rs` and the `uis` field; `shutdown` keeps its former order.

### Step 4: `context.element` and the own-action mark
- Files / symbols: `context.rs` `element`; `command.rs` `call_py_command` (`ActionMark::enter`).
- Change: the getter reads `UIPayload::handle` of the payload; the mark is set for the command's
  blocking call.
- Depends on: Step 3.
- Proof: tests 6, 7 and 22.
- Rollback: remove the getter and the mark; `wait_idle` inside an action then deadlocks until its
  timeout, which is why the mark is in this design.

### Step 5: the Python names, stubs and docstrings
- Files / symbols: `python/liquers/_ui.py` (`wait_idle_async`), `_env.py` (`ui`), `__init__.py`,
  `__init__.pyi`, `_native.pyi`.
- Depends on: Step 3.
- Proof: tests 10 and 24, and `test_STUBS_declarations.py`, `test_DOCS_docstrings.py`,
  `test_DOCS_api_reference.py`.
- Rollback: remove the names; the extension classes stay unreachable from `liquers`.

### Step 6: documents and module docs
- Files / symbols: Phase 2's documentation table; module docs of `ui.rs`, `environment.rs`,
  `builtins.rs`, `value.rs`, `context.rs`, `command.rs`, `exit.rs`, `lib.rs`, `_ui.py`, `_env.py`,
  `__init__.py`.
- Depends on: Steps 1 to 5.
- Proof: `python3 scripts/check_api_docs.py <file>` on each module;
  `python3 scripts/docs_index.py --check`; `cargo run -p liquers-docs -- check`.
- Rollback: documents only.

## Testing Plan

Each step runs its tests after `maturin develop --features debug-handles` in the venv of
`scripts/test-liquers-py.sh`; the last step runs the whole script (pytest, then the Rust unit
tests with `--no-default-features`). No `cfg(feature)` of liquers-lib changes in this design, so
the build matrix runs once, for `egui-native-app`.

## Rollback Plan

The three commit groups revert in reverse order. Without this design the window group cannot stand,
and `egui-native-app` stands alone (its examples use it).

## Documentation Updates

As Phase 2's table: `PYTHON_INTEGRATION.md` §18 (new), `PYTHON_GUIDE.md` "Show a Liquers UI",
`docs/guides/python/11-ui.md`, `UI_INTERFACE_FSD.md`, `PAYLOAD_GUIDE.md`, `liquers-py/README.md`,
`LANGUAGE-INTEGRATION_GUIDE.md` §6, `specs/README.md`; a History row and `reviewed:` bump on each
reference and guide. Close `PY-CANNOT-USE-A-LIQUERS-UI`; regenerate `specs/index.csv`. No command
signature changes, so `specs/command_registry.yaml` stays.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
