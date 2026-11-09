---
id: PY-STORE-SET-OF-BYTES-IGNORES-THE-KEY-EXTENSION
kind: issue
title: Environment.store_set of bytes records the format b whatever the key's extension
status: draft
priority: P3
complexity: S
area: [py]
design: 
created: 2026-11-09
github:
---
## Problem

`Environment.store_set(key, data)` with `bytes` (or a `str`) records the value's own data format,
`b` (or `txt`), whatever the key's extension is. `Store.set(key, data)` on the same key records the
format of the extension (`csv`), as every store does since `store-key-format-seeding`.

```python
env = lq.Environment()
env.store_set("data/sales.csv", b"region,amount\nEU,120\n")
env.evaluate("-R/data/sales.csv/-/ns-rec/to_record")
# GeneralError: TableFormat::from_data_format: unknown data format 'b'
env.store.set("data/s2.csv", b"region,amount\nEU,120\n")
env.evaluate("-R/data/s2.csv/-/ns-rec/to_record")      # works
```

`store_set` goes through the asset manager (the metadata says "State set externally", status
`Source`, type `Bytes`), so the format is the serializer's for a bytes value, not the key's.

## Impact

`store_set` is the call the Python guide gives for putting test input into a store, and the only
write that invalidates dependent results, so the natural way to place a CSV, JSON-lines or
Parquet file fails at the first table command. Workarounds: `env.store.set` (which does not
invalidate, see `WEB-STORE-WRITE-DOES-NOT-INVALIDATE-RESULTS` for the same split in the browser),
or naming the format in the query (`to_record-csv`). The user guide documents both
(`docs/guides/python/06-stores.md` §6.5).

Related: `BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT` (a bytes value cannot carry a named format at
all), which a fix probably shares.

## Expected behaviour

Bytes set at a key whose extension names a data format are recorded in that format, as a file
written to the store is.

## Discovery

Writing the first example of the Python user guide (chapter 1), 2026-11-09.
