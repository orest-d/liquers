# Phase 2: Solution & Architecture - Memory-Store Prefix Support

## Overview

Change the existing `is_supported(&Key) -> bool` implementations for `AsyncMemoryStore` and
`MemoryStore` to require both key absoluteness and segment-wise membership in `self.prefix`.
Keep the traits, routing algorithm, storage representation, and fallible operation guards unchanged.

The support contract is cumulative for every store: a supported key is absolute, belongs to the
store's configured prefix, and passes any additional store-specific exclusions (for example,
reserved folders, unsupported file types, or metadata-sidecar collisions). This change supplies
the missing prefix term for the two memory stores; it does not repurpose `is_supported` as the
fallible enforcement mechanism for direct operations.

## Known-Issue Preflight

| Issue | Status | Priority | Solution impact | First? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX` | draft | P1 | Source issue; requires async correction and checking sync parity. | Yes | No | Implement both predicates and focused tests. |
| `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` | accepted | P1 | The missing shared suite allowed divergence; this S-sized fix must not absorb that L-sized work. | No | No | Add local regression evidence; monitor the future suite. |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | P3 | Explains why the infallible predicate retains `!key.is_relative()` while fallible methods use `Key::as_absolute()`. It is future type-system hardening, not a prerequisite. | No | No | Preserve current guards; no type redesign. |
| `MEMORYSTORE-MAKEDIR-SUCCEEDS-WITHOUT-CREATING-A-DIRECTORY` | draft | P0 | Same sync store, separate method and behavior. | No | No | Keep out of scope and fix separately. |

### Blocking and Priority Decision

No prerequisite or blocker exists. The P0 `makedir` issue is independent rather than prerequisite
work. Keep all current priorities: the P3 absolute-key issue describes future structural hardening,
while normal routers mask the source issue, justifying P1 rather than P0.

## Data Structures

No new or changed structs, enums, fields, ownership, or `ExtValue` variants. Both implementations
reuse their existing owned `prefix: Key` and borrow the input key.

## Trait Implementations

Modify two existing implementations without changing their traits or signatures:

```rust
impl AsyncStore for AsyncMemoryStore {
    fn is_supported(&self, key: &Key) -> bool;
}

