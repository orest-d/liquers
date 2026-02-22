# Issues and Open Problems

## Open

### Issue: ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS
Status: Open
Priority: High

#### Problem
Asset execution currently assumes that only `Context` sends service messages (`LogMessage`, `UpdatePrimaryProgress`, `UpdateSecondaryProgress`, etc.) and that no new messages are sent after plan execution completes.

This assumption needs thorough verification and explicit handling. In future, additional producers may appear (for example websocket/user-originated messages), which can introduce late or concurrent messages after execution finalization.

#### Risks
1. Late progress/control messages may mutate metadata after execution is finished.
2. Message-order races may cause inconsistent status/progress transitions.
3. Additional producers can break current single-producer assumptions and reintroduce deadlocks or blocked completion paths.

#### Scope of investigation
1. Audit all `AssetServiceMessage` producers and sender ownership/lifetime.
2. Verify end-of-execution guarantees for `Context` and plan evaluation.
3. Define and enforce post-finish message policy (ignore/reject/log/error) per message kind.
4. Define behavior for future external producers (e.g. websocket messages), including authorization and allowed message subset.
5. Add tests covering:
   1. late message arrival after `JobFinishing`/`JobFinished`,
   2. concurrent producers,
   3. cancellation + error + completion race ordering.

#### Expected outcome
A formalized message lifecycle contract for assets, with implementation and tests ensuring correctness under current and future multi-source message scenarios.
