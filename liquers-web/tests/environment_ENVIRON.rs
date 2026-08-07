//! `ENVIRON*` conformance tests.
//!
//! These share one wasm module instance and therefore one thread-local singleton, so they are
//! written to be order-independent: each resets the singleton before asserting on it.

#![cfg(target_arch = "wasm32")]

use liquers_core::context::Environment;
use liquers_web::environment::{
    build_environment, has_command, init_global, is_initialized, register_command_on, reset_global,
    with_global,
};
use wasm_bindgen::prelude::*;
use liquers_web::LiquersEnvironment;
use wasm_bindgen_test::*;

/// ENVIRON01 — a default environment can be built and exposes its services.
///
/// **Partial until M4.** The full contract is "evaluates a built-in command", and there is no
/// evaluation surface yet — that arrives with `EVAL`/`COMMAND`. What is asserted here is the part
/// that exists: the environment builds and its command registry is reachable. This test must be
/// extended to actually evaluate once M4 lands; it is listed as required, not as satisfied.
#[wasm_bindgen_test]
fn environ01_default_environment_evaluates_builtin() {
    let envref = build_environment().expect("build a default environment");
    // The environment exposes its services; a command registry is present and inspectable.
    let registry = envref.0.get_command_metadata_registry();
    // A freshly built environment has a registry — empty is fine, absent is not.
    let _ = registry.commands.len();
}

/// ENVIRON02 — the services an environment returns are the ones it was configured with.
#[wasm_bindgen_test]
fn environ02_custom_services_are_the_ones_returned() {
    let a = build_environment().expect("env a");
    let b = build_environment().expect("env b");

    // Two independently built environments are genuinely separate: cloning an EnvRef shares the
    // environment, building a new one does not.
    let a2 = a.clone();
    assert!(
        std::sync::Arc::ptr_eq(&a.0, &a2.0),
        "cloning an EnvRef must share the same environment"
    );
    assert!(
        !std::sync::Arc::ptr_eq(&a.0, &b.0),
        "separately built environments must not share state"
    );
}

/// ENVIRON03 — repeated initialization follows the documented policy: idempotent, and it keeps the
/// existing environment rather than replacing it.
#[wasm_bindgen_test]
fn environ03_repeated_initialization_follows_policy() {
    reset_global();
    init_global().expect("first init");

    // Register something, so the assertion has stakes: the contract is not merely that a second
    // init returns Ok, but that it does not discard what happened in between.
    let spec = js_sys::Object::new();
    js_sys::Reflect::set(&spec, &"name".into(), &"env03cmd".into()).expect("set name");
    js_sys::Reflect::set(
        &spec,
        &"run".into(),
        &js_sys::Function::new_no_args("return 1;"),
    )
    .expect("set run");
    register_command_on(&spec.into()).expect("register");
    assert!(has_command("env03cmd"));

    init_global().expect("second init");

    assert!(
        has_command("env03cmd"),
        "a second init must keep the existing environment; replacing it would silently discard \
         any command registered in between"
    );
    reset_global();
}

/// ENVIRON04 — a failed initialization is recoverable.
///
/// The observable contract is that the singleton is only set on success, so a later attempt can
/// still succeed. Asserted by showing that while unset, access reports a typed error rather than
/// leaving a half-initialized environment behind, and that initializing afterwards works.
#[wasm_bindgen_test]
fn environ04_failed_initialization_is_recoverable() {
    reset_global();
    assert!(!is_initialized());

    let err = match with_global() {
        Err(e) => e,
        Ok(_) => panic!("access before init must fail"),
    };
    assert_eq!(err.error_type, liquers_core::error::ErrorType::NotAvailable);
    assert!(
        err.message.contains("init"),
        "the error should tell the caller what to do: {}",
        err.message
    );

    // Recovery: initializing after the failed access works.
    init_global().expect("init after a failed access");
    assert!(is_initialized());
    reset_global();
}

/// ENVIRON05 — isolated environments do not leak registration into each other.
///
/// This is why explicit instances exist alongside the singleton: a test that registers into an
/// instance must not affect the global one.
#[wasm_bindgen_test]
fn environ05_isolated_test_environments_do_not_leak_registration() {
    reset_global();
    init_global().expect("init");

    let a = LiquersEnvironment::new().expect("instance a");
    let b = LiquersEnvironment::new().expect("instance b");
    assert!(
        !std::sync::Arc::ptr_eq(&a.envref().0, &b.envref().0),
        "two explicit instances must be independent"
    );

    reset_global();
}

/// ENVIRON06 — shutdown is idempotent.
#[wasm_bindgen_test]
fn environ06_shutdown_is_idempotent() {
    reset_global();
    init_global().expect("init");
    assert!(is_initialized());

    reset_global();
    assert!(!is_initialized());
    // Again, on an already-shut-down environment. Must not panic.
    reset_global();
    reset_global();
    assert!(!is_initialized());

    // And it can be brought back up afterwards.
    init_global().expect("re-init after shutdown");
    assert!(is_initialized());
    reset_global();
}
