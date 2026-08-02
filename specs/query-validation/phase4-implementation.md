# Phase 4: Implementation Plan - Query Validation Utility

## Overview

**Feature:** Query validation utility for coding agents — `liquers_core::validate` plus two CLIs.

**Architecture:** A dependency-free `validate` module in `liquers-core` holds all logic; a
`liquers-validate` binary (feature `cli`) wraps it, and an `export-command-registry` binary in
`liquers-lib` produces the command metadata it consumes.

**Shape of the change:** almost entirely additive. Eight new files; four existing files touched,
by one line each in three of them:

| Existing file | Change |
|---|---|
| `liquers-core/Cargo.toml` | optional `clap`, `cli` feature, one `[[bin]]` |
| `liquers-core/src/lib.rs` | `pub mod validate;` |
| `liquers-lib/Cargo.toml` | optional `clap`, `cli` feature, one `[[bin]]` |
| `CLAUDE.md` | "Validating queries" and the `specs/command_registry.yaml` policy |

No existing signature changes, so `liquers-py`, `liquers-axum` and the `register_command!` macro
cannot break. `clap` 4 (current 4.6.5, crates.io reachable — verified) is the only new dependency,
and it is off by default.

**Sequencing rationale:** types → registry assembly → validation logic → unit tests → binaries →
integration tests → committed artifact → docs. Each step compiles on its own, so a failure never leaves a half-built
module. Unit tests land at step 5, before the binaries, so the library contract is pinned before
anything depends on it.

**Disk budget:** per `CLAUDE.md`, use `CARGO_INCREMENTAL=0` and `cargo test -p <crate> --lib
--tests`. `clap` adds a small dependency tree (`clap_builder`, `anstyle`, `strsim`); trivial next
to polars, but this is the first build after adding it, so expect one slow compile.

---

## Implementation Steps

### Step 1 — Cargo manifests and feature gating

**Files:** `liquers-core/Cargo.toml`, `liquers-lib/Cargo.toml`

```toml
# both crates
[features]
cli = ["dep:clap"]

[dependencies]
clap = { version = "4", features = ["derive"], optional = true }

# liquers-core
[[bin]]
name = "liquers-validate"
path = "src/bin/liquers_validate.rs"
required-features = ["cli"]

# liquers-lib
[[bin]]
name = "export-command-registry"
path = "src/bin/export_command_registry.rs"
required-features = ["cli"]
```

`cli` is **not** added to either `default` list. The `[[bin]]` blocks must be explicit: auto-
discovered binaries in `src/bin/` cannot carry `required-features`, so without these the crate
would fail to build whenever `cli` is off.

Create the two `src/bin/*.rs` files as `fn main() {}` stubs in this step so the manifest is valid.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo check -p liquers-core                      # cli off: no clap
CARGO_INCREMENTAL=0 cargo check -p liquers-core --features cli
CARGO_INCREMENTAL=0 cargo check -p liquers-core --no-default-features
```

**Agent:** haiku · skills: rust-best-practices · knowledge: both `Cargo.toml`s, Phase 2
"Integration Points". Mechanical, but the `required-features` trap makes it worth its own step.

---

### Step 2 — Report types

**File:** `liquers-core/src/validate/report.rs` (new)

Implement exactly the types in Phase 2 "Data Structures", plus `RecipeCheck` and
`ValidationResult.line` and `.recipe_check`:

`ValidationLevel`, `ValidationStatus`, `WarningSource`, `RecipeCheck`, `ValidationWarning`,
`ValidationResult`, `RegistryProvenance`, `ValidationCounts`, `ValidationReport`.

Plus:

```rust
impl std::str::FromStr for ValidationLevel { type Err = Error; /* "parse" | "plan" */ }
impl std::fmt::Display for ValidationLevel {}
impl std::fmt::Display for ValidationStatus {}

