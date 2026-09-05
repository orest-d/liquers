# Phase 2: Solution & Architecture - Runtime validation of variadic command metadata

## Overview

Keep `CommandMetadata::check()` as the authoritative description of command-metadata validity,
but return the new composable `IssueReport` rather than a command-specific vector. The environment
builder stores a fresh report from its preflight `validate()` method, emits any non-empty report,
and rejects errors before it assembles an environment, refreshes versions, allocates an `EnvRef`,
or starts an asset manager. The existing command-declaration converter adopts the same check after
deserializing a declaration, eliminating its divergent private tail validator.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---:|---|---|---|---|---|
| `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` | draft | P2 | Subject: macro declarations are guarded, but hand-built metadata is not. | no | no | Close at Phase 5. | Keep P2. |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | A context/injected argument after `multiple` consumes no action parameter and remains valid. | no | no | Preserve the exemption; do not alter macro ordering. | Keep P2. |
| `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` | draft | P2 | Another deserialization inconsistency, but it is independent of the variadic invariant. | no | no | Do not broaden this validation work. | Keep P2. |
| `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS` | in_progress | P3 | May add a metadata field, but does not affect the current checks or startup order. | no | no | Monitor only. | Keep P3. |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | Concerns mutations after sharing; this design validates only before first sharing. | no | no | State the boundary explicitly. | Keep P3. |

No unresolved blocker remains: the user selected environment construction as the complete-metadata
boundary. The closed `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED` regression already preserves
the identity fields used in diagnostics.

## Data Structures and Validation Contract

No metadata serde field, trait, generic bound, command, or query syntax changes. The public,
serializable report supports printing, logging, transport, inspection, and conversion without each
integration reimplementing diagnostics.

```rust
impl CommandMetadata {
    pub fn check(&self) -> IssueReport;
}

impl CommandMetadataRegistry {
    pub fn check(&self) -> IssueReport;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IssueReport {
    issues: Vec<Issue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Issue {
    /// A diagnostic not tied to a command registry entry.
    Generic {
        severity: IssueSeverity,
        message: String,
    },
    CommandRegistry {
        severity: IssueSeverity,
        realm: String,
        namespace: String,
        name: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity { Debug, Info, Warning, Error }

impl Issue {
    pub fn generic(severity: IssueSeverity, message: impl Into<String>) -> Self;
    pub fn command_registry(
        severity: IssueSeverity, realm: impl Into<String>, namespace: impl Into<String>,
        name: impl Into<String>, message: impl Into<String>,
    ) -> Self;
    pub fn severity(&self) -> IssueSeverity;
    pub fn is_error(&self) -> bool;
    pub fn message(&self) -> &str;
}

impl IssueReport {
    pub fn issues(&self) -> &[Issue];
    pub fn append(&mut self, other: IssueReport);
    pub fn extend<I: IntoIterator<Item = Issue>>(&mut self, issues: I);
    pub fn error_count(&self) -> usize;
    pub fn warning_count(&self) -> usize;
    pub fn has_errors(&self) -> bool;
    pub fn is_empty(&self) -> bool;
    pub fn short_summary(&self, subject: &str) -> Option<String>;
    pub fn to_error(&self, subject: &str) -> Option<Error>;
    pub fn emit(&self);
}

impl Display for IssueReport { /* full, ordered report */ }
```

`Issue::Generic` is available immediately for diagnostics with no command key. It carries each
of `Debug`, `Info`, `Warning`, and `Error`, so report behavior can be tested without inventing
metadata or waiting for a production warning source. Its `message`, `severity`, and `is_error`
behavior is identical to command-registry issues. A generic error contributes its first-error
message to a short summary but consumes no command-key slot.

`CommandMetadata::check` retains its existing name checks, delegates local argument checks to
`ArgumentInfo::check`, and scans `arguments` in declaration order. Those checks construct
`Issue::CommandRegistry` values with `Error` severity. `ArgumentInfo::check` reports
an empty argument name and an argument that combines `multiple` and `injected`. The command scan
records the first non-injected `multiple` argument; every later non-injected argument produces one
error naming both the starving argument and the starved argument. An injected argument after
`multiple` is valid and does not clear the remembered variadic argument, so a later ordinary
argument is still rejected. This matches the macro's existing mutually-exclusive flags and avoids
the inconsistent `multiple`/injection resolution paths in the planner.

