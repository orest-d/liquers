# Phase 3: Examples and Tests - Account for `entities.rs` and `cache.rs`

## Evidence Instead of Tests

The change adds a comment and removes an unused import, so no Rust test is added: a test could
only assert that a comment exists. What must be proven is the audit, and that nothing breaks.

| Check | Command | Expected |
|---|---|---|
| `entities` is live | `grep -rn "entities::" liquers-core/src --include=*.rs` | hits in `escape.rs` and `bin/generate_entities.rs` |
| `cache` has no core caller | `grep -rn "crate::cache\|cache::" liquers-core/src --include=*.rs` | only `lib.rs`'s `pub mod cache;` |
| `cache`'s compiled consumers | `grep -rn "liquers_core::cache" --include=*.rs liquers-*/src` | `liquers-py/src/context.rs` (compiled) and `liquers-py/src/cache.rs` (orphan, not declared in `lib.rs`) |
| core still builds | `cargo check -p liquers-core` and `cargo check -p liquers-core --target wasm32-unknown-unknown` | no new warning |
| py still builds | `cargo check -p liquers-py --lib` | unchanged warnings |
| docs | `python3 scripts/docs_index.py --check` | 0 errors |

The evidence table goes into the issue's resolution, so the next audit starts from it.
