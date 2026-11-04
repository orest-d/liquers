---
id: PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED
kind: issue
title: A pipeline command's placeholder operand types and GUI info are not checked or inferred
status: draft
priority: P3
complexity: S
area: [core/commands]
design: composite-command-definition
created: 2026-11-04
github:
---
# A pipeline command's placeholder operand types and GUI info are not checked or inferred

## Problem

Phase 2 of `design/composite-command-definition/` says `CommandMetadata::check` refuses a
placeholder whose declared type does not fit its element (a key for `resource`, `key_value` and
`cwd`; a query for `evaluate` and `query_value`), and that registration fills an omitted
`gui_info` from the slot a placeholder fills. Neither is implemented:
`CommandMetadata::check_pipeline_definition` (`liquers-core/src/command_metadata.rs`) checks only
undeclared placeholders, injected arguments, a body filename and a source command reading the
input state, and `CommandMetadataRegistry::complete_pipeline_command` fills only `argument_type`
and `multiple`.

## Impact

A mistyped operand is reported when the command is called (binding fails while planning), not
when it is registered; a pipeline command's argument shows the default text field in a UI.

## Expected behaviour

Registration reports a misfitting operand as an `Issue::CommandRegistry`, and an omitted
`gui_info` comes from the slot.

## Discovery

Deferred while implementing Step 11 of `design/composite-command-definition/`.
