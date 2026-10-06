# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): an expired value "can get the expiry reason (or other helpful
  diagnostics) written into the log, but it should be made clear that this happened after the
  expiration. Hence, in the moment of expiration there should be a warning message logged: 'Asset
  expired'." The design applies that to the one route that bypasses `record_expiry`.
- **Open questions:** None. Interpretation, stated for review: the structured `expiry_reason`
  field stays empty for a value supplied already expired, because its cause is genuinely unknown
  to Liquers. The log carries the diagnostics instead. Inventing `Direct{Explicit}` would assert a
  cause nobody observed.

## Problem

`AssetManager::set_binary` and `set_state` (both managers, `liquers-core/src/assets.rs`) keep a
supplied `Status::Expired` and write it without going through `AssetManager::record_expiry`. The
stored record has `status: Expired`, no `expiry_reason`, and nothing in its log says it expired.
Every other route into `Expired` logs "<subject> expired: <reason>" through `record_expiry`.

## Expected behaviour and acceptance

When a supplied value's status is `Expired`:

1. The written metadata's log gains, at write time, a **warning** `"Asset expired"` (the moment
   Liquers learns of the expiry, which for this route is the write).
2. It then gains an **info** entry stating that the diagnostics are recorded after the
   expiration: `"Expiry recorded after the fact: <key> was written already expired (set_state);
   its original cause is unknown"` (`set_binary` for that route). If the supplied metadata carries
   an `expiry_reason`, that reason is kept, and the info entry names it instead of "unknown".
3. Existing log entries of the supplied metadata are kept, and the new entries are appended.
4. `expiry_reason()` is unchanged from what was supplied (often `None`).
5. Non-`Expired` inputs are unaffected.

## Scope

The four write sites. No cascade beyond what `publish_version` already does for the written version.

## Design Dependencies

- `immediate-set-state-status-match` — **requires** (soft). Both edit the same four sites. Recommended
  as one PR (merge candidate M2).

## Documentation assessment

- Reference: `specs/reference/DEPENDENCIES_STATUS.md` (where expiry reasons are described):
  "a value written already `Expired` has no recorded cause; its log says so".

## Consolidated Findings

- Liquers-legacy metadata (`Metadata::LegacyMetadata`) is left untouched, as `record_expiry` does.
- The wording "Asset expired" is the decided warning text. The subject (key) goes in the info
  entry, so the warning stays the exact decided phrase.
