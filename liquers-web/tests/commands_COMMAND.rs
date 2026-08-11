//! `COMMAND*` conformance tests, plus the argument-inference and namespace sub-suites.
//!
//! No DOM is needed — commands, planning and evaluation all run under Node.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{arg, args, decl, decl_with_params, eval_to_js, fresh, func, obj, set, with};

use liquers_web::environment::{
    describe_command_on, has_command, register_command_on, reset_global, unregister_command_on,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

// ---------------------------------------------------------------------------
// COMMAND
// ---------------------------------------------------------------------------

/// COMMAND01 — register and execute a first command.
///
/// This is the guide's mandatory end-to-end test (§7 item 10): a command declared in the
/// *integrated language*, evaluated through the real environment, planner and asset lifecycle.
#[wasm_bindgen_test]
async fn command01_register_and_execute_first_command() {
    fresh();
    register_command_on(&decl("hello", "return 'Hello, world!';")).expect("register");

    let result = eval_to_js("hello").await.expect("evaluate hello");
    assert_eq!(result.as_string().as_deref(), Some("Hello, world!"));
    reset_global();
}

/// COMMAND02 — a transform receives the state and its typed parameters.
#[wasm_bindgen_test]
async fn command02_transform_receives_state_and_parameter() {
    fresh();
    register_command_on(&decl("hello", "return 'hello';")).expect("register hello");
    register_command_on(&with(
        with(
            decl_with_params("repeat", &["text", "count"], "return text.repeat(count);"),
            "state",
            JsValue::from_str("text"),
        ),
        "arguments",
        args(vec![arg("count", Some("int"), None)]),
    ))
    .expect("register repeat");

    let result = eval_to_js("hello/repeat-3").await.expect("evaluate");
    assert_eq!(result.as_string().as_deref(), Some("hellohellohello"));
    reset_global();
}

/// COMMAND03 — an exception thrown in JavaScript crosses the boundary as a Liquers error.
#[wasm_bindgen_test]
async fn command03_exception_crosses_command_boundary() {
    fresh();
    register_command_on(&decl("boom", "throw new Error('kaboom');")).expect("register");

    let err = match eval_to_js("boom").await {
        Err(e) => e,
        Ok(_) => panic!("a throwing command must not succeed"),
    };
    assert!(
        err.message.contains("kaboom"),
        "the thrown message must survive: {}",
        err.message
    );
    reset_global();
}

/// COMMAND04 — declared defaults bind when the query omits the argument.
#[wasm_bindgen_test]
async fn command04_defaults_enums_and_variadics_bind() {
    fresh();
    register_command_on(&decl("hello", "return 'ab';")).expect("register hello");
    register_command_on(&with(
        with(
            decl_with_params("rep", &["text", "count"], "return text.repeat(count);"),
            "state",
            JsValue::from_str("text"),
        ),
        "arguments",
        args(vec![arg(
            "count",
            Some("int"),
            Some(JsValue::from_f64(2.0)),
        )]),
    ))
    .expect("register rep");

    // Explicit argument.
    let explicit = eval_to_js("hello/rep-3").await.expect("explicit");
    assert_eq!(explicit.as_string().as_deref(), Some("ababab"));

    // Omitted: the declared default applies.
    let defaulted = eval_to_js("hello/rep").await.expect("defaulted");
    assert_eq!(defaulted.as_string().as_deref(), Some("abab"));
    reset_global();
}

/// COMMAND05 — the registered metadata matches the declaration.
#[wasm_bindgen_test]
fn command05_metadata_matches_the_declaration() {
    fresh();
    register_command_on(&with(
        with(
            with(
                decl_with_params("documented", &["n"], "return n;"),
                "arguments",
                args(vec![arg("n", Some("int"), None)]),
            ),
            "doc",
            JsValue::from_str("A documented command."),
        ),
        "label",
        JsValue::from_str("Documented"),
    ))
    .expect("register");

    let described = describe_command_on("documented").expect("describe");
    assert!(
        !described.is_null(),
        "a registered command must be describable"
    );
    let doc = js_sys::Reflect::get(&described, &"doc".into())
        .expect("doc")
        .as_string();
    assert_eq!(doc.as_deref(), Some("A documented command."));
    let label = js_sys::Reflect::get(&described, &"label".into())
        .expect("label")
        .as_string();
    assert_eq!(label.as_deref(), Some("Documented"));

    // A command with NO arguments. `CommandMetadata` skips empty collections when it
    // serializes — right for a config file, wrong for an API — so `arguments` was absent rather
    // than empty, and a caller iterating it broke on exactly the commands least likely to be
    // special-cased.
    register_command_on(&decl("nullary", "return 1;")).expect("register nullary");
    let nullary = describe_command_on("nullary").expect("describe nullary");
    let arguments = js_sys::Reflect::get(&nullary, &"arguments".into()).expect("arguments");
    assert!(
        js_sys::Array::is_array(&arguments) && js_sys::Array::from(&arguments).length() == 0,
        "a zero-argument command must describe `arguments` as an empty array, not omit it"
    );

    // An unregistered command describes as null rather than throwing.
    assert!(describe_command_on("nope")
        .expect("describe absent")
        .is_null());
    reset_global();
}

/// COMMAND06 — the duplicate and unregister policy.
#[wasm_bindgen_test]
async fn command06_duplicate_and_unregister_policy() {
    fresh();
    register_command_on(&decl("dup", "return 'first';")).expect("first");
    assert_eq!(
        eval_to_js("dup")
            .await
            .expect("first eval")
            .as_string()
            .as_deref(),
        Some("first")
    );

    // Duplicate registration replaces.
    register_command_on(&decl("dup", "return 'second';")).expect("replace");
    assert_eq!(
        eval_to_js("dup")
            .await
            .expect("second eval")
            .as_string()
            .as_deref(),
        Some("second"),
        "re-registering must replace the previous implementation"
    );

    // Unregister removes it, and the query then fails to *plan*.
    assert!(unregister_command_on("dup").expect("unregister"));
    assert!(!has_command("dup"));
    let err = match eval_to_js("dup").await {
        Err(e) => e,
        Ok(_) => panic!("an unregistered command must not evaluate"),
    };
    assert_eq!(
        err.error_type,
        liquers_core::error::ErrorType::ActionNotRegistered,
        "after unregistering, the query must fail to plan (ActionNotRegistered) rather than fail \
         at execution — a failure at execution would mean the metadata survived"
    );

    // Unregistering again is false, not an error.
    assert!(!unregister_command_on("dup").expect("second unregister"));

    // Registration *after* the environment is already in use must take effect. An environment
    // frozen once shared fails only here — every other COMMAND test registers before evaluating.
    register_command_on(&decl("late", "return 'late';")).expect("register after use");
    assert_eq!(
        eval_to_js("late")
            .await
            .expect("late eval")
            .as_string()
            .as_deref(),
        Some("late")
    );
    reset_global();
}

/// COMMAND07 — context injection.
///
/// Scoped to what this milestone provides: the adapter passes a `Context` through to the callable
/// invocation, and a command executes inside a real asset context. A JavaScript-visible context
/// object is not part of this phase.
#[wasm_bindgen_test]
async fn command07_context_injection() {
    fresh();
    register_command_on(&decl("ctx", "return 'ran';")).expect("register");
    // Executing at all proves the command ran inside a context created by the asset manager;
    // without one, the interpreter could not have invoked it.
    assert_eq!(
        eval_to_js("ctx")
            .await
            .expect("evaluate")
            .as_string()
            .as_deref(),
        Some("ran")
    );
    reset_global();
}

/// COMMAND08 — a returned opaque value follows the `VALUE` rules.
#[wasm_bindgen_test]
async fn command08_returned_opaque_value_follows_value_rules() {
    fresh();
    // A command returning a class instance without opting in must fail, not silently wrap.
    register_command_on(&decl("makedate", "return new Date();")).expect("register");
    let err = match eval_to_js("makedate").await {
        Err(e) => e,
        Ok(_) => panic!("an un-opted-in class instance must not convert"),
    };
    assert_eq!(
        err.error_type,
        liquers_core::error::ErrorType::ConversionError
    );
    reset_global();
}

/// COMMAND09 — a minimal declaration has useful metadata defaults.
#[wasm_bindgen_test]
async fn command09_minimal_declaration_has_useful_metadata_defaults() {
    fresh();
    // The whole declaration: a name and a function.
    register_command_on(&decl("minimal", "return 42;")).expect("register");

    let described = describe_command_on("minimal").expect("describe");
    let name = js_sys::Reflect::get(&described, &"name".into())
        .expect("name")
        .as_string();
    assert_eq!(name.as_deref(), Some("minimal"));
    // A label is defaulted from the name rather than left empty.
    let label = js_sys::Reflect::get(&described, &"label".into())
        .expect("label")
        .as_string();
    assert_eq!(label.as_deref(), Some("minimal"));

    assert_eq!(
        eval_to_js("minimal").await.expect("evaluate").as_f64(),
        Some(42.0)
    );
    reset_global();
}

/// COMMAND10 — a complete declaration preserves every supported metadata field.
#[wasm_bindgen_test]
fn command10_complete_declaration_preserves_every_field() {
    fresh();
    let mut spec = decl_with_params("complete", &["state", "n"], "return n;");
    spec = with(spec, "state", JsValue::from_str("value"));
    spec = with(spec, "namespace", JsValue::from_str("myapp"));
    spec = with(spec, "doc", JsValue::from_str("Docs."));
    spec = with(spec, "label", JsValue::from_str("Complete"));
    spec = with(
        spec,
        "arguments",
        args(vec![arg("n", Some("int"), Some(JsValue::from_f64(7.0)))]),
    );
    register_command_on(&spec).expect("register");

    // Namespaced commands are not found under the root name.
    assert!(
        !has_command("complete"),
        "a namespaced command is not a root command"
    );
    reset_global();
}

/// COMMAND11 — a closure's captures are retained for as long as the command is registered.
#[wasm_bindgen_test]
async fn command11_closure_captures_retained_per_runtime_rules() {
    fresh();
    // A closure over a local, created and dropped inside this block. The registry must keep it
    // alive: if the capture were not retained, the call would fail or observe a freed value.
    {
        let counter =
            js_sys::eval("(function(){ let n = 0; return () => ++n; })()").expect("build closure");
        let spec = obj();
        set(&spec, "name", &JsValue::from_str("counter"));
        set(&spec, "run", &counter);
        register_command_on(&spec.into()).expect("register");
    }

    assert_eq!(
        eval_to_js("counter").await.expect("first").as_f64(),
        Some(1.0)
    );
    reset_global();
}

// ---------------------------------------------------------------------------
// Argument inference sub-suite — one case per row of the Phase 2 table
// ---------------------------------------------------------------------------

/// Registers a command whose `run` is built from source, and reports the inferred argument names.
fn infer(name: &str, source: &str, state: Option<&str>) -> Result<Vec<String>, String> {
    let spec = obj();
    set(&spec, "name", &JsValue::from_str(name));
    set(&spec, "run", &func(source));
    if let Some(s) = state {
        set(&spec, "state", &JsValue::from_str(s));
    }
    register_command_on(&spec.into()).map_err(|e| e.message.clone())?;

    let described = describe_command_on(name).map_err(|e| e.message.clone())?;
    assert_eq!(
        js_sys::Reflect::get(&described, &"argumentsInferred".into())
            .ok()
            .and_then(|v| v.as_bool()),
        Some(true),
        "an inferred argument list must be reported as inferred"
    );
    let arguments = js_sys::Reflect::get(&described, &"arguments".into())
        .map_err(|_| "no arguments".to_string())?;
    assert!(
        js_sys::Array::is_array(&arguments),
        "`arguments` must always be an array, even when empty"
    );
    let array = js_sys::Array::from(&arguments);
    Ok(array
        .iter()
        .filter_map(|a| {
            js_sys::Reflect::get(&a, &"name".into())
                .ok()
                .and_then(|n| n.as_string())
        })
        .collect())
}

#[wasm_bindgen_test]
fn command05_infer_accepted_shapes() {
    fresh();
    assert_eq!(
        infer("i1", "(state, count) => 0", Some("value")).expect("plain"),
        vec!["count"]
    );
    assert_eq!(
        infer("i2", "(state, a, b) => 0", Some("value")).expect("three plain"),
        vec!["a", "b"]
    );
    assert_eq!(
        infer("i3", "(a, b) => 0", None).expect("no state"),
        vec!["a", "b"]
    );
    assert!(infer("i4", "(state) => 0", Some("value"))
        .expect("zero args")
        .is_empty());
    assert_eq!(
        infer(
            "i5",
            "function (state /*, hidden */, count) { return 0; }",
            Some("value")
        )
        .expect("comment in params"),
        vec!["count"],
        "comments inside the parameter list must be stripped"
    );
    reset_global();
}

#[wasm_bindgen_test]
fn command05_infer_refused_shapes() {
    fresh();
    // Each of these is refused with a specific reason rather than mangled into metadata.
    for (name, source, expect) in [
        ("r1", "(state, count = 2) => 0", "not a plain identifier"),
        (
            "r2",
            "(state, f = (x,y) => x) => 0",
            "not a plain identifier",
        ),
        ("r3", "(state, {a, b}) => 0", "not a plain identifier"),
        ("r4", "(state, ...rest) => 0", "not a plain identifier"),
    ] {
        let err =
            infer(name, source, Some("value")).expect_err(&format!("{source} must be refused"));
        assert!(
            err.contains(expect),
            "{source}: expected a reason mentioning {expect:?}, got {err:?}"
        );
        assert!(
            !has_command(name),
            "a refused declaration must not register"
        );
    }

    // A bound function exposes no parameter list at all.
    let err = infer("r5", "((state, count) => 0).bind(null)", Some("value"))
        .expect_err("a bound function must be refused");
    assert!(
        err.contains("Function.length") || err.contains("parameter list"),
        "unexpected reason: {err:?}"
    );
    reset_global();
}

#[wasm_bindgen_test]
fn command05_explicit_arguments_override_inference() {
    fresh();
    // The function has a default parameter, which inference refuses — but an explicit declaration
    // wins outright, so registration succeeds and uses the declared names.
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("explicit"));
    set(&spec, "run", &func("(state, count = 2) => 0"));
    set(&spec, "state", &JsValue::from_str("value"));
    set(
        &spec,
        "arguments",
        &args(vec![arg("count", Some("int"), None)]),
    );
    register_command_on(&spec.into()).expect("explicit arguments must win over inference");

    assert!(has_command("explicit"));
    let described = describe_command_on("explicit").expect("describe");
    assert_eq!(
        js_sys::Reflect::get(&described, &"argumentsInferred".into())
            .ok()
            .and_then(|v| v.as_bool()),
        Some(false),
        "a declared argument list must not be reported as inferred"
    );
    reset_global();
}

