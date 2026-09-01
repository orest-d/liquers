# Phase 2: Solution and Architecture - Hand-Built Variadic Metadata Tail Validation

## Overview

Add the starved-argument rule to `CommandMetadata::check()`: scan declared `arguments` in order,
remember a previous non-injected `multiple` argument, and report each later non-injected argument
as unreachable. Do not yet claim registration enforcement: `register_command` stores default
metadata and returns `&mut CommandMetadata`, after which macro and hand-built callers populate the
arguments. There is no finalization boundary at which rejection is both complete and timely.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED` | closed | P3 | Fixed at HEAD; constructor and regression tests preserve realm, namespace and name. It is no longer a blocker. | no |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | draft | P3 | Future composite variadic support does not change the tail-position rule. | no |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | draft | P3 | UI editing is separate and should only consume valid metadata. | no |

## Files and Symbols

Primary file for the rule: `liquers-core/src/command_metadata.rs` for `CommandMetadata::check`,
`ArgumentInfo::injected`/`multiple` inspection, and `CommandRegistryIssue`. The unresolved boundary
spans `liquers-core/src/commands.rs::CommandRegistry::{register_command,register_async_command}`
and `CommandMetadataRegistry::{add_command,get_mut}`. Integration check:
`liquers-py/src/command_metadata.rs::add_python_command`
currently marks only the last argument `multiple`.

## Data, Ownership, Serialization and Errors

No serialized metadata schema changes. Validation returns existing `CommandRegistryIssue` values;
registration enforcement converts any error issue to `Error::parameter_error` or another existing
typed constructor, preserving Liquers' single error type.

## Sync, Async and API Effects

Validation is synchronous, but the existing `Result` from registration occurs before metadata
mutation. Checking there cannot see the invalid tail; checking on every `get_mut` is too early;
checking only during planning/export changes the promised "rejected at registration" timing.

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
| Recovery | The rule itself is local; any future registration/finalization API needs its own compatibility rollback. |
| Certainty | High on the rule and current API limitation; no safe enforcement boundary is selected. |

## Rust Review

The scan can borrow `self.arguments` without cloning. Use explicit conditionals, no default enum
match, and typed `Error` construction when converting registry issues to registration failure.

## Continuation Blocker

The repository must choose error timing and API ownership. An explicit `finish_registration(key)`
is reliable but adds a public workflow step; validating during PlanBuilder catches every use but
allows invalid registries to exist and changes the issue's acceptance contract; accepting complete
metadata in registration is clean but breaks current macro and language-binding construction. Until
one is chosen, Phase 3 cannot state where the rejection is observed and Phase 4 cannot be executable.
