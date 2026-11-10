---
id: CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND
kind: issue
title: The Rust QueryEditor adds an action for a command that is not registered
status: draft
priority: P3
complexity: S
area: [core/query]
design:
created: 2026-11-10
github:
---

## Problem

`QueryEditor::action(name)` in `liquers-core` appends an action without checking that a command of
that name exists in the metadata registry it was given; the mistake surfaces only when an argument
is set by name, or when the query is planned. The JavaScript editor built on it does refuse:
`new liquers.QueryEditor().action("nosuch")` throws `action_not_registered`.

## Impact

The editors are described as checking every edit (`PIPELINE_GUIDE.md` §3). A Rust or Python caller
that builds a query from user input gets a query that fails later, at a place further from the
mistake. Workaround: look the command up in the registry before calling `action`.

## Expected behaviour

`action` with an unknown name is refused with `ActionNotRegistered`, as in JavaScript, or the guide
says that only arguments are checked. One behaviour in all three languages.

## Discovery

Writing section 3.9 of the Rust user guide (2026-11-10): the example could not assert a refusal.
The JavaScript behaviour was confirmed by the example of `docs/guides/web/03-queries.md`, which
the Node runner executes. Re-check the Rust side when picking this up.