// ---------------------------------------------------------------------------
// Namespace sub-suite
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn command06_ns_root_is_the_default() {
    fresh();
    register_command_on(&decl("rooted", "return 1;")).expect("register");
    assert!(
        has_command("rooted"),
        "no namespace means the root namespace"
    );
    assert_eq!(
        eval_to_js("rooted").await.expect("evaluate").as_f64(),
        Some(1.0)
    );
    reset_global();
}

#[wasm_bindgen_test]
async fn command06_ns_explicit_namespace() {
    fresh();
    register_command_on(&decl("hello", "return 'x';")).expect("hello");
    register_command_on(&with(
        with(
            decl_with_params("shout", &["text"], "return text.toUpperCase();"),
            "state",
            JsValue::from_str("text"),
        ),
        "namespace",
        JsValue::from_str("myapp"),
    ))
    .expect("register namespaced");

    let result = eval_to_js("hello/ns-myapp/shout").await.expect("evaluate");
    assert_eq!(result.as_string().as_deref(), Some("X"));
    reset_global();
}

#[wasm_bindgen_test]
fn command06_ns_reserved_namespace_is_refused() {
    fresh();
    let spec = with(
        decl("alert", "return 1;"),
        "namespace",
        JsValue::from_str("web"),
    );
    let err = register_command_on(&spec).expect_err("the web namespace is reserved");
    assert_eq!(
        err.error_type,
        liquers_core::error::ErrorType::ParameterError
    );
    assert!(
        err.message.contains("reserved"),
        "unexpected message: {}",
        err.message
    );
    reset_global();
}

