# Phase 3: Examples and Tests - Documenting `payload:`, `expires:` and `version:`

## Snippets for the Guide and FSD

```rust
// A command that reads the evaluation payload. The payload type implements
// InjectedFromContext<E> (see PAYLOAD_GUIDE.md).
fn whoami(_state: &State<Value>, user: UserId) -> Result<Value, Error> {
    Ok(Value::from(user.0))
}

register_command!(cr,
    fn whoami(state, user: UserId injected) -> result
    payload: required
)?;
```

```rust
// A versioned command whose cached results expire after five minutes. `version: auto` hashes
// the function's source, so editing the function invalidates results computed by the old code.
#[liquers_macro::command_version]
fn shout(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}

register_command!(cr,
    fn shout(state) -> result
    expires: "in 5 min"
    version: auto
)?;
```

Both follow existing compiled forms: `volatility_integration.rs::test_payload_required_sets_metadata_and_volatile`
(statement after the signature, bare identifier), `expiration_integration.rs` (`expires: "in 5 min"`),
and `liquers-lib/src/commands.rs` (`#[liquers_macro::command_version]` with `version: auto`, ≈19/≈268).

## Validation (no new tests)

The change adds no code, so no unit tests are added.

1. **Grammar.** `cargo test -p liquers-macro version` and
   `cargo test -p liquers-core --test volatility_integration test_payload_required` and
   `cargo test -p liquers-core --test expiration_integration test_register_command_expires_in_plan`.
2. **Negative forms.** Confirm from the parser that `payload: "required"` / `payload: true` are
   rejected (`input.parse::<syn::Ident>()`), that `version: later` is rejected with "Unknown version
   specification", and that `expires` accepts only a string literal. No trybuild test is added (the
   macro crate has none to extend).
3. **`version: now` claim.** Confirm the emitter calls `Version::from_time_now()` (≈1346).
4. **Links.** `python3 scripts/docs_index.py --check`.

## Coverage Review

Criteria 1-6 are textual and checked against Phase 2; criterion 7 by item 1.
