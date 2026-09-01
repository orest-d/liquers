# Phase 3: Examples and Tests

## Examples and Tests

1. `Error::general_error("x").with_key(parse_key("a/b.txt")?)` has
   `key == Some("a/b.txt")` and `query == None`.
2. The equivalent `with_query` case populates only `query`.
3. Applying both builders retains both distinct values; serialized JSON exposes them under their
   matching keys and round-trips.
4. `dependency_cycle` retains its intentional dual context.
5. A representative `recipes.rs` or `plan.rs` failing path enriched with `with_key` reports key
   context without fabricating query context.

Place pure unit tests in `liquers-core/src/error.rs`; extend an existing caller test only if its
fixture is already local. Run `cargo test -p liquers-core --lib error` and the full core suite.
