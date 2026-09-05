# Phase 1: High-Level Design - Runtime validation of variadic command metadata

## Feature Name

Runtime validation of variadic command metadata

## Purpose

Reject invalid command metadata before an environment becomes usable. In particular, a variadic
(`multiple`) argument must not be followed by a non-injected argument, since it consumes every
remaining query parameter and starves the later argument.
Metadata that marks one argument as both `multiple` and `injected` is invalid for the same reason:
an injected argument cannot consume query parameters.

## Core Interactions

### Query System
No syntax change; this protects existing positional action-parameter interpretation.
### Store System / Asset System / Value Types / Web/API / UI
None.
### Command System
`CommandMetadata::check()` is the command-metadata source of truth. The environment builder
composes it into a stored, printable issue report; it emits non-empty reports, refuses error
reports, and therefore protects metadata deserialized from JSON or YAML.

## Crate Placement

`liquers-core` only: metadata owns the invariant and the core environment builder owns startup.
No dependency or public cross-crate registration surface is needed.

## Documentation Intent

**Reference:** Extend `specs/reference/COMMAND_DECLARATION.md` with the invariant and builder-time
validation behavior, so metadata producers know what is valid.
**Guide:** Extend `specs/guides/COMMAND_REGISTRATION_GUIDE.md` with the practical constraint for
hand-built or imported metadata; no new guide is warranted.
**Other documents to create:** None; this is a safety rule for an existing model.
**Specific documents to update:** `specs/README.md` (design index), the linked issue (resolution),
and generated registry documentation only if validation-driven tests alter it.

Command authors and metadata importers should understand that `multiple` consumes the tail and a
successful environment build is the safety check for serialized metadata.

## Open Questions

1. Should validation run in shared `Environment::try_to_ref` as well as through
   `EnvironmentBuilder::build`, so manually assembled environments get the same protection?
2. How should all error-level `CommandRegistryIssue`s be represented in returned core `Error`
   while retaining each command and argument diagnostic?

## References

- `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md`
- `specs/design/variadic-arguments-declaration/` (the existing macro-only guard)
- `liquers-core/src/command_metadata.rs`, `liquers-core/src/environment_builder.rs`
