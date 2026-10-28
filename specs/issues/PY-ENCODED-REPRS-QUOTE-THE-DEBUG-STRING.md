---
id: PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING
kind: issue
title: The reprs of Key, Expires and ExpirationTime put Rust debug text inside Python quotes
status: draft
priority: P3
complexity: S
area: [py]
design: py-encoded-reprs
created: 2026-10-27
github:
---
## Problem

Three `__repr__`s in `liquers-py` write Python string quotes themselves and fill them with Rust's
`{:?}` formatting, so the quoted text is not the object's text form:

- `liquers-py/src/query.rs` `Key::__repr__`: `format!("Key('{:?}')", self.0.encode())`. `{:?}`
  of a `String` adds a second pair of double quotes.
- `liquers-py/src/expiration.rs` `Expires::__repr__`: `format!("Expires('{:?}')", self.0)`, and
  `ExpirationTime::__repr__`: `format!("ExpirationTime('{:?}')", self.0)`. `{:?}` of the core enum
  shows the Rust variant (`InDuration(300s)`), not the text that the constructor takes and that
  `encode()` returns.

Each of the three classes has a constructor taking that text, so the fixed repr evaluates back.
`Query::__repr__` has the same quoting defect, but `Query` has no text constructor, so its repr
depends on the convention still open in `PY-REPRS-SHOW-RUST-DEBUG-FORMAT`, which keeps it.

## Problem Example

```python
import liquers as lq
repr(lq.parse_key("data/a.txt"))    # today: Key('"data/a.txt"')         expected: Key('data/a.txt')
repr(lq.Expires("in 5 min"))        # today: Expires('InDuration(300s)') expected: Expires('in 5 min')
repr(lq.ExpirationTime("never"))    # today: ExpirationTime('Never')     expected: ExpirationTime('never')
```

## Impact

Cosmetic, but visible in every interactive session, notebook and pytest failure message. None of
the three reprs can be pasted back as Python, although their constructors take exactly the text
that should be there.

## Expected behaviour

Each of the three reprs is `Name(<Python repr of the text form>)`: `encode()` for `Key`, and the
`Display` text (what `encode()` returns) for `Expires` and `ExpirationTime`.

## Discovery

Split on 2026-10-27 from `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` (bulk design). Its design,
`design/py-python-style-reprs/`, found that these sites are a plain bug that needs no repr
convention. `Query` was first included and moved back on review the same day, because it cannot be
rebuilt from its repr.
