# Phase 1: High-Level Design - Query Validation Utility

## Feature Name

Query Validation Utility (`liquers-lib` library module + `liquers-validate` binary)

## Purpose

Give coding agents and developers a fast, offline way to check that a Liquers query string is
well-formed *before* it is committed into an example, a doc snippet or a unit test. Level 1
validates parsing only and prints the `Query` as JSON; level 2 additionally builds the execution
plan against a command registry and prints the `Plan` as JSON, catching unknown commands and
bad argument counts that parsing alone cannot detect.

## Core Interactions

### Query System
Consumes `liquers_core::parse::parse_query`. Read-only: no new syntax, no encoding changes.
Serializes the resulting `Query` via its existing `Serialize` derive.

### Store System
None. Validation is purely static — no store is opened, so resource queries (`-R/...`) are
checked structurally only, not for key existence.

### Command System
No new liquers commands. The utility *consumes* a `CommandMetadataRegistry` from four sources:
(1) empty, (2) the commands registered by `liquers-lib`, (3) a YAML/JSON registry file merged
into the base, (4) permissive command names given on the command line (accepting a single
`multiple`/`Any` argument, so any argument list validates).

### Asset System
None. No assets are created, evaluated or cached.

### Value Types
None. No `ExtValue` variant is added; the utility never instantiates a `Value`.

### Web/API
None.

### UI
None (CLI only; textual/JSON output).

## Crate Placement

**liquers-lib** — new module `liquers-lib/src/validate/` (library API) plus a `[[bin]]` target.
Rationale: the "commands registered in liquers-lib" registry is only reachable from this crate,
and the crate already sits above `liquers-core`/`liquers-macro` in the dependency flow. Nothing
is added to `liquers-core`; `Plan`, `Query` and `CommandMetadataRegistry` are already
`Serialize`/`Deserialize` there.

## Open Questions

1. Which liquers-lib command set is the "built-in" registry — `register_all_commands_fn`
   (core + egui + image) or also polars/lui, which need feature flags and a `UIPayload`?
   → Resolve in Phase 2 by checking what compiles under default features.
2. Argument-parsing dependency: add `clap` to `liquers-lib` (feature-gated on the binary) or
   hand-roll a small parser to avoid a new dependency? → Phase 2 decision.
3. Merge semantics for a registry file: overwrite on duplicate `CommandKey`, or error?
4. Output shape: bare `Query`/`Plan` JSON, or a wrapper envelope carrying `{status, error, …}`
   so an agent can parse failures uniformly? Exit codes to accompany it.
5. Should level 2 treat `Plan.error` / `Step::Error` entries as validation failures, or report
   them and still exit 0?

## References

- `liquers-core/src/parse.rs` (`parse_query`), `liquers-core/src/plan.rs` (`PlanBuilder`, `Plan`)
- `liquers-core/src/command_metadata.rs` (`CommandMetadataRegistry`, `ArgumentInfo::any_argument`,
  `set_multiple`)
- `liquers-lib/src/commands.rs` (`register_all_commands!`, `register_all_commands_fn`)
- `specs/PROJECT_OVERVIEW.md`, `specs/REGISTER_COMMAND_FSD.md`
