---
id: COMMAND-METADATA-ENHANCEMENTS
kind: feature
title: Global enums cannot be registered, checked or versioned
status: accepted
priority: P2
complexity: M
area: [core/commands, core/validate]
design: global-enum-registration
created: 2026-08-08
github:
---
# COMMAND-METADATA-ENHANCEMENTS

## Re-scoped 2026-10-20 (bulk design)

This was an umbrella over four milestones. Checked at HEAD (18732dd), they stand as follows:

| Milestone | State at HEAD | Now tracked by |
|---|---|---|
| 1. Enum schema, local | **Done.** `EnumArgument` / `ArgumentType::Enum` (`liquers-core/src/command_metadata.rs`), the `register_command!` `enum:` syntax with typed values, `others: true` and GUI defaults (`liquers-macro/src/registration.rs` `EnumParameterSpec`), plan-time alias expansion (`plan.rs` `ParameterValue::from_string`, `Enum` arm), documented in `REGISTER_COMMAND_FSD.md` §Parameter Metadata. `ns-img/resize`'s `method` is one in use | — |
| 1. Enum schema, global | **Partly.** `ArgumentType::GlobalEnum`, `CommandMetadataRegistry::global_enums`, `get_global_enum`, `enum_ref:` and plan-time resolution exist. Missing: a registration method, any check of a reference (a typo surfaces only when a query plans), validation of the enum itself, and a metadata version that follows the enum's content | **this issue**, `design/global-enum-registration/` |
| 2. Dynamic enum providers | Not started (`// TODO: support for dynamic global enums`) | `COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED` |
| 3. Specialization | Not started (`//TODO: implement command specialization by type`) | `COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING` |
| 4. Input/output typing | Not started | `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` |

So this issue is now the global-enum remainder of milestone 1: complexity `M` (one crate,
`liquers-core`), title changed accordingly. Related gaps filed at the same time:
`GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS`, `ENUM-ALTERNATIVES-HAVE-NO-LABEL`. The old
sourceless draft `design/register-command-enum/` designed the `enum:` / `enum_ref:` DSL, which has
shipped.

### Problem example

```rust
cr.command_metadata_registry.global_enums.insert("img.method".into(), method_enum); // only way in
register_command!(cr, fn shrink(state, method: String (enum_ref: "img.methd")) -> result)?; // typo
builder.build()?;                         // succeeds
envref.evaluate("shrink-nearest").await;  // fails only here: "Global enum img.methd not defined"
```

Expected: a registration method that validates the enum, a `build` error naming `shrink`,
`method` and `img.methd`, and cached results invalidated when the enum's alternatives change.

## Original text (2026-08-08)


## Summary
Evolve command metadata from a mostly static registry schema into an extensible model that supports:
1. reusable enums (including global enums),
2. dynamic enum providers for runtime-generated alternatives,
3. command specialization/overloads,
4. explicit input/output typing and compatibility rules.

## Problem
Current command metadata is useful for registration and basic planning, but several advanced capabilities are unresolved:
1. global enum lifecycle and lookup consistency,
2. dynamic enum behavior and caching/validation strategy,
3. specialization precedence/conflict handling,
4. normalized typing model for command input/output contracts.

Without these, planner/UI/integration behavior can diverge, and metadata-driven tooling remains limited.

## Goals
1. Define stable schema for static + global + dynamic enums.
2. Define specialization model with deterministic resolution.
3. Add explicit input/output type constraints in metadata.
4. Preserve backward compatibility for existing command registrations.

## Non-Goals
1. Full redesign of command execution engine.
2. Language-binding parity in the first phase (Python bindings can follow).

## Proposed Scope
### A. Enum Model
1. Local enum definitions on arguments.
2. Registry-level global enums with namespaced identifiers.
3. Dynamic enum provider hooks with deterministic error behavior.

### B. Specialization Model
1. Allow specialized variants of a base command metadata entry.
2. Define precedence/ranking and ambiguity errors.
3. Ensure planner can choose one variant deterministically.

### C. Input/Output Typing
1. Add optional input type constraints per command.
2. Add optional output type descriptor per command.
3. Define validation points (registration, planning, runtime fallback).

## Cross-Cutting Requirements
1. Schema must remain serializable and backward-compatible.
2. Error messages must include command key + argument context.
3. Metadata APIs must be usable by planner and UI without duplicate logic.

## Suggested Milestones
1. Milestone 1: enum schema consolidation (local/global).
2. Milestone 2: dynamic enum provider contract and planner integration.
3. Milestone 3: specialization resolution algorithm + tests.
4. Milestone 4: input/output typing fields + validation hooks.

## Acceptance Criteria
1. Global enums can be registered, referenced, and validated consistently.
2. Dynamic enum arguments behave deterministically (including errors).
3. Specialization conflicts are detected and reported.
4. Input/output typing metadata is available and consumed by planner/UI paths.
5. Existing command registration patterns continue to work unchanged.
