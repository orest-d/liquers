---
id: VARIADIC-METADATA-TAIL-CHECK-PHASE3
kind: design-phase
title: Examples and tests for runtime metadata validation
---

# Phase 3: Examples and Tests

## High-Level Introduction

This phase makes the approved validation path observable. A command author, GUI, and custom
logger inspect the same complete `IssueReport`; normal environment construction still rejects
an invalid command registry. Phase 4 will add the runnable example
`liquers-core/examples/issue_report_validation_demo.rs` without a store or web runtime.

## Example Type

The requested example type is runnable. The snippets specify the behavior that the executable
and tests will demonstrate.

## Overview Table

| Scenario | Input | Expected result | Verification |
| --- | --- | --- | --- |
| GUI preflight | Non-final ordinary variadic argument | Full report remains accessible | Unit test and example |
| Default build | Invalid registry | Detailed report emitted; bounded error returned | Integration test |
| Browser build | Same invalid registry on wasm | Browser console receives report | Wasm smoke test |
| Composition | Multiple producers | Appended issues preserve order | Unit test |

## Example 1: GUI or Custom-Logging Preflight

### Connection to the High-Level Design

The builder is the boundary that protects applications from manually constructed or deserialized
metadata. A GUI needs detailed diagnostics before consuming `build`.

### Sequence of Steps

1. Register metadata whose ordinary variadic argument is followed by an ordinary argument.
2. Call `validate` on the mutable builder.
3. Render every `validation_report().issues()` entry.
4. Do not call `build` while `has_errors` is true.

### Core Example Code

```rust,ignore
let mut builder = EnvironmentBuilder::new();
builder
    .command_registry
    .command_metadata_registry
    .add_command(&invalid_metadata);
let report = builder.validate();
assert!(report.has_errors());
for issue in report.issues() {
    gui.show(issue.severity(), issue.message());
}
assert_eq!(builder.validation_report().error_count(), 1);
```

### Guide and Executable Example

The executable creates invalid `CommandMetadata`, preflights it, displays the full report, then
shows that `build` returns the compact error. The environment guide will recommend this pattern
for GUI and custom logging. It must not present `GenericEnvironment::try_to_ref` as validation.

## Example 2: Safe Deserialization and Default Builder Failure

JSON or YAML metadata can deserialize structurally while having an unexecutable argument
layout. The registry check creates `Issue::CommandRegistry` values. `build` performs the same
validation before store construction, emits the nonempty report, and fails if it has errors.
Both serialization formats must be covered: deserialize invalid JSON into a registry and repeat
the same assertion with invalid YAML. The format is not allowed to bypass the common check.

```rust,ignore
let registry: CommandMetadataRegistry = serde_json::from_str(json)?;
let mut builder = EnvironmentBuilder::new();
builder.command_registry.command_metadata_registry = registry;
assert!(builder.validate().has_errors());
let error = match builder.build() {
    Ok(_) => panic!("invalid metadata must not build"),
    Err(error) => error,
};
assert!(error.to_string().contains("Command metadata registry contains 1 error"));
```

The YAML case uses the same setup with `serde_yaml::from_str(yaml)`. The returned build error is
deliberately short. The complete report is available only when the caller preflights with
`validate` (or reads `validation_report`) before consuming the builder with `build`; the
consuming build API cannot expose the builder afterward. The report's `Display` output retains
every diagnostic for users and logs.

## Example 3: Composing Reports and Bounded Summaries

Future validators append issues to the same report; neither nested nor optional reports are
needed. The exact tests use real command keys. The short summary gives the first issue plus at
most four more unique command keys in first-seen order and reports omitted keys.

```rust,ignore
let mut report = command_registry.check();
report.append(other_report);
assert_eq!(report.error_count(), 6);
assert!(report.short_summary("Command metadata registry").is_some());
assert_eq!(report.issues()[0].message(), "first diagnostic");
assert_eq!(report.issues()[1].message(), "second diagnostic");
let summary = report.short_summary("Command metadata registry").unwrap();
assert!(summary.contains("first diagnostic"));
assert!(summary.contains("realm-namespace-first"));
assert!(!summary.contains("realm-namespace-first, realm-namespace-first"));
assert!(format!("{report}").contains("first diagnostic"));
assert!(format!("{report}").contains("sixth diagnostic"));
```

## Corner Cases

| Case | Expected behavior |
| --- | --- |
| Ordinary variadic argument is last | No issue |
| Only injected arguments follow it | No starvation issue |
| Variadic argument is injected | Error: `multiple` and `injected` cannot combine |
| Several ordinary arguments follow | Each is reported with its blocking variadic argument |
| Invalid declarations share a command key | Summary names that key once |
| More than five command keys fail | Summary limits keys and gives omitted count |
| Warning-only report | Emitted and retained, but not turned into an error |
| Generic debug, info, or warning issue | Can be constructed without metadata and retains its severity |
| Generic error issue | Appears in the short summary but consumes no command-key slot |
| Empty report | No emission, summary, or error |
| Repeated builder validation | Report is rebuilt without duplicate issues |
| Native and wasm | stderr on native; browser console on wasm |

## Test Plan

### Unit Tests

| Component | Test |
| --- | --- |
| Metadata checking | Valid tail, starved argument, injected follower, invalid injected variadic |
| Registry checking | Aggregation in registration order |
| `Issue` | `Issue::Generic` at debug, info, warning, and error severity; severity, `is_error`, and message for every variant |
| `IssueReport` | Empty, append and extend order, counts, generic warning-only behavior, and generic-error summary behavior |
| Short summary | Singular and plural grammar, unique keys, five-key limit, omitted count |
| Display | All issues in stable order, without summary limit |
| Declaration builder | Uses metadata check instead of a divergent local layout validator |

### Integration Tests

| Flow | Test |
| --- | --- |
| Builder preflight | Stored non-optional report is exposed and rebuilt on repeat validation |
| Safety boundary | Programmatic, JSON, and YAML invalid metadata makes `build` fail |
| Failure ordering | A concrete counting `StoreFactory` proves failure precedes store construction and assembly |
| Compact and full diagnostics | Build error is bounded while report includes every invalid command |
| Target adapter | Native nonempty emission; the wasm adapter compiles and its console-emission function is callable |
| Runnable example | `cargo run -p liquers-core --example issue_report_validation_demo` succeeds |

### Manual Validation

Run focused core tests and the example, then compile the core crate for wasm and invoke the
callable console-emission adapter in the wasm smoke test. In a Liquers web development build,
trigger invalid metadata and manually verify that the browser developer console receives the
full report without a machine-local filesystem path.

## Documentation and Learning Log

Phase 5 will document the ordinary variadic-tail rule, the injected-argument exception, and
builder `validate` plus `validation_report` for GUI and custom logging. It will distinguish the
compact build error from full diagnostics and document target-specific emission. The report only
carries command-registry issues now. Follow-up `ISSUE-REPORT-PLAN-AND-METADATA-LOGGING` records
the separate design needed before plan validation or metadata logging adopts it.
