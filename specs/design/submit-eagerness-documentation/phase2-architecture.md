# Phase 2: Solution and Architecture

## Changes

1. `liquers-core/src/context.rs`, `Context::submit` doc: replace "the queued manager starts the
   job at once" with "the queued manager starts the job at once when it has capacity, and
   otherwise queues it on this asset's local queue, where it starts at the first
   `wait_for_dependency` or `evaluate`". Keep the issue reference.
2. `specs/reference/DEPENDENCIES_STATUS.md` §2: "`submit` is not lazy: the dependency has started,
   and on the inline manager finished, before it returns" becomes "…has started (or, on a
   saturated queued manager, is queued locally to start at the first wait), and on the inline
   manager finished, before it returns".
3. Test `submit_queues_locally_when_queued_manager_is_saturated` in `context.rs` `mod tests`.

## Known-issue preflight

None.

## Relevant commands

Test-only commands (see Phase 3).

## Documentation architecture

As above. `DEPENDENCIES_STATUS.md` gets a History row and a `reviewed:` bump.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `context.rs` (doc + test), `DEPENDENCIES_STATUS.md` |
| Behaviour | None |
| Test risk | The saturated-queue test must not deadlock. The parent waits for the dependency, and draining the local queue runs it inline. This is the documented progress guarantee (`drain_dependencies`). |
| Certainty | High |
