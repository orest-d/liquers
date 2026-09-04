---
id: INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS
kind: issue
title: An inline run dropped mid-flight leaves callers already waiting on it parked forever
status: draft
priority: P2
complexity: M
area: [core/assets, web]
created: 2026-09-04
github:
---

## Problem

`run_with_future_inline` gives the run to whoever wins `try_claim_for_run_inline`; a caller that
does not win **waits**, via `wait_to_finish()`. That wait is released by exactly one notification:

```rust
match notification {
    AssetNotificationMessage::JobFinished => return Ok(()),
    _ => {}
}
```

If the winner's future is then dropped — cancellation, a panic unwinding past the claim — the
`InlineRunClaim` `Drop` repair returns the asset to `Status::Recipe` so it can be claimed again.
That repairs the *asset*, not the *waiter*: nobody is running the asset any more, so `JobFinished`
will never be sent, and the parked caller waits indefinitely.

The queued path does not have this shape. `RunClaim`'s repair re-submits to the `JobQueue`, so a
worker picks the asset up and eventually emits `JobFinished`; liveness comes from the
re-submission, which is precisely what the queue-less path cannot do.

## Impact

Confined to managers with no job queue — the immediate/inline managers, i.e. `wasm32` and
`liquers-web` — and to the case where a second caller is *already parked* at the moment the runner
is dropped. A caller arriving after the repair is fine: it sees `Recipe`, claims, and runs.

Note that this is a trade rather than a regression. Before `InlineRunClaim`
(`INLINE-PATH-LACKS-EXECUTE-ONCE`) the second caller did not wait at all — it ran the body itself,
which is the double-run the claim exists to prevent. The claim converts a wrong result into a
possible hang, which is the better failure but still a failure.

## Expected behaviour

The waiter should re-examine the asset when a repair hands it back, rather than waiting only for
`JobFinished`. The shape that fits the existing code is a retry loop around claim acquisition:

```rust
let claim = loop {
    match self.try_claim_for_run_inline().await? {
        Some(c) => break c,
        None => {
            if self.status().await.is_finished() { return Ok(()); }
            // wait for one notification, then re-examine rather than assuming JobFinished
        }
    }
};
```

The evaluation future is untouched in the losing branch, so it can still be run after the loop —
no restructuring of the eval/psm join is needed. What needs care is the exit condition: each
iteration must either claim, observe a finished asset, or block on a genuine change, or the loop
becomes a spin.

`Drop` already sends `StatusChanged(Recipe)` on repair, which is the notification such a loop
would wake on; nothing consumes it today.

## Discovery

Found on 2026-09-04 while fixing PR #61 review comment 2 (the `Dependencies` half of the same
`Drop` repair). Recorded rather than fixed there: the review asked for the status repair, and the
waiter loop is a distinct change to the wait protocol with its own livelock risk, which deserves
its own tests rather than riding along on a review round.
