---
id: MEMORY-STORE-RAW-WRITE-IS-NOT-SEARCHED
kind: issue
title: A file written with Store.set into a memory store is listed but its content is not searched
status: draft
priority: P3
complexity: S
area: [core/store]
design: 
created: 2026-11-09
github:
---
## Problem

Markdown files written into the default memory store with `Store.set(key, bytes)` are listed by
`ns-search/catalog`, but with status `None` and no title, and `catalog-t-t` reads no content for
them, so a content search finds nothing. The same files written with `Environment.store_set(key,
str)`, or lying in a `filesystem` store, are read and found.

```python
env = lq.Environment()
env.store.set("notes/milk.md", b"---\ntitle: Shopping\n---\n# Shopping\n\nMilk and bread.\n")
env.evaluate("-R-key/notes/-/ns-search/catalog-t-t/search-bread").to_list()     # []
env.evaluate("-R-key/notes/-/ns-search/field_extract").to_list()                # no fields
```

Observed through `liquers-py`; the metadata `Store.set` leaves on a memory store has status `None`
where a file store reports `Source`, which is probably what `catalog` goes by.

## Impact

Search silently returns nothing for files placed by a raw store write into a memory store. Affects
tests and examples; a file store is not affected. Workaround: write with `store_set`.

## Expected behaviour

A stored file with data is readable by `catalog` whichever store holds it, or the row says why its
content was skipped.

## Discovery

Writing chapter 13 of the Python user guide, 2026-11-09.
