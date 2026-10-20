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
use liquers_web::LiquersEnvironment;
use wasm_bindgen::prelude::*;
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
            .get(liquers_core::command_metadata::CommandKey::new(
                "", "", "to_text"
            ))
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
        !std::sync::Arc::ptr_eq(
            &a.resolve().expect("instance a resolves").0,
            &b.resolve().expect("instance b resolves").0
        ),
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
    js_sys::Reflect::set(&spec, &"run".into(), &js_sys::Function::new_no_args(body))
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

/// `fvt8.1` — the environment knows the JavaScript value type.
///
/// `js.Value` is registered by `new_environment`, because `liquers-lib`'s value type cannot
/// describe it statically: the identifier belongs to this crate. Without the registration, storing
/// a retained JavaScript value is refused by the write path
/// (`FOREIGN-VALUE-TYPES-NOT-REGISTERED`).
#[wasm_bindgen_test]
fn fvt8_1_the_environment_knows_the_javascript_type() {
    let envref = build_environment().expect("build a default environment");
    let registry = envref.0.get_type_registry();

    assert!(
        registry.contains(liquers_web::value::JS_VALUE_TYPE_IDENTIFIER),
        "a retained JavaScript value must be a type this build knows"
    );
    assert!(
        registry.contains("Text") && registry.contains("None"),
        "and the base value type's own types must survive the extension, which an empty base \
         registry would have lost"
    );
    assert!(
        !registry.contains("error"),
        "there is no error type: an errored state is typed by the value it holds, which is none"
    );
}

