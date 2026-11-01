# Phase 1: High-Level Design — default namespaces for plan building

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible` — adds public items (`Environment::default_namespaces`,
  `EnvironmentBuilder::with_default_namespaces`, a registry field, a configuration key) and spans
  `liquers-core`, `liquers-template`, `liquers-lib` and `liquers-py` (`auto-fix.md` rules 3, 4, 6).
- **Leading issue:** whether a configured list implies root (question 1).
- **Explanation:** the user settled the lookup semantics on 2026-11-01 (see Scope Changes); the
  remaining questions have recommended answers and Phases 2-4 are written for them.
- **Open questions:**
  1. **Open design question — root after a configured list.** `default_namespaces: [pl]` in
     `EnvironmentConfig` replaces the main realm's registry chain. Recommended: root is appended
     when the list does not name it (`[pl]` → `["pl", "", "root"]`), so configuring a namespace
     never makes core commands unreachable; a list that names `root` keeps its own order. The
     registry field keeps its current meaning — the whole chain, exactly as written — so code that
     sets it is unaffected.
  2. **Proposed resolution — a realm without an entry uses the main chain.** The new per-realm map
     overrides `default_namespaces` only for the realms it names, so today's behaviour (one chain
     for every realm) is what an empty map means.
  3. **Proposed resolution — generated queries stay portable.** `plan::append_action` emits a bare
     action only when it resolves to the requested command both with the environment's chain and
     with the registry default `["", "root"]`; otherwise it writes `ns-<ns>`, `ns-root` included
     when a configured namespace would shadow a root command.
  4. **Proposed resolution — invalid entries.** An empty-string entry in a configured list is a
     `ConfigurationError`; a namespace in which no registered command lives is a build warning.
  5. **Proposed resolution — stored values and changed configuration.** A recipe's text does not
     change when the configuration does, so a value stored under one configuration may be served
     under another that resolves the same query to a different command. Documented, not
     prevented; a recipe that must be portable writes its `ns`.

## Scope Changes

**2026-11-01, user review.** (a) An explicit `ns` adds to the chain — its namespaces first, highest
priority first, then the defaults — instead of replacing a configured list; this is today's rule,
verified below, so the planner's lookup does not change. The former AC-5 (bare `ns` searches root
only) is dropped. (b) Defaults are held per realm in the registry, which every environment has,
because not every environment is built from `EnvironmentConfig`. (c) `EnvironmentConfig` sets the
main realm only and takes precedence over the registry. The separate `implicit_namespaces` field
of the first draft is gone.

**Verified current logic** (`liquers-core/src/plan.rs` `namespaces_for_query`,
`command_metadata.rs` `find_command_in_namespaces`): the list is the parameters of the last `ns`
before the action in its transform segment, followed by `CommandMetadataRegistry::default_namespaces`;
`find_command_in_namespaces` returns the first namespace in which `realm/namespace/name` exists. So
`ns` extends; the chain is first-match. `default_namespaces` is one `Vec<String>` used for every
realm — it cannot be set per realm today.

## Purpose

Let an environment declare which command namespaces a query searches after any it names, so that
users of a library-heavy environment (Polars, UI) do not write `ns-pl` into every query and recipe.

## Problem Example

Against `specs/command_registry.yaml` (checked with `liquers-validate`):

```
-R/data/sales.csv/-/ns-pl/head-3     -> plans; head resolves to pl/head
-R/data/sales.csv/-/head-3           -> ActionNotRegistered: 'head' (keys tried: '--head')
```

With `default_namespaces: [pl]` in the environment configuration, the second query plans like the
first.

## Scope and Acceptance Criteria

- **AC-1** Unqualified action resolves in a configured namespace
  WHEN an environment configured with `default_namespaces: [pl]` evaluates `-R/data/sales.csv/-/head-3`
  THEN `head` resolves to `pl/head`, and the plan equals the plan of `-R/data/sales.csv/-/ns-pl/head-3`
- **AC-2** Nothing configured, nothing changes
  WHEN neither the configuration nor the registry sets namespaces
  THEN every query plans as today and `Environment::default_namespaces("")` is `["", "root"]`
- **AC-3** An explicit `ns` comes first, then the defaults
  WHEN the main chain is `[pl, root]` and the query is `…/ns-lui/x`
  THEN `x` is looked up in `lui`, then `pl`, then root, and the first match wins
- **AC-4** Root is searched after a configured list
  WHEN `default_namespaces: [pl]` is configured and an action exists only in root (`to_text`)
  THEN it resolves to the root command; when it exists in both, the `pl` one wins
- ~~**AC-5** Bare `ns` asks for root only~~ — dropped 2026-11-01: `ns` extends the chain
  (Scope Changes).
- **AC-6** Configuration round-trip and precedence
  WHEN `default_namespaces` is read from YAML or JSON, written back, or applied by `with_config` to a
  builder whose registry already sets the main chain
  THEN the list survives the round-trip in order, an absent key is not written and leaves the
  registry's chain in place, a present key replaces it, and a Python environment built from the
  document resolves an unqualified action in the configured namespace
- **AC-7** Invalid entries
  WHEN a configured entry is the empty string, or names a namespace no registered command uses
  THEN `build()` fails with `ConfigurationError` for the first, and reports a warning for the second
- **AC-8** Generated queries stay portable
  WHEN `append_action` appends `pl/head`, or a root `x` that `pl/x` would shadow, in an
  environment configured with `[pl]`
  THEN the result carries `ns-pl`, respectively `ns-root`, and plans to the requested command both
  with the configured chain and with `["", "root"]`
- **AC-9** Other resolvers agree with the planner
  WHEN template diagnostics, template parameter overrides and `next` presets resolve an action
  THEN they use the same chain as the planner, per realm
- **AC-10** Offline validation can follow the environment
  WHEN `liquers-validate --default-namespace pl -- '-R/data/sales.csv/-/head-3'` runs
  THEN it plans with `head` resolved to `pl/head`; without the flag it reports the error as today
- **AC-11** Exported registry unchanged by default
  WHEN the command registry is exported with no per-realm chains set
  THEN the output is as before (no new key); an export with per-realm chains carries them
- **AC-12** A realm has its own chain
  WHEN the registry sets `img` as the chain of realm `py` and nothing for the main realm
  THEN an action in a `-py/…` segment is looked up in `img` (after any `ns`), while a main-realm
  action still uses `["", "root"]`

Non-goals: per-user or per-session namespaces; changing the chain after `build()`; setting
non-main realms from `EnvironmentConfig`; renaming `default_namespaces`; configuring namespaces
through `liquers-web`'s JavaScript API (it can call the builder setter later).

## Core Interactions

- Query / plan: the namespace chain for an action, per realm.
- Commands: `CommandMetadataRegistry` holds the chains and states the lookup rule once.
- Environment: `EnvironmentConfig`, `EnvironmentBuilder`, `Environment` accessor.
- Validation: a `liquers-validate` flag; registry overlay merge.
- Bindings: `liquers-py`'s `EnvCell::configured_builder` applies configuration fields one by one,
  not through `with_config`, so it gains one call.

## Crate Placement

`liquers-core` holds the field, the lookup rule, configuration and builder. `liquers-template`
(`diagnostics.rs`, `query/instantiate.rs`) and `liquers-lib` (`utils.rs`) replace their copies of
the rule with the core one. `liquers-py` (`environment.rs`) passes the configured list to its
builder. No new crate dependency.

## Documentation Intent

- Reference: extend `reference/ENVIRONMENT_CONFIG.md` (new key) and
  `reference/RECIPES_AND_PLANS.md` §Planner instructions (`ns` paragraph: chain, per-realm defaults).
- Guide: extend `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` with a short "default namespaces" section.
- Other documents: the `liquers-validate` flag in `CLAUDE.md`'s validation table.
- Documents to update: `specs/README.md` capability line, `specs/index.csv`.

## References

- `liquers-core/src/plan.rs` `namespaces_for_query`, `append_action`
- `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::default_namespaces`, `find_command_in_namespaces`
- `specs/design/template-diagnostics-namespace-scope/` (the per-segment lookup rule, complete)
- `specs/reference/ENVIRONMENT_CONFIG.md`, `specs/reference/RECIPES_AND_PLANS.md`
