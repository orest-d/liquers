---
id: RESOURCE-NAME-ASCII-ONLY
kind: issue
title: Non-ASCII resource names are unaddressable
status: draft
priority: P2
complexity: M
area: [core/query]
design: 
created: 2026-08-14
github:
---
## Problem

A file whose name contains a non-ASCII character cannot be named in a query. `resource_name`
(`liquers-core/src/parse.rs:339-341`) accepts a character only when
`AsChar::is_alphanum(c as u8)` holds, plus `_`, `.` and `-`. Measured at HEAD:

```
-R/data/café.csv     Can't parse query completely
-R/data/Łódź.csv     Can't parse query completely
-R/data/ŁŁ.csv       parses
```

The third line is the same `c as u8` truncation described in `PARAMETER-ESCAPING-INCOMPLETE`: `Ł` is
U+0141, whose low byte `0x41` is `A`, so it slips through while `é` does not. Which non-ASCII
characters are accepted is therefore arbitrary — a function of the code point modulo 256 — and none
of it is usable, because a name is accepted only if *every* character in it happens to survive the
truncation.

The `parameter-entity-escaping` design (D10) deliberately narrows `resource_name` to
`is_ascii_alphanumeric()`, which makes the behaviour coherent and predictable. It does not make
non-ASCII names addressable, and it removes the accidental cases: `-R/data/ŁŁ.csv` parses today and
will stop parsing.

There is also no escape mechanism to fall back on. The tilde entities that design introduces
(`~U00E9~`, `~neacute~`) are part of the **action-parameter** grammar; `resource_name` has no
entity production at all, so unlike a string parameter, a resource name has no way to spell a
character the character class rejects.

## Impact

Any store containing a file with a non-ASCII name has content that queries cannot reach — the file
can be listed but not fetched, and no recipe can refer to it. This is a plausible situation rather
than a contrived one: a file store over a real directory tree, a user upload, or any dataset naming
its files in a language other than English.

The scope is bounded by the fact that it has always been broken, so nothing regresses for existing
users, and by the workaround: rename the file, or address it through a store whose keys are
ASCII-normalised on write. That is why this is P2 rather than P1 — but it is the same
internationalization defect as `PARAMETER-ESCAPING-INCOMPLETE`, in the half of the grammar that
design does not touch.

The `-R/data/ŁŁ.csv` narrowing is a breaking change in the strict sense. Any query relying on it is
relying on a truncation bug, and such a query is unlikely to exist, but it should be noted in the
grammar change rather than discovered.

## Expected behaviour

Non-ASCII file names should be addressable. Three routes, not ranked:

1. **Widen the character class** to `char::is_alphanumeric()`, mirroring what
   `parameter-entity-escaping` does for action parameters. Simplest, and consistent with the
   parameter half — but resource names become store keys and reach real filesystems, so it raises
   the normalization question (`é` as U+00E9 versus `e` + U+0301 are different keys naming the same
   file on a macOS volume) that the parameter side avoids by encoding to ASCII.
2. **Give `resource_name` an entity production**, so a name can be spelled `caf~U00E9~.csv` in
   ASCII. Consistent with how the parameter grammar solves the identical problem, and keeps query
   text ASCII-safe, at the cost of extending the entity mechanism into a second production and
   making key encoding and decoding non-trivial.
3. **Both** — widen the accepted class *and* provide entities, so hand-written queries can use
   whichever suits and the encoder always emits the ASCII form.

Whichever is chosen must state what `Key::encode` produces, since keys are identity for assets and
the cache.

## Discovery

Found while designing `parameter-entity-escaping`
(`specs/design/parameter-entity-escaping/phase1-high-level-design.md`), which fixes the same
truncation for action parameters. The maintainer decided resource names should be ASCII alphanumeric
"for now" (D10), which resolves the incoherence but leaves the underlying limitation, so it is
recorded here rather than left implicit in a design decision.
