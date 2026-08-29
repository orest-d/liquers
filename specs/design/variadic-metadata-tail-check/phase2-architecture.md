# Phase 2: Solution and Architecture - Hand-Built Variadic Metadata Tail Validation

## Overview

Add the starved-argument rule to `CommandMetadata::check()` and make command registration reject
metadata whose check produces errors. The rule scans declared `arguments` in order, remembers a
previous non-injected `multiple` argument, and reports each later non-injected argument as
unreachable.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED` | draft | P3 | `CommandRegistryIssue::error` currently transposes namespace/name. This design should either fix that first in the same small command-metadata area or avoid relying on the broken constructor for new tests. | yes |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | draft | P3 | Future composite variadic support does not change the tail-position rule. | no |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | draft | P3 | UI editing is separate and should only consume valid metadata. | no |

## Files and Symbols

Primary files: `liquers-core/src/command_metadata.rs` for `CommandMetadata::check`,
`ArgumentInfo::injected`/`multiple` inspection, and `CommandRegistryIssue`; `liquers-core/src/commands.rs`
for `CommandRegistry::register_command` and `register_async_command` if validation becomes
registration-enforced. Integration check: `liquers-py/src/command_metadata.rs::add_python_command`
currently marks only the last argument `multiple`.

## Data, Ownership, Serialization and Errors

No serialized metadata schema changes. Validation returns existing `CommandRegistryIssue` values;
registration enforcement converts any error issue to `Error::parameter_error` or another existing
typed constructor, preserving Liquers' single error type.

## Sync, Async and API Effects

Validation is synchronous. Public registration methods already return `Result`, so they can reject
invalid metadata without signature changes. Async command registration must run the same metadata
check as sync registration.

## Alternatives

Rejected: rely only on macro validation; Python and any future language integration build metadata
directly. Rejected: change `ParameterValue::pop_value` to leave room for later parameters; that
would make variadic arity context-sensitive and break the established "multiple consumes rest"
model.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 2 core source/test files likely (`command_metadata.rs`, `commands.rs`), possible issue/design/index specs. |
| Impact area | Command metadata validation and hand-built registration. |
| Module/crate reach | Core implementation; Python integration affected only as a caller and already valid by construction. |
| Existing-test breakage | At most command-registration tests if they build invalid metadata; expected none. |
| New validation | Unit tests for check rule, injected-after-multiple allowance, and registration rejection. |
| Behavioural risk | Invalid hand-built commands now fail earlier; no persistence, concurrency or security concern. |
| Recovery | Revert check and registration enforcement. |
| Certainty | Medium because the broken `CommandRegistryIssue` constructor is a real prerequisite/blocker. |

## Rust Review

The scan can borrow `self.arguments` without cloning. Use explicit conditionals, no default enum
match, and typed `Error` construction when converting registry issues to registration failure.
