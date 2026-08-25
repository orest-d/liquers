---
id: UI-VARIADIC-ARGUMENT-LIST-EDITOR
kind: feature
title: A variadic argument cannot be edited as a list in a parameter editor
status: draft
priority: P3
complexity: M
area: [lib/ui, lib/egui, web, core/query]
design: variadic-arguments-declaration
created: 2026-08-25
github:
---
## Problem

A `multiple` argument holds a list of values. A parameter editor should render it as a list — the
same widget the element type would get on its own, repeated, with add, delete and reorder. Nothing
in the workspace does this, and the obstacle is not the widget.

## Why this is a query-manipulation problem, not a widget problem

`ArgumentGUIInfo` needs no new variant: it already describes **one element's** widget, and
`ArgumentInfo.multiple` already says "there are several of these". The gap is in addressing and
editing the underlying query.

Nothing in `liquers-lib`, `liquers-web`, `liquers-axum` or `liquers-py` reads `gui_info` at all, so
there is no in-repo editor to extend. The one working consumer is an out-of-tree prototype,
[`orest-d/egui-midi-test`](https://github.com/orest-d/egui-midi-test/blob/master/src/editor.rs)
(HEAD `ed3fb10`), whose `edit_query` (`:328`) matches on `param.info.gui_info` and draws one egui
widget per argument. It is the right model, and it shows three concrete obstacles:

| Obstacle | Where | Why a variadic argument breaks it |
|---|---|---|
| Argument slot is assumed equal to parameter position | `extract_editor_records`, `:283` — zips `action.parameters` against `action_info.arguments.get(parameter_number)` and errors "Extra parameter N" otherwise | One `ArgumentInfo` owns a **range** of parameter positions, not one. An argument→parameter-range mapping is needed |
| The only edit operation overwrites or appends | `set_parameter_value`, `:245` — writes at `parameter_number`, or pushes when it equals `parameters.len()` | Add is nearly free; **delete and reorder need insert / remove / move** on `ActionRequest::parameters`, which does not exist |
| Parameters are located by counting argument types | `find_numeric_parameter`, `:41` — counts `ArgumentType::Integer`/`Float` occurrences to map MIDI controls onto parameters | With a variadic numeric argument the count no longer identifies a unique slot |

## Expected behaviour

A parameter editor renders an argument marked `multiple` as an ordered list of the element widget,
with controls to append, delete and reorder elements, writing each change back into the query.

## Fix direction

1. An argument→parameter-range mapping usable by any editor: given `CommandMetadata` and an
   `ActionRequest`, which parameter positions belong to which argument. This is the reusable part
   and belongs in `liquers-core` beside the existing resolution code, not in a UI crate.
2. Insert / remove / move operations on an action's parameters. See
   `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`, which concerns the correctness of writing a
   parameter value back into a query and should be fixed before or with this.
3. The renderer itself, in whichever UI crate grows a parameter editor first.

Steps 1 and 2 are the substance; step 3 is the part that looks like the feature.

## Discovery

Filed from `specs/design/variadic-arguments-declaration/` Phase 1, decision 5. That design makes
`multiple` declarable through `register_command!` but deliberately stops at the metadata contract:
`ArgumentGUIInfo` describes one element, `multiple` means "render a list of these". Everything
above is downstream of that contract and blocks nothing in it.
