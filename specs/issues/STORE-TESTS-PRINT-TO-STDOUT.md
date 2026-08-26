---
id: STORE-TESTS-PRINT-TO-STDOUT
kind: issue
title: liquers-store's OpenDAL tests use println! against the blanket stdout rule
status: draft
priority: P3
complexity: S
area: [store/backends, docs]
design:
created: 2026-08-25
github:
---
## Problem

`CLAUDE.md` states the no-stdout rule as blanket — "it applies inside `#[cfg(test)]` modules too"
— so that nobody has to reason about whether a given line sits in a code path some future binary
will call. `liquers-store/src/opendal_store.rs` breaks it twelve times inside the `#[cfg(test)]`
module that begins at line 524:

```
liquers-store/src/opendal_store.rs:635  println!("After set: {:?}", store.keys().await.unwrap());
liquers-store/src/opendal_store.rs:637  println!("Key {i}: {}", k.encode());
liquers-store/src/opendal_store.rs:686  println!("After set: {:?}", store.keys().await.unwrap());
liquers-store/src/opendal_store.rs:688  println!("Key {i}: {}", k.encode());
liquers-store/src/opendal_store.rs:716  println!(…)
liquers-store/src/opendal_store.rs:722  println!(…)
liquers-store/src/opendal_store.rs:734  println!("Item {i}: {k}");
liquers-store/src/opendal_store.rs:743  println!("KEY Item {i}: {k}");
liquers-store/src/opendal_store.rs:750  println!("----------------------------");
liquers-store/src/opendal_store.rs:753  println!("----------------------------");
liquers-store/src/opendal_store.rs:759  println!("Names: {:?}", names);
liquers-store/src/opendal_store.rs:761  println!("Expected AssetInfo value, got {:?}", s.data_unchecked());
```

These are debugging leftovers rather than assertions: several print a key listing that the very
next line asserts on, and two print only a row of dashes.

## Impact

Low. Inside `#[cfg(test)]` the lines never reach a binary, and cargo captures them for passing
tests, so nothing is corrupted today. What they cost is the rule's blanket form: a reader who sees
`println!` accepted in one test module has to decide case by case in the next one, which is the
reasoning the rule was written to remove. Grepping for violations also returns twelve
false-positive-looking hits that have to be re-checked each time.

## Expected behaviour

Either the lines go (most of them state what the following assertion already checks), or they
become `eprintln!`. Removal is preferable: a passing test should be silent, and a failing
assertion already prints the value.

If instead the project decides test modules are genuinely exempt, `CLAUDE.md`'s *Diagnostic
Output* section should say so, because it currently says the opposite.

## Discovery

Found on 2026-08-25 while fixing `LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED`: auditing test output
across feature configurations turned up stray printing, and a `grep` for `println!` outside
`[[bin]]` targets found these.
