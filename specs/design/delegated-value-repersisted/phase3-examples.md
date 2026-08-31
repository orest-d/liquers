# Phase 3: Examples and tests

Extend the `keyed_delegation_default` and `keyed_delegation_immediate` arrangements in `liquers-core/tests/manager_parametric.rs` with a small `AsyncStore` wrapper that delegates storage and increments an atomic or mutex-protected counter only for value `set` calls. Evaluate the delegating query and assert one producer invocation, the expected value, and exactly one write for the owner key.

Add an ordinary keyed/nondelegated evaluation case proving the counter is still one for a single computation, and rely on existing persistence tests for direct `set_state`. The default and immediate managers cover async and immediate workflows. Run `cargo test -p liquers-core keyed_delegation` and `cargo test -p liquers-core --lib --tests`.
