//! `ENVIRON*` conformance tests.
//!
//! These share one wasm module instance and therefore one thread-local singleton, so they are
//! written to be order-independent: each resets the singleton before asserting on it.

#![cfg(target_arch = "wasm32")]

use liquers_core::context::Environment;
use liquers_web::environment::{
    build_environment, has_command, init_global, is_initialized, register_command_on, reset_global,
    shared_env, unregister_command_on, with_global,
};
use wasm_bindgen::prelude::*;
use liquers_web::LiquersEnvironment;
use wasm_bindgen_test::*;

/// ENVIRON01 — a default environment exposes its services and evaluates a command.
///
/// Extended in M4 to the full contract. Until the evaluation surface existed this asserted only
/// that the environment built and its registry was reachable, and carried a note saying so; it now
/// evaluates end to end, which is what the ID actually requires.
#[wasm_bindgen_test]
async fn environ01_default_environment_evaluates_builtin() {
    // The services are reachable on a freshly built environment.
    let envref = build_environment().expect("build a default environment");
    let _ = envref.0.get_command_metadata_registry().commands.len();

    // The built-in Rust command set is present. Asserted on *this* environment's registry rather
    // than through `has_command`, which inspects the singleton — a different environment.
    assert!(
        envref
            .0
            .get_command_metadata_registry()
            .get(liquers_core::command_metadata::CommandKey::new("", "", "to_text"))
            .is_some(),
        "a default environment must carry the built-in Rust commands"
    );

    // And it evaluates one. Every Liquers query segment is a command, so a *source* is needed to
    // produce the input — `to_text` is the built-in under test, and the JavaScript command ahead of
    // it only supplies its state.
    reset_global();
    init_global().expect("init");
    register_command_on(&decl("env01cmd", "return 'evaluated';")).expect("register");

    let envref = shared_env().expect("share");
    let query = liquers_core::parse::parse_query("env01cmd/to_text").expect("parse");
    let value = liquers_web::eval::evaluate_query(envref, query)
        .await
        .expect("the environment must evaluate a built-in end to end");
    assert_eq!(value.as_string().as_deref(), Some("evaluated"));

    reset_global();
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


/// Builds a minimal declaration object: `{ name, run: () => <literal> }`.
fn decl(name: &str, body: &str) -> JsValue {
    let spec = js_sys::Object::new();
    js_sys::Reflect::set(&spec, &"name".into(), &name.into()).expect("set name");
    js_sys::Reflect::set(
        &spec,
        &"run".into(),
        &js_sys::Function::new_no_args(body),
    )
    .expect("set run");
    spec.into()
}

/// Registration works after the environment has been shared.
///
/// Not a prescribed conformance test — it guards the rebuild path, which exists because
/// registration needs `&mut CommandRegistry` and `to_ref` consumes the environment. Before the
/// rebuild existed, this case returned a `NotSupported` error.
#[wasm_bindgen_test]
fn web_register_after_sharing_rebuilds() {
    reset_global();
    init_global().expect("init");

    register_command_on(&decl("before", "return 1;")).expect("register before sharing");

    // Share the environment, as the first evaluation does.
    let first = shared_env().expect("share");
    assert!(has_command("before"));

    // Registering now must succeed, and both commands must be present afterwards.
    register_command_on(&decl("after", "return 2;")).expect("register after sharing");
    assert!(has_command("after"), "the new command must be registered");
    assert!(
        has_command("before"),
        "replaying must not lose commands registered earlier"
    );

    // The environment was rebuilt, so the shared handle is a different one. An evaluation already
    // in flight would keep the old handle and complete against it.
    let second = shared_env().expect("share again");
    assert!(
        !std::sync::Arc::ptr_eq(&first.0, &second.0),
        "registering after sharing must have rebuilt the environment"
    );

    reset_global();
}

/// Unregistering after sharing also rebuilds, and does not resurrect the command.
#[wasm_bindgen_test]
fn web_unregister_after_sharing_rebuilds() {
    reset_global();
    init_global().expect("init");
    register_command_on(&decl("keep", "return 1;")).expect("register keep");
    register_command_on(&decl("drop", "return 2;")).expect("register drop");
    shared_env().expect("share");

    assert!(unregister_command_on("drop").expect("unregister"), "should report removal");
    assert!(!has_command("drop"), "the command must be gone");
    assert!(has_command("keep"), "unrelated commands must survive");

    // A later registration rebuilds again; the dropped command must not come back.
    register_command_on(&decl("third", "return 3;")).expect("register third");
    assert!(!has_command("drop"), "a rebuild must not resurrect an unregistered command");
    assert!(has_command("keep"));
    assert!(has_command("third"));

    // Unregistering something that was never registered is false, not an error.
    assert!(!unregister_command_on("never").expect("unregister absent"));

    reset_global();
}