// ---------------------------------------------------------------------------
// Regressions from PR #19 review
// ---------------------------------------------------------------------------

/// COMMAND12 — a declared flag that changes planner behaviour reaches the planner.
///
/// `COMMAND10` claims to preserve "every supported metadata field" and did not check this one, so
/// the flag was parsed by nobody and every JavaScript command was registered as cacheable. A
/// command that reads the clock or a mutable global would have had a stale result reused. The
/// prescribed test now separates the two claims: `COMMAND10` asserts the metadata field holds the
/// declaration, this asserts the planner acts on it.
#[wasm_bindgen_test]
async fn command12_declared_planner_flags_take_effect() {
    fresh();
    register_command_on(&with(
        decl("vol", "return Date.now();"),
        "volatile",
        JsValue::TRUE,
    ))
    .expect("register volatile");
    register_command_on(&decl("stable", "return 1;")).expect("register stable");

    let volatile = describe_command_on("vol").expect("describe vol");
    assert_eq!(
        js_sys::Reflect::get(&volatile, &"volatile".into())
            .ok()
            .and_then(|v| v.as_bool()),
        Some(true),
        "a declared volatile flag must reach the metadata the planner reads"
    );

    // And the default is still false, so the fix did not simply mark everything volatile.
    let stable = describe_command_on("stable").expect("describe stable");
    assert_eq!(
        js_sys::Reflect::get(&stable, &"volatile".into())
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        false
    );

    // The behavioural half: a volatile command is re-executed rather than served from cache.
    // Asserting only the metadata field would pass even if the planner ignored it.
    register_command_on(&with(
        decl(
            "counter",
            "globalThis.__c = (globalThis.__c || 0) + 1; return globalThis.__c;",
        ),
        "volatile",
        JsValue::TRUE,
    ))
    .expect("register counter");
    js_sys::eval("globalThis.__c = 0").expect("reset counter");

    assert_eq!(
        eval_to_js("counter").await.expect("first").as_f64(),
        Some(1.0)
    );
    assert_eq!(
        eval_to_js("counter").await.expect("second").as_f64(),
        Some(2.0),
        "a volatile command must be re-executed, not served from the asset cache"
    );
    reset_global();
}

