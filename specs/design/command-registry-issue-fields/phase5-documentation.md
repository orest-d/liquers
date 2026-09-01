# Phase 5: Documentation - Command registry issue identity fields

## Implementation Summary

`CommandRegistryIssue::warning` and `::error` now forward realm, namespace, and command name in
their declared order. Unit tests protect both severity helpers and the reserved-name diagnostic
produced by `CommandMetadata::check()`.

## Documentation Review

No reference or guide describes this internal constructor mapping, so no current-state document
needed maintenance. The canonical issue records the verified resolution, and this design records
the implementation outcome.

## Validation and Remaining Work

Focused `liquers-core` unit tests pass. The correction does not change serialized fields, public
signatures, registry formats, or command execution. No scoped work or documentation proposal
remains.
