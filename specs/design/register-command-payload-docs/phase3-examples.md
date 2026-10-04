# Phase 3: Examples and Tests - Documenting `payload: required`

## Snippet for the Guide and FSD

```rust
// The payload type implements InjectedFromContext<E> (see PAYLOAD_GUIDE.md).
fn whoami(_state: &State<Value>, user: UserId) -> Result<Value, Error> {
    Ok(Value::from(user.0))
}

register_command!(cr,
    fn whoami(state, user: UserId injected) -> result
    payload: required
)?;
```

This is the same shape as `PAYLOAD_GUIDE.md`'s quick start (≈35-42) and as
`test_payload_required_sets_metadata_and_volatile` (statement after the signature, bare
identifier).

## Validation (no new tests)

The change adds no code, so no unit tests are added. Validation:

1. **Snippet compiles** — the statement form is already compiled by
   `liquers-core/tests/volatility_integration.rs::test_payload_required_sets_metadata_and_volatile`;
   the injected-parameter form by `PAYLOAD_GUIDE.md`'s examples' counterparts in
   `liquers-core/tests` (search `injected` + `payload: required`). Run
   `cargo test -p liquers-core --test volatility_integration test_payload_required` to confirm
   the grammar at implementation time.
2. **Negative forms** — confirm the FSD's claim that `payload: "required"` and `payload: true`
   are rejected: read the parser arm (`input.parse::<syn::Ident>()`), which rejects literals; no
   trybuild test is added (the macro crate has none to extend).
3. **Links** — `python3 scripts/docs_index.py --check` (dead relative links fail it).

## Coverage Review

Criteria 1-4 are textual and checked by review against Phase 2; criterion 5 by item 1.
