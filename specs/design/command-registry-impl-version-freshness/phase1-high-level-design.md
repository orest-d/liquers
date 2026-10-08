# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — test-only change in `liquers-lib/tests/registry_export.rs`
  (plus guide text); no `pub` item, command or registry format changes.
- **Leading issue:** None
- **Explanation:** Decided (maintainer, 2026-10-08, backlog compaction D6): compare `impl_version`
  exactly in a separate test, and refuse `version: now` in exported groups, as proposed below.
- **Open questions:**
  1. **Resolved — compare exactly, in a separate test** (maintainer, 2026-10-08). Keep `committed_registry_is_fresh`
     (signatures) as is, and add `committed_registry_impl_versions_are_fresh`, which compares
     `impl_version` per command and names each stale command plus the regenerate command. Two
     tests give two clear messages: "signature changed" and "implementation changed".
  2. **Resolved — `version: now` is not allowed in exported command groups** (maintainer, 2026-10-08). The new
     test fails with a dedicated message if a committed `impl_version` differs on two consecutive
     exports in the same process, which is the signature of `now`.

## Problem

`liquers-lib/tests/registry_export.rs` compares commands by `signature_of`, which zeroes
`impl_version`. `specs/command_registry.yaml` can therefore carry stale implementation versions
undetected. It did (two commands, found in `variadic-arguments-declaration` Step 6), and a later
regeneration fixed them, but nothing prevents a recurrence. `impl_version` is semantic:
`dep/command_implementation` returns it, and stored assets are invalidated when it changes.

## Expected behaviour and acceptance

1. Editing the body of a `version: auto` command without regenerating makes the new test fail,
   naming `<realm>/<ns>/<name>` with committed vs current versions.
2. Regenerating makes it pass. A pure reformat of the YAML does not affect it (structural compare).
3. A command using `version: now` in an exported group makes the test fail with a message saying
   `now` cannot be committed.
4. Same feature gating as the existing test (all four optional groups).
5. CLAUDE.md "Maintaining `specs/command_registry.yaml`" mentions implementation edits.

## Scope

The export test, CLAUDE.md, and the registry guide text if any. No macro change.

## Design Dependencies

- `command-cache-flag` and `command-metadata-command-hints` — **overlap** (both regenerate the
  registry. No ordering constraint).
- `register-command-payload-docs` — **overlaps** (documents `version:`; its text should mention the
  freshness rule).

## Documentation assessment

`CLAUDE.md` (registry section: "regenerate also after editing a versioned command's body").
`specs/reference/REGISTER_COMMAND_FSD.md` `version:` row (via `register-command-payload-docs`).

## Consolidated Findings

- At HEAD the committed file is fresh, so the new test passes on introduction.
- Detecting `now` by `Version::kind` is unreliable, so detect it by instability (two exports in a
  row differ).
