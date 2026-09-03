---
id: OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE
kind: issue
title: test_opendal_localfs passes even if -R-dir/src does not return AssetInfo
status: closed
priority: P3
complexity: S
area: [store/backends]
design: opendal-localfs-assetinfo-test
created: 2026-08-26
github:
---

## Problem

`liquers_store::opendal_store::tests::test_opendal_localfs` evaluates `-R-dir/src` and then
branches on the result type:

```rust
if let Value::AssetInfo(a) = s.data_unchecked().as_ref() {
    let names: std::collections::HashSet<String> = a
        .iter()
        .map(|x| x.filename.as_ref().unwrap().clone())
        .collect();
    eprintln!("Names: {:?}", names);
} else {
    eprintln!("Expected AssetInfo value, got {:?}", s.data_unchecked());
}
```

Neither branch asserts or panics. If `-R-dir/src` ever stopped returning `Value::AssetInfo` (a
regression in the `-dir` command, in plan evaluation, or in this store's `get_asset_info`), the
`else` branch would print a diagnostic and the test would still report `ok`.

## Impact

Low probability, moderate cost if it fires: this is the only test in the file exercising
`-R-dir/…` end to end through `SimpleEnvironment`/`EnvRef`, so a regression in that path has no
other test to catch it here.

## Expected behaviour

The `else` branch should fail the test, e.g. `panic!("Expected AssetInfo value, got {:?}",
s.data_unchecked())`, and the `names` set this test already computes should be asserted against
(at minimum, that it is non-empty, or that it contains an expected filename such as
`"opendal_store.rs"`) rather than only logged.

## Discovery

Found on 2026-08-26 while fixing `STORE-TESTS-PRINT-TO-STDOUT`: converting the branch's
diagnostic `println!`s to `eprintln!` (in scope for that issue) surfaced that the branch was never
an assertion to begin with (out of scope for that issue, which was about stdout only).

## Resolution, 2026-09-02

`test_opendal_localfs` destructures with a `let ... else` that panics, and asserts the computed
`names` set contains `"opendal_store.rs"` rather than printing it. Folded into
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/) because that work changed
`get_asset_info`, and this test is its only end-to-end coverage through the interpreter — the
regression it could not have caught was one this change might have caused.
