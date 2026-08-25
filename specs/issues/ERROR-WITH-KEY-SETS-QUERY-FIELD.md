---
id: ERROR-WITH-KEY-SETS-QUERY-FIELD
kind: issue
title: '`Error::with_key` writes the key into the `query` field'
status: draft
priority: P2
complexity: S
area: [core/error]
design: 
created: 2026-08-25
github:
---
## Problem

`Error::with_key` sets `self.query`, not `self.key`:

```rust
pub fn with_key(mut self, key: &crate::query::Key) -> Self {
    self.query = Some(key.encode());   // <- should be self.key
    self
}
```

`Error::with_query` sets the same field, so the two builders are indistinguishable in their
effect, and `with_key` leaves `Error::key` `None` no matter what it is given.

The dedicated constructors are inconsistent with it rather than with each other:
`key_not_supported`, `key_not_absolute`, `key_read_error` and `key_write_error` all populate
`key` correctly, while `dependency_version_mismatch` and `dependency_cycle` populate *both*
`query` and `key` with the same encoded key.

## Impact

Any caller that enriches an error with `with_key` and later reads `error.key` gets `None`, and
anything reading `error.query` gets a store key where a query is expected. `liquers-web` exposes
both as separate accessors (`LiquersError::query()` / `.key()`), so the confusion reaches the
JavaScript API.

Nothing in the workspace appears to depend on the current behaviour, but confirm that before
changing it — the fix moves data between two serialized fields of `Metadata::error_data`.

## Expected behaviour

`with_key` sets `key`. Decide deliberately whether `dependency_version_mismatch` and
`dependency_cycle` should keep mirroring the key into `query`, and document whichever rule wins.

## Discovery

Noticed while boxing the error payload for `CORE-ERROR-PAYLOAD-SIZE` (2026-08-25). That change
preserved the behaviour exactly rather than widening its scope.
