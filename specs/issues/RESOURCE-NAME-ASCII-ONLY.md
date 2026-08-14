---
id: RESOURCE-NAME-ASCII-ONLY
kind: issue
title: Non-ASCII resource names are unaddressable
status: draft
priority: P2
complexity: L
area: [core/query, core/store]
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

**This issue needs a decision before it needs an implementation.** Two questions, and the second is
the one that makes this harder than the action-parameter case it resembles.

### Question 1 — how much of the character space?

| Option | Accepted in a resource name | Needs entities? |
|---|---|---|
| **A. ASCII alphanumeric** (`[A-Za-z0-9_.-]`, the status quo after D10) | a fraction of ASCII | no |
| **B. Full ASCII** | every ASCII character, including `:` `?` `%` `#` space `&` | yes — `/` and `-` are structural and can never be literal |
| **C. Full Unicode** | every code point | yes, plus Question 2 |

**A partial-Unicode middle — Latin-1, or "European letters" — is not a candidate.** It is exactly
the arbitrariness of the `c as u8` truncation this issue describes, chosen deliberately rather than
by accident: a user still cannot predict whether a given character works, and the boundary has no
principle behind it. The choice is between A, B and C.

B is worth separating from C because it is the cheaper half: it addresses files with spaces,
colons and punctuation in their names — common in practice — without raising any of Question 2's
Unicode-specific problems, though it does raise the forbidden-character problem below.

### Question 2 — what does a key mean physically?

Entities make a resource name's *text* and its *value* differ, and a store has to map one of them
onto a physical name. Both readings are defensible and they are not equivalent.

| Contract | `-R/data/caf~U00E9~.csv` addresses | |
|---|---|---|
| **1. Encoded text is the name** | a file literally called `caf~U00E9~.csv` | round-trips exactly; the physical name is always ASCII; nothing is ever forbidden |
| **2. Decoded value is the name** | a file called `café.csv` | addresses files that already exist; the store looks normal to a human browsing it |

**Contract 2 is what makes this a real design problem**, because a decoded name can contain
characters a backend forbids or reinterprets:

- **`/` is the sharp one.** `a~U2F~b.csv` decodes to `a/b.csv`, which is *two* path components — so
  the key's segment structure stops matching the filesystem's. This is an injection, and it
  compounds `STORE-FILESTORE-PATH-TRAVERSAL` (P0), where `..` already escapes the store root
  because `AsyncFileStore::key_to_path` is `path.push(key.to_string())`. Any move to contract 2 has
  to land after, or together with, that fix, and has to define what is rejected and where.
- **Other forbidden or reinterpreted names:** NUL and control characters; `\ : * ? " < > |` on
  Windows, along with the reserved names `CON`, `PRN`, `AUX`, `NUL`, `COM1`…, trailing dots and
  spaces, and case-insensitive collisions; 255-byte component limits; `.` and `..`.
- **Normalization.** macOS APFS and HFS+ normalize file names to NFD, so a file created with NFC
  `café.csv` lists back decomposed — the key you wrote is not the key you read. Contract 1 is immune
  because the stored name is ASCII.
- **Backends genuinely differ.** A filesystem path is OS-dependent, an S3 object key is an arbitrary
  UTF-8 string, and a `localStorage` key is UTF-16. No single literal contract is true everywhere.

Contract 1 has the opposite weakness, and it is not small: a file created *outside* liquers with a
real Unicode name stays unaddressable, which is most of the motivation for option C in the first
place. It also makes a store's directory listing look mangled, and needs its own rule for a real
file whose name genuinely contains `~`.

A **third contract** follows from the divergence above: the key is abstract and **each backend
declares its own mapping** and its own rejection rules. The most honest option, at the cost that one
key names different physical objects in different stores.

### What any answer must also settle

- **`Key::encode` must be canonical** — keys are asset identity and cache identity, so a key must
  have exactly one spelling. Under B or C with contract 2, both a literal character and its entity
  decode to the same name, so the encoder picks one and the parser accepts both, mirroring what
  `parameter-entity-escaping` does for action parameters.
- **Where validation happens** — in `resource_name` at parse time, in `Key` construction, or at the
  store boundary. Only the last can know a backend's rules; only the first gives a good error
  position.
- **Whether `Key` gains a fallible constructor**, which the action-parameter encoder deliberately
  avoided (`encode_token` is total). If a decoded name can be forbidden, key construction can fail,
  and that is an API change.

## Discovery

Found while designing `parameter-entity-escaping`
(`specs/design/parameter-entity-escaping/phase1-high-level-design.md`), which fixes the same
truncation for action parameters. The maintainer decided resource names should be ASCII alphanumeric
"for now" (D10), which resolves the incoherence but leaves the underlying limitation, so it is
recorded here rather than left implicit in a design decision. The maintainer also asked that the
option space be written down rather than reopened later, which is what the two questions above are.

Marked `complexity: L` because Question 2 reaches `Key`, `AsyncStore` and every backend rather than
only `parse.rs`, so a design folder is required (`DOCS_STRUCTURE_GUIDE.md` §4.5). None exists yet.
