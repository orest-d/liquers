---
id: TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST
kind: feature
title: A template cannot be rendered from outside a command without encoding it into a query
status: draft
priority: P3
complexity: S
area: [lib/commands]
design: 
created: 2026-10-25
github:
---
## Problem

A template's query tags are evaluated by a `TemplateHost`. The only host that evaluates queries is
`liquers_lib::template::ContextHost`, which needs a command's `Context`. Outside a command — a
script calling `env.expand(text, data=…)` — there is no `Context`, and writing a host that calls the
store or executor directly would bypass the asset lifecycle (caching, dependencies, volatility,
cancellation, the log).

The route available today is an ad-hoc `AssetManager::apply` of `ns-template/render-…` to a state
holding the data. It works, but the template text has to be encoded into the query as a parameter,
and the data must first become a `Value` (structurally converted, or retained as a foreign value,
which is opaque until `FOREIGN-VALUE-HAS-NO-DATA-NODE` is fixed).

## Impact

Every language integration offering top-level rendering (`TEMPLATE04`, `TEMPLATE05` in
`guides/LANGUAGE-INTEGRATION_GUIDE.md`) builds the same detour, and cannot pass a native structure
in place at the top level.

## Expected behaviour

A helper in `liquers-lib::template`, for example
`render_in_env(envref, template: &str, frames: Vec<(AnyNode, FrameKind)>, options) -> Result<(String, Vec<Error>), Error>`,
that creates the ad-hoc asset itself, runs the renderer under a `ContextHost` for that asset's
context, and returns the text with the recovered warnings. Integrations then call one function from
the top level and the same frames code inside a command.

## Discovery

Writing the `TEMPLATE` section of the language integration guide (2026-10-25).
