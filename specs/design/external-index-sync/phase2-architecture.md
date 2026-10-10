# Phase 2: Solution & Architecture — external-index-sync

## Overview
<The chosen solution in 2-4 sentences. Rejected alternatives, one line each with the reason.>

## Known-Issue Preflight
| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
<Open issues linked to this design or touching its areas and integration points; `None found` if
none. A blocker is resolved first or designed around, and is at least P1.>

## Interfaces
<New and changed items only, as Rust signatures without bodies: structs and enums (with ownership:
`Arc` / `Box` / owned, and serde), trait impls and bounds, public functions, ExtValue variants,
commands (`register_command!` DSL line). For each: sync or async, and why. Write `None` for a
category that does not apply.>

## Integration Points
<Files and modules to create or modify, per crate, with the call sites affected.>

## Error Handling
<Failure cases and the `Error` constructor each uses (no `Error::new`).>

## Relevant Commands
<New commands with signatures; existing namespaces this interacts with (e.g. `pl`, `lui`).>

## Documentation Architecture
<For each document to create or update: exact path, kind, audience, area, and the change. The
proposed `affects_docs` set. Links to add in `specs/README.md`.>

## Risks
| Assessment | Finding |
|---|---|
| Files likely to change | |
| Crates and workflows affected | |
| Existing tests likely to change | |
| New validation | |
| Compatibility / data / concurrency / performance / security | |
| Recovery | |
| Certainty and open questions | |
