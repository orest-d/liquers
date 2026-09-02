---
id: WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND
kind: issue
title: A JsStore delegate has no way to signal absence, so a missing key reads as a read error
status: draft
priority: P2
complexity: S
area: [web, core/store]
design: store-conformance-suite
created: 2026-09-02
github:
---
## Problem

`JsStore`'s protocol (`liquers-web/src/store/js_store.rs`) has a delegate signal every failure by
throwing, and the adapter maps any thrown value to `ErrorType::KeyReadError`. There is no way for a
page object to say "this key is not here" as distinct from "the read failed".

`STORE_SEMANTICS.md` §4 makes that distinction load-bearing, and the conformance rules catch it:

```
FAILED absence01 [§4] get(...) on an absent key gave KeyReadError, not KeyNotFound
FAILED remove03  [§5] after remove(...), get_metadata gave KeyReadError rather than KeyNotFound
```

## Impact

Every caller of a page-defined store has to match on message text to tell absence from failure,
which is exactly what the typed `ErrorType` exists to avoid — and message text is not stable
(`CORE-ERROR-STORE-NAME-NOT-STRUCTURED`). Asset resolution treats `KeyNotFound` as "not cached yet"
and other errors as failures, so a page store makes a missing key look like a broken one.

## Expected behaviour

Give the protocol a way to say absence. Candidates, cheapest first:

1. **A sentinel return.** `get(key)` returning `null`/`undefined` means absent; throwing still means
   failure. Backwards-compatible: an existing delegate that throws keeps working.
2. **A recognised error shape.** A thrown object carrying `{ kind: "not-found" }`, or an error whose
   `name` is `"KeyNotFound"`.
3. **A `contains` pre-check** inside the adapter — rejected: it doubles every read and races.

Option 1 fits the protocol's existing style, where an absent optional method is meaningful.

## Discovery

Found on 2026-09-02 by `C10` of the conformance suite (Phase 4 step 12 of
`design/store-conformance-suite/`), against a stub delegate implementing the full protocol. Recorded
as an allowed failure on that suite, so `H5` will report it the moment it is fixed.