impl ValidationReport {
    pub fn exit_code(&self) -> i32;
    pub fn diagnostic_lines(&self) -> Vec<String>;
    pub fn to_json(&self) -> Result<String, Error>;
    pub fn to_yaml(&self) -> Result<String, Error>;
}
```

**Watch:** derive `PartialEq` on `ValidationWarning`, `RegistryProvenance`, `ValidationCounts`
but **not** on `ValidationResult`/`ValidationReport` — they embed `Plan`, which is not `PartialEq`
(`plan.rs:1399`). `FromStr::Err` is `Error` via `Error::general_error`, never a new error type.
Serialization errors wrap with `Error::from_error(ErrorType::General, e)`.

**Validation:** `CARGO_INCREMENTAL=0 cargo check -p liquers-core`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Data Structures" and "Trait
Implementations", `liquers-core/src/error.rs`, `plan.rs` `Plan`/`Step`.

---

### Step 3 — Registry assembly

**File:** `liquers-core/src/validate/registry.rs` (new)

```rust
pub fn from_json_or_yaml<T: serde::de::DeserializeOwned>(
    source_name: &str, text: &str,
) -> Result<T, Error>;

pub struct ValidationRegistryBuilder { /* registry, provenance, allow_overwrite */ }

impl ValidationRegistryBuilder {
    pub fn new() -> Self;
    pub fn with_overwrite_allowed(self, allow: bool) -> Self;
    pub fn merge_str(&mut self, source_name: &str, text: &str) -> Result<&mut Self, Error>;
    pub fn add_permissive_command(&mut self, spec: &str) -> Result<&mut Self, Error>;
    pub fn build(self) -> (CommandMetadataRegistry, RegistryProvenance);
}
```

**Two traps, both load-bearing:**

1. **Duplicate detection must precede `add_command`.** `CommandMetadataRegistry::add_command`
   overwrites silently (`command_metadata.rs:1067`), so check `self.registry.get(key.clone())`
   first and return `Err` unless `allow_overwrite`.
2. **`from_json_or_yaml` tries JSON then YAML and reports *both* failures.** Returning only the
   YAML error would mislead on a malformed JSON file.

Permissive command construction:
```rust
let mut cm = CommandMetadata::from_key(key);   // payload_required defaults to None — correct
cm.arguments = vec![ArgumentInfo::any_argument("arguments").set_multiple()];
```
Spec grammar: `name` → `("", "root", name)`; `ns/name`; `realm/ns/name`. Anything else is `Err`
naming the accepted forms. Also union `default_namespaces` and `global_enums` on merge.

**Validation:** `CARGO_INCREMENTAL=0 cargo check -p liquers-core`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Function Signatures →
registry.rs", `command_metadata.rs` (`CommandMetadataRegistry`, `CommandMetadata::from_key`,
`ArgumentInfo`), Phase 3 corner cases C3 and C6.

---

### Step 4 — Validation logic

**Files:** `liquers-core/src/validate/mod.rs` (new), `liquers-core/src/lib.rs` (one line)

```rust
pub mod registry;
pub mod report;
pub use registry::{from_json_or_yaml, ValidationRegistryBuilder};
pub use report::*;

pub fn validate_query(source: &str, index: usize, level: ValidationLevel,
                      cmr: &CommandMetadataRegistry) -> ValidationResult;
pub fn validate_recipe(recipe: &Recipe, index: usize, level: ValidationLevel,
                       cmr: &CommandMetadataRegistry) -> ValidationResult;
pub fn validate_recipes(recipes: &RecipeList, level: ValidationLevel,
                        cmr: &CommandMetadataRegistry) -> Vec<ValidationResult>;
pub fn build_report(level: ValidationLevel, registry: RegistryProvenance,
                    results: Vec<ValidationResult>) -> ValidationReport;
