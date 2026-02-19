# Issue 13 Solution Proposal: CONTEXT-PARAM-ORDER

## Goal

Make `register_command!` robust to `context` placement by ensuring generated extraction indices match metadata/action parameter indices.

## Proposed Fix

Update `CommandSignature::extract_all_parameters()` in `liquers-macro/src/lib.rs` to increment argument index only for `CommandParameter::Param`, not for `CommandParameter::Context`.

### Current (problematic)

```rust
self.parameters
    .iter().enumerate()
    .map(|(i, p)| p.parameter_extractor(i))
```

### Proposed (index over Param only)

```rust
let mut extractors = Vec::new();
let mut arg_index = 0usize;
for p in &self.parameters {
    extractors.push(p.parameter_extractor(arg_index));
    if matches!(p, CommandParameter::Param { .. }) {
        arg_index += 1;
    }
}
```

This preserves current behavior for existing valid signatures and removes the context-position dependency.

Important: `CommandParameter::Param { .. }` includes both normal and `injected` parameters. That is desired, because injected parameters still occupy slots in metadata/plan argument vectors (they are just not consumed from query tokens).

## Why This Is Correct

- Metadata argument vector is built from `Param` only (`filter_map` over `argument_info_expression`).
- Runtime `CommandArguments` indexing is positional over action arguments.
- Aligning extraction index progression to `Param` count makes all three layers consistent:
  - macro extraction code
  - command metadata `arguments`
  - runtime resolved parameter values

## Optional Hardening (Recommended)

1. Keep context-position flexible (no syntax restriction), but add a clear comment in macro code that index is `Param`-only.
2. Add a targeted compile-time diagnostic only if multiple `context` parameters are present (if not already rejected by parser behavior), to avoid future ambiguity.
3. Add a short note in docs that `context` is "non-argument injected by wrapper".

## Test Plan

### A. Macro unit tests (`liquers-macro/src/lib.rs`)

Add/adjust tests around `extract_all_parameters`:

1. `context` first:

```rust
fn t(state, context, a: i32) -> result
```

Expect `a` extracted with index `0`.

2. `context` middle:

```rust
fn t(state, a: i32, context, b: String injected, c: f64) -> result
```

Expect:
- `a` -> `get(0, ...)`
- `b` -> `get_injected(1, ...)`
- `c` -> `get(2, ...)`

3. `context` last (regression):

```rust
fn t(state, a: i32, b: String, context) -> result
```

Expect unchanged indices `0`, `1`.

### B. Integration tests (`liquers-core/tests`)

Add one command registration/execution test where `context` is in middle and command executes successfully with both regular and injected params.

## Documentation Updates

Update these docs after implementation:

- `specs/REGISTER_COMMAND_FSD.md`
- `specs/COMMAND_REGISTRATION_GUIDE.md`

### Suggested wording

- `context` may appear anywhere in the parameter list.
- `context` is not part of action/query argument indexing.
- Query arguments map only to non-context parameters.

## Backward Compatibility

- Fully backward compatible for existing commands.
- Commands relying on workaround (`context` last) continue to work.
- Previously broken signatures become valid.

## Risks

- Low: localized macro codegen change.
- Main risk is snapshot-style tests expecting old token strings; update expected outputs accordingly.
