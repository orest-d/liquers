# COMBINED-EXPIRES

Status: Draft

## Summary
Add combinable expiration expressions for `Expires` using `|` (logical "earliest expiry wins").

This feature fixes plan expiration analysis by allowing a plan to carry a composed expiration derived from all dependency expiration constraints.

Core addition:
1. `Expires::Combination(BTreeSet<Expires>)`
2. Normalized combine operation `x | y -> Expires`
3. `Plan.expires` computed as the combination of expirations from all dependencies

## Motivation
Current expiration analysis assumes one expiration shape at a time. In plan analysis this loses information when multiple dependencies have different expiration rules.

We need a stable symbolic form that:
1. Preserves all relevant expiration constraints.
2. Collapses obvious/safe simplifications.
3. Evaluates to the earliest runtime expiration (`min ExpirationTime`).

## Semantics
`X | Y` means: asset expires at the earlier of `X` and `Y`.

Equivalent runtime form:
`to_expiration_time(X | Y) = min(to_expiration_time(X), to_expiration_time(Y))`

Algebraic properties:
1. Idempotent: `X | X == X`
2. Symmetric (commutative): `X | Y == Y | X`

## Required Rules
The following must hold:
1. `X | Immediately == Immediately`
2. `X | Never == X`
3. `InDuration(a) | InDuration(b) == InDuration(min(a,b))`
4. `X | X == X`
5. `EndOfDay | EndOfWeek == EndOfDay`
6. `EndOfDay | EndOfMonth == EndOfDay`
7. `AtDateTime(a) | AtDateTime(b) == AtDateTime(min(a,b))`

## Canonise Operation
Define a canonicalization operation:

`canonise: Expires -> Expires`

Requirements:
1. Flatten nested combinations recursively.
2. Canonise members recursively before combining.
3. Apply absorbing/identity/idempotent simplifications.
4. Return `Never` for empty combination.
5. Return the element for singleton combination.
6. Return `Combination(BTreeSet<...>)` otherwise.

`canonise` must be idempotent:
`canonise(canonise(X)) == canonise(X)`.

## Lifted Combination Rules
Use `Combination` as the symbolic container (notation in equations may use `Combined(...)` for readability, but it maps to `Combination(...)`).

1. `X | Combination({Y1, Y2, ..., Yn}) == canonise(Combination({X|Y1, X|Y2, ..., X|Yn}))`
2. `Combination({X1, X2, ..., Xm}) | Y == canonise(Combination({X1|Y, X2|Y, ..., Xm|Y}))`
3. `Combination(A) | Combination(B) == canonise(Combination({a|b | a in A, b in B}))`

Implementation note:
1. For performance, implementations may use incremental/folded evaluation if result is equivalent to the pairwise definition above.

## Additional Meaningful Simplifications
These are safe and should also be applied:
1. Combination flattening: `Combination({... , Combination(T), ...})` flattens to one level before other reductions.
2. `Combination({}) == Never`
3. `Combination({X}) == X`
4. `EndOfDay(tz1) | EndOfDay(tz2)` is reducible to one `EndOfDay` only when `tz1 == tz2`; otherwise keep both in `Combination`.
5. `EndOfWeek(tz1) | EndOfWeek(tz2)` reducible only when `tz1 == tz2`.
6. `EndOfMonth(tz1) | EndOfMonth(tz2)` reducible only when `tz1 == tz2`.

Not safely reducible in general (keep as `Combination`):
1. `AtTimeOfDay` with `AtTimeOfDay` (result depends on reference time).
2. `AtTimeOfDay` with `EndOf*`/`OnDayOfWeek`.
3. `OnDayOfWeek` with `OnDayOfWeek`.
4. `InDuration` with calendar-based expirations.
5. `AtDateTime` with relative/calendar expirations.
6. `EndOfWeek` with `EndOfMonth`.

## Data Model
Extend `Expires`:
```rust
pub enum Expires {
    Never,
    Immediately,
    InDuration(std::time::Duration),
    AtTimeOfDay { ... },
    OnDayOfWeek { ... },
    EndOfDay { ... },
    EndOfWeek { ... },
    EndOfMonth { ... },
    AtDateTime(DateTime<Utc>),
    Combination(BTreeSet<Expires>),
}
```

Implementation note:
1. `BTreeSet` requires total ordering; define `Ord`/`PartialOrd` for `Expires` deterministically.
2. Ordering should be structural and stable (recommended: variant rank + field tuple).

## Combine API
Add:
1. `impl std::ops::BitOr for Expires`
2. `impl std::ops::BitOrAssign for Expires`
3. `impl Expires { fn combine(self, other: Expires) -> Expires }`
4. `impl Expires { fn canonise(self) -> Expires }`

`combine` must always return normalized output:
1. Evaluate via lifted rules (including `Combination | Combination` pairwise semantics).
2. Call `canonise` on the result.

## Serialization Format
`Expires` remains string-serialized.

For non-combination variants: unchanged (`Display` format).

For combinations:
1. Serialize as comma-separated list of individual expiration strings.
2. Example: `"in 5 min,end of day,2026-03-01T12:00:00+00:00"`
3. Deterministic order must follow `BTreeSet` order.

Deserialization:
1. If string contains comma, split by comma and parse each token as a single `Expires`.
2. Reduce parsed set via normalization (`Combination(empty)->Never`, singleton collapse).

Constraint:
1. Individual expiration string representations must not contain comma.
2. Parser must reject (or sanitize with explicit error) any new atomic syntax that would introduce commas.

## ExpirationTime for Combination
`Expires::to_expiration_time` for `Combination`:
1. Compute `to_expiration_time` for each member.
2. Return `min(...)`.
3. Empty combination must behave as `Never`.

## Plan Integration
Plan expiration analysis must aggregate all dependency expirations:
1. Initialize plan expiration as `Never`.
2. For each dependency expiration `d`, do `plan_expires = plan_expires | d`.
3. Store normalized result in `Plan.expires`.

This preserves all constraints and ensures plan expiration matches earliest dependency-triggered expiry.

## Compatibility
1. Existing atomic expiration strings remain valid.
2. New comma-separated form is backward compatible for readers that upgrade parser.
3. Legacy plans without combinations still evaluate identically.

## Acceptance Criteria
1. All required algebraic rules pass unit tests.
2. `canonise` flattening/normalization/idempotence rules pass unit tests.
3. String serde round-trip works for mixed combinations.
4. `to_expiration_time` for combinations returns earliest member expiration.
5. Plan expiration aggregation uses OR-combination across all dependencies.
6. No atomic expiration format includes commas.
7. `X | Combination(...)` and `Combination | Combination` satisfy the lifted rules.

## Suggested Tests
1. Rule tests for each required identity/reduction.
2. Lifted rule tests: `X | Combination(...)` and `Combination | Combination`.
3. Cross-variant non-reduction tests (must produce `Combination`).
4. Empty/singleton combination normalization tests.
5. `canonise` idempotence tests.
6. Serialization determinism test (`BTreeSet` stable order).
7. Deserialization error test for malformed comma lists.
8. Plan-level test: multiple dependencies with different `Expires` produce combined normalized plan expiry.