pub fn apply_cwd(recipes: &mut RecipeList, cwd: &str) -> Result<Vec<usize>, Error>;
fn collect_warnings(plan: &Plan) -> Vec<ValidationWarning>;
```

**Four behaviours that must be got right:**

1. **These functions never return `Err`.** A bad query is `ValidationResult { status: Error,
   error: Some(..) }`. This is the design's central decision; getting it wrong inverts the API.
2. **Recipe branch selection — and `store_to_key` is fallible.** It calls
   `filename()` → `get_query()` → `parse_query`, so a recipe whose *query* does not parse (the
   commonest defect there is) errors here, before `to_plan` is reached. `?` is **not available**:
   this function returns `ValidationResult`, not `Result`. Writing `?` will not compile, and
   "fixing" it by changing the return type would invert the design's central decision.
   ```rust
   let plan = match recipe.store_to_key() {
       Err(e)        => return ValidationResult::failed(index, source, e),  // recipe_check: None
       Ok(Some(key)) => { check = Some(RecipeCheck::Stored);
                          recipe.to_plan_for_key(cmr, &key) }
       Ok(None)      => { check = Some(RecipeCheck::AdHoc);
                          recipe.to_plan(cmr) }
   };
   ```
   Both `Ok` branches are correct outcomes; `AdHoc` is not a fallback. Record `recipe_check` in
   both, and leave it `None` when the key could not be computed at all.
3. **`collect_warnings` de-duplicates.** `Plan::set_error` sets `error` *and* pushes a
   `Step::Error` into `init_steps` (`plan.rs:1496`), so emit the `PlanError` warning once and skip
   the `init_steps` entry whose message equals `plan.error`'s.
4. **No `_ =>` arm on `Step`, and `Step::Plan` must recurse.** `Step` has **18** variants
   (`plan.rs:125-150`): one arm each for `Error` and `Warning`, one for `Plan(Plan)` which
   **recurses** — a nested plan carries its own `steps`, `init_steps` and `error`, so without
   recursion an error inside one is invisible and the result wrongly reports `Ok` — and one
   `|`-joined arm naming the remaining 15. Prefix nested messages with `nested plan: ` so depth is
   readable from the flat `warnings` list.

`build_report` computes `status` as the max over results (`ValidationStatus: Ord`) and fills
`counts`; **zero results must yield `Ok`, not `Error`** (Phase 3 C8).

**Validation:** `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Function Signatures" and
"Payload requirement", Phase 3 Examples 3 and 5, `recipes.rs` (`to_plan`, `to_plan_for_key`,
`store_to_key`), `plan.rs` (`Step`, `set_error`).

---

### Step 5 — Unit tests

**File:** `liquers-core/src/validate/mod.rs` and the two submodules, `#[cfg(test)] mod tests`

Implement U1–U26 from Phase 3 "Test Plan", plus U30 asserting that an error inside a
`Step::Plan` nested plan is collected (see behaviour 4). Distribute: U1–U10 and U19 in `mod.rs`, U11–U15 in
`registry.rs`, U16–U18 and U20–U26 in `report.rs`.

**Plus U27–U29, covering Phase 3 corner case C9** (a recipe carrying its own `cwd` while `--cwd`
is given). Step 4 exposes:

```rust
/// Apply a CLI-supplied cwd to a recipe list.
/// Returns the indices of recipes that already declare their own `cwd`; those are reported as
/// per-recipe findings, not as a whole-run failure. `Err` only for an unparseable `cwd`.
pub fn apply_cwd(recipes: &mut RecipeList, cwd: &str) -> Result<Vec<usize>, Error>;
```

**Do not delegate to `RecipeList::set_cwd`**, for two reasons found by inspection: it aborts on
the *first* offender (so a list with three bad recipes yields one finding, defeating batch mode),
and it *partially mutates* before returning `Err`, leaving earlier recipes already assigned.
`apply_cwd` iterates itself, assigning `cwd` where absent and collecting the rest.

It must `parse_key(cwd)?` **first** and pass `.encode()` on, mirroring `DefaultRecipeProvider`
(`recipes.rs:473`). `set_cwd` alone accepts any string — verified: `set_cwd("not a key!!")`
returns `Ok(())` — and the error would then surface inside `store_to_key`, in the no-`Err` zone.

- **U27**: a list with two `cwd`-carrying recipes returns both indices, and both surface as
  `ValidationResult`s with `status: Error` — *not* a single `Err`.
- **U28**: `apply_cwd` with an unparseable cwd returns `Err` (`ParseError`) and mutates nothing.
- **U29**: recipes lacking `cwd` are assigned it; those carrying one are left untouched.

**Watch:** compare **fields**, not whole `ValidationResult`s (no `PartialEq`). U3 must assert
`error.position.line == 1` and `column > 0` — position fidelity is what makes the output useful,
and it silently degrades if a constructor drops it. `unwrap()`/`expect()` are permitted here and
only here.

