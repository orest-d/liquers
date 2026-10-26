# Phase 2: Solution & Architecture — automatic conversion of a command's state and link arguments

## Overview

In `do_step`'s `Step::Action` arm (`liquers-core/src/interpreter.rs`), after the link arguments are
resolved and before `execute_async`, the interpreter looks up the metadata of the command it is
about to run. For the state and for each link-resolved argument whose `ArgumentInfo::accepts` is
declared and does not contain the value's identifier, one private helper consults the
environment's `ConversionRegistry` with `ConversionPolicy::Automatic`, converts to the first
declared identifier reachable by an automatic edge, rewrites the state's metadata, and logs. When no
such edge exists, or it fails, the value passes through with a warning (question 1).

Rejected: converting inside the macro wrapper by Rust type (`to_type_identifier::<V, T>()` in
`CommandArguments::get`) — needs `TypeIdentifiedIn` impls that do not exist, and would leave
Python/JS commands and the state out; a converting accessor handed to the command (draft question
4) — every command would change; converting in the plan (a `Step::Convert`) — the input's type is
mostly unknown at plan time, so the step would still decide at run time.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `VALUE-CONVERSION-CAPABILITY` (`value-conversion-registry`) | draft | P2 | Provides the registry | yes | yes (order) | `requires` |
| `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` (`command-io-type-declarations`) | draft | P2 | Provides `accepts` | yes | yes (order) | `requires` |
| `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` | draft | P3 | Not needed: metadata-driven | no | no | None |
| `COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING` | draft | P3 | Selection precedes conversion | no | no | `overlaps` |

## Interfaces

No new public item. Private, in `liquers-core/src/interpreter.rs`:

```rust
/// Converts `state` to the first type in `accepts` it reaches by an automatic edge.
/// Returns it unchanged when it is already accepted, when `accepts` is any, or (with a warning
/// logged) when no automatic edge reaches a declared type or the conversion fails.
fn convert_to_declared<E: Environment>(
    envref: &EnvRef<E>,
    context: &Context<E>,
    command: &CommandKey,
    site: ConversionSite<'_>,          // State | Argument { index, name }
    accepts: &TypeSet,
    state: State<E::Value>,
) -> Result<State<E::Value>, Error>;   // Err only from logging

enum ConversionSite<'a> { State, Argument { index: usize, name: &'a str } }
```

Algorithm (sync, no `.await`):

1. `let from = state.type_identifier();` if `accepts.accepts(&from)` → return `state`.
2. `let registry = envref.get_conversion_registry();` for each `to` in `accepts.identifiers()`:
   if `registry.can_convert(&from, to)` is `Some(class)` with `class.is_automatic()`, call
   `registry.convert(&ConversionInput { value: &*state.data, data_format: declared }, to,
   ConversionPolicy::Automatic)` where `declared` is the metadata's declared data format, if any.
   - `Ok((value, class))`: clone the metadata; on `Metadata::MetadataRecord` set
     `with_type_identifier(to)` and `data_format = None`; return
     `State::from_value_and_metadata(value, Arc::new(metadata))` after
     `context.info("converted {site} {from} → {to} ({class:?}) for '{command}'")`.
   - `Err(e)`: `context.warning("{site} {from} → {to} failed ({e}); passed unchanged …")`, return
     `state`.
3. No automatic edge: `context.warning("{site}: no automatic conversion {from} → {accepts}{refused};
   passed unchanged to '{command}'")`, where `refused` lists any non-automatic class found
   (`Lossy`, `Fallible`). Return `state`.

Call sites in the `Step::Action` arm:

```rust
let metadata = envref.get_command_metadata_registry().find_command(&realm, &ns, &action_name);
// links: before arguments.set_value(i, …)
let arg_state = match metadata.as_ref().and_then(|m| m.arguments.get(i)) {
    Some(info) if !info.accepts.is_any() =>
        convert_to_declared(&envref, &context, &command_key,
            ConversionSite::Argument { index: i, name: &info.name }, &info.accepts, arg_state)?,
    Some(_) | None => arg_state,
};
// state: before execute_async
let input = match metadata.as_ref().and_then(|m| m.state_argument.as_ref()) {
    Some(sa) if !sa.accepts.is_any() =>
        convert_to_declared(&envref, &context, &command_key, ConversionSite::State, &sa.accepts, input)?,
    Some(_) | None => input,
};
```

The `Arc` of a converted argument is stored with `arguments.set_value(i, converted.value()?)`, so
`CommandArguments::get`'s fast path extracts from the converted value unchanged.

## Integration Points

- `liquers-core/src/interpreter.rs` — `do_step` `Step::Action` arm; the helper and `ConversionSite`
  (with an exhaustive `Display`).
- Reads only: `CommandMetadataRegistry::find_command`, `ArgumentInfo::accepts`, `TypeSet`
  (`command-io-type-declarations`); `Environment::get_conversion_registry`, `ConversionInput`,
  `ConversionPolicy`, `ConversionClass` (`value-conversion-registry`).
- No change to `commands.rs`, the macro, `liquers-lib`, the bindings.

## Error Handling

The helper never fails on a conversion problem (question 1); it returns `Err` only when writing the
log entry fails, which the arm already propagates. Under the strict alternative, step 3 and the
`Err(e)` branch would instead return `Error::conversion_error_with_message(from, accepts, reason)`
with the command key and position attached by the existing `map_err`.

## Relevant Commands

None added or changed. Any command that declares `accepts` (the 114 declared by
`command-io-type-declarations`) is affected; for most of them the declared list already contains
every type that reaches them in practice, so nothing converts.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | §Conversion: "Automatic conversion at a command" — sites, target choice, pass-through, metadata; History, `reviewed:` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | under the declaration section: what `accepts` buys; History, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | "Declaring conversions": which classes are used automatically; History, `reviewed:` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/interpreter.rs`; tests in `liquers-core/tests/` |
| Crates and workflows affected | core; every evaluation of a declared command |
| Existing tests likely to change | none expected: no core test command declares `accepts` |
| New validation | Phase 3 tests; `cargo test -p liquers-lib --lib --tests` for the declared library commands |
| Compatibility / data | Behaviour changes only for declared commands receiving an undeclared type with an automatic edge. A converted state's `data_format` is reset, so a later `to_*` writes the new type's default format — intended |
| Concurrency / performance | One metadata lookup (linear) per action, plus a `can_convert` map lookup per declared mismatch. A large structural conversion runs synchronously on the executor thread (accepted by `value-conversion-registry` question 5) |
| Security | none |
| Recovery | Revert the arm; declarations stay inert |
| Certainty | Medium-high: depends on two designs' interfaces as written; re-check their signatures when they land |
