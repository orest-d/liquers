# Phase 2: Solution and architecture

Change the two `CommandRegistryIssue::new` calls at `liquers-core/src/command_metadata.rs:37` and `:40` to pass `(realm, namespace, name, ...)`. Keep signatures and owned `String` fields unchanged. Add inline unit tests in the existing module because constructors and fields are local to that file.

| Risk | Affected workflow | Validation and containment | Certainty |
|---|---|---|---|
| One helper remains swapped | registry validation diagnostics | separate warning and error tests with distinct values | High |
| Test only checks severity | field attribution | assert realm, namespace, name, and `is_error` | High |
| API drift | downstream users | no signature or serialization change | High |

The fix borrows the existing `&str` inputs and lets `new` own its strings, so it adds no clone or lifetime path. No alternatives merit additional abstraction: a typed command key would change the public constructor surface for a two-call-site defect.
