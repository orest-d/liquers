---
id: RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT
kind: issue
title: Every -R-sdir/ evaluation prints two debug lines to stderr
status: draft
priority: P3
complexity: S
area: [core/plan]
design: resource-directory-step-debug-output-removal
created: 2026-10-21
github:
---
## Problem

`Step::GetResourceDirectory` in `liquers-core/src/interpreter.rs` writes two unconditional lines on
every evaluation:

```rust
eprintln!("Getting resource directory for key {:?}", key);
// ...
eprintln!("Got resource directory: {:?}", d);
```

The second prints the whole `Debug` form of the listing — every entry's `AssetInfo` — so a large
folder floods stderr. They read as leftovers from developing the `-R-dir/` / `-R-sdir/`
instructions (2026-10-10), not as intended diagnostics. Only `-R-sdir/` (`-R-store_directory/`,
planned as `GetResourceDirectory`) prints. `-R-dir/` is `GetAssetDirectory` and does not; it was
named here by mistake until the 2026-10-26 bulk design.

## Problem Example

```
-R-sdir/data/-/to_text
```

Today: besides its result, evaluating this query writes `Getting resource directory for key …` and
`Got resource directory: [AssetInfo { … }, …]` to stderr — in the axum server log, in the agent
memory binary's stderr and in a test's captured output. Should be: nothing is printed; a caller who
wants a trace gets it through the diagnostics mechanism `CORE-TRACING-MIGRATION` chooses.

## Impact

Noise rather than a wrong result: stdout is untouched, so the CLI rule holds. It hides real
diagnostics in server logs and makes listing a large folder costlier than it needs to be.

## Expected behaviour

Remove both lines (or, once `CORE-TRACING-MIGRATION` lands, emit at most a `trace`-level event
without the listing body).

## Discovery

Seen while checking `STORE-COMMAND-NAMESPACE-MISSING` against HEAD during the bulk design of the
store cluster (2026-10-21). Related: `CORE-TRACING-MIGRATION`, whose design moves `interpreter.rs`
diagnostics to `tracing` and does not single these two out as lines to delete rather than migrate.
