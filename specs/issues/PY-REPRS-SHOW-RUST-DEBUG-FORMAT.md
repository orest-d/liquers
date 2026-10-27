---
id: PY-REPRS-SHOW-RUST-DEBUG-FORMAT
kind: issue
title: Most Python classes' repr is Rust debug output, and no repr convention exists
status: draft
priority: P3
complexity: M
area: [py]
design: py-python-style-reprs
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

```python
repr(lq.ActionRequest("to_text"))   # today:    ActionRequest { name: "to_text", parameters: [], … }
                                    # expected: a short Python-style repr, e.g. ActionRequest('to_text')
repr(lq.LogEntryKind.Info)          # today:    'Info'      expected: LogEntryKind.Info
repr(lq.parse("-R/data/a.txt"))     # today:    Query('"-R/data/a.txt"')
                                    # expected: Query('-R/data/a.txt') (Query has no text constructor)
```

Re-checked at HEAD (2026-10-26, `grep -n -A3 __repr__ liquers-py/src`): the "quotes inside
quotes" pattern is in seven places, and twenty `__repr__`s return bare `format!("{:?}", …)`:

```text
quotes in quotes: query.rs        Query('{:?}'), Key('{:?}'), QuerySource('{:?}')
                  dependencies.rs DependencyRelation('{:?}')
                  expiration.rs   Expires('{:?}'), ExpirationTime('{:?}')
                  metadata.rs     '{:?}' (LogEntryKind)
Debug only:       query.rs (7 classes), command_metadata.rs (7), metadata.rs (DependencyRecord,
                  Metadata, MetadataRecord, LogEntry), recipes.rs (1), dependencies.rs (1)
Already Python-style: Version, DependencyKey, Status, AssetRef, Issue
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

Designed 2026-10-27 in `design/py-python-style-reprs/` (bulk design). The design separates the
plain bug (part A: `Key`, `Query`, `Expires`, `ExpirationTime`, whose quotes are filled with
`{:?}`) from the choice of a repr convention for the other 23 classes (part B, a maintainer
decision). Complexity raised from `S` to `M`: 27 `__repr__`s in six files of one crate.

**Split on 2026-10-27.** Part A (`Key`, `Expires`, `ExpirationTime`, whose constructors take the
text their repr should show) is now its own issue, `PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING`, with
the design `design/py-encoded-reprs/` (ready, eligible for automatic fixing). This issue keeps the
other 24 classes (part B), `Query` included because it cannot be rebuilt from its repr, and the
repr convention they wait for; it stays `M` (24 `__repr__`s in five files) and closes when
`design/py-python-style-reprs/` lands.
