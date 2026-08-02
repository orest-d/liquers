# Phase 2: Solution & Architecture - Query Validation Utility

## Overview

Three deliverables, in two crates:

| Deliverable | Location | Depends on |
|---|---|---|
| `validate` module — the whole validation logic, no I/O | `liquers-core/src/validate/` | nothing new |
| `liquers-validate` CLI | `liquers-core/src/bin/liquers_validate.rs` | `clap` (optional, feature `cli`) |
| `export-command-registry` CLI | `liquers-lib/src/bin/export_command_registry.rs` | `clap` (optional, feature `cli`) |

The module is the product; the binaries are thin argument-parsing shells over it. That split lets
unit tests exercise validation directly without spawning a process, and lets other crates
(liquers-axum, liquers-py) reuse validation later without inheriting a CLI.

**Central design decision: validation failures are data, not `Err`.** `validate_query` returns a
`ValidationResult`, never `Result<_, Error>`. A malformed query is the *expected output* of a
validator, so it lands in `result.error` (the serialized `liquers_core::error::Error`, which
already carries `position` and `query`). `Err` is reserved for the tool genuinely failing —
unreadable file, unparseable registry, bad CLI arguments. Conflating the two would force callers
to distinguish "the query is bad" from "the validator is broken" by inspecting message text.

### Discovered constraints that shape the design

1. **An unknown command is a hard `Err` from `PlanBuilder::build()`**, not a `Step::Error`
   (`plan.rs:1075` → `Error::action_not_registered`). So against an empty registry, *any* query
   containing an action fails at level 2. This is correct behaviour, and it is exactly why the
   `--command` override exists. Level 2 against an empty registry is only meaningful for pure
   key/resource queries.
2. **`CommandMetadataRegistry::add_command` silently overwrites** an existing `CommandKey`
   (`command_metadata.rs:1058`). Duplicate detection must therefore happen *before* the call, via
   `get(key)` — `add_command` cannot report the collision.
3. **`Recipe::to_plan` ignores `cwd`** entirely. `cwd` feeds only `Recipe::key()` and
   `store_to_key()`. So `--cwd` changes the reported target key, never the plan.
4. **`Plan::steps` and `Plan::init_steps` both carry diagnostics**; `Plan::error` is set by
   `set_error`, which *also* pushes a `Step::Error` into `init_steps`. Warning collection must
   therefore de-duplicate, or a plan error is reported twice.