**Validation:** `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib validate`

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: Phase 3 "Test
Plan" table, `liquers-unittest` references/test-patterns.md.

---

### Step 6 — `liquers-validate` binary

**File:** `liquers-core/src/bin/liquers_validate.rs`

clap derive struct per Phase 2 "CLI: liquers-validate": positional `Vec<String>` queries;
`--query-file`, `--recipes` (both accept `-` for stdin) in a `group(required = true,
multiple = false)` with the positional; `--registry-file` (repeatable, env fallback
`LIQUERS_COMMAND_REGISTRY`), `--command` (repeatable), `--allow-overwrite`, `--level`, `--cwd`
(`requires = "recipes"`), `--format`, `--quiet`.

```rust
fn run() -> Result<i32, Error> { ... }
fn main() { match run() { Ok(code) => std::process::exit(code),
                          Err(e) => { eprintln!("ERROR  {e}"); std::process::exit(2) } } }
```

**Watch:** stdout carries **only** the serialized envelope — the `CLAUDE.md` rule this feature
motivated. Diagnostics and errors go to stderr. `--level` defaults to `Plan` when any registry
source (flag or env var) was supplied, else `Parse`. Query-file parsing skips blank and `#` lines
while tracking the true 1-based `line`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo run -p liquers-core --features cli --bin liquers-validate -- \
  '-R/data/report.txt/-/to_text'
CARGO_INCREMENTAL=0 cargo run -p liquers-core --features cli --bin liquers-validate -- \
  'bad query with spaces'; echo "exit=$?"     # expect 1

# C9 smoke: a recipe declaring its own cwd, plus --cwd, must fail as a tool error
printf 'recipes:\n  - query: -R/a/b.txt\n    cwd: elsewhere\n' > /tmp/c9.yaml
CARGO_INCREMENTAL=0 cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --recipes /tmp/c9.yaml --cwd reports; echo "exit=$?"   # expect 2, message names not_supported

# clap smoke: --cwd without --recipes is a usage error
CARGO_INCREMENTAL=0 cargo run -p liquers-core --features cli --bin liquers-validate -- \
  'to_text' --cwd reports; echo "exit=$?"     # expect 2
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 CLI section, Phase 3
Examples 1–5, clap 4 derive docs.

---

### Step 7 — `export-command-registry` binary

**File:** `liquers-lib/src/bin/export_command_registry.rs`

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() { ... }
```

**Three traps, all discovered during Phase 3 and each fatal if missed:**

1. **A tokio runtime is required, and it must be `current_thread`.** `DefaultEnvironment::new()`
   constructs `DefaultAssetManager`, which calls `tokio::spawn`; without a runtime it panics at
   *runtime*, not compile time. But bare `#[tokio::main]` expands to `Builder::new_multi_thread()`,
   which needs the `rt-multi-thread` feature — and `liquers-lib` enables only
   `["sync", "rt", "macros", "time"]` (`liquers-lib/Cargo.toml:63`), so it would fail to
   *compile*. `flavor = "current_thread"` works with plain `rt` and is sufficient: the exporter
   awaits nothing and the spawned asset-manager task never needs to run.
2. **Do not use `register_all_commands!`.** It expands to `register_egui_commands!` and
   `register_polars_commands!` unconditionally, and those macros do not exist when their features
   are off. Invoke each inside its own `#[cfg(feature = "…")]` block.
3. **Polars needs its own call** — `register_all_commands_fn` does *not* include it.

Environment: `DefaultEnvironment<Value, SimpleUIPayload>` (the payload the `lui` group needs).
Groups: `core`, `lui` (always available), `egui`, `image`, `polars` (feature-gated).
`--list-groups` prints what this binary actually has. `--groups` naming an uncompiled group is a
clear error listing the available ones, never a silent empty export.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo run -p liquers-lib --features cli --bin export-command-registry -- \
  --list-groups
CARGO_INCREMENTAL=0 cargo run -p liquers-lib --features cli --bin export-command-registry -- \
  -o /tmp/reg.json && python3 -c "import json;print(len(json.load(open('/tmp/reg.json'))['commands']))"
