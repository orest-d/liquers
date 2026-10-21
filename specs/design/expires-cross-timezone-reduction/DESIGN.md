---
id: EXPIRES-CROSS-TIMEZONE-REDUCTION
kind: design
title: Expiration combination reduces end-of-period rules only within one time zone
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [COMBINED-EXPIRES]
created: 2026-10-20
---
# Expiration combination reduces end-of-period rules only within one time zone

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

`X | Y` on `Expires` promises "expire at the earlier of X and Y". The feature `COMBINED-EXPIRES` is
implemented (`Expires::Combination`, `combine`, `canonise`, `|`, the comma text form, plan
aggregation), but one simplification breaks the promise: `EndOfDay | EndOfWeek` and
`EndOfDay | EndOfMonth` collapse to the `EndOfDay` operand whatever the operands' time zones, and
the end of day in one zone can be later than the end of week in another. The fix makes those two
reductions apply only when both operands have the same `tz_offset`; otherwise the pair stays a
`Combination`, which `to_expiration_time` already evaluates as the true minimum.

### Problem Example

At HEAD, with reference time Sunday 2026-10-18 23:30 UTC (`liquers-core/src/expiration.rs`):

```rust
let day  = Expires::EndOfDay  { tz_offset: Some(14 * 3600) }; // local: Monday 13:30
let week = Expires::EndOfWeek { tz_offset: Some(0) };          // local: Sunday 23:30
let combined = day.clone() | week.clone();
// HEAD:   combined == day                    -> expires Monday 10:00 UTC (end of Monday at +14)
// should: combined == Combination{day, week} -> expires Monday 00:00 UTC (end of the UTC week)
```

The asset outlives its end-of-week dependency by ten hours. `combine_atomic_pair` returns the
`EndOfDay` operand for `(EndOfDay{..}, EndOfWeek{..})` and `(EndOfDay{..}, EndOfMonth{..})` with
`..` ignoring `tz_offset`, and the existing test `test_combine_required_rules` pins exactly this
(`EndOfDay{None} | EndOfWeek{Some(3600)} == EndOfDay{None}`). `None` means "the system time zone",
which is not known when combining, so even `None` against `Some(_)` is not provably reducible.

Reachability: only through the Rust API. The text forms (`end of day`, `eow`, `eom`, a
command's `expires:`, a recipe's `expires:`) always parse to `tz_offset: None`, so two parsed rules
share a zone and reduce correctly. A Rust caller (a command setting `Expires` on its metadata, an
environment, liquers-lib code) that constructs zoned rules gets the wrong deadline.

### Scope and Acceptance Criteria

- **AC-1** Different zones are kept apart
  WHEN `EndOfDay{tz: a} | EndOfWeek{tz: b}` or `EndOfDay{tz: a} | EndOfMonth{tz: b}` is combined (or canonised as a set) with `a != b`, including `None` against `Some(_)`
  THEN the result is the two-member `Combination`, in either operand order
- **AC-2** Same zone still reduces
  WHEN both operands carry the same `tz_offset` (both `None`, or the same `Some(x)`)
  THEN the result is the `EndOfDay` operand, as the required rules 5-6 of `COMBINED-EXPIRES` state
- **AC-3** The combination keeps its promise
  WHEN any two rules from a sample over every variant and several offsets are combined, for reference times around a day, week and month boundary
  THEN `to_expiration_time(x | y)` equals `min(to_expiration_time(x), to_expiration_time(y))`
- **AC-4** Existing algebra unchanged
  WHEN the other rules (identity, absorption, idempotence, commutativity, lifted rules, text round-trip of parsed rules) are exercised
  THEN they behave as at HEAD

Non-goals: carrying a time zone in the text form (`EXPIRES-TEXT-FORM-DROPS-TIME-ZONE`); new
reductions (for example `EndOfWeek | EndOfMonth`); any change to `ExpirationTime`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in one private function of
  `liquers-core/src/expiration.rs` (`combine_atomic_pair`) restoring the documented semantics of
  `|`, plus test changes; no `pub` item, format or command changes (rules 1-6).
- **Leading issue:** None.
- **Explanation:** the reduction is an optimization; withholding it can only leave a
  `Combination`, which every consumer already evaluates as a minimum. No contract question remains.
- **Open questions:** None.

### Design Dependencies

- `EXPIRES-TEXT-FORM-DROPS-TIME-ZONE` (issue, filed with this design) — **overlaps**: the text form
  drops `tz_offset` for `OnDayOfWeek` and `EndOf*`, so a zoned combination serializes to a string
  that parses back zone-less and then reduces. Independent change (a syntax change, not
  eligible for automatic fixing), so kept separate under overlap exclusion E1. No order.
- `expiration-mechanism`, `expiration-safety` (complete) — background; not extended (E4).

### Consolidated Findings

- `COMBINED-EXPIRES` is otherwise done at HEAD: `Expires::Combination(BTreeSet<Expires>)`
  (`expiration.rs:52`), `canonise` (`:216`), `combine` (`:242`), `BitOr`/`BitOrAssign`
  (`:349`, `:357`), comma text form (`FromStr` `:455`, `Display` `:710`), `to_expiration_time` as a
  fold of `min` (`:187`), and plan aggregation with `|=` (`plan.rs:1410`, `:2918`, `:3023`,
  `:3513`; `recipes.rs:362`).
- The fix is a guard on two match arms; `reduce_combination` calls `combine_atomic_pair`, so
  `canonise` and the parser's combination path are fixed by the same change.