5. **Keyed recipes are a payload boundary** (added by PR #14). `Recipe::to_plan_for_key(cmr, key)`
   (`recipes.rs:193`) runs `to_plan` and then rejects the result if `plan.payload_required` is
   `Required`, because a key names one shared asset while a payload is supplied per evaluation.
   Its doc comment notes it cannot be folded into `to_plan`, since only the caller that looked the
   recipe up knows the key it is registered under — and the validator *is* such a caller:
   `Recipe::store_to_key()` yields exactly that key from `cwd` + filename. See "Payload
   requirement" below.

## Data Structures

All types live in `liquers-core/src/validate/report.rs` unless noted, derive
`Serialize, Deserialize, Debug, Clone`, and are owned (no lifetimes) so they serialize freely.

```rust
/// How far validation goes.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// Parse only.
    #[default]
    Parse,
    /// Parse, then build the plan against a command registry.
    Plan,
}

/// Outcome of one validation. Ordered: Ok < Warning < Error.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ValidationStatus {
    #[default]
    Ok,
    /// Validation succeeded; the plan carries an error or warning step.
    Warning,
    /// The query did not parse, or the plan could not be constructed.
    Error,
}

/// Where a warning came from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSource {
    /// `Plan::error`.
    PlanError,
    /// `Step::Error` in `steps` or `init_steps`.
    StepError,
    /// `Step::Warning` in `steps` or `init_steps`.
    StepWarning,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValidationWarning {
    pub source: WarningSource,
    pub message: String,
}

/// Result for a single query or recipe.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValidationResult {
    /// Position in the input; 0 for a single query.
    pub index: usize,
    /// The query text exactly as supplied.
    pub source: String,
    /// The query re-encoded from the parsed `Query` (`Query::encode()`, `query.rs:1447`).
    /// Diffing this against `source` reveals mis-parses at level 1, with no registry —
    /// see "Why `encoded` earns its place" below. Absent when parsing failed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub encoded: Option<String>,
    /// 1-based line in the source file, when the input came from `--query-file`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub line: Option<usize>,
    /// Which recipe check ran. `None` for query inputs, and when the storage key
    /// could not be computed because the recipe's own query does not parse.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recipe_check: Option<RecipeCheck>,
    /// Recipe title, when the input was a recipe list.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
    pub status: ValidationStatus,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub query: Option<Query>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan: Option<Plan>,
    /// Storage key the recipe result would land under — `Recipe::store_to_key()`,
    /// i.e. `cwd` joined with the filename. Recipes only; `None` without `--cwd`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub key: Option<Key>,
    /// Set when `status == Error`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<Error>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<ValidationWarning>,
}

/// Where the registry came from — so an agent can tell an "unknown command" caused by a
/// genuinely wrong name from one caused by validating against the wrong registry.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RegistryProvenance {
    /// Files merged in, in order.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub merged_files: Vec<String>,
    /// Permissive commands added from the command line.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cli_commands: Vec<CommandKey>,
    /// Total commands in the assembled registry.
    pub command_count: usize,
    /// Namespaces searched by default.
    pub default_namespaces: Vec<String>,
}

/// The whole envelope: one of these is serialized to stdout per run.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValidationReport {
    /// Worst status across `results`.
    pub status: ValidationStatus,
    pub level: ValidationLevel,
    pub registry: RegistryProvenance,
    pub results: Vec<ValidationResult>,
    pub counts: ValidationCounts,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ValidationCounts {
    pub total: usize,
    pub ok: usize,
    pub warning: usize,
    pub error: usize,
}
```

Registry assembly, in `liquers-core/src/validate/registry.rs`:

```rust
/// Assembles a `CommandMetadataRegistry` from an empty base plus overrides,
/// recording provenance as it goes.
pub struct ValidationRegistryBuilder {
    registry: CommandMetadataRegistry,
    provenance: RegistryProvenance,
    allow_overwrite: bool,
}
```

Owned, not borrowed: it is built once per run and consumed by `build()`. No `Arc` — there is a
single owner and no sharing across threads.

## Trait Implementations

Deliberately few. This feature adds **no new traits** and implements no Liquers trait.

| Type | Derives | Notes |
|---|---|---|
| `ValidationLevel`, `ValidationStatus`, `WarningSource` | `Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq` | fieldless, so `Copy` is sound and avoids clone noise |
| `ValidationStatus` | additionally `PartialOrd, Ord` | lets `results.iter().map(\|r\| r.status).max()` compute the report status |
| `ValidationLevel`, `ValidationStatus` | additionally `Default` | `Parse` and `Ok` are the natural zero values |
| `ValidationResult`, `ValidationReport` | `Serialize, Deserialize, Debug, Clone` | not `Copy` (contain `String`/`Vec`); **not `PartialEq`** — they embed `Plan`, which derives only `Serialize, Deserialize, Debug, Clone` (`plan.rs:1399`). `Error` *is* `PartialEq` (`error.rs:41`), so it is `Plan` alone that blocks it. Tests must therefore compare fields, not whole results |
| `ValidationWarning`, `RegistryProvenance`, `ValidationCounts` | `Serialize, Deserialize, Debug, Clone, PartialEq` | these embed no `Plan`, so `PartialEq` is available and worth having for test assertions |
| `ValidationLevel` | `std::str::FromStr` | `"parse"` / `"plan"` for clap; returns `Error` |

`std::fmt::Display` on `ValidationStatus` and `ValidationLevel` for the human-readable stderr
lines. No `Serialize` is hand-written; every embedded type (`Query`, `Plan`, `Key`, `Error`,
`CommandKey`) already derives it.

## Sync vs Async

**Everything is synchronous, deliberately.** This is an explicit, justified exception to the
"async is the default" rule, on the grounds the rule itself allows (genuinely I/O-free code):

- `parse_query`, `PlanBuilder::build`, `Recipe::to_plan`, `RecipeList::set_cwd` are all sync.
- No store is opened, no asset is evaluated, no network or cache is touched.
- The only I/O is reading argv, files and stdin in the binaries — `std::fs` / `std::io` in `main`,
  outside the module.

Making the module async would add a runtime dependency and an executor to every caller while
awaiting nothing. If `liquers-axum` later wants to expose validation over HTTP, it can call these
sync functions from its async handler directly: they are CPU-bound and bounded by query length.

## Function Signatures

### `liquers-core/src/validate/mod.rs`

```rust
/// Validate a single query string. Never fails: a bad query is a `ValidationResult`
/// with `status == Error` and `error` set.
pub fn validate_query(
    source: &str,
    index: usize,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> ValidationResult;

/// Validate one recipe. Uses `Recipe::to_plan`, which additionally checks that every
/// `arguments` and `links` override names something in the plan's last action — and
/// `Recipe::to_plan_for_key` instead when the recipe resolves to a storage key, which
/// additionally enforces the payload boundary. See "Payload requirement".
pub fn validate_recipe(
    recipe: &Recipe,
    index: usize,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> ValidationResult;

/// Validate every recipe in a list, in order.
pub fn validate_recipes(
    recipes: &RecipeList,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> Vec<ValidationResult>;

/// Assemble results into the report, computing `status` and `counts`.
pub fn build_report(
    level: ValidationLevel,
    registry: RegistryProvenance,
    results: Vec<ValidationResult>,
) -> ValidationReport;

/// Deserialize JSON, falling back to YAML; on failure report *both* diagnostics, since
/// which parser "should" have succeeded is not knowable from the text alone.
pub fn from_json_or_yaml<T: serde::de::DeserializeOwned>(
    source_name: &str,
    text: &str,
) -> Result<T, Error>;
```

`index` is a parameter rather than derived inside, so `validate_recipes` can number entries and a
single query can be numbered 0 without a special case.

```rust
impl ValidationReport {
    /// 0 when nothing failed (warnings included), 1 when any result is `Error`.
    pub fn exit_code(&self) -> i32;
    /// Human-readable `WARNING  …` / `ERROR  …` lines, for stderr.
    pub fn diagnostic_lines(&self) -> Vec<String>;
    pub fn to_json(&self) -> Result<String, Error>;
    pub fn to_yaml(&self) -> Result<String, Error>;
}

/// Collect diagnostics from a successfully built plan, de-duplicating the
/// `Plan::error` / `init_steps` overlap noted in Overview constraint 4.
fn collect_warnings(plan: &Plan) -> Vec<ValidationWarning>;
```

`collect_warnings` matches `Step` exhaustively with **no `_ =>` arm**. `Step` has **18** variants
(`plan.rs:125-150`), so: one arm for `Step::Error`, one for `Step::Warning`, one for
`Step::Plan(Plan)` — which **recurses**, since a nested plan carries its own `steps`,
`init_steps` and `error`, and an error inside one would otherwise be invisible and the result
would report `Ok` — and one `|`-joined arm naming the remaining 15 variants explicitly. Adding a
`Step` variant then becomes a compile error, as the convention intends.

Nested-plan warnings are prefixed (`nested plan: …`) so an agent can tell depth from the flat
`warnings` list without the envelope growing a tree.

### `liquers-core/src/validate/registry.rs`

```rust
impl ValidationRegistryBuilder {
    /// Start from an empty registry (`default_namespaces` = `["", "root"]`).
    pub fn new() -> Self;
    /// Permit a merged command to replace one already present.
    pub fn with_overwrite_allowed(self, allow: bool) -> Self;

    /// Merge a serialized `CommandMetadataRegistry` (JSON or YAML). Errors on a duplicate
    /// `CommandKey` unless overwrite is allowed. Unions `default_namespaces` and `global_enums`.
    pub fn merge_str(&mut self, source_name: &str, text: &str) -> Result<&mut Self, Error>;

    /// Add a permissive command from a CLI spec: `name`, `ns/name` or `realm/ns/name`.
    /// The command takes one `Any` + `multiple` argument, so it accepts any argument list.
    pub fn add_permissive_command(&mut self, spec: &str) -> Result<&mut Self, Error>;

    pub fn build(self) -> (CommandMetadataRegistry, RegistryProvenance);
}
```

`&mut self -> Result<&mut Self, Error>` rather than a consuming builder, because these are called
in a fallible loop over CLI arguments where a consuming builder forces awkward reassignment.

**Why merging is repeatable, and why `--allow-overwrite` exists.** The motivating workflow is
designing commands, not just using them. A design document proposes new commands or *changes an
existing command's signature*; its queries must be validated against the proposed state, which is
the committed registry **plus** a design-time overlay:

```bash
liquers-validate --registry-file specs/command_registry.yaml \
                 --registry-file specs/my-feature/proposed_commands.yaml \
                 --allow-overwrite -- '<query from the design doc>'
```

Adding a command needs only the second file. **Changing a signature needs `--allow-overwrite`**,
because the key already exists in the base and a silent overwrite would be indistinguishable from
a typo — which is exactly why the default is an error. Later files win, so the overlay is applied
in argument order.

The permissive command is built as:

```rust
let mut cm = CommandMetadata::from_key(key);   // state_argument = any, definition = Registered
cm.arguments = vec![ArgumentInfo::any_argument("arguments").set_multiple()];
cm.doc = "Permissive command declared on the command line; accepts any arguments.".to_string();
```

`multiple` makes `ParameterValue::pop_value` consume *all* remaining parameters
(`plan.rs:574`), and `ArgumentType::Any` accepts any of them — so argument count and type are
both unconstrained, as required.

### CLI: `liquers-validate`

```
liquers-validate [OPTIONS] [QUERY]

Input (mutually exclusive, exactly one required):
  [QUERY]...                  One or more queries as positional arguments (primary path)
      --query-file <FILE>     Newline-separated queries from FILE, or `-` for stdin
      --recipes <FILE>        RecipeList (recipes.yaml shape) from FILE, or `-` for stdin

Registry:
      --registry-file <FILE>  Merge a serialized registry; repeatable, applied in order
      --command <SPEC>        Permissive command `name`|`ns/name`|`realm/ns/name`; repeatable
      --allow-overwrite       Permit duplicate CommandKey when merging
      --no-registry           Ignore LIQUERS_COMMAND_REGISTRY; force an empty base

Validation:
      --level <parse|plan>    Default: `plan` if any registry source was given, else `parse`
      --cwd <KEY>             Working directory for recipes; requires --recipes

Output:
      --format <json|yaml>    Default json
      --detail <summary|full> Default full (status + Query + Plan). `summary` drops Query and
                              Plan, keeping status, source, encoded, error and warnings
      --quiet                 Suppress the human-readable stderr lines
```

#### No short flags, and why

**Liquers queries begin with `-`.** `-R/data/…` is a resource query; `-` also opens transform
segments (`parse.rs:304,328,339`). A short `-R` for `--registry-file` would make the tool's own
primary documented invocation silently wrong:

```bash
liquers-validate '-R/data/report.txt/-/to_text'
#  clap reads this as --registry-file=/data/report.txt/-/to_text
#  -> reads a nonexistent registry file, validates nothing, never reaches the positional
```

That is a wrong answer rather than an error, in the first command a new user runs. So:

1. **No short flags at all.** `-R` and `-r` are the two characters most likely to begin a query,
   and reserving any single letter invites the same collision later. Long flags only.
2. **The positional takes `allow_hyphen_values(true)`**, so a leading `-` is a value, not a flag.
3. **`--` is documented as the robust form** — `liquers-validate -- '-R/…'` — and used in every
   example that starts with a query, so copy-pasted commands stay correct even if a future flag
   is added.

#### Why `encoded` earns its place

`Query::encode()` (`query.rs:1447`) round-trips a parsed query back to text — the same
normalization `Recipe::new` already applies. Emitting it turns the `-R/` swallowing trap
(see Phase 3 Example 3) into a one-line diff an agent can check **at level 1, with no registry
and no export step**:

```
source:  -R/data/report.txt/to_text        encoded: -R/data/report.txt/to_text
source:  -R/data/report.txt/-/to_text      encoded: -R/data/report.txt/-/to_text
```

It costs one `String` per result and is the cheapest high-value signal in the envelope. `--detail
summary` keeps it precisely because it carries most of the diagnostic value at a fraction of the
size: a full `Plan` serializes to hundreds of bytes per result, mostly `position` objects, which
matters when an agent batch-validates thirty queries to find two failures.

Exit codes: **0** all results ok or warning · **1** at least one result errored · **2** usage or
I/O failure (clap's default for argument errors).

`--level` defaults by presence of a registry source because level 2 is meaningless without one
(constraint 1) — but `--level plan` with no registry is still accepted and will simply report
`action_not_registered` for every action, which is a legitimate thing to ask for.

`--cwd` is recipe-only, enforced by `requires = "recipes"` in clap: nothing in the query plan path
consumes a cwd, so accepting it for a bare query would silently do nothing.

#### Batch queries

All three input forms produce the same `Vec<ValidationResult>`, so batching is not a third input
mode — only a different count. Positional arguments accept one or more queries; `--query-file`
takes **one query per line**.

Newline separation is safe rather than merely conventional, and so is the comment marker: the
query grammar admits only alphanumerics, `_ + . - / ~` and entity escapes
(`parse.rs:201-237`), so neither a newline nor `#` can occur inside a query. A stray newline is
therefore impossible to mistake for query content, and `parse_query` rejects unconsumed input
(`parse.rs:758`) rather than silently truncating, so the separator is self-checking.

Blank lines and lines whose first non-whitespace character is `#` are skipped, letting an agent
annotate a query list it generates. Because skipping shifts the ordinals, `ValidationResult`
gains one field so a finding can be traced back to the file:

```rust
/// 1-based line in the source file, when the input came from --query-file.
#[serde(skip_serializing_if = "Option::is_none", default)]
pub line: Option<usize>,
```

### CLI: `export-command-registry`

```
export-command-registry [OPTIONS]

  -o, --output <FILE>         Default: stdout
  -f, --format <json|yaml>    Default json
  -g, --groups <LIST>         core,egui,image,polars,lui — default: all compiled in
  -n, --namespaces <LIST>     Keep only these namespaces — default: all
      --list-groups           Print the groups this binary was built with, and exit
```

Selection has **two axes** and both are needed. Cargo features are compile-time and decide what
*exists* (`cargo run -p liquers-lib --features cli,polars --bin export-command-registry`);
`--groups` and `--namespaces` are runtime filters over that. `--list-groups` is the bridge — it
reports what this particular binary can offer, so a caller never guesses.

The exporter builds `DefaultEnvironment<Value, SimpleUIPayload>` (the payload is required by the
`lui` group), invokes the selected `register_*_commands!` macros, and serializes
`env.get_command_metadata_registry()`.

#### The committed artifact and its header

`specs/command_registry.yaml` is checked in. YAML rather than JSON precisely because YAML has
comments: the file documents its own provenance and history.

```yaml
# Liquers command registry — GENERATED, do not edit by hand.
# Regenerate: cargo run -p liquers-lib --features cli --bin export-command-registry -- \
#               --format yaml -o specs/command_registry.yaml
# Origin:   groups=core,lui,egui,image,polars   features=egui,image-support,polars
# Commands: 95                                  Generated: 2026-08-02
# CHANGELOG-BEGIN
# 2026-08-02  initial export (95 commands)
# CHANGELOG-END
commands:
  - name: to_text
    ...
```

`serde_yaml` does not round-trip comments, so the exporter writes the header itself. When the
output file already exists it **preserves the lines between `# CHANGELOG-BEGIN` and
`# CHANGELOG-END` verbatim** and regenerates everything else. That is the whole mechanism — a
hand-maintained changelog that survives regeneration, without teaching the exporter to understand
YAML comments in general.

#### Registry resolution order

The validator takes the first source that exists:

1. `--registry-file` (repeatable, explicit — always wins)
2. `$LIQUERS_COMMAND_REGISTRY`
3. `specs/command_registry.yaml`, found by walking up from the working directory
4. empty

`--no-registry` forces 4. `RegistryProvenance.merged_files` always names what was actually used,
so the resolution is never hidden. Source 3 is what makes level 2 the default experience rather
than an opt-in: in a checkout of this repo, `liquers-validate -- '<query>'` plans against the real
command set with no build step at all.

### Payload requirement

PR #14 added `PayloadRequirement` (`command_metadata.rs:707`) and threaded it through plan
building. Three consequences for this design, none of them breaking:

1. **`Plan` gained `payload_required`**, so it is already in the serialized envelope for free —
   no new field of ours. It carries `#[serde(default)]`, so plans serialized before the field
   existed still deserialize.
2. **`CommandMetadata` gained `payload_required`**, defaulting to `PayloadRequirement::None` in
   both `new()` and `from_key()`. So the permissive CLI command needs no change and correctly
   declares no payload requirement. The field is `skip_serializing_if` its default, so exported
   registries stay compact and pre-existing registry files still load.
3. **Recipe validation should prefer `to_plan_for_key`** when the recipe has a storage key:

```rust
// store_to_key() is fallible for *query* reasons, not just cwd ones: it calls
// filename() -> get_query() -> parse_query. A recipe whose query does not parse —
// the single most common defect — errors HERE, before to_plan is ever reached.
// `?` is therefore not available: this function returns ValidationResult, not Result.
let plan = match recipe.store_to_key() {
    Err(e) => return ValidationResult::failed(index, source, e),   // recipe_check: None
    Ok(Some(key)) => { check = Some(RecipeCheck::Stored);          // payload boundary applies
                       recipe.to_plan_for_key(cmr, &key) }
    Ok(None)      => { check = Some(RecipeCheck::AdHoc);           // it does not
                       recipe.to_plan(cmr) }
};
```

**Both branches are first-class, not a primary path and a fallback.** A recipe need not be stored
under a key at all: `Recipe::filename()`'s own doc notes that "ad-hoc recipes (stemming e.g. from
web API calls) of queries converted to recipes do not need to have a filename", and such a recipe
is evaluated directly rather than becoming a keyed asset. The payload boundary is a property of
being keyed, so for an ad-hoc recipe there is nothing to enforce and `to_plan` is the *correct*
check, not a weaker one. `store_to_key()` is `cwd.join(filename)` and returns `None` when either
part is missing, which is exactly the ad-hoc case.

For a stored recipe the extra check is a real gain in coverage: "this recipe requires an
evaluation payload but is stored under a key" is precisely the kind of `recipes.yaml` defect this
tool exists to catch, and `assets.rs` now enforces it at evaluation time, so the validator matches
production.

Because the two branches check genuinely different things, **which one ran must be visible** —
otherwise an agent that forgot `--cwd` would silently receive the weaker check and read it as a
clean bill of health:

```rust
/// Which recipe check was applied. `None` for query inputs.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeCheck {
    /// Recipe resolves to a storage key; the payload boundary was enforced.
    Stored,
    /// Ad-hoc recipe with no storage key; the payload boundary does not apply.
    AdHoc,
}
```

carried on `ValidationResult` as
`#[serde(skip_serializing_if = "Option::is_none", default)] pub recipe_check: Option<RecipeCheck>`.

Note the two key-shaped methods are *not* interchangeable: `Recipe::key()` is the key of the
recipe's **query**, while `store_to_key()` is where the **result** is stored. `ValidationResult.key`
reports the latter.

## Integration Points

| Crate | Change | Risk |
|---|---|---|
| `liquers-core/Cargo.toml` | `clap = { version = "4", optional = true }`; `[features] cli = ["dep:clap"]`; `[[bin]] required-features = ["cli"]` | none — `cli` is **not** in `default`, so liquers-py and wasm builds are untouched |
| `liquers-core/src/lib.rs` | `pub mod validate;` | additive |
| `liquers-lib/Cargo.toml` | same `clap` + `cli` feature and `[[bin]]` | none, same reasoning |
| `liquers-py`, `liquers-axum`, `liquers-store` | **no change** | — |

No existing signature changes, so nothing in `liquers-py` or the `register_command!` macro can
break. The dependency flow is respected: `validate` sits in core and uses only core types;
the exporter sits in lib and looks only downward.

The `cli` feature must be checked in the matrix the conventions require: `--no-default-features`,
`--features cli`, and default. The binary and every `use clap::…` are gated together, and the
`validate` module itself is *not* gated — it has no optional dependency.

## Relevant Commands

**This feature registers no new liquers commands.** It consumes command metadata; it does not
extend the command system.

The namespaces that matter are the ones the exporter can emit, discovered by survey of
`liquers-lib/src`:

| Group | Namespace(s) | Explicit-namespace count | Cargo feature | Macro |
|---|---|---|---|---|
| `core` | `root`, `dep` | `dep`: 2 | always | `register_core_commands!` |
| `lui` | `lui` | 14 | **always** | `register_lui_commands!`; needs a `UIPayload` |
| `egui` | `root` | — | `egui` | `register_egui_commands!` |
| `image` | `img` | 47 | `image-support` | `register_image_commands!` |
| `polars` | `pl` | 26 | `polars` | `register_polars_commands!` |

`lui` is **not** gated on `egui`: `pub mod ui;` in `liquers-lib/src/lib.rs:8` carries no `cfg`, and
`ui/commands.rs` references no egui type. So `core` and `lui` are always exportable, and only the
last three groups depend on cargo features.

Counts are from a real export (95 commands with default features): `img` 47, `pl` 26, `lui` 14,
`""` 6, `dep` 2. Note the `register_command!` DSL keyword for a namespace is **`ns:`**, not
`namespace:` — grepping for the latter misses the 47 image commands entirely and makes `img`
look like `root`.

**The exporter must not use `register_all_commands!`.** That composite macro expands to
`register_egui_commands!` / `register_polars_commands!` unconditionally, and those macros do not
*exist* when their features are off (they are defined inside `#[cfg(feature = …)] pub mod` blocks),
so it only compiles with every feature enabled. The exporter instead invokes each macro inside its
own `#[cfg(feature = "…")]` block — which is also what makes `--list-groups` honest.

**Question for the user:** is this the right group decomposition, or should `core` split further
(e.g. `dep` separately), and should `webui` appear as a group once it registers commands?

## Error Handling

Every error is `liquers_core::error::Error`, built with typed constructors — no `Error::new`, no
new error type, no `unwrap()`/`expect()` outside tests.

| Situation | Handling |
|---|---|
| Query does not parse | `parse_query`'s `Error` → `ValidationResult.error`, status `Error`. Not an `Err`. |
| Command not registered | `PlanBuilder::build` → `Error::action_not_registered` → `ValidationResult.error`, status `Error` |
| Recipe override names a missing argument | `Recipe::to_plan`'s `Error::general_error` → `ValidationResult.error` |
| Stored recipe requires an evaluation payload | `Recipe::to_plan_for_key`'s `Error::general_error` → `ValidationResult.error`. Not reachable for an ad-hoc recipe, where the boundary does not apply; `recipe_check` records which case held |
| Plan built but carries `Plan::error` / `Step::Error` | status `Warning`, `warnings` populated, **exit 0** |
| Registry file is neither JSON nor YAML | `Err(Error::general_error)` quoting *both* parser messages, with the source name |
| Duplicate `CommandKey` on merge | `Err(Error::general_error)` naming the key and the file, unless `--allow-overwrite` |
| Malformed `--command` spec | `Err(Error::general_error)` showing the accepted forms |
| Unparseable `--cwd` | `apply_cwd` calls `parse_key(cwd)?` **first**, then passes `.encode()` on, mirroring `DefaultRecipeProvider` (`recipes.rs:473`). Propagated as `Err` — a CLI mistake, not a finding about the input. Note `RecipeList::set_cwd` alone would *not* catch this: it accepts any string and the error would surface later inside `store_to_key`, in the no-`Err` zone |
| Recipe carries its own `cwd` while `--cwd` given | Reported **per recipe** as `ValidationResult.status = Error`, not as a whole-run `Err` — see below |
| File unreadable / stdin closed | `Error::from_error(ErrorType::General, io_err)` |

In the binaries, argument handling and I/O live in a `run() -> Result<i32, Error>` function;
`main` prints any error to **stderr** and sets the exit code. Per the rule added to `CLAUDE.md`,
stdout carries only the serialized envelope.

### `--cwd` collisions are per-recipe findings, not a whole-run abort

`apply_cwd` does **not** delegate to `RecipeList::set_cwd`, for two reasons discovered by
inspection:

1. `set_cwd` **aborts on the first offender** and returns `Err`, so a list with three bad recipes
   yields one finding per run. That defeats the point of batch validation, whose whole value is
   seeing every problem at once.
2. It **partially mutates before failing** — recipes earlier in the list already have `cwd`
   assigned when it returns `Err`, leaving the list in a half-applied state.

So `apply_cwd` iterates itself: it sets `cwd` on recipes that lack one, and records the rest as
per-recipe `ValidationResult`s with `status: Error` carrying the same `Error::not_supported`
message. The *semantics are unchanged* — a recipe declaring its own `cwd` is still an error, and
such a `recipes.yaml` would still fail in `DefaultRecipeProvider` — but all offenders are
reported, and the finding lands where every other finding lands, as data.

This restores consistency with the central decision: `Err` from `apply_cwd` is reserved for the
one genuine CLI mistake (an unparseable `--cwd`), which is about the *invocation*, not the input.

### What stdout carries on each exit path

| Exit | Meaning | stdout |
|---|---|---|
| 0 | All results ok or warning | The envelope |
| 1 | At least one result errored | The envelope — the errors are *in* it |
| 2 | Tool or usage failure | **Empty**; the message is on stderr |

Exit 2 is the only path with no envelope, and it is deliberately narrow: bad arguments, an
unreadable file, an unparseable registry, an unparseable `--cwd`. An agent can therefore treat
"stdout is empty" as "I invoked it wrong" without parsing anything.

## Resolved Open Questions from Phase 1

1. **Zero-setup registry path — yes.** `--registry-file` falls back to the
   `LIQUERS_COMMAND_REGISTRY` environment variable when not given, so an agent that exports once
   can validate repeatedly without threading a path through every call. An explicit flag always
   wins; `RegistryProvenance.merged_files` records which was used, so the choice is never hidden.
2. **Exporter granularity — two axes, bridged by `--list-groups`** (see the CLI section).
3. **`--cwd` is recipe-only**, enforced by clap.

## Resolved Open Questions from this Phase

1. **Group decomposition confirmed** as `core` / `lui` / `egui` / `image` / `polars`.
2. **Batch queries: newline-separated**, plus multiple positional arguments — see "Batch queries".
3. **No per-result namespace reporting is needed.** `Step::Action` already carries the resolved
   `realm` and `ns` alongside `action_name`, `position` and `parameters` (`plan.rs:136-142`), so
   every action in a serialized plan is already self-describing. `RegistryProvenance` covers the
   registry-level default list, and the two together leave no gap.

## Open Questions

None outstanding for Phase 2. Two items carried forward as implementation-time checks:

- The `cli` feature must be verified across the build matrix (`--no-default-features`,
  `--features cli`, default) before Phase 4 sign-off.
- `liquers-store` (12), `liquers-macro` (7) and `liquers-py` (1) still contain `println!`;
  outside this feature's path, still awaiting a decision.