`CommandMetadataRegistry::check` borrows the registry, calls `CommandMetadata::check` for every
command in stored order, and appends the resulting reports. It performs no mutation,
deduplication, or early exit. `IssueReport::append` composes report vectors in order, which is the
same mechanism future store and plan checks use. The `issues` field remains private: callers
inspect the ordered slice or report counts, while construction and aggregation stay controlled.

`Display` renders the complete report in stored/argument order, one severity-labelled diagnostic
per line. `emit()` is a no-op for an empty report; otherwise it writes that full rendering to native
stderr, and on `wasm32` calls `web_sys::console::error` so the browser developer console receives
the same report. `liquers-core/Cargo.toml` gains the wasm-only `web-sys` dependency with its
`console` feature. No logging facade is introduced; `Display` lets an application send the report
to its own logger instead.

## Trait Implementations

None. The existing inherent methods and `Environment` default readiness method are sufficient;
adding a trait or changing `Environment`'s required methods would expand the public implementor
contract without providing additional safety.

## Function Signatures

The report and the existing public command/registry checks are deliberate diagnostic API.
`EnvironmentBuilder` gains `validate(&mut self) -> &IssueReport` and
`validation_report(&self) -> &IssueReport`; its private report field is `IssueReport::default()` in
`new()`. `Environment::try_to_ref` and `EnvironmentBuilder::build` signatures stay unchanged,
preserving all generic bounds and callers.

## Sync vs Async

Validation is synchronous because it only reads in-memory metadata. It completes before the
existing potentially async manager startup and never holds a lock or performs blocking I/O.

## Error Handling

`EnvironmentBuilder::validate` clears its stored report, appends the current command-registry
report, and returns the stored report. It is deliberately the central composition seam for future
store, recipe, and asset-manager validation. GUI and custom logging code call it before `build()`,
then read `validation_report()` even for warning-only outcomes. `build(self)` calls `validate()`
by default; on a non-empty report it calls `emit()`, and on `has_errors()` it returns
`to_error("Command metadata registry")` before assembling the environment. This preserves the
builder's consuming API, so callers needing the full report after an error must preflight first.

For command metadata, the summary says `Command metadata registry contains N error(s); first: MESSAGE on COMMANDKEY.`
It then lists additional *unique* command keys with errors, up to five keys including the first:
`Further command keys with errors: KEY2, KEY3, ...`. If more unique keys exist it ends with
`(and N more)`. The error count always counts diagnostics, not keys, so several errors on one
command remain visible in the count while the key list stays compact. This is clearer than a raw
full report in an `Error`, while the emitted report retains every diagnostic. Singular wording is
used for one error/key.

Validation is synchronous and runs while the builder is exclusively owned; there is no I/O, async
work, lock, clone of the registry, or new trait method. It occurs before store construction and
environment assembly. `Environment::try_to_ref` keeps its existing readiness work (metadata
version refresh, `EnvRef` creation, manager startup) and does not retain a report; manually
assembled environments are outside this builder-owned preflight contract. Integrations that can
receive serialized metadata should use the fallible builder and call `validate()` when they need
the full report.

Deserialization and `ValidationRegistryBuilder::merge_str` deliberately remain parsing and merge
operations: they may construct an invalid registry so callers can collect their own diagnostics.
The safety guarantee begins when that registry is installed in an environment and builder
construction reaches `EnvironmentBuilder::build`; this is the chosen complete-metadata boundary.

`CommandDeclaration::build` is the exception with a stronger, declaration-local contract: it must
continue to reject invalid metadata immediately after serde conversion. Its private `validate`
function is replaced by an adapter over `CommandMetadata::check` that converts all error-level
diagnostics, in their deterministic order, into one existing declaration `parameter_error` using
the report summary. This
removes the incompatible non-injected-tail rule while preserving declaration-specific error typing
and warning behavior; it intentionally also makes the declaration path reject the existing
reserved-name diagnostic.

## Integration Points