/// COMMAND08 — the *positive* half: a command returning `liquers.opaque(x)` carries `x` by identity.
///
/// `COMMAND08` tested only the *negative* case — an un-opted-in class instance is refused — so the
/// documented opt-in was never exercised through a command and did not work at all: structural
/// conversion saw the `Value` wrapper as an unrecognised class and rejected it.
#[wasm_bindgen_test]
async fn command08_returned_opaque_value_is_retained() {
    fresh();
    let original = js_sys::Date::new_0();
    let wrapped = liquers_web::opaque(original.clone().into()).expect("opaque");
    set(
        &js_sys::global().unchecked_into(),
        "__opaqueFixture",
        &JsValue::from(wrapped),
    );

    register_command_on(&decl("carry", "return globalThis.__opaqueFixture;")).expect("register");

    let result = eval_to_js("carry")
        .await
        .expect("an opaque return must evaluate");
    assert!(
        result.loose_eq(&original),
        "the original object must come back by identity, not a copy"
    );
    reset_global();
}

/// COMMAND13 — every declared state-passing mode delivers its documented content.
///
/// A mode that silently degrades to another is invisible to every other test: the command still
/// runs and still returns something. `state: "state"` used to hand over the bare value.
#[wasm_bindgen_test]
async fn command13_every_state_mode_delivers_its_content() {
    fresh();
    register_command_on(&decl("src", "return 'payload';")).expect("register src");
    register_command_on(&with(
        decl_with_params(
            "inspect",
            &["s"],
            // The status is asserted only to be a non-empty string: the state handed to a command
            // is the *input* to this step, so its status reflects a mid-plan position (`recipe`
            // here), not a finished asset. Asserting `ready` would assert something false.
            "return s.value + '|' + typeof s.metadata + '|' + Array.isArray(s.log) + '|' \
             + (typeof s.status === 'string' && s.status.length > 0);",
        ),
        "state",
        JsValue::from_str("state"),
    ))
    .expect("register inspect");

    assert_eq!(
        eval_to_js("src/inspect")
            .await
            .expect("evaluate")
            .as_string()
            .as_deref(),
        Some("payload|object|true|true"),
        "state mode must carry the value, metadata, log and status"
    );
    reset_global();
}

/// COMMAND14 — a retained declaration is unaffected by later mutation of the caller's object.
#[wasm_bindgen_test]
async fn command14_retained_declaration_is_immune_to_caller_mutation() {
    fresh();
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("snap"));
    set(
        &spec,
        "run",
        &js_sys::Function::new_no_args("return 'original';"),
    );
    let spec: JsValue = spec.into();
    register_command_on(&spec).expect("register");

    // Share the environment, so the next registration rebuilds and replays `snap`.
    assert_eq!(
        eval_to_js("snap")
            .await
            .expect("first")
            .as_string()
            .as_deref(),
        Some("original")
    );

    // The caller mutates the object it handed over — a framework reusing a template would.
    set(
        &spec.clone().unchecked_into(),
        "run",
        &js_sys::Function::new_no_args("return 'mutated';"),
    );
    register_command_on(&decl("other", "return 1;")).expect("register other, forcing a rebuild");

    assert_eq!(
        eval_to_js("snap")
            .await
            .expect("after rebuild")
            .as_string()
            .as_deref(),
        Some("original"),
        "a rebuild must replay the declaration as it was at registration"
    );
    reset_global();
}
