---
id: ERROR-WITH-KEY-SETS-QUERY-FIELD
kind: issue
title: Error context cannot distinguish asset keys from nested queries
status: draft
priority: P2
complexity: L
area: [core/error, core/query, core/assets, core/store, web, py, axum]
design: error-with-key-field
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

The encoded key is valid query text: converting a `Key` creates a headerless resource query and
preserves the key, while `Query::key()` can identify and extract a pure-key query. (`TryFrom<Query>
for Key` is not a purity test because it disregards a nontrivial resource header.) The defect is
therefore not that `query` contains an
unparseable value. It is that the field loses the value's **role**. A key naming the asset or recipe
being evaluated is not necessarily the query stored by that recipe, and neither is necessarily a
resource or link query that fails during evaluation.

The current flat payload has only one `query`, one `key`, one `position`, and one `command_key`.
That cannot represent a diagnostic such as: while evaluating recipe key `reports/daily.csv`, its
query `source/-/build` evaluated link argument `raw/data.csv/-/decode`, whose resource key
`raw/data.csv` failed in action `decode` at a position in the outer query. Repeated `with_query` and
`with_position` calls overwrite earlier context instead of preserving the evaluation path.

The narrow bug also exposes more missing-key cases. `Error::key_not_found` mentions the key only in
its message and leaves `Error.key` empty; there are 44 source-tree call sites relying on that
constructor. Keyed recipe and asset boundaries sometimes propagate `?` without attaching the
owning key, while lower store/resource errors may already contain a different key that must not be
overwritten. `dependency_version_mismatch` and `dependency_cycle` currently put one converted key
in both flat fields, which is syntactically valid but still does not describe its role.

## Impact

Callers cannot reliably identify the asset or recipe that failed, and nested evaluation can report
only one of several relevant queries/keys. `LogEntry::from_error` copies only the flat query and
position; Axum copies one query/key pair into both `ErrorDetail` and top-level `ApiResponse`;
`liquers-web` exposes one query and one key; Python's core error wrapper exposes neither. The
ambiguity therefore persists through metadata, HTTP, and language boundaries. It also overlaps
`ASSETS-IMPROVEMENTS`, whose persistence warnings require complete key/query/asset context.

A one-line `with_key` repair improves simple errors but does not define which key/query wins when
contexts differ. Changing that assignment alone risks establishing accidental precedence as the
public serialized contract.

## Expected behaviour

1. A key can be converted to a query and recognized as a pure key; conversion must not erase
   whether it was an asset key, recipe key, resource key, or evaluated query.
2. Every error produced for a keyed asset or keyed recipe carries that owning key if the same
   role/key context is not already present.
3. Nested evaluation preserves an ordered path of recipe, query, link/resource, action, and
   position contexts without overwriting the inner cause.
4. Existing flat fields have an explicit compatibility/projection rule, and metadata, web, and
   Python consumers have a migration contract.
5. Human-readable or link markup, if provided, is rendered from structured context and is not the
   only authoritative representation.

The structured representation and compatibility projection remain design decisions. Until they
are chosen, this issue is not a safe one-assignment implementation.

## Discovery

Noticed while boxing the error payload for `CORE-ERROR-PAYLOAD-SIZE` (2026-08-25). Scope expanded
after review on 2026-09-03 showed that recipe identity and nested query context cannot be expressed
by the existing flat fields.
