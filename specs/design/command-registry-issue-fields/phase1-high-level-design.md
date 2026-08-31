# Phase 1: High-level design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The helpers' arguments are visibly transposed, and the correction has no API or data-format ambiguity.
- **Open questions:** None.

## Problem and outcome

`CommandRegistryIssue::warning` and `::error` pass `name` before `namespace` to `new`. Correct the forwarding order so validation reports identify the actual command, then pin both helpers with targeted unit tests.

Acceptance criteria: warning and error instances preserve all three identifier components and their severity; `CommandMetadata::check()` reports namespace/name correctly for a reserved command name.

## Scope and constraints

Only `liquers-core/src/command_metadata.rs` changes. This is a latent reporting bug; no registry format, query, or command execution behaviour changes. Do not rename constructors or introduce a new identifier type for this small correction.

## Design Dependencies

- `variadic-metadata-tail-check` - **required-by**: its planned registration validation can rely on correctly attributed issues.

## Documentation assessment

No reference or guide documents describe this internal diagnostic field mapping. The issue and generated index are the only required documentation records.

## Consolidated Findings

The same-typed `&str` parameters allowed the bug; tests must use distinct realm, namespace, and name values rather than exercise only `CommandMetadata::check()` with default `root` namespace.