# expect 95 with default features (81 + the 14 always-available lui commands)
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Relevant Commands" and
exporter CLI, Phase 3 verification findings 3 and 4, `liquers-lib/src/commands.rs`,
`liquers-lib/src/environment.rs`, `liquers-lib/src/ui/payload.rs`.

---

### Step 8 — Integration tests

**Files:** `liquers-core/tests/validate_integration.rs`,
`liquers-lib/tests/export_registry_integration.rs` (both new)

I1–I3 and I4–I6 from Phase 3. I3 (`swallowing_trap_plans_differ`) is the highest-value test in the
suite: it asserts both queries validate `Ok`, that step counts are 2 and 1, and that the 1-step
plan's `GetAsset` key has **three** elements. That pins the behaviour the whole design is built
around, so a future parser change cannot silently invalidate it.

Exporter tests need `#[tokio::test]`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: Phase 3 Test Plan
integration table and Example 3.

---

### Step 8b — The committed registry and its maintenance policy

**Files:** `specs/command_registry.yaml` (new, generated),
`liquers-lib/tests/registry_freshness.rs` (new), `CLAUDE.md`

Three pieces, in order:

1. **Header support in the exporter** (extends step 7). When `--format yaml` writes to a file,
   emit the provenance header from Phase 2 "The committed artifact and its header". If the target
   already exists, read it first and **carry the `# CHANGELOG-BEGIN … # CHANGELOG-END` block over
   verbatim**; `serde_yaml` does not round-trip comments, so nothing else survives regeneration
   and nothing else needs to.
2. **Generate the artifact:**
   ```bash
   CARGO_INCREMENTAL=0 cargo run -p liquers-lib --features cli --bin export-command-registry -- \
     --format yaml -o specs/command_registry.yaml
   ```
   Commit it. Expect 95 commands with default features.
3. **Freshness test** — `liquers-lib/tests/registry_freshness.rs`, `#[tokio::test]`: build the
   registry in-process, deserialize `specs/command_registry.yaml`, and compare the sets of
   `CommandKey`s **and** each command's `metadata_version`. Fail with the regenerate command in
   the message. Compare parsed structures, never file bytes — key order and formatting are not
   the contract, and a byte comparison would fail spuriously on a serde_yaml version bump.

**Watch — corrected during implementation:** `metadata_version` **cannot** be used for this. It
is `#[serde(skip)]` (`command_metadata.rs:876`), so it never reaches the file and always reads
back as zero; comparing it silently compares every command against 0 and the test fails
immediately. Instead reproduce what `calculate_metadata_version` hashes — the command with
`impl_version` zeroed, serialized to JSON — which tracks the signature while ignoring
implementation identity. Verified by tampering: the test catches both a changed `doc` string and
a removed command.

