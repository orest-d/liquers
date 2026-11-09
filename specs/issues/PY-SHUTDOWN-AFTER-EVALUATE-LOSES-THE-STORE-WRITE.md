---
id: PY-SHUTDOWN-AFTER-EVALUATE-LOSES-THE-STORE-WRITE
kind: issue
title: shutdown right after an evaluation loses the store write of a keyed result
status: draft
priority: P2
complexity: M
area: [py]
design: 
created: 2026-11-09
github:
---
## Problem

`Environment.evaluate` of a keyed, recipe-backed result returns as soon as the value is ready; the
write to the store happens afterwards, in the background. `shutdown()` (or the end of the program)
immediately after the evaluation drops that write.

```python
root = pathlib.Path(tempfile.mkdtemp()); (root / "data").mkdir()
(root / "data" / "sales.csv").write_text("region,amount\nEU,120\n")
(root / "data" / "recipes.yaml").write_text(
    "recipes:\n  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv\n")
env = lq.Environment(config={"store": {"stores": [
    {"type": "filesystem", "prefix": "data", "config": {"path": str(root)}}]}})
env.evaluate("-R/data/eu.csv")
env.shutdown()
(root / "data" / "eu.csv").exists()      # False
```

Without the `shutdown()` the file appears about a millisecond after `evaluate` returns. With
`get_asset(...).get()` the status is already `Ready` while the file is not there yet.

## Impact

A script that computes a few recipe results and exits, the simplest batch use of recipes, can leave
some of them unwritten, silently; they are recomputed on the next run. A test that looks at the
store folder right after an evaluation is flaky. Workaround: wait, which a caller cannot do
reliably because nothing reports that the write is done.

## Expected behaviour

Either `shutdown()` completes the store writes of finished assets before it returns, or a caller
can wait for persistence. `ENVIRONMENT-ORDERLY-SHUTDOWN` is the natural home for the first.

## Discovery

Writing the recipes example of the Python user guide (`docs/guides/python/07-assets.md` §7.8),
which now sleeps before reading the file, 2026-11-09. Not checked from Rust; the cause is likely
in the asset manager rather than in `liquers-py`.
