# Phase 2: Solution and Architecture

## Changes in `liquers-lib/src/value/simple.rs`

- `as_bytes(&self, format)`:
  - `"txt" | "html" | "rs" | "py" | "css" | "js"`: add `Bytes` (raw), `Query` (`encode()`),
    `Key` (`encode()`) to the existing scalar arms.
  - New `"bytes" | "b" | "bin"` arm: `Bytes` → raw, `Text` → UTF-8 bytes, others →
    serialization error.
- `deserialize_from_bytes`: mirror core's `"txt"…` and `"bytes"…` reading arms for the identifiers
  `Text`, `Bytes`, `Query`, `Key` (read core's implementation of that function first and copy its
  decisions, including how `Query`/`Key` text is parsed back).
- Test: remove `UNWRITABLE` and the branch that tolerated it.

## Alternatives

Narrower `TypeInfo` for `SimpleValue` (rejected; see Phase 1).

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` (P2, L) | Shared format vocabulary would prevent this drift class | No |

## Relevant commands

None.

## Documentation architecture

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-lib/src/value/simple.rs` |
| Existing tests | The round-trip test changes from "recorded as unwritable" to strict |
| Compatibility | Writes that failed now succeed. No previously successful write changes bytes. |
| Recovery | Revert |
| Certainty | High |
