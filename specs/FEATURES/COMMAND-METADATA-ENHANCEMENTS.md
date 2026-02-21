# COMMAND-METADATA-ENHANCEMENTS

Status: Draft

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
