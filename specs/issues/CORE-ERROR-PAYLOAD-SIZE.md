---
id: CORE-ERROR-PAYLOAD-SIZE
kind: issue
title: `Error` is large enough to bloat every `Result`
status: closed
priority: P2
complexity: S
area: [core/error]
design: core-error-payload-size
created: 2026-08-08
github:
---
## Problem

`liquers_core::error::Error` carries its fields inline — message, position, query, key, command
key — so every `Result<T, Error>` in the workspace is at least that wide. The archived review
counted 421 clippy warnings on this.

## Impact

A pervasive, cheap-to-fix cost: every fallible call moves more bytes than it needs to, in a
codebase where almost every function is fallible.

## Expected behaviour

Box the payload — `Error(Box<ErrorInner>)` — keeping the public API unchanged. Re-run clippy to
confirm the count before and after; the number is the acceptance criterion.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-9. Verified against HEAD: not re-measured during triage — confirm the clippy count before scheduling. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Resolution

Closed 2026-08-25. `Error` is now a newtype over a boxed payload:

```rust
pub struct ErrorPayload { /* the fields Error used to carry, unchanged */ }

#[derive(Serialize, Deserialize, ...)]
#[serde(transparent)]
pub struct Error(Box<ErrorPayload>);
```

The whole change is in `liquers-core/src/error.rs`. **No call site anywhere in the workspace
changed**: `Error` implements `Deref`/`DerefMut` to its payload, so `err.message` and
`err.position = pos` compile exactly as before, and `#[serde(transparent)]` keeps the serialized
form a flat object with the same keys — which `Metadata::error_data` persists and therefore could
not change.

`ErrorPayload` and `From<ErrorPayload> for Error` / `Error::into_payload` are additions to the
public surface; nothing was removed or altered. The payload type is named `ErrorPayload` rather
than the `ErrorInner` this issue proposed, because boxing makes it a public type and `Inner` reads
poorly as a public name.

### Acceptance criterion — the clippy count

Measured with `CARGO_INCREMENTAL=0 cargo clippy --lib --tests` over the default members, the same
command before and after:

| | before | after |
|---|---|---|
| `result_large_err` warnings | **715** | **0** |
| all clippy warnings | 1032 | 316 |
| `Err`-variant size | 176 bytes | 8 bytes (one pointer) |

176 bytes was over clippy's 128-byte threshold; the boxed error is one pointer. Three
`large_enum_variant` warnings on enums that *hold* an `Error` improved as a side effect
(`metadata.rs` 704 → 544 bytes, `ui/element.rs` 896 → 576 bytes) and one in `plan.rs` cleared
entirely. No new warning of any kind appeared.

### Validation

Five tests added in `liquers-core/src/error.rs` guard the result: `Error` is pointer-sized and
`Result<(), Error>` stays under the lint threshold; the serialized form is asserted against a
literal flat JSON object and round-trips; `command_key` remains `#[serde(skip)]`; payload fields
are still read *and assigned* directly; `into_payload` round-trips.

Green: all suites of `liquers-core` (609 lib + 18 integration), `liquers-lib`, `liquers-store`,
`liquers-macro`, `liquers-axum`, `liquers-py`, and the `liquers-web` wasm conformance loop except
one pre-existing unrelated failure filed as `WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH` (verified
to reproduce with this change stashed).

`Error::with_key` was found to write into the `query` field; the behaviour was preserved rather
than fixed here, and is filed as `ERROR-WITH-KEY-SETS-QUERY-FIELD`.