impl Store for MemoryStore {
    fn is_supported(&self, key: &Key) -> bool;
}
```

## Generic Parameters & Bounds

None. No bounds or dispatch strategy changes.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `AsyncMemoryStore::is_supported` | No | `AsyncStore` intentionally keeps this pure selection predicate synchronous. |
| `MemoryStore::is_supported` | No | Existing synchronous trait method; only borrowed comparisons are required. |

## Function Signatures

Signatures remain `fn is_supported(&self, key: &Key) -> bool`. Both implementations use:

```rust
!key.is_relative() && key.has_key_prefix(&self.prefix)
```

Short-circuit order retains the universal absolute-key rule first. `has_key_prefix` supplies
segment-wise comparison, makes an empty prefix match every absolute key, and does not confuse
`data` with `database`. No helper is added: two explicit expressions match `AsyncFileStore`,
`FileStore`, and `AsyncOpenDALStore` directly without introducing an abstraction.

These two terms are the baseline, not the complete policy for every backend. Implementations may
append store-specific exclusions after them, such as rejecting reserved folders, unsupported file
types, or ambiguous metadata-sidecar names. Neither term may be omitted.

## Integration Points

- `liquers-core/src/store.rs`: change both predicates, replace obsolete deferral comments, and add
  direct sync and async tests beside existing store tests. Update the `Store::is_supported` and
  `AsyncStore::is_supported` rustdoc for implementors: document the cumulative absolute + prefix +
  optional-exclusions contract, distinguish support reporting from fallible absolute-key
  enforcement, and remove the references to nonexistent `with_overlay` / `with_fallback` helpers.
- No dependency, feature, router, binding, endpoint, serialization, or public API changes.
- `StoreRouter` and `AsyncStoreRouter` continue applying their independent prefix check before
  `is_supported`; their behavior and selection order are unchanged. No current non-router
  overlay/fallback implementation exists in the repository. Direct callers and future composition
  mechanisms nevertheless need each store's predicate to be truthful without relying on a router.
- Validate with `cargo test -p liquers-core --lib store::tests` and the focused Phase 3 test names.
  After tracked-document edits, regenerate generated documentation metadata with
  `python3 scripts/docs_index.py`, then require `python3 scripts/docs_index.py --check` to pass.

## Documentation Architecture

### Reference Plan

Extend `specs/reference/STORE_SEMANTICS.md` (reference, internal, `core/store`) in Phase 5. Replace
the section 6 warning with the verified cumulative rule, state that optional exclusions are the
reserved store-specific role of `is_supported`, and remove the implication that overlay/fallback
layering exists today. Add regression tests to its enforcement evidence, bump `reviewed:`, and add
a History row linking this design.

### Guide Plan

None. There is no new repeatable workflow. Reconsider only if implementation exposes guidance not
already owned by `STORE_SEMANTICS.md` and the absolute-key rustdoc.

### Other Documents to Create

Only the mandatory `phase5-documentation.md` summary. No new reference, guide, or ancillary document.

### Existing Documents to Review or Update

- `specs/issues/CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md`: retain its design link now;
  set `status: closed`, add resolution evidence, and repair its stale source locations in Phase 5.
  Prefer current symbol names and stable source links over bare line-number claims where practical;
  correct its implementation comparison/count, and replace its claim that layering constructs
  currently exist with the verified direct/future composition rationale.
- `specs/reference/STORE_SEMANTICS.md`: authoritative current-behavior update described above.
- `specs/README.md`: retain the temporary design capability link while work is live; after Phase 5,
  remove it as redundant with the existing Store behavioral semantics entry.
- `liquers-core/src/store.rs`: replace stale implementation comments with the code change and
  update the implementor-facing `Store` / `AsyncStore` method rustdoc described above.
- Discard `specs/reference/STORE_CONFIG_FSD.md`: it already specifies both router predicates and
  delegates behavior to `STORE_SEMANTICS.md`.
- Discard `specs/reference/PROJECT_OVERVIEW.md` and `specs/guides/LANGUAGE_INTEGRATION_GUIDE.md`:
  neither asserts this implementation-specific divergence.

Authoritative `affects_docs`: `specs/reference/STORE_SEMANTICS.md` and the source issue document.

### Design and Capability Links

The issue and temporary `specs/README.md` capability entry link this design. Phase 5 leaves the
design reachable through the issue and reference History row, while the capability map keeps the
higher-stage `STORE_SEMANTICS.md` entry rather than retaining a duplicate.

### Evidence to Collect During Implementation

Record direct predicate results for inside, outside, equal-to-prefix, empty-prefix, relative, and
segment-lookalike keys for both memory stores. Confirm existing router tests remain unchanged and
capture the exact test names in the reference and issue resolution. The direct predicate tests are
the contract evidence; do not claim coverage through a non-router layering implementation, because
none currently exists.

## Relevant Commands

### New Commands

None.

### Relevant Existing Namespaces

None. This is below command execution and introduces no query action. The user was asked at Phase 2
start to flag any command interaction; none is evident from the source or references.

## Web Endpoints (if applicable)

None. Existing API paths using a configured router already receive its independent prefix filter;
that router behavior is unchanged.

## Error Handling

No new error path or constructor. `is_supported` remains infallible and returns `false` for relative
or out-of-prefix keys, plus any backend-specific exclusions an implementation defines. Fallible
methods continue returning `ErrorType::KeyNotAbsolute` through `Key::as_absolute()` for relative
keys; `is_supported` reports capability and does not replace that enforcement. Out-of-prefix direct
operations remain outside this issue.

## Serialization Strategy

Not applicable; no serialized state changes.

## Concurrency Considerations

No change. Both predicates borrow immutable keys and touch no maps, locks, or async state.

## Compilation Validation

The expression uses existing `Key` methods available on native and wasm builds. No feature gates
change. Focused core tests are sufficient; the full build matrix is unnecessary because no
conditional code or dependency changes.

## References to liquers-patterns.md

- Placement remains in `liquers-core`, where both built-in memory stores live.
- Borrowed `&Key` inputs and a boolean return avoid allocation and preserve trait contracts.
- The async-native implementation remains primary; sync parity is corrected without a wrapper.
- No error, enum match, generic bound, lock, clone, allocation, or new default arm is introduced.
