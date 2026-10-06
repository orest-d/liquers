# Phase 2: Solution and Architecture

- `liquers-records/src/formats/ndjson.rs`, `read_inferred_objects`: after the union loop,
  `names.sort();`. The `seen` set stays for de-duplication.
- `liquers-records/src/formats/shapes.rs`: in the `columns` shape the column names are the
  top-level keys (`obj.iter()`), which a `BTreeMap` already sorts. Make it explicit (collect +
  `sort()`) so it does not depend on the map type. In the `index` shape, the column names are the
  union of inner keys, so collect, de-duplicate, then `sort()`. Keep the id/index field placement as
  it is today.
- Add a short doc comment at each site: "JSON has no key order; columns are sorted by name for a
  stable order (RECORD_STREAMS.md)".

## Rejected alternatives

- `serde_json/preserve_order` or an order-capturing parse. Rejected by the decision: file order is
  not meaningful for JSON.

## Known-issue preflight

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `formats/ndjson.rs`, `formats/shapes.rs` (+ tests) |
| Existing tests | Tests with multi-row key sets in first-appearance order may flip. Search the records tests. |
| Compatibility | Column order of some schema-less reads changes once, to the stable order |
| Recovery | Revert |
| Certainty | High |
