---
id: PLAN-DEFAULT-NAMESPACES-NOT-CONFIGURABLE
kind: feature
title: An environment cannot name default namespaces for plan building
status: draft
priority: P2
complexity: L
area: [core/plan, core/context, core/commands]
design: plan-default-namespaces
created: 2026-11-01
github: 235
---

## Problem

A command registered outside the root namespace (`pl`, `lui`, `img`, …) is reachable only from a
query that names its namespace with an `ns` instruction. An environment whose users work mostly
with one library has to write `ns-pl` into every query and every recipe.

**Example.** Against `specs/command_registry.yaml`, `-R/data/sales.csv/-/ns-pl/head-3` plans
(`head` resolves to `pl/head`), while `-R/data/sales.csv/-/head-3` fails with `ActionNotRegistered`
("Action 'head' not registered; command keys tried: '--head'"). Both checked with
`liquers-validate`. Expected, once an environment declares `pl` as a default namespace: the second
query plans exactly like the first.

The planner searches the namespaces of the last `ns` before an action in its transform segment,
then `CommandMetadataRegistry::default_namespaces` (`["", "root"]`) —
`liquers-core/src/plan.rs` `namespaces_for_query`. Nothing in `EnvironmentConfig` or
`EnvironmentBuilder` sets namespaces, and `Environment` has no accessor stating them. The only way
in is mutating the registry's `default_namespaces` by hand, which is not documented, not
configurable from a document, and changes the root fallback rather than acting like an `ns`.

## Impact

Every query and recipe written for such an environment repeats the same `ns` prefix, and a query
copied from a namespace-heavy notebook into a recipe silently depends on it. No workaround through
configuration; a Rust embedding can mutate the registry field directly.

## Expected behaviour

- An environment can declare **default namespaces**, searched while building a plan as if the
  query carried an implicit `ns` instruction.
- They are held by the `Environment` and settable through `EnvironmentConfig` (so YAML, JSON and
  TOML documents and the Python binding get them) and through `EnvironmentBuilder`.
- When not specified, behaviour is unchanged: only the root namespace is searched.

Designed in design/plan-default-namespaces/ — readiness: needs-decision; automatic fixing:
not-eligible.

## Discovery

User request in the project thread, 2026-11-01 (GitHub #235). `specs/index.csv` searched for
`namespace` and `default_namespaces`: no duplicate; related closed design
`template-diagnostics-namespace-scope` shares the lookup rule this extends.
