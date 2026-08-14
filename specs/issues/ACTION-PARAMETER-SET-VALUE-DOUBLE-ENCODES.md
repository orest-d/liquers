---
id: ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES
kind: issue
title: ActionParameter::set_value double-encodes its value
status: draft
priority: P1
complexity: S
area: [core/query]
design: parameter-entity-escaping
created: 2026-08-14
github:
---
## Problem

`ActionParameter::set_value` stores an **encoded** token where every other constructor and reader of
the variant stores a **decoded** one (`liquers-core/src/query.rs:614`):

```rust
pub fn set_value(&mut self, value: &str) {
    *self = Self::String(encode_token(value), Position::unknown())
}
```

`ActionParameter::String` holds the decoded value everywhere else — `new_string` stores its argument
verbatim, the parser stores the result of decoding entities, and `string_value()` hands the contents
back as the value. `encode` (`query.rs:607`) therefore escapes a second time:

```rust
let mut p = ActionParameter::new_string(String::new());
p.set_value("a b");        // stored as "a~.b"
p.encode();                // "a~~.b"  — decodes back to "a~.b", not "a b"
```

`string_value()` is wrong by the same mechanism: it returns `"a~.b"` where the caller set `"a b"`.

This is already recorded as a P1 row in the risk table of
`specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md:357`, which notes the method "pre-encodes
its stored value, which `encode` then escapes again", but it was never filed as an issue, so nothing
tracks it.

## Impact

Any programmatic query construction that goes through `set_value` rather than `new_string` produces
a query whose parameter differs from what was set. The value is not lost — it is escaped one time too
many — so the failure is silent and shows up as a wrong argument reaching a command, not as an error.

The method is public, and public in `liquers-py` and `liquers-web`'s dependency surface, so a host
language binding that maps a natural "set this parameter" operation onto it inherits the defect.

**This gets worse, not better, once `parameter-entity-escaping` lands.** Today the four escaped
characters are `~`, space, `/` and `-`, so the damage is limited to values containing those. After
that design, `encode_token` escapes everything outside `[A-Za-z0-9_+.]`, so `set_value("12:30")`
stores `12~ncolon~30` and encodes to `12~~ncolon~~30` — a longer, more confusing corruption over a
much wider set of inputs. The design also promises `parse(encode(s)) == s` for every `s`, and that
promise is false through this method for as long as it stands.

## Expected behaviour

`set_value` stores the decoded value, matching every other path into the variant:

```rust
pub fn set_value(&mut self, value: &str) {
    *self = Self::String(value.to_owned(), Position::unknown())
}
```

`encode` then escapes exactly once and `string_value()` returns what was set. Two alternatives, if
the current behaviour turns out to be relied on:

1. **Rename rather than change.** Keep an encoded-input setter under a name that says so
   (`set_encoded_token`) and add the decoded `set_value` beside it. Safer, but leaves a method whose
   only correct use is passing text that has already been escaped, which is the trap itself.
2. **Remove `set_value`.** Callers use `*p = ActionParameter::new_string(v.to_owned())`, which is
   barely longer and unambiguous.

Whichever is chosen, a round-trip test must cover the setter and not only `encode_token`, since the
existing tests exercise the function and miss the method.

## Discovery

Found during Phase 2 of the `parameter-entity-escaping` design, while enumerating every caller of
`encode_token` to size the change. The defect was already documented in `DOC_02` as a known P1 risk
but had no issue record, so it was invisible to `specs/index.csv` and to any query over the backlog.
