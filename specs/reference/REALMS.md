---
title: Realms
kind: reference
audience: internal
area: [core/plan, core/commands, store/config]
reviewed: 2026-10-16
---
# Realms

A **realm** is the first component of a command's identity. It is chosen by the header of the
transform segment an action is written in. This document covers what the planner and the executor
do with realms today. It also covers the questions a *foreign* realm raises: a realm whose entries
are declared like commands but are not commands the interpreter can run. `store_config` is the
first such realm, and the worked case throughout.

For the query syntax around realms, see [`PROJECT_OVERVIEW.md`](PROJECT_OVERVIEW.md) §1. For
writing `store_config` specs, see
[`guides/STORE_CONFIG_GUIDE.md`](../guides/STORE_CONFIG_GUIDE.md).

## What a realm is

A command is identified by `CommandKey { realm, namespace, name }`
(`liquers-core/src/command_metadata.rs`). `CommandMetadata::with_realm` sets the realm. A command
registered without one, which covers every `register_command!` command, is in the **default
realm `""`**.

A query selects the realm with the header of a transform segment: `-<realm>/action/action`. A
segment with no header, or with an empty header name (`-/action`), is in realm `""`.
`TransformQuerySegment::name()` returns the header name, or `""` when there is no header.

```
to_text                               realm ""
-store_config/defaults/file_facts     realm "store_config", both actions
-R/a/b.txt/-/to_text                  realm "" (empty header name)
```

A *resource* segment's header name is not a realm. `PlanBuilder` warns that it ignores the name
("Resource header name is ignored", `plan.rs` `process_resource_query`), which keeps the name free
for a later meaning.

## How the planner applies a realm

**A segment's realm applies to every action in it.** `PlanBuilder::process_query` passes the
segment's `transform.name()` to `process_action`, which passes it on to `get_command_metadata`.
`get_command_metadata` looks the action up with
`CommandMetadataRegistry::find_command_in_namespaces(realm, namespaces, name)` (all in
`liquers-core/src/plan.rs`). Two other functions use the segment's realm the same way:
- `promote_relative_default_links`;
- `append_action` (`Query::last_transform_realm`).

The resulting `Step::Action` carries `realm`. Before `design/store-adapters/` Phase 4 step 0, only
the last action of a one-segment query was looked up in its realm. Every other action fell back to
`""`, so `-store_config/defaults/file_facts` failed. The tests `realm_header_applies_to_every_action`,
`realm_header_applies_to_a_single_action`, `realm_header_applies_after_a_resource_segment` and
`default_realm_plans_unchanged` in `plan.rs` pin the current behaviour.

Consequences:
- **Each transform segment has its own realm**, so one query can span several:
  `-store_config/defaults/-/to_text` plans `defaults` in `store_config` and `to_text` in `""`.
  The planner accepts this. Whether it can *run* depends on the executor (below).
- **Namespaces work the same in every realm.** The registry's `default_namespaces` (`""`, `"root"`)
  and any `ns-…` instruction are searched inside the segment's realm (`namespaces_for_query`).
- **An action missing from the realm is an ordinary "not registered" error** that names every
  command key it tried: `-foo/to_text` reports `Action 'to_text' not registered; command keys tried
  (realm/namespace/name): 'foo//to_text', 'foo/root/to_text'`, so a wrong realm shows as plainly as
  a missing namespace.

## How a realm is executed

There is one interpreter and one `CommandExecutor`, the environment's `CommandRegistry`
(`liquers-core/src/commands.rs`). It looks executors up by the full `CommandKey`, so a step in
realm `r` runs only if a command was registered under that key in realm `r`. Nothing dispatches a
realm to a different interpreter, environment or machine. That is
[`CORE-MULTI-REALM-INTERPRETER`](../issues/CORE-MULTI-REALM-INTERPRETER.md).

## Foreign realms

A foreign realm reuses the command machinery to *describe* things that are not commands. The
machinery covers parsing, planning, argument metadata, validation and registry export. Six
questions come up. `store_config` answers each as follows.

### Addressable, but not executable

An entry is **addressable** when a query can name it, the planner can resolve it, and
`liquers-validate` can check it. It is **executable** when the interpreter can run it.
`store_config` entries are declared in the ordinary `CommandMetadataRegistry`, so they are
addressable. They are never executable:
- `NON_EXECUTABLE_REALMS` (`["store_config"]`, `command_metadata.rs`) lists the realms that are
  declared only.
- `CommandRegistry`'s `execute` and `execute_async` call `refuse_non_executable_realm`. It returns
  `Error::not_supported`: "Command 'defaults' belongs to realm 'store_config', which is
  configuration and cannot be executed". So evaluating `-store_config/…` through the asset manager
  fails with that error. Test: `executor_refuses_the_store_config_realm`.

### A subject that is not a `Value`

A command transforms a `State<V>`. A `store_config` handler shapes a `MetadataRecord`, which is not
a `Value`, so it cannot be an executor of the environment. A foreign realm therefore brings its
own *consumer* of the plan. For `store_config` this is a plan **compiler**,
`StoreConfigFunctions::compile` (`liquers-core/src/store/realm.rs`). It plans the spec once, when
the store is built, and turns the plan into one closure that the store owns. Nothing walks the plan
at run time. The builders live in `StoreConfigFunctions`, beside the registry: `CommandRegistry`
has a `store_config` field, data only, never executed.

### Filling an omitted realm before planning

