---
id: SIMPLE-TEMPLATE-HAS-NO-COMMAND
kind: feature
title: Simple templates have no command and lose their sub-queries' dependencies
status: draft
priority: P2
complexity: M
area: [core/query, lib/commands]
design: liquers-mustache
created: 2026-10-12
github:
---
## Problem

`liquers-core` parses and evaluates simple templates — text with `$<query>$` spans, `$$` for a
literal dollar (`parse.rs` `parse_simple_template`, `SimpleTemplate`; `interpreter.rs`
`evaluate_simple_template`) — but nothing in the workspace calls them: no command, recipe or UI
exposes the mechanism, so a template cannot be used from a query. The evaluator also:

- runs each span through the free `evaluate()` (`interpreter.rs`, marked "TODO: this should be
  decommissioned"), on a temporary `AssetRef` outside the asset manager, so span results are not
  cached and spans run one after another;
- discards each span's metadata and dependencies, so the rendered text records no dependency on
  what it embedded and is not invalidated when an embedded asset changes, and does not inherit a
  span's volatility;
- accepts only results convertible to a string (`Value::try_into_string`): an object, array or
  record result is a conversion error, so a search result cannot be embedded without a converter.

## Impact

Any document assembled from several queries — a context pack for an agent, a report, a generated
index page — has to be built by code. The predecessor project (`ranees-website`, `ranees-liquers`)
had an `expand` command over this exact mechanism and used it for page includes, so the capability
was in use before and was lost when commands moved to `register_command!`. Workaround: a custom
command per document.

## Expected behaviour

An `expand` command (state = template text) in `liquers-lib`, evaluating spans through the
`Environment` / asset manager with their dependencies and volatility carried into the result's
metadata, plus a way to render records as text (a Markdown table or list) so a search can be
embedded. Optionally a logic-bearing template command (`render`, mustache- or Jinja-like) whose
data model is the state's JSON; `design/agent-memory-mvp/brainstorm-workflow-improvements.md` §4
compares the options.

## Discovery

Templating research for `agent-memory-mvp`, 2026-10-12: assembling "context packs" for agents
needs a document built from several queries.
