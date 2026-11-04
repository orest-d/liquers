---
id: WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE
kind: feature
title: The JavaScript pipeline editors have no command-definition mode, so they refuse placeholders
status: draft
priority: P3
complexity: S
area: [web]
design: 
created: 2026-11-04
github:
---
## Problem

`liquers-web`'s `PipelineEditor` and `QueryEditor` (`liquers-web/src/pipeline.rs`) wrap core's
`PipelineEditor::new` / `from_pipeline` / `QueryEditor`, which refuse placeholders and the input
state outside a command definition. Core's `PipelineEditor::for_definition(cmr, arguments)`, which
accepts placeholders for declared arguments, is not wrapped, and the JavaScript editors offer no
`placeholder` or `state` method. A page writing a pipeline command's body cannot build it through a
checked editor; it writes the object form and relies on `registerCommands` / `validateCommands` to
check it.

## Impact

Small: the batch check catches the same mistakes (unknown step, unknown argument, undeclared
placeholder), only later and as a report instead of a refused edit.

## Expected behaviour

`PipelineEditor.forCommand(arguments)` (or an option on the constructor) opening a definition-mode
editor, with `placeholder(at, arg, name)` and `state(at, arg)` methods.

## Discovery

Implementing design `web-pipelines` on 2026-11-04.
