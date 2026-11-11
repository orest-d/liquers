---
id: MACRO-VALUE-STATE-KEYWORD-FAILS-OUTSIDE-CORE
kind: issue
title: register_command! with the value state keyword does not compile outside liquers-core
status: draft
priority: P2
complexity: S
area: [macro]
design:
created: 2026-11-10
github:
---

## Problem

`REGISTER_COMMAND_FSD.md` documents three state keywords, `state`, `value` and `text`. The `value`
form expands to code that reads the private field `State::data`, so it compiles only inside
`liquers-core`:

```rust
fn kind(value: Value) -> Result<Value, Error> { ... }
register_command!(cr, fn kind(value) -> result)?;
// error[E0616]: field `data` of struct `liquers_core::state::State` is private
```

## Impact

A documented form of the macro is unusable in `liquers-lib`, in tests and in every user crate.
Workaround: take `state` and read the value from it. The Rust user guide (chapter 5) therefore
documents `state` and `text` only.

## Expected behaviour

`fn cmd(value)` compiles in any crate: the macro reads the value through a public accessor of
`State`, or `State` gains one. Alternatively the keyword is removed from the macro and from
`REGISTER_COMMAND_FSD.md`.

## Discovery

Writing the examples of `docs/guides/rust/05-commands.md` in
`liquers-lib/tests/user_guide_GUIDE.rs` (2026-11-10); the compiler error above is from that file.