A configuration field may leave the realm out (`on_read: media_type/file_facts`). `PlanBuilder`
has no option for a default realm. Instead the consumer fills the realm into the *parsed* query
before planning, with `realm::fill_realm`:
- a transform segment with no header gets `-store_config`;
- a header with an empty name gets the name;
- a header naming another realm is a `configuration_error`, and so is a header with a level or
  header parameters.

Filling works on the AST, never on the text, so a spec that begins with a resource segment keeps
its shape. The filled query's `encode()` is what error messages quote and what `liquers-validate`
should be given. **The validator does not fill the realm.** A headerless spec given to it plans in
realm `""` and fails with "not registered".

### Which plan steps a realm accepts

The planner can produce any `Step`. A foreign realm's consumer decides which ones mean anything to
it, and refuses the rest. `StoreConfigFunctions::compile_plan` matches `Step` exhaustively:

| Step | In a `store_config` spec |
|---|---|
| `Action` (realm `store_config`) | the entry's builder, called with the resolved parameters. Literals only: a link, placeholder or injected parameter is refused |
| `Info`, `Warning`, `Error` | append the message to the record's log, once |
| `Plan` | compiled in place |
| `SetCwd` | refused |
| `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`, `UseKeyValue` | refused: "a resource segment, which is not supported in store configuration yet" |
| `GetAsset*`, `Evaluate`, `UseQueryValue` | refused: they need the asset layer |
| `Filename` | refused |

The planner emits its own diagnostics into `Plan::init_steps`. The compiler compiles those before
`steps`, so a planner warning ends up in the record's log. A component spec (`storage`, `writes`)
must plan to exactly one `Action`. Its message steps are ignored. The refusals are deliberate
places to extend later. `-R-key/docs/-store_config/front_matter`, for example, could become "these
steps for this key". The analysis is in `design/store-adapters/phase2-architecture.md` §4, and
the gap is
[`STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY`](../issues/STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY.md).

### Namespaces inside a realm

Every built-in `store_config` entry is in namespace `""`. The realm already keeps the entries apart
from ordinary commands, and the default namespaces make them reachable with no `ns` instruction.
An entry's *kind* (handler, storage model or write strategy) is recorded in `StoreConfigFunctions`.
It is checked against the field the spec is written in, so names need not encode it. Each entry
also carries a hint, `store_config: handler | storage | writes`, which is visible in the registry
export. Another crate may register entries in its own namespace, reached with `ns-<name>/…`.

### Registry export and overviews

A foreign realm's entries are in `specs/command_registry.yaml`, so validation sees them without
linking the crates that define them. The exporter (`liquers-lib/src/bin/export_command_registry.rs`)
has a group `store_config`, which is on by default. It includes `liquers-lib`'s
`front_matter` / `front_matter_sync` when the `records` feature is on.

An overview of *what can be run* must not list declared-only entries.
`CommandMetadataRegistry::executable_commands()` skips `NON_EXECUTABLE_REALMS`. Three overviews use
it:
- `commands_doc` (`liquers-lib/src/commands.rs`);
- `ns-search/commands` (`liquers-lib/src/search/command_catalog.rs`);
- `commandNames` (`liquers-web/src/environment.rs`).

The planner, the exporter and `liquers-validate` use the full `commands` list. Tests:
`executable_commands_skip_the_store_config_realm`, `commands_doc_omits_the_store_config_realm`
(`liquers-lib/tests/commands_doc.rs`) and `command_catalog_omits_the_store_config_realm`
(`liquers-lib/tests/search_commands.rs`).

## `store_config` at a glance

| Question | Answer |
|---|---|
| Realm name | `STORE_CONFIG_REALM = "store_config"` (`store/realm.rs`) |
| Declared by | `register_builtin_store_config`, called by `CommandRegistry::new()` and `StoreConfigFunctions::builtin()`; `liquers_lib::store_config::register_lib_store_config` |
| Consumer | `StoreConfigFunctions::compile` / `compile_component`, called by `StoreRouterBuilder` at store build |
| Subject | `MetadataRecord`, by `&mut` |
| Executable | no: `NON_EXECUTABLE_REALMS` |
| Realm prefix | optional in configuration and filled by `fill_realm`; required when validating |
| Namespaces | `""` for the built-ins |

## Open questions

- **Realm-aware dispatch.** A realm that *is* executable, but somewhere else (a browser frontend
  beside a server backend), needs an interpreter that routes steps by realm:
  [`CORE-MULTI-REALM-INTERPRETER`](../issues/CORE-MULTI-REALM-INTERPRETER.md).
- **Types per realm.** A query spanning realms cannot ask which value types the other realm holds:
  [`TYPE-REGISTRY-NOT-REALM-AWARE`](../issues/TYPE-REGISTRY-NOT-REALM-AWARE.md).
- **Language-defined entries.** A `store_config` builder is a plain `fn` pointer, so a Python or
  JavaScript callable cannot be one:
  [`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`](../issues/LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE.md).
- **Mixed-realm queries.** The planner accepts `-store_config/defaults/-/to_text`. Neither the
  `store_config` compiler (one realm only) nor the executor (refuses `store_config`) can use it.
  Nothing rejects such a query at plan time.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-16 | The not-registered error names the full command keys tried (realm/namespace/name). | `design/store-adapters/` phase-5 review |
| 2026-10-15 | Created by store-adapters: realms as a `CommandKey` component, the planner applying a segment's realm to every action, execution, the foreign-realm questions with `store_config` as the worked case, overviews via `executable_commands()`. | `design/store-adapters/` phase-5 |
