---
id: EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE
kind: feature
title: The query and pipeline editor shows a nested pipeline element read-only
status: draft
priority: P3
complexity: M
area: [lib/ui]
design:
created: 2026-11-17
github:
---
## Problem

The editor element of `specs/design/query-pipeline-editor/` edits link arguments as subtrees, but
a nested pipeline (`ElementKind::Pipeline`, a realm block, or a `Source::Pipeline` argument) is
drawn as an `EditorNode::Nested` (or a `FieldValue::Other("a pipeline")`) that cannot be edited:
`with_level` in `liquers-lib/src/ui/editor/state.rs` walks only `Element` and link `Argument`
steps.

## Impact

A recipe pipeline with a realm block or a pipeline-valued argument can be edited only around the
nested part. Workaround: edit the recipe's pipeline YAML by hand, or the nested pipeline in its own
editor bound to an accessor of that part.

## Expected behaviour

`NodePath` steps into a nested pipeline (its elements, and a pipeline argument's elements) and every
edit applies there as at the top level, written back into the enclosing element.

## Discovery

Implementing `query-pipeline-editor` Step 6, 2026-11-17; recorded in its Phase 5.
