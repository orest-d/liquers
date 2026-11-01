# Phase 1: High-Level Design — default namespaces for plan building

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible` — adds public items (`Environment::default_namespaces`,
  `EnvironmentBuilder::with_default_namespaces`, a registry field) and a configuration key, and
  spans `liquers-core`, `liquers-template` and `liquers-lib` (`auto-fix.md` rules 3, 4 and 6).
- **Leading issue:** the interaction of an explicit `ns` with the configured list (question 1).
- **Explanation:** a working design exists under each answer to the questions below and Phases 2-4
  are written for the recommended one; the questions change query meaning, so they are the user's.
- **Open questions:**
  1. **Open design question — explicit `ns` replaces or extends.** With `default_namespaces: [pl]`,
     does `ns-lui/x` search `lui, root` (*replace*) or `lui, pl, root` (*extend*)? Recommended:
     **replace.** It is what "an implicit `ns` instruction" means under the existing rule that a later
     `ns` replaces an earlier one (`RECIPES_AND_PLANS.md` §Planner instructions), it keeps a query
     that names its namespaces independent of the environment it runs in, and bare `ns` becomes a
     way to ask for root only. *Extend* needs no new registry field (prepend to
     `default_namespaces`), but then no query can escape the configured list.
  2. **Proposed resolution — root is always searched.** The configured list is searched before
     root and cannot remove it; listing `root` explicitly is allowed and only changes its position.
     Removing root would break every query that uses a core command.
  3. **Proposed resolution — generated queries stay portable.** `plan::append_action`, which adds
     an `ns-<ns>` when an appended command would not resolve to the requested namespace, ignores the
     configured list, so a query a UI builds in one environment means the same in another.
  4. **Proposed resolution — an unknown namespace warns.** A configured namespace in which no
     registered command lives is a build-time warning in the validation report, not an error; an
     empty string is a `ConfigurationError`.
  5. **Proposed resolution — stored values and changed configuration.** A recipe's text does not
     change when the configuration does, so a value stored under one configuration may be served
     under another that resolves the same query to a different command (see Phase 2 Risks).
     Documented, not prevented; a recipe that must be portable writes its `ns`.

## Purpose

Let an environment declare which command namespaces a query searches when it names none, so that
users of a library-heavy environment (Polars, UI) do not write `ns-pl` into every query and recipe.

## Problem Example

Against `specs/command_registry.yaml` (checked with `liquers-validate`):

```
-R/data/sales.csv/-/ns-pl/head-3     -> plans; head resolves to pl/head
-R/data/sales.csv/-/head-3           -> ActionNotRegistered: 'head' (keys tried: '--head')
```

Today an environment cannot change the second result from a configuration document. With

```yaml
default_namespaces: [pl]
```

the second query plans exactly like the first.

## Scope and Acceptance Criteria

- **AC-1** Unqualified action resolves in a configured namespace
  WHEN an environment configured with `default_namespaces: [pl]` evaluates `-R/data/sales.csv/-/head-3`
  THEN `head` resolves to `pl/head`, and the plan equals the plan of `-R/data/sales.csv/-/ns-pl/head-3`
- **AC-2** Nothing configured, nothing changes
  WHEN no default namespaces are configured (key absent, or no setter called)
  THEN every query plans as today and `Environment::default_namespaces()` is empty
- **AC-3** An explicit `ns` replaces the implicit one
  WHEN `default_namespaces: [pl]` and the query is `…/ns-lui/x`
  THEN `x` is looked up in `lui`, then root, and not in `pl`
- **AC-4** Root is searched last
  WHEN `default_namespaces: [pl]` and an action exists only in root (`to_text`)
  THEN it resolves to the root command; and when the action exists in both, the `pl` one wins
- **AC-5** Bare `ns` asks for root only
  WHEN `default_namespaces: [pl]` and the query is `…/ns/head-3`
  THEN `head` is looked up in root only, as without configuration
- **AC-6** Configuration round-trip and composition
  WHEN `default_namespaces` is read from YAML or JSON, written back, or applied by `with_config`
  THEN the list survives the round-trip in order, an absent key is not written, and an absent key
  leaves a list set by `with_default_namespaces` in place
- **AC-7** Invalid entries
  WHEN a configured entry is the empty string, or names a namespace no registered command uses
  THEN `build()` fails with `ConfigurationError` for the first, and reports a warning for the second
- **AC-8** Generated queries stay portable
  WHEN `append_action` appends `pl/head` to a query in an environment configured with `[pl]`
  THEN the result still carries `ns-pl`
- **AC-9** Other resolvers agree with the planner
  WHEN template diagnostics, template parameter overrides and `next` presets resolve an action
  THEN they use the same namespace list as the planner, implicit namespaces included
- **AC-10** Offline validation can follow the environment
  WHEN `liquers-validate --default-namespace pl -- '-R/data/sales.csv/-/head-3'` runs
  THEN it plans with `head` resolved to `pl/head`; without the flag it reports the error as today
- **AC-11** Exported registry unchanged by default
  WHEN the command registry is exported with no default namespaces configured
  THEN the output is byte-for-byte as before (no new key), and an export from a configured registry
  carries the list

Non-goals: per-user or per-session namespaces; changing the namespace search at run time after
`build()`; renaming the registry's existing `default_namespaces` field (the root fallback);
configuring namespaces in `liquers-web`'s JavaScript API (it can call the builder setter later).

## Core Interactions

- Query / plan: the namespace list for an action that has no `ns` before it in its segment.
- Commands: `CommandMetadataRegistry` holds the list and computes lookup lists for every resolver.
- Environment: `EnvironmentConfig`, `EnvironmentBuilder`, `Environment` accessor.
- Validation: a `liquers-validate` flag; registry overlay merge.
- Bindings: Python gets the key through `EnvironmentConfig` documents with no binding change.

## Crate Placement

`liquers-core` holds the field, the lookup rule, configuration and builder. `liquers-template`
(`diagnostics.rs`, `query/instantiate.rs`) and `liquers-lib` (`utils.rs`) replace their copies of
the lookup rule with the core one. No new crate dependency.

## Documentation Intent

- Reference: extend `reference/ENVIRONMENT_CONFIG.md` (new key) and
  `reference/RECIPES_AND_PLANS.md` §Planner instructions (`ns` paragraph).
- Guide: extend `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` with a short "default namespaces" section.
- Other documents: the `liquers-validate` flag in `CLAUDE.md`'s validation table.
- Documents to update: `specs/README.md` capability line, `specs/index.csv`.

## References

- `liquers-core/src/plan.rs` `namespaces_for_query`, `append_action`
- `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::default_namespaces`, `find_command_in_namespaces`
- `specs/design/template-diagnostics-namespace-scope/` (the per-segment lookup rule, complete)
- `specs/reference/ENVIRONMENT_CONFIG.md`, `specs/reference/RECIPES_AND_PLANS.md`