- One existing assertion encodes the bug and must change (`test_combine_required_rules`).
- No Liquers query is involved (nothing for `liquers-validate`).

## Phase 2: Architecture

### Solution

In `combine_atomic_pair` (`liquers-core/src/expiration.rs:274`), bind the offsets and guard the
two end-of-period arms:

```rust
(Expires::EndOfDay { tz_offset: a }, Expires::EndOfWeek { tz_offset: b })
| (Expires::EndOfDay { tz_offset: a }, Expires::EndOfMonth { tz_offset: b }) if a == b => left.clone(),
(Expires::EndOfWeek { tz_offset: a }, Expires::EndOfDay { tz_offset: b })
| (Expires::EndOfMonth { tz_offset: a }, Expires::EndOfDay { tz_offset: b }) if a == b => right.clone(),
```

An unguarded pair falls through to the existing `_ =>` arm, which builds the two-member
`Combination` (that arm matches a pair of `Expires`, not a Liquers status enum; it is the
"no reduction" case by definition and stays). `None == None` reduces: `to_expiration_time` passes
one `tz_offset_default` to every member, so two `None` rules share a zone at evaluation time.

Rejected: resolving `None` to the system offset while combining (makes the canonical form depend on
the host, and the stored plan would differ across machines); computing the reduction from a
reference time (the result must be reference-time independent).

Known-issue preflight: none blocking. `EXPIRES-TEXT-FORM-DROPS-TIME-ZONE` is independent.

### Changes

- `liquers-core/src/expiration.rs` — `combine_atomic_pair`: the guard above. No signature changes.
- Tests in the same file's `mod tests` (Phase 3).
- Documents: none describe the reduction rules (`grep -rn "end of day" specs/reference specs/guides`
  finds nothing); the issue file records the resolution.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/expiration.rs` only |
| Crates and workflows affected | `liquers-core`; plan and recipe expiration aggregation inherit the fix |
| Existing tests likely to change | `test_combine_required_rules` (its two cross-zone assertions become same-zone, plus new cross-zone ones) |
| New validation | three unit tests, including a brute-force minimum property |
| Compatibility / data | a zoned pair now serializes as `end of day,end of week` instead of `end of day`; parsed back it reduces (both `None`), so no reader breaks. Stored plans of parsed rules are unchanged |
| Concurrency / performance / security | none; one more comparison per pair |
| Recovery | revert the commit |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

```rust
let d0 = Expires::EndOfDay { tz_offset: None };
let w0 = Expires::EndOfWeek { tz_offset: None };
assert_eq!(d0.clone() | w0, d0.clone());                         // same zone: reduces

let w1 = Expires::EndOfWeek { tz_offset: Some(3600) };
assert!(matches!(d0 | w1, Expires::Combination(ref s) if s.len() == 2)); // kept apart
```

### Tests

All in `liquers-core/src/expiration.rs` `mod tests`:

- `test_combine_required_rules` (changed) — the `EndOfDay`/`EndOfWeek` and `EndOfDay`/`EndOfMonth`
  assertions use one zone (`None` and `Some(3600)` on both sides): AC-2, AC-4.
- `test_combine_end_of_period_across_time_zones_is_kept` (new) — both operand orders, for
  `EndOfWeek` and `EndOfMonth`, with `None`/`Some(3600)` and `Some(0)`/`Some(50400)`; also
  `Expires::Combination({..}).canonise()` of the same pairs: AC-1.
- `test_combine_is_the_earliest_expiration` (new) — rules: `Never`, `Immediately`, `InDuration` (60 s,
  1 h), `AtTimeOfDay` (`Some(0)`), `OnDayOfWeek`, `EndOfDay`, `EndOfWeek`, `EndOfMonth` with
  offsets `{None, Some(0), Some(-43200), Some(50400)}`, `AtDateTime`; reference times Sunday
  2026-10-18 23:30 UTC, Saturday 2026-10-31 23:30 UTC and Wednesday 2026-10-14 12:00 UTC;
  `tz_offset_default = 0`. For every ordered pair, `(x | y).to_expiration_time(t, 0)` equals
  `x.to_expiration_time(t, 0).min(y.to_expiration_time(t, 0))`. This test fails at HEAD on the
  Problem Example pair: AC-1, AC-3.
- Existing `test_combine_idempotent`, `test_combine_commutative`,
  `test_combine_lifted_rules_with_combination`, `test_roundtrip_combination`,
  `test_serde_expires_combination_json` stay unchanged and pass: AC-4.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib expiration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/expiration.rs` `mod tests` — add `test_combine_end_of_period_across_time_zones_is_kept` and `test_combine_is_the_earliest_expiration`; confirm they fail — `cargo test -p liquers-core --lib expiration`. Rollback: delete them.
- [ ] 2. `liquers-core/src/expiration.rs` `combine_atomic_pair` — the `if a == b` guards; change `test_combine_required_rules` to same-zone operands — `cargo test -p liquers-core --lib expiration` all green. Rollback: revert the commit. Depends on 1.
- [ ] 3. Plan and recipe users of `|` — `cargo test -p liquers-core --lib plan` and `cargo test -p liquers-core --test expiration_integration` (unchanged behaviour for parsed rules). Depends on 2.
- [ ] 4. Records — `specs/issues/COMBINED-EXPIRES.md` `status: closed` with a resolution naming the tests; `python3 scripts/docs_index.py` then `--check`. Depends on 3.

### Validation

`cargo fmt -p liquers-core -- --check`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
diff review: only `expiration.rs`, the issue file and the generated index change.
