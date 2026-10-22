---
id: PY-REPRS-SHOW-RUST-DEBUG-FORMAT
kind: issue
title: Several Python classes' repr is Rust debug output, and Key and Query quote their text twice
status: draft
priority: P3
complexity: S
area: [py]
design:
created: 2026-10-19
github:
---

## Problem

`repr(lq.parse("-R/data/a.txt"))` is `Query('"-R/data/a.txt"')`, and `repr` of a `Key` is
`Key('"data/a.txt"')`: `liquers-py/src/query.rs` formats the encoded string with `{:?}` inside
quotes it adds itself (`format!("Query('{:?}')", …)`, `format!("Key('{:?}')", …)`). Other classes
(`ActionRequest`, `ResourceName`, the segment classes, `LogEntry` inside a list) return the Rust
`Debug` rendering, such as `LogEntry { kind: Info, message: "…", message_html: None, … }`.

## Problem Example

At HEAD (2026-10-20), the same "quotes inside quotes" pattern is in five places, and seventeen
`__repr__`s return `format!("{:?}", …)`:

```text
liquers-py/src/query.rs        Query('{:?}'), Key('{:?}')       -> Query('"-R/data/a.txt"')
liquers-py/src/dependencies.rs DependencyRelation('{:?}')
liquers-py/src/expiration.rs   Expires('{:?}'), ExpirationTime('{:?}')
liquers-py/src/metadata.rs     '{:?}' (LogEntryKind)
Debug only: metadata.rs (AssetInfo, Metadata, MetadataRecord, LogEntry), query.rs (5 classes),
            command_metadata.rs (7 classes), dependencies.rs (1)
```

## Impact

Cosmetic but visible in every interactive session, notebook and test failure message. A `repr`
cannot be pasted back as Python, and the debug form exposes Rust field names.

## Expected behaviour

`repr(q) == "Query('-R/data/a.txt')"`, and `eval(repr(q))` within `liquers`' namespace gives an
equal object where the class has a public constructor. The other classes have a short Python-style
`repr` naming their main fields.

## Discovery

Found while writing `guides/PYTHON_GUIDE.md` for `design/liquers-py/` Phase 5 (2026-10-19).