**The `CLAUDE.md` policy** states: the file is generated, never hand-edited; regenerate whenever a
`register_command!` signature changes or a command is added or removed; add a dated changelog line
inside the markers; the freshness test in CI is what enforces it.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test registry_freshness
# then break it deliberately: add a command, re-run, confirm the test fails with the hint
```

**Agent:** sonnet · skills: rust-best-practices, liquers-unittest · knowledge: Phase 2 "The
committed artifact and its header", step 7, `command_metadata.rs` versioning.

---

### Step 9 — Documentation

**Files:** `CLAUDE.md`, `specs/query-validation/README.md` (new)

`CLAUDE.md` gains two entries:

- **"Validating queries"** under Common Tasks — the zero-setup invocation (`liquers-validate --
  '<query>'`, which finds `specs/command_registry.yaml` by itself), the design-overlay form with
  `--allow-overwrite` for a changed signature, and the warning that a green result means "here is
  what your query means", not "your query is right" (Example 3). This is the entry point for the
  agents the tool is *for*: if it is not in `CLAUDE.md`, the tool will not get used.
- **"Maintaining `specs/command_registry.yaml`"** — the policy from step 8b.

`PROJECT_OVERVIEW.md` needs **no** change: no core concept, Query encoding or Key encoding moved.

**Validation:** read-through; confirm the documented commands run verbatim.

**Agent:** haiku · skills: none · knowledge: `CLAUDE.md` Common Tasks section, Phase 3 Examples 1,
2 and 3.

---

### Step 10 — Feature matrix and final gate

**Files:** none

```bash
CARGO_INCREMENTAL=0 cargo check -p liquers-core --no-default-features
CARGO_INCREMENTAL=0 cargo check -p liquers-core --features cli
CARGO_INCREMENTAL=0 cargo check -p liquers-lib --no-default-features --features cli
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests
grep -rn '\bprintln!' liquers-core/src liquers-lib/src | grep -v eprintln! | grep -v '^\S*:[0-9]*://'
```

The last line must print nothing but the one doc-comment example: the binaries are the only
legitimate stdout writers, and they go through the envelope.

**Agent:** haiku · skills: rust-best-practices · knowledge: `CLAUDE.md` "Building and testing".

---

## Testing Plan

| When | What | Command |
|---|---|---|
| After steps 1–4 | Compiles, nothing regressed | `cargo check -p liquers-core` |
| After step 5 | Unit tests U1–U29 | `cargo test -p liquers-core --lib validate` |
| After steps 6–7 | Binaries run; manual smoke | the `cargo run` lines above |
| After step 8 | Full suites, both crates | `cargo test -p liquers-{core,lib} --lib --tests` |
| After step 8b | Registry freshness | `cargo test -p liquers-lib --test registry_freshness` |
| Step 10 | Feature matrix + stdout audit | the block above |

**Regression watch:** liquers-core currently passes **352** lib tests and liquers-lib **363**
across 14 suites (measured after the PR #14 merge). Any drop is a regression, not a rounding
error. Run `cargo clean` first if `target/` has seen several profiles — per `CLAUDE.md`, a clean
`liquers-lib` test build is ~4.2 GB and ~3 min.

**Not tested, deliberately:** store interaction (there is none), command execution (never runs
one), and clap's own parsing beyond the one `--cwd`-without-`--recipes` smoke test in step 6.
Phase 3's C10 (very long / deeply nested queries) is also deliberately untested: we impose no
limit the rest of the system lacks, so there is no behaviour of ours to assert.

---

## Agent Assignment

| Step | Model | Skills | Why this model |
|---|---|---|---|
| 1 Cargo | haiku | rust-best-practices | Mechanical edits; the only subtlety is `required-features` |
| 2 Report types | sonnet | rust-best-practices | Derive selection has a real trap (`PartialEq` vs `Plan`) |
| 3 Registry | sonnet | rust-best-practices | Two silent-failure traps (overwrite, dual-parser) |
| 4 Logic | sonnet | rust-best-practices | The core contract: failures-as-data, recipe branching, de-dup |
| 5 Unit tests | sonnet | liquers-unittest, rust-best-practices | 26 tests against a no-`PartialEq` type |
| 6 Validator CLI | sonnet | rust-best-practices | clap groups, env fallback, conditional default |
| 7 Exporter CLI | sonnet | rust-best-practices | Three fatal traps, one of them runtime-only |
| 8 Integration | sonnet | liquers-unittest, rust-best-practices | Cross-crate contract between the binaries |
| 8b Registry artifact | sonnet | rust-best-practices, liquers-unittest | Header preservation and a freshness test that must not be spuriously fragile |
| 9 Docs | haiku | — | Prose |
| 10 Gate | haiku | rust-best-practices | Running commands and reading output |

Every agent needs, as baseline knowledge: `CLAUDE.md` (conventions — no `unwrap`, no `println!`,
no `_ =>`, typed error constructors), and Phase 2 for the contract it implements.

**Steps 2 and 3 are independent** and can run in parallel; everything else is sequential.

---

## Rollback Plan

The change is additive, so rollback is per-step and cheap.

| Step | Rollback |
|---|---|
| 1 | Revert both `Cargo.toml`s; delete the two stub `src/bin/` files. Nothing else references them |
| 2–5 | `rm -r liquers-core/src/validate/` and drop `pub mod validate;` from `lib.rs`. Nothing outside the module imports it |
| 6 | Delete `liquers-core/src/bin/liquers_validate.rs` and its `[[bin]]` block |
| 7 | Delete `liquers-lib/src/bin/export_command_registry.rs` and its `[[bin]]` block |
| 8 | Delete the two `tests/` files |
| 8b | Delete `specs/command_registry.yaml` and the freshness test; the validator falls back to the env var, then empty. Nothing else reads the file |
| 9 | Revert the `CLAUDE.md` section; delete the README |

**Full rollback:** `git revert` the feature commits. Because `cli` is not a default feature and
`validate` is imported by nothing, a partially-applied plan still leaves both crates building and
all existing tests passing — the worst case is an unused module.

**The one irreversible-ish item is already landed and separate:** the `println!` → `eprintln!`
conversion across liquers-core and liquers-lib, committed ahead of this plan. It is independently
useful and should *not* be reverted with the feature.

---

## Open Risks

1. **`clap` is a new workspace dependency.** Small, but it is the first non-optional-by-default
   crate added in a while. Mitigated by `optional = true` + non-default feature: every existing
   build configuration is byte-identical without `--features cli`.
2. **The exporter's tokio requirement is runtime-only.** A missing `#[tokio::main]` compiles fine
   and panics on first run. Step 7's validation command runs the binary, which is the only way to
   catch it.
