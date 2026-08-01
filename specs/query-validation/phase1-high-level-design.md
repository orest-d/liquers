# Phase 1: High-Level Design - Query Validation Utility

## Feature Name

Query Validation Utility — `liquers_core::validate` module + `liquers-validate` CLI,
with a companion `export_command_registry` CLI in `liquers-lib`.

## Purpose

Give coding agents and developers a fast, offline way to check that a Liquers query — or a whole
recipe list — is well-formed *before* it is committed into an example, a doc snippet or a unit
test. Level 1 validates parsing only and reports the `Query` as JSON; level 2 additionally builds
the execution plan against a command registry and reports the `Plan` as JSON, catching unknown
commands and bad argument counts that parsing alone cannot detect.

## Core Interactions

### Query System
Consumes `liquers_core::parse::parse_query`. Read-only: no new syntax, no encoding changes.
Serializes the resulting `Query` via its existing `Serialize` derive.

### Store System
None. Validation is purely static — no store is opened, so resource queries (`-R/...`) and recipe
`cwd` keys are checked structurally only, never for key existence.

### Command System
No new liquers commands. The validator *consumes* a `CommandMetadataRegistry` assembled from an
empty base plus two overrides: a YAML/JSON registry file merged in, and permissive command names
given on the command line (one `Any` + `multiple` argument each, so any argument list validates).
Metadata is **data, not linked code** — `export_command_registry` in liquers-lib serializes its
registered commands to JSON/YAML, with selectable command groups and namespaces
(`root`, `pl`, `lui`, `dep`), so the validator never links egui/polars/image.

### Asset System
None. No assets are created, evaluated or cached. Recipes are validated as *specifications*
(`Recipe::to_plan`), never instantiated as assets.

### Value Types
None. No `ExtValue` variant is added; the validator never instantiates a rich `Value`.

### Web/API
None.

### UI
None (CLI only; JSON envelope on stdout, human-readable `WARNING …` lines on stderr, exit codes
for pass/fail).

## Crate Placement

**`liquers-core`** — `src/validate.rs` (pure library API, no new dependencies) plus a
`[[bin]] liquers-validate` gated behind a non-default `cli` feature with `clap` as an optional
dependency. Everything the validator needs already lives here: `parse_query`, `PlanBuilder`,
`CommandMetadataRegistry`, `Recipe::to_plan`, `RecipeList`, and a `Serialize` `Error` carrying
`position`. The feature gate keeps the "liquers-core stays minimal" rule intact: default builds
(liquers-py, wasm) pull in neither clap nor the binary.

**`liquers-lib`** — `[[bin]] export_command_registry` only. Builds the environment, registers the
selected command groups, and dumps `CommandRegistry::command_metadata_registry`.

## Resolved Design Decisions

1. **Registry as exported data.** A `build.rs` inside liquers-lib cannot produce it — a build
   script cannot use the crate it is building, and the metadata comes from `register_command!` in
   that crate's own code. An exporter binary is the equivalent that works.
2. **Two binaries, two crates.** The validator needs nothing outside liquers-core; only the
   exporter needs liquers-lib's heavy optional features.
3. **`clap`** for argument parsing, optional dependency behind the `cli` feature.
4. **Duplicate `CommandKey` on merge is an error** by default; `--allow-overwrite` permits it.
5. **Output is a diagnostic envelope**, not a bare `Query`/`Plan`: status, the serialized `Query`
   and `Plan`, the serialized `Error` (already carrying `position` line/column/offset and the
   offending query), and registry provenance. Serialized `Plan` is the floor, not the cap.
6. **A `Plan` carrying `error` or `Step::Error` is a successful validation, reported as a
   warning.** Planning succeeded, and the caller inspects the serialized plan to judge whether it
   encodes the intended behaviour. The envelope carries a `warnings` list and the CLI prints
   `WARNING  Plan contains error: …` (from `Plan::error`, `Step::Error`, `Step::Warning`, and
   `init_steps`, using the existing `Step::is_error`/`is_warning` helpers). Only parse failure and
   plan-*construction* failure are non-zero exits.
7. **Recipes and recipe lists are first-class inputs.** `Recipe::to_plan(&cmr)` already exists in
   `liquers-core/src/recipes.rs` and validates more than a bare query: it also checks that every
   `arguments` and `links` override names something present in the plan's last action. A
   `RecipeList` (the `recipes.yaml` format) supplies batch mode naturally.
8. **No checked-in registry artifact for now.** The exporter writes a file on demand; the
   validator reads it via `--registry-file`. If a committed artifact proves useful later, it can
   be added with a freshness test, without changing either tool's interface.
9. **Input kind comes from explicit CLI parameters, never inference.** A bare positional argument
   is a query — the shortest, most scriptable form and the primary path for an agent. Recipes come
   from a file or stdin (`--recipe`, `--recipe-list`, `-` meaning stdin); a query may optionally
   be read the same way. Nothing sniffs file extensions or content shape.
10. **Recipe `cwd` is supplied on the command line** (`--cwd`), since a recipe validated in
    isolation has lost the folder it came from. Note that `cwd` does *not* affect the plan —
    `Recipe::to_plan` never consults it. It affects `Recipe::key()` and `store_to_key()`, i.e. the
    resolved absolute target key, which the envelope reports as a diagnostic. An unparseable
    `--cwd` is itself a validation error.

## Open Questions

1. Does the validator need a zero-setup path to the liquers-lib registry (a conventional file
   path or `LIQUERS_COMMAND_REGISTRY` env var), or is passing `--registry-file` explicitly on
   every call good enough for an agent? → Phase 2.
2. Exporter selection granularity: cargo features are compile-time (`--features polars,egui`)
   while namespace filtering is runtime. Does one flag surface cover both, or are they separate?
   → Phase 2.
3. Does the exporter emit YAML as well as JSON, and does the validator accept both on input?
   → Phase 2 (both crates already depend on `serde_yaml`, so this is nearly free).
4. `--cwd` against a recipe list: `RecipeList::set_cwd` hard-errors when *any* recipe already
   carries its own `cwd` (and prints to stdout). Should the validator use it as-is, or default
   cwd only on the recipes that lack one and report the rest? → Phase 2.

## References

- `liquers-core/src/parse.rs` (`parse_query`), `liquers-core/src/plan.rs` (`PlanBuilder`, `Plan`)
- `liquers-core/src/recipes.rs` (`Recipe::to_plan`, `RecipeList`, `Recipe::get_cwd`)
- `liquers-core/src/command_metadata.rs` (`CommandMetadataRegistry`,
  `ArgumentInfo::any_argument`, `set_multiple`)
- `liquers-core/src/commands.rs` (`CommandRegistry::command_metadata_registry`, public)
- `liquers-lib/src/commands.rs` (`register_all_commands!`), `liquers-lib/src/ui/payload.rs`
  (`SimpleUIPayload`, needed for the `lui` group)
- `specs/PROJECT_OVERVIEW.md`, `specs/REGISTER_COMMAND_FSD.md`
