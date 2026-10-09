# Phase 3: Examples and Tests

| Test | Checks | Scenarios |
|---|---|---|
| `msrv-declared` (`cargo metadata`) | every workspace member reports `rust_version == "1.95"` | AC-1 |
| `msrv-is-floor` (`cargo metadata`) | no non-workspace package's `rust_version` exceeds it | AC-2 |
| `msrv-builds` (`cargo +1.95 check -p liquers-lib --lib`, if installable) | workspace code builds on the minimum | AC-2 |
| `default-loop` (`cargo test -p liquers-lib --lib --tests`) | the cloud toolchain is unaffected | AC-3 |

These are command checks, not Rust tests: the contract is build metadata, which no test binary can
observe.