/// `fvt8.2` — a rebuild keeps it.
///
/// The pitfall this guards: registering a type anywhere other than `new_environment` would work
/// until the first command registration *after* the first evaluation, which rebuilds the
/// environment from scratch and replays only what `REGISTERED_SPECS` retained. Siting the
/// registration in `new_environment` is what makes it survive, since both rebuild paths start
/// there.
#[wasm_bindgen_test]
fn fvt8_2_a_rebuild_keeps_the_javascript_type() {
    reset_global();
    init_global().expect("init");

    let first = shared_env().expect("share");
    assert!(
        first
            .0
            .get_type_registry()
            .contains(liquers_web::value::JS_VALUE_TYPE_IDENTIFIER),
        "the premise: the shared environment knows the type"
    );

    // Force a rebuild, exactly as `web_register_after_sharing_rebuilds` does.
    register_command_on(&decl("after", "return 2;")).expect("register after sharing");
    let second = shared_env().expect("share again");
    assert!(
        !std::sync::Arc::ptr_eq(&first.0, &second.0),
        "the premise: the environment really was rebuilt"
    );

    assert!(
        second
            .0
            .get_type_registry()
            .contains(liquers_web::value::JS_VALUE_TYPE_IDENTIFIER),
        "a rebuild must not lose the JavaScript type"
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

    assert!(
        unregister_command_on("drop").expect("unregister"),
        "should report removal"
    );
    assert!(!has_command("drop"), "the command must be gone");
    assert!(has_command("keep"), "unrelated commands must survive");

    // A later registration rebuilds again; the dropped command must not come back.
    register_command_on(&decl("third", "return 3;")).expect("register third");
    assert!(
        !has_command("drop"),
        "a rebuild must not resurrect an unregistered command"
    );
    assert!(has_command("keep"));
    assert!(has_command("third"));

    // Unregistering something that was never registered is false, not an error.
    assert!(!unregister_command_on("never").expect("unregister absent"));

    reset_global();
}

// ---------------------------------------------------------------------------
// Regressions from PR #19 review
// ---------------------------------------------------------------------------

/// ENVIRON07 (part 2 of 2) — the accessors work in every state the lifecycle reaches.
///
/// The window between `init()` and the first evaluation is the one that gets missed, because
/// every test that evaluates first walks straight past it.
///
/// It did not: `init()` populates the *pending* environment and only the first evaluation shares
/// it, so `global()` threw for the whole normal post-initialization window while `isInitialized()`
/// reported true.
#[wasm_bindgen_test]
async fn environ07_accessors_work_immediately_after_init() {
    reset_global();
    init_global().expect("init");
    register_command_on(&decl("g", "return 'from global';")).expect("register");

    assert!(liquers_web::is_initialized());
    let env = LiquersEnvironment::global().expect("global() must work right after init()");

    let value = wasm_bindgen_futures::JsFuture::from(env.evaluate("g"))
        .await
        .expect("the global handle must evaluate");
    assert_eq!(value.as_string().as_deref(), Some("from global"));
    reset_global();
}

/// ENVIRON07 (part 1 of 2) — every documented environment operation is callable.
///
/// `ENVIRON05` asserted only that two instances hold distinct `Arc`s — a Rust-level fact that says
/// nothing about whether JavaScript can do anything with one. The class had no methods at all.
#[wasm_bindgen_test]
async fn environ07_documented_operations_are_callable() {
    reset_global();
    let env = LiquersEnvironment::new().expect("instance");

    // The built-in Rust command set is present on an instance.
    let names = env.command_names();
    assert!(
        names
            .iter()
            .any(|n| n.as_string().as_deref() == Some("to_text")),
        "an instance must carry the built-in commands"
    );
    assert!(!env.describe_command("to_text").expect("describe").is_null());
    assert!(env
        .describe_command("nope")
        .expect("describe absent")
        .is_null());

    // And it evaluates — against its own environment, with no singleton in play.
    assert!(!liquers_web::is_initialized());
    let asset = wasm_bindgen_futures::JsFuture::from(env.get_asset("to_text"))
        .await
        .expect("an instance must resolve a query to an asset");
    assert!(asset.is_object());

    // The planner really consulted *this* instance's registry: an unknown command fails to plan
    // rather than failing somewhere later.
    let err = wasm_bindgen_futures::JsFuture::from(env.evaluate("no_such_command"))
        .await
        .expect_err("an unknown command must not evaluate");
    assert_eq!(
        js_sys::Reflect::get(&err, &"errorType".into())
            .ok()
            .and_then(|v| v.as_string())
            .as_deref(),
        Some("action_not_registered")
    );

    // Registering on an instance is refused with a typed error that says what to do instead,
    // rather than being silently ignored.
    let err = env
        .register_command(decl("x", "return 1;"))
        .expect_err("registration on an instance must be refused");
    let message = js_sys::Reflect::get(&err, &"message".into())
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_default();
    assert!(
        message.contains("liquers.registerCommand"),
        "the refusal must point at the supported path: {message}"
    );

    // Validating a batch is callable with no singleton at all, and does not create one.
    let report = liquers_web::validate_commands(decl("y", "return 1;").unchecked_into(), None);
    assert_eq!(get(&report, "hasErrors").as_bool(), Some(false));
    assert!(!liquers_web::is_initialized());
    reset_global();
}

// ---------------------------------------------------------------------------
// ENVIRON08 — the validation report for imported command metadata
// (`specs/design/web-environ-validation-report/`).
// ---------------------------------------------------------------------------

fn get(value: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(value, &JsValue::from_str(key)).expect("property readable")
}

fn issues(report: &JsValue) -> Vec<JsValue> {
    js_sys::Array::from(&get(report, "issues")).iter().collect()
}

/// The `name` of an issue's command, or `None` for `command: null`.
fn issue_command(issue: &JsValue) -> Option<String> {
    let command = get(issue, "command");
    if command.is_null() {
        None
    } else {
        get(&command, "name").as_string()
    }
}

fn implementations(entries: &[(&str, &str)]) -> JsValue {
    let impls = js_sys::Object::new();
    for (name, body) in entries {
        js_sys::Reflect::set(
            &impls,
            &JsValue::from_str(name),
            &js_sys::Function::new_no_args(body),
        )
        .expect("set implementation");
    }
    impls.into()
}

fn validate(input: &JsValue, impls: &JsValue) -> JsValue {
    liquers_web::validate_commands(
        input.clone().unchecked_into(),
        Some(impls.clone().unchecked_into()),
    )
    .into()
}

fn register(input: &JsValue, impls: &JsValue) -> Result<JsValue, JsValue> {
    liquers_web::register_commands(
        input.clone().unchecked_into(),
        Some(impls.clone().unchecked_into()),
    )
    .map(Into::into)
}

async fn evaluate(query: &str) -> JsValue {
    let envref = shared_env().expect("share");
    let query = liquers_core::parse::parse_query(query).expect("parse");
    liquers_web::eval::evaluate_query(envref, query)
        .await
        .expect("evaluates")
}

const STARVED_REGISTRY: &str = "
commands:
  - name: join
    arguments:
      - { name: xs, argument_type: string, multiple: true }
      - { name: a, argument_type: string }
      - { name: b, argument_type: string }
";

/// ENVIRON08 — imported metadata gets the registry check, and its report lists every finding.
#[wasm_bindgen_test]
fn environ08_serialized_registry_reports_every_issue() {
    reset_global();
    let report = validate(
        &JsValue::from_str(STARVED_REGISTRY),
        &implementations(&[("join", "return '';")]),
    );
    assert_eq!(get(&report, "hasErrors").as_bool(), Some(true));
    assert_eq!(get(&report, "errorCount").as_f64(), Some(2.0));
    let found = issues(&report);
    assert_eq!(found.len(), 2);
    for (issue, starved) in found.iter().zip(["'a'", "'b'"]) {
        assert_eq!(get(issue, "severity").as_string().as_deref(), Some("error"));
        assert_eq!(issue_command(issue).as_deref(), Some("join"));
        let message = get(issue, "message").as_string().unwrap_or_default();
        assert!(message.contains(starved), "{message}");
    }
    let summary = get(&report, "summary").as_string().expect("a summary");
    assert!(
        summary.starts_with("Command metadata contains 2 errors"),
        "{summary}"
    );
}

/// ENVIRON08 — a refused batch registers nothing, and its error carries the full report.
#[wasm_bindgen_test]
fn environ08_refused_batch_registers_nothing_and_carries_the_report() {
    reset_global();
    init_global().expect("init");
    let batch = js_sys::Array::new();
    batch.push(&decl("fine", "return 1;"));
    batch.push(&JsValue::from(42));
    let error = register(&batch.into(), &JsValue::UNDEFINED).expect_err("the batch has an error");

    assert_eq!(
        get(&error, "errorType").as_string().as_deref(),
        Some("parameter_error")
    );
    let report = get(&error, "report");
    assert_eq!(get(&report, "errorCount").as_f64(), Some(1.0));
    assert_eq!(
        get(&error, "message").as_string(),
        get(&report, "summary").as_string(),
        "the compact message is the report's summary"
    );
    assert!(
        !has_command("fine"),
        "all or nothing: the valid command is not registered"
    );
    reset_global();
}

/// ENVIRON08 — a valid batch registers every command through one rebuild and returns its report.
#[wasm_bindgen_test]
async fn environ08_valid_batch_registers_with_one_rebuild() {
    reset_global();
    init_global().expect("init");
    register_command_on(&decl("first", "return 'first';")).expect("register");
    assert_eq!(
        evaluate("first").await.as_string().as_deref(),
        Some("first")
    );
    let before = with_global().expect("shared");

    let batch = js_sys::Array::new();
    for name in ["b1", "b2", "b3"] {
        batch.push(&decl(name, &format!("return '{name}';")));
    }
    let report = register(&batch.into(), &JsValue::UNDEFINED).expect("a valid batch registers");
    assert_eq!(get(&report, "hasErrors").as_bool(), Some(false));
    assert!(issues(&report).is_empty());

    let after = with_global().expect("shared");
    assert!(
        !std::sync::Arc::ptr_eq(&before.0, &after.0),
        "registering after sharing rebuilds"
    );
    for name in ["first", "b1", "b2", "b3"] {
        assert_eq!(evaluate(name).await.as_string().as_deref(), Some(name));
    }
    reset_global();
}

/// ENVIRON08 — in a registry, a command with no implementation is an error and an implementation
/// naming no command is a warning.
#[wasm_bindgen_test]
fn environ08_missing_and_unknown_implementations() {
    reset_global();
    let registry = "commands:\n  - name: c1\n  - name: c2\n";
    let report = validate(
        &JsValue::from_str(registry),
        &implementations(&[("c1", "return 1;"), ("extra", "return 2;")]),
    );
    let found = issues(&report);
    assert_eq!(found.len(), 2, "{:?}", get(&report, "summary"));
    let missing = found
        .iter()
        .find(|i| get(i, "severity").as_string().as_deref() == Some("error"))
        .expect("an error");
    assert_eq!(issue_command(missing).as_deref(), Some("c2"));
    let unknown = found
        .iter()
        .find(|i| get(i, "severity").as_string().as_deref() == Some("warning"))
        .expect("a warning");
    assert!(get(unknown, "message")
        .as_string()
        .unwrap_or_default()
        .contains("extra"));
    assert_eq!(issue_command(unknown), None);
}

/// ENVIRON08 — the JavaScript-specific refusals of a declaration join the same report.
#[wasm_bindgen_test]
fn environ08_declaration_refusals_are_report_entries() {
    reset_global();
    let no_run = js_sys::Object::new();
    js_sys::Reflect::set(&no_run, &"name".into(), &"nofn".into()).expect("set");
    let reserved = decl("w", "return 1;");
    js_sys::Reflect::set(&reserved, &"namespace".into(), &"web".into()).expect("set");
    let batch = js_sys::Array::new();
    batch.push(&JsValue::from(42));
    batch.push(&no_run);
    batch.push(&reserved);

    let report = validate(&batch.into(), &JsValue::UNDEFINED);
    let commands: Vec<Option<String>> = issues(&report).iter().map(issue_command).collect();
    assert_eq!(
        commands,
        vec![None, Some("nofn".to_string()), Some("w".to_string())]
    );
    assert_eq!(get(&report, "errorCount").as_f64(), Some(3.0));

    // A declaration without `run` takes it from the implementations, by name.
    let bound = validate(&no_run.into(), &implementations(&[("nofn", "return 1;")]));
    assert_eq!(get(&bound, "hasErrors").as_bool(), Some(false));
}

/// ENVIRON08 — validation registers nothing and builds no environment, before or after `init()`.
#[wasm_bindgen_test]
async fn environ08_validation_is_pure() {
    reset_global();
    let report = validate(&decl("pure", "return 1;"), &JsValue::UNDEFINED);
    assert_eq!(get(&report, "hasErrors").as_bool(), Some(false));
    assert!(!is_initialized(), "validation must not initialize");

    init_global().expect("init");
    register_command_on(&decl("seed", "return 1;")).expect("register");
    evaluate("seed").await;
    let before = with_global().expect("shared");
    validate(&decl("pure", "return 1;"), &JsValue::UNDEFINED);
    let after = with_global().expect("shared");
    assert!(
        std::sync::Arc::ptr_eq(&before.0, &after.0),
        "validation must not rebuild"
    );
    assert!(!has_command("pure"));
    reset_global();
}

/// ENVIRON08 — `registerCommand` is a batch of one: the same compact message, now with `report`.
#[wasm_bindgen_test]
fn environ08_register_command_is_a_batch_of_one() {
    reset_global();
    init_global().expect("init");
    let spec = decl("join", "return '';");
    let arguments = js_sys::Array::new();
    for (name, multiple) in [("xs", true), ("a", false)] {
        let argument = js_sys::Object::new();
        js_sys::Reflect::set(&argument, &"name".into(), &name.into()).expect("set");
        js_sys::Reflect::set(&argument, &"multiple".into(), &multiple.into()).expect("set");
        arguments.push(&argument);
    }
    js_sys::Reflect::set(&spec, &"arguments".into(), &arguments).expect("set");

    let error = liquers_web::register_command(spec.unchecked_into()).expect_err("refused");
    assert_eq!(
        get(&error, "message").as_string().as_deref(),
        Some(
            "Command metadata contains 1 error; first: Argument 'a' is starved by multiple \
             argument 'xs' on --join"
        )
    );
    assert_eq!(issues(&get(&error, "report")).len(), 1);
    reset_global();
}

/// ENVIRON08 — commands imported as metadata are replayed when the environment rebuilds.
#[wasm_bindgen_test]
async fn environ08_registry_form_commands_survive_a_rebuild() {
    reset_global();
    init_global().expect("init");
    let impls = js_sys::Object::new();
    js_sys::Reflect::set(
        &impls,
        &"imported".into(),
        &js_sys::Function::new_with_args("who", "return `Hello, ${who}!`;"),
    )
    .expect("set implementation");
    register(
        &JsValue::from_str(
            "commands:\n  - name: imported\n    arguments:\n      - { name: who, argument_type: string }\n",
        ),
        &impls.into(),
    )
    .expect("the registry imports");
    let hello = || async { evaluate("imported-world").await.as_string() };
    assert_eq!(hello().await.as_deref(), Some("Hello, world!"));

    // Registering after sharing rebuilds, which replays the imported command.
    register_command_on(&decl("later", "return 'later';")).expect("register");
    assert_eq!(hello().await.as_deref(), Some("Hello, world!"));
    assert_eq!(
        evaluate("later").await.as_string().as_deref(),
        Some("later")
    );

    // And `unregisterCommand` removes it from what a rebuild replays.
    assert!(unregister_command_on("imported").expect("unregister"));
    assert!(!has_command("imported"));
    reset_global();
}

/// ENVIRON08 — `{ run, state?, async? }` binds a declaration as it binds imported metadata.
#[wasm_bindgen_test]
async fn environ08_object_form_implementation_binds_a_declaration() {
    reset_global();
    init_global().expect("init");
    let declaration = js_sys::Object::new();
    js_sys::Reflect::set(&declaration, &"name".into(), &"wrapped".into()).expect("set");
    let wrapper = js_sys::Object::new();
    js_sys::Reflect::set(
        &wrapper,
        &"run".into(),
        &js_sys::Function::new_no_args("return 'wrapped';"),
    )
    .expect("set");
    js_sys::Reflect::set(&wrapper, &"async".into(), &JsValue::FALSE).expect("set");
    let impls = js_sys::Object::new();
    js_sys::Reflect::set(&impls, &"wrapped".into(), &wrapper).expect("set");

    let report = register(&declaration.into(), &impls.into()).expect("the declaration binds");
    assert_eq!(get(&report, "hasErrors").as_bool(), Some(false));
    assert_eq!(
        evaluate("wrapped").await.as_string().as_deref(),
        Some("wrapped")
    );
    reset_global();
}

/// ENVIRON08 — an object-form implementation is copied, so changing it later does not change
/// what a rebuild replays.
#[wasm_bindgen_test]
async fn environ08_object_form_implementation_is_snapshotted() {
    reset_global();
    init_global().expect("init");
    let wrapper = js_sys::Object::new();
    js_sys::Reflect::set(
        &wrapper,
        &"run".into(),
        &js_sys::Function::new_no_args("return 'original';"),
    )
    .expect("set");
    let impls = js_sys::Object::new();
    js_sys::Reflect::set(&impls, &"snap".into(), &wrapper).expect("set");
    register(
        &JsValue::from_str("commands:\n  - name: snap\n"),
        &impls.into(),
    )
    .expect("the registry imports");
    assert_eq!(
        evaluate("snap").await.as_string().as_deref(),
        Some("original")
    );

    js_sys::Reflect::set(&wrapper, &"run".into(), &JsValue::from(42)).expect("mutate");
    register_command_on(&decl("later", "return 'later';")).expect("a rebuild still succeeds");
    assert_eq!(
        evaluate("snap").await.as_string().as_deref(),
        Some("original")
    );
    reset_global();
}

/// ENVIRON08 — a registry without `commands` is an error, not an empty batch.
#[wasm_bindgen_test]
fn environ08_registry_without_commands_is_refused() {
    reset_global();
    let report = validate(
        &JsValue::from_str("commandz:\n  - name: c1\n"),
        &implementations(&[("c1", "return 1;")]),
    );
    assert_eq!(get(&report, "hasErrors").as_bool(), Some(true));
    let summary = get(&report, "summary").as_string().unwrap_or_default();
    assert!(summary.contains("commands"), "{summary}");
}
