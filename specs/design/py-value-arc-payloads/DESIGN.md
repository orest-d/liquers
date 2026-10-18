---
id: PY-VALUE-ARC-PAYLOADS
kind: design
title: liquers-py's large value payload wrappers share their core record through Arc
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [py]
issues: [PY-VALUE-ENUM-OVERSIZED]
created: 2026-10-17
---
# liquers-py's large value payload wrappers share their core record through Arc

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented. Re-sized `S` → `M` (four wrapper types,
about 150 mechanical edits).

## Phase 1: High-Level Design

### Purpose

`liquers-py/src/value.rs` `Value` is a `#[pyclass]` complex enum whose `Metadata`, `Recipe`,
`CommandMetadata` and `Query` variants hold the pyclass wrappers `MetadataRecord`
(`src/metadata.rs`, `inner: CoreMetadataRecord`, 624 bytes), `Recipe` (`src/recipes.rs`, 256),
`CommandMetadata` (`src/command_metadata.rs`, tuple of the core type, 736) and `Query`
(`src/query.rs`, 64) by value. Every Python-side `Value`, `None` included, is therefore about 744
bytes, and every clone deep-copies the payload. `specs/reference/VALUE_TYPE_SYSTEM.md` §"What a
value variant may hold" requires O(1) clones; `core-value-enum-oversized` applies it to the core
and lib enums with a 32-byte bound and scopes this enum out.

### Problem Example

```rust
// cargo test -p liquers-py --no-default-features --lib
eprintln!("{}", std::mem::size_of::<liquers_py::value::Value>());
```

Today ≈744 (estimated from the core sizes measured in `core-value-enum-oversized`; liquers-py was not
built). Should be ≤32, and `Value::Metadata { value }.clone()` should bump a reference count, not copy
a 624-byte record with its log.

### Scope and Acceptance Criteria

- **AC-1** Size bound
  - WHEN `size_of::<liquers_py::value::Value>()` is evaluated
  - THEN it is at most 32 bytes
- **AC-2** Clone shares the core payload
  - WHEN a `MetadataRecord`, `Recipe`, `CommandMetadata` or `Query` wrapper (alone or inside a
    `Value`) is cloned
  - THEN both copies point at the same core allocation (`Arc::ptr_eq`)
- **AC-3** Value semantics kept
  - WHEN one of two clones is mutated through a wrapper setter (e.g. `MetadataRecord::set_message`,
    `Recipe::set_title`)
  - THEN the other clone is unchanged
- **AC-4** Python surface unchanged
  - WHEN Python code constructs `Value.Metadata(value=...)`, reads `.value`, or calls any wrapper
    method
  - THEN names, signatures and results are as today, and `liquers-py/tests/*.py` pass

Non-goals: `AssetInfo` (already behind `Vec`), `Key` (24 bytes), the core enums
(`core-value-enum-oversized`), and returning shared objects to Python.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — changes the type of the `pub` fields `inner` / `.0` of four
  exported wrapper types (rule 4, if unsure not eligible), and it is a representation change rather
  than a restored behaviour (rule 2).
- **Leading issue:** None
- **Explanation:** The bound (32) and `Arc` over `Box` were decided by the maintainer for the core
  enums on 2026-10-12 and carry over; Python sees no difference, so the remaining choice of where
  the `Arc` sits is an implementation detail resolved in Phase 2.
- **Open questions:** None. The issue's "64 bytes or less" predates the core decision of 32;
  32 is adopted.

### Design Dependencies

- `core-value-enum-oversized` (in_review, ready) — **overlaps**: same remedy and bound, different
  enum and files (T3 fails). The issue suggested doing it second so the bound settles; the bound is
  settled, so there is no ordering. Its AC-5 ("`liquers-py` builds unmodified") holds either way:
  the core accessors stay owned.

### Consolidated Findings

- `Arc` cannot go directly into the enum: PyO3 0.21 generates a getter and constructor per complex
  enum field, needing `IntoPy<PyObject>`/`FromPyObject` for the field type, which `Arc<Wrapper>`
  lacks and cannot be given (orphan rule). Putting the `Arc` inside each wrapper leaves the enum and
  the Python API untouched.
- Copy-on-write (`Arc::make_mut`) in the `&mut self` setters keeps value semantics: Python's
  `v.value` is still a copy as far as mutation is concerned.
- liquers-py has no Rust dependants (`crate-type = ["cdylib", "rlib"]`, no other crate depends on
  it), so the field-type change is internal in practice.

## Phase 2: Architecture

### Solution

Each wrapper holds `Arc<Core…>`:

```rust
pub struct MetadataRecord { pub inner: Arc<CoreMetadataRecord> }          // src/metadata.rs
pub struct Recipe { pub inner: Arc<CoreRecipe> }                          // src/recipes.rs
pub struct CommandMetadata(pub Arc<liquers_core::command_metadata::CommandMetadata>);
pub struct Query(pub Arc<liquers_core::query::Query>);                    // src/query.rs
```

