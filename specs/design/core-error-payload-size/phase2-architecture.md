# Phase 2: Solution and Architecture

`ErrorPayload` owns the prior public fields and derives serde/debug/clone/equality. `Error` is a
transparent newtype over `Box<ErrorPayload>`, implements `Deref` and `DerefMut`, and keeps typed
constructors. `From<ErrorPayload>` and `into_payload` expose controlled ownership conversion.

Reject boxing individual strings because the outer result remains wide, and reject changing every
call site because direct field compatibility is available. Library errors remain typed; no
`anyhow`, panic, async, or trait-object change is introduced.

| Risk | Control |
|---|---|
| Files/crates | One core source file; workspace callers compile unchanged. |
| Existing tests | Error equality/serde snapshots must retain flat shape. |
| Validation | Pointer size, Result size, JSON literal/round-trip, clippy counts, crate suites. |
| Compatibility/data | Transparent serde and deref preserve contracts; public payload is additive. |
| Performance | One allocation only on error paths; success Result size shrinks. |
| Recovery | Revert commit `42695a0` as one representation change. |
| Certainty | High; implementation and measurements are recorded. |
