# Phase 4: Implementation Plan

1. Run T3 with the awk snippet alone and compare it with `rg 'js_name *=' liquers-web/src`.
   Proof: the expected list.
2. Patch `check-stubs.sh` (Phase 2). Proof: T1 (needs the wasm build: `cargo clean`, then
   `./liquers-web/examples-web/quickstart/build.sh`). Agent: haiku tier.
3. T2 by hand, then revert.
4. Issue resolution, index. Diff review.
