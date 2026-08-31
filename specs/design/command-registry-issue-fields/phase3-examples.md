# Phase 3: Examples and tests

Add inline tests in `command_metadata.rs`:

1. `warning_preserves_realm_namespace_and_name` constructs `("realm", "namespace", "command")` and verifies those fields plus `is_error == false`.
2. `error_preserves_realm_namespace_and_name` uses distinct strings and verifies the fields plus `is_error == true`.
3. `check_reports_reserved_name_in_its_namespace` builds metadata with namespace `custom`, name `ns`, and asserts the produced issue attributes it to `custom/ns`.

Run `cargo test -p liquers-core command_registry_issue` and `cargo test -p liquers-core --lib`.