3. **`--list-groups` honesty depends on `cfg` discipline.** If a group is registered outside its
   `cfg` block, the listing and the export disagree. I5 covers the filtering, but a wrong-`cfg`
   build only shows up in the feature matrix at step 10.

---

## Implementation Notes (recorded during execution)

The plan was followed as written except for four points, each found by running the thing rather
than reading it.

1. **`metadata_version` cannot drive the freshness test.** Step 8b said to compare on it. It is
   `#[serde(skip)]` (`command_metadata.rs:876`), so it never reaches the file and always reads
   back as `Version(0)` — the test failed on its first run for every command. The working
   comparison reproduces what `calculate_metadata_version` hashes: the command with
   `impl_version` zeroed, serialized to JSON. That still tracks the signature and still ignores
   implementation identity, which was the intent. Verified by tampering — it catches both an
   edited `doc` string and a deleted command.

2. **clap's `requires = "recipes"` did not fire** for `--cwd`, so `--cwd` without `--recipes`
   exited 0 instead of 2. Replaced with an explicit check in `run()`, which also produces a
   better message than clap's generic one.

3. **The registration macros need more in scope than `type CommandEnvironment`.** They expand to
   `register_command!` invocations naming `Context`, `State`, `ArgumentType`,
   `CommandDefinition`, `CommandParameterValue`, `ValueInterface`, `SimpleValue` and
   `CommandRegistryAccess` unqualified. The exporter and the export test both import that set
   behind `#[allow(unused_imports)]`, since which ones are actually used depends on the enabled
   features.

4. **`cargo check -p liquers-core --no-default-features` fails**, with 142 unresolved-import
   errors for `futures` and `async_trait`. This is **pre-existing** — verified identical on
   `origin/main` — because `async_store` is a default feature the crate's code depends on
   unconditionally. It is unrelated to this feature and was dropped from the step 10 gate. The
   meaningful checks (`cli` off, `cli` on, and liquers-lib with `--no-default-features --features
   cli`) all pass.

One design claim was also corrected: **image commands are in namespace `img`, not `root`.** The
`register_command!` DSL keyword is `ns:`, not `namespace:`, so an earlier grep missed all 47 of
them. The real export is `img` 47, `pl` 26, `lui` 14, `""` 6, `dep` 2 = **95**.

### Final state

| Check | Result |
|---|---|
| `cargo test -p liquers-core --lib --tests` | 486 passed, 0 failed |
| `cargo test -p liquers-lib --lib --tests` | 367 passed, 0 failed |
| `cargo check -p liquers-core` (cli off) | clean; clap absent from the dependency tree |
| `cargo check -p liquers-core --features cli` | clean |
| `cargo check -p liquers-lib --no-default-features --features cli` | clean — proves the per-`cfg` group registration works where `register_all_commands!` would not compile |
| stdout audit | the only `println!` outside `src/bin/` is a doc-comment example |
| `specs/command_registry.yaml` | 95 commands, freshness test green, changelog survives regeneration |
