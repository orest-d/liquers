# Phase 1: High-Level Design - Query Validation Utility

## Feature Name

Query Validation Utility (`liquers-validate` CLI + exported command-registry metadata)

## Purpose

Give coding agents and developers a fast, offline way to check that a Liquers query string is
well-formed *before* it is committed into an example, a doc snippet or a unit test. Level 1
validates parsing only and reports the `Query` as JSON; level 2 additionally builds the execution
plan against a command registry and reports the `Plan` as JSON, catching unknown commands and
bad argument counts that parsing alone cannot detect.

## Core Interactions

### Query System
Consumes `liquers_core::parse::parse_query`. Read-only: no new syntax, no encoding changes.
Serializes the resulting `Query` via its existing `Serialize` derive.

### Store System
None. Validation is purely static — no store is opened, so resource queries (`-R/...`) are
checked structurally only, never for key existence.

### Command System
No new liquers commands. The utility *consumes* a `CommandMetadataRegistry` assembled from a
base (empty, or the exported liquers-lib metadata) plus two overrides: a YAML/JSON registry file
merged in, and permissive command names given on the command line (one `Any` + `multiple`
argument each, so any argument list validates). Metadata is **data**, not linked code: a small
exporter in `liquers-lib` serializes its registered commands into a checked-in JSON file, so the
validator never links egui/polars/image and needs no rebuild when liquers-lib changes.

### Asset System
None. No assets are created, evaluated or cached.

### Value Types
None. No `ExtValue` variant is added; the utility never instantiates a `Value`.

### Web/API
None.

### UI
None (CLI only; JSON on stdout, exit codes for pass/fail).

## Crate Placement

**New workspace member `liquers-validate`** — depends only on `liquers-core` + `clap` + serde.
`parse_query`, `PlanBuilder` and `CommandMetadataRegistry` all live in `liquers-core`, so this
keeps the tool small and independent of liquers-lib's heavy optional features.

**`liquers-lib`** — gains only the registry *exporter* (a `[[bin]]` that dumps
`CommandRegistry::command_metadata_registry` as JSON) plus a freshness test that fails when the
checked-in export drifts from the registered commands. Nothing else in liquers-lib changes.

## Resolved Design Decisions

1. **Registry as generated data, not linked code.** A `build.rs` inside `liquers-lib` cannot do
   this — a build script cannot use the crate it is building, and command metadata is produced by
   `register_command!` in that crate's own code. The equivalent that works is an exporter binary
   plus a checked-in artifact, kept honest by a freshness test in CI.
2. **`clap`** is used for argument parsing (dependency of `liquers-validate` only).
3. **Duplicate `CommandKey` on merge is an error** by default; `--allow-overwrite` permits it.
4. **Output is a diagnostic envelope**, not a bare `Query`/`Plan`: status, the serialized
   `Query` and `Plan`, the serialized `Error` (which already carries `position` line/column/offset
   and the offending query), and registry provenance. Serialized `Plan` is the floor, not the cap.
5. **A `Plan` carrying `error` or `Step::Error` is a successful validation** — planning
   succeeded, and it is the caller's job to inspect the serialized plan and decide whether it
   encodes the intended behaviour. Only parse failure and plan-*construction* failure are errors.

## Open Questions

1. Which command set does the export cover — `register_all_commands!` (core + egui + image +
   polars + lui, requiring a `UIPayload` such as `SimpleUIPayload`), or a narrower default? One
   export per feature combination, or one maximal export? → Phase 2.
2. Where does the exported artifact live (`liquers-lib/registry/…` vs. a workspace-level
   `registries/…`), and does the validator embed it with `include_str!` or read it at runtime?
   → Phase 2.
3. Should the CLI accept a batch of queries (file / stdin, one per line) as well as a single
   query argument, so an agent can validate a whole example set in one call? → Phase 2.

## References

- `liquers-core/src/parse.rs` (`parse_query`), `liquers-core/src/plan.rs` (`PlanBuilder`, `Plan`)
- `liquers-core/src/command_metadata.rs` (`CommandMetadataRegistry`,
  `ArgumentInfo::any_argument`, `set_multiple`)
- `liquers-core/src/commands.rs` (`CommandRegistry::command_metadata_registry`, public)
- `liquers-lib/src/commands.rs` (`register_all_commands!`), `liquers-lib/src/ui/payload.rs`
- `specs/PROJECT_OVERVIEW.md`, `specs/REGISTER_COMMAND_FSD.md`