| File | Change |
|---|---|
| `liquers-core/src/command_metadata.rs` | Implement the two checks and focused unit tests. |
| `liquers-core/src/issue_report.rs` | Add the generic severity, issue variants, composable report, full rendering, emission, and bounded summary. |
| `liquers-core/src/command_declaration.rs` | Replace the duplicate private variadic validator with `CommandMetadata::check` diagnostics and align declaration tests. |
| `liquers-core/src/lib.rs` | Export the new `issue_report` module. |
| `liquers-core/src/environment_builder.rs` | Store and expose builder validation reports; preflight before construction and add regression tests. |
| `liquers-core/Cargo.toml` | Add target-specific `web-sys` console support for report emission. |
| `specs/reference/COMMAND_DECLARATION.md` | Define the non-injected-tail rule and say build-time validation rejects error diagnostics. |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | Explain that manual/imported metadata receives the same validation at construction. |
| `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | Document validation as part of the readiness guarantee and the `to_ref` consequence. |

No command namespace is relevant: this introduces no command and changes no command signature.

## Relevant Commands

None. This design changes metadata validation and environment construction only; it creates no
command, changes no namespace, and needs no command-library registration.

## Tests and Acceptance Criteria

`command_metadata.rs` unit tests prove: a valid final `multiple` passes; a later ordinary argument
reports an error naming the command and both arguments; injected arguments may follow it; an
injected-and-multiple argument reports its command identity; and report aggregation, counts,
full rendering, unique-key truncation, and singular/plural summaries are deterministic. Native
tests capture report rendering through `Display`; wasm tests verify the console-emission adapter
is callable without changing report contents. `command_declaration.rs` tests replace the
existing non-final-`multiple` case with three cases: an injected tail is accepted, an ordinary tail
after an injected tail is rejected, and a final injected-and-multiple argument is rejected.

`environment_builder.rs` tests construct invalid metadata directly and by raw JSON/YAML serde into
`CommandMetadataRegistry`, install each into the builder's public registry, call `validate()`, and
assert that `validation_report()` is the same complete report before calling `build()`. The build
error is the bounded summary; the stored report and `Display` retain every offending command and
argument in stored and argument order. A separate multi-command builder test pins diagnostic line
order and proves the failure occurs before store construction or manager startup. `Display` tests are the deterministic evidence for exactly what
`emit()` sends; target-specific emission is kept to the one-line adapter and tested only for
successful invocation on its target. `IssueReport` unit tests construct `Issue::Generic` values
at debug, info, warning, and error severity. They prove warning-only reports emit but have no
error conversion, while a generic error participates in the summary. A valid variadic command,
including an injected tail argument, still builds with `Inline`
without a Tokio runtime.

The change is backwards compatible for valid metadata. Invalid hand-built metadata becomes an
early startup failure rather than a misleading plan-time result. Post-start registry mutation is
out of scope and remains governed by `POST-INIT-COMMAND-REGISTRATION`.

## Rust Review

Applied `rust-best-practices`: this is core-only and respects dependency direction; validation is
pure synchronous work; the wasm-only console binding is target-gated; `Issue` matching remains
exhaustive as variants grow; it adds no bounds or locks; and it uses the existing typed
`Error::general_error` constructor rather than a new error type or direct `Error::new`. No blocking
Rust-idiom finding remains.

## Documentation Architecture

| Path | Kind / audience / area | Phase 5 change and links |
|---|---|---|
| `specs/reference/COMMAND_DECLARATION.md` | Reference / internal / `core/commands` | Replace `must be last` with `last query-consuming argument`, state the `injected` exemption, the mutual exclusion, report semantics, and declaration/build-time checks; link to the registration guide. |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | Guide / internal / `core/commands, macro` | Add the manual/imported-metadata report and construction check beside the existing macro diagnostics; link to the declaration reference and construction guide. |
| `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | Guide / both / `core/context, core/assets, core/store` | Add builder preflight/report emission and explain that callers needing diagnostics invoke `validate()` before `build()`; link to both command documents. |
| `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md` | Issue / internal / `core/commands` | Close with the macro and construction-time protections. |
| `specs/README.md` | Capability map / internal | Keep the single surviving design link current. |

The authoritative `affects_docs` set is the three reference/guide paths in `DESIGN.md`; no new
document is needed because this specifies a safety property of existing metadata and environment
construction.
