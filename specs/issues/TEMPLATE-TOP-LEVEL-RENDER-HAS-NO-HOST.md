---
id: TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST
kind: feature
title: A template cannot be rendered from outside a command without encoding it into a query
status: draft
priority: P3
complexity: M
area: [lib/commands]
design: template-render-in-env
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

## Problem Example

A script wants `"Ann has 42"` from `{{name}} has {{$ number-42}}` and `{name: "Ann"}`. Today it
must build `ns-template/render-<encode_token(text)>--strict` itself, call
`AssetManager::apply(recipe, state_with_data, None)`, read the state, and search the asset's log for
warnings. Each binding repeats this code. Verified at `cd6d5da`: no top-level render exists in
`liquers-lib`, `liquers-py` or `liquers-web`.

## Impact

Every language integration offering top-level rendering (`TEMPLATE04`, `TEMPLATE05` in
`guides/LANGUAGE-INTEGRATION_GUIDE.md`) builds the same detour, and cannot pass a native structure
in place at the top level.

## Expected behaviour

A helper in `liquers-lib::template` that creates the ad-hoc asset itself (an ad-hoc `apply` of
`ns-template/render`), encodes the template, and returns the text with the recovered warnings.
Integrations then call one function from the top level. Note: a signature that takes live frames
(`Vec<(AnyNode, FrameKind)>`) cannot be built without a core change. An ad-hoc asset receives only
a recipe, a `State` and the payload, so the data travels as the state (see
`specs/design/template-render-in-env/`). Complexity corrected from S to M: a new module, integration
tests and two documents in one crate.

## Discovery

Writing the `TEMPLATE` section of the language integration guide (2026-10-25).