Each gets a private `fn inner_mut(&mut self) -> &mut Core…` returning `Arc::make_mut(&mut …)` (the
core types are `Clone`) and an `impl From<Core…> for Wrapper`. Reads (`self.inner.message.clone()`,
`self.0.encode()`) compile unchanged through `Deref`; writes go through `inner_mut()`;
constructions `Wrapper { inner: x }` / `Wrapper(x)` become `Wrapper::from(x)`; moves out of the
wrapper use `Arc::unwrap_or_clone` (Rust 1.76; workspace minimum 1.95). `Value` and its match arms
in `value.rs` are unchanged except the four construction sites (≈lines 623, 635, 658, and the
samples in `mod tests`) and the two `value.0.clone()` reads (lines 347, 682), which become
`(*value.0).clone()`.

Rejected: `Box<Wrapper>` fields with hand-written `IntoPy`/`FromPyObject` for `Box<…>` (legal since
`Box` is fundamental, but clones still deep-copy and it depends on PyO3 macro bounds unverified
here); `Py<Wrapper>` fields (needs the GIL to construct and read, and makes clones alias mutations
visible to Python).

### Changes

- `liquers-py/src/metadata.rs` `MetadataRecord` — field type, `inner_mut`, `From`, its ≈35 setters
  and `with_*`/log methods (only the `MetadataRecord` impl at ≈line 247; `AssetInfo` and the other
  wrappers in the file are untouched).
- `liquers-py/src/recipes.rs` `Recipe` — field type, `inner_mut`, `From`, `set_query`/`set_title`/
  `set_description`, constructors at ≈lines 50, 110, 122; `RecipeList::add_recipe` passes
  `(*recipe.inner).clone()`.
- `liquers-py/src/command_metadata.rs` `CommandMetadata` — tuple field, constructors ≈334-356, uses of
  `.0` that mutate or move.
- `liquers-py/src/query.rs` `Query` — tuple field, constructors ≈501-681 (`Query(x)` →
  `Query::from(x)`); `metadata.rs:273` `Query(self.inner.query.clone())` likewise.
- `liquers-py/src/value.rs` — the construction sites above and a size-guard test.
- No Python-visible change, no core change, no command, no registry change.

### Risks

| Risk | Note |
|---|---|
| Files | the five `liquers-py/src` files above |
| Other wrapper users | `context.rs`, `plan.rs`, `dependencies.rs`, `commands.rs` may build `Query(…)`/read `.0`; the compiler lists each (no silent change: a write through `Arc` does not compile) |
| Semantics | a setter on a shared wrapper copies once (`make_mut`) — same observable result as today's deep clone |
| Build | can only be checked with `cargo test -p liquers-py --no-default-features` (links libpython) and maturin for the Python tests; not run in this design |
| Certainty | Medium-high: mechanical, compiler-guided; the PyO3 complex-enum field bound is the reason for the chosen shape |

## Phase 3: Examples and Tests

### Examples

Python, unchanged behaviour:

```python
m = liquers.MetadataRecord(); v = liquers.Value.Metadata(value=m)
w = v.value; w.message = "x"          # v.value.message is still ""
```

### Tests

In `liquers-py/src/value.rs` `mod tests` (GIL-free, like the existing ones):

- `py_value_is_at_most_32_bytes` — `assert!(size_of::<Value>() <= 32)` — AC-1
- `wrapper_clone_shares_core_payload` — for each of the four wrappers, `Arc::ptr_eq` after clone,
  also through `Value::clone` — AC-2
- `wrapper_setter_copies_on_write` — clone a `MetadataRecord`, `set_message` on one, the other's
  `message()` unchanged and the `Arc`s no longer equal; same for `Recipe::set_title` — AC-3
- existing `type_descriptions_match_identifier` and the other `samples()`-based tests — AC-4 (Rust side)
- `liquers-py/tests/test_basic_objects.py`, `test_parser.py` under maturin — AC-4 (Python side)

Run: `cargo test -p liquers-py --no-default-features --lib`; Python per `liquers-py/README.md`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `value.rs` `mod tests` — add `py_value_is_at_most_32_bytes`; confirm it fails and record the
  real size in this design — `cargo test -p liquers-py --no-default-features --lib py_value_is`
- [ ] 2. `query.rs` `Query` → `Arc`, `From`; fix the call sites the compiler reports —
  `cargo check -p liquers-py --no-default-features --tests`
- [ ] 3. `command_metadata.rs` `CommandMetadata` → `Arc`, `From`, call sites — same
- [ ] 4. `recipes.rs` `Recipe` → `Arc`, `inner_mut`, `From`, setters — same
- [ ] 5. `metadata.rs` `MetadataRecord` → `Arc`, `inner_mut`, `From`, setters — same
- [ ] 6. `value.rs` — construction sites, the two `.0.clone()` reads, `samples()`; add the AC-2/AC-3
  tests — `cargo test -p liquers-py --no-default-features --lib`
- [ ] 7. Python tests under maturin (`liquers-py/README.md`); if maturin is unavailable, say so in the
  PR — `pytest liquers-py/tests`
- [ ] 8. Close `PY-VALUE-ENUM-OVERSIZED`; regenerate the index —
  `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-py --no-default-features --lib`; `cargo build -p liquers-py` (default
features, the wheel configuration). Rollback per wrapper: each of steps 2-5 is independently
revertible. Final diff review: liquers-py only, no `.py` or stub change.
