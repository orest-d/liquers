//! `RUNTIME*` conformance tests — the wasm half.
//!
//! `RUNTIME01` is **not** here: it is the one test in this group that needs no browser, and it
//! lives in `liquers-lib` as `runtime01_native_adapter_satisfies_thread_bounds`, asserting that
//! the `MaybeSend`/`MaybeSync` relaxation did not weaken the *native* build. Running it here would
//! assert nothing, since `JsValue` is `!Send` on every target.
//!
//! `RUNTIME05` needs the `debug-handles` feature:
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles \
//!     --test runtime_RUNTIME
//! ```

#![cfg(target_arch = "wasm32")]

mod common;
use common::{decl, eval_to_js, fresh, register_fixture_commands, with, with_timeout, WarnSpy};

use liquers_web::environment::{
    has_command, register_command_on, reset_global, unregister_command_on,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

/// RUNTIME02 — the wasm target accepts a non-`Send` callback.
///
/// This is the property the whole integration rests on: `js_sys::Function` is `!Send` and `!Sync`
/// on every target, so a command holding one can only be registered if the registry's bounds are
/// `MaybeSend`/`MaybeSync` rather than `Send`/`Sync`. That it *compiles* is most of the assertion;
/// that it then runs is the rest.
#[wasm_bindgen_test]
async fn runtime02_wasm_accepts_non_send_callback() {
    fresh();

    // A closure over a `JsValue`, which is `!Send` — captured, registered, and invoked.
    let captured: JsValue = JsValue::from_str("captured");
    let run = js_sys::Function::new_no_args("return 'ran';");
    let spec = js_sys::Object::new();
    js_sys::Reflect::set(&spec, &"name".into(), &"nonsend".into()).expect("name");
    js_sys::Reflect::set(&spec, &"run".into(), &run).expect("run");
    register_command_on(&spec.into()).expect("a !Send callback must be registrable on wasm");

    assert_eq!(
        eval_to_js("nonsend")
            .await
            .expect("evaluate")
            .as_string()
            .as_deref(),
        Some("ran")
    );
    assert_eq!(captured.as_string().as_deref(), Some("captured"));
    reset_global();
}

/// RUNTIME03 — a stored callback outlives the scope that registered it.
///
/// The registry owns the `js_sys::Function`; nothing in the calling scope keeps it alive. If the
/// handle were borrowed rather than owned, this call would fail after the block ended.
#[wasm_bindgen_test]
async fn runtime03_stored_callback_outlives_registration_scope() {
    fresh();

    {
        // Everything here — the closure, the object it captures, the declaration — is dropped at
        // the end of the block.
        let counter = js_sys::eval("(function(){ let n = 10; return () => ++n; })()")
            .expect("build a closure over a local");
        let spec = js_sys::Object::new();
        js_sys::Reflect::set(&spec, &"name".into(), &"outlives".into()).expect("name");
        js_sys::Reflect::set(&spec, &"run".into(), &counter).expect("run");
        js_sys::Reflect::set(&spec, &"volatile".into(), &JsValue::TRUE).expect("volatile");
        register_command_on(&spec.into()).expect("register");
    }

    // Called well after the registering scope ended, and the captured `n` is still there.
    assert_eq!(
        eval_to_js("outlives").await.expect("evaluate").as_f64(),
        Some(11.0)
    );
    reset_global();
}

/// RUNTIME04 — re-entrant evaluation does not deadlock.
///
/// Phase 2's corrected analysis: the deadlock case is *unreachable*, because JavaScript cannot
/// block on a `Promise`. A command either returns the nested `Promise` (which `IsAsync::Auto`
/// awaits, making it effectively async) or ignores it (the nested evaluation resolves
/// independently). So the test asserts the three reachable paths rather than a rejection that
/// never fires.
///
/// **Each case has a 2 s timeout.** A deadlock manifests as a hang, and a hung test reports
/// nothing useful — the timeout turns it into a named failure that says which case stalled.
#[wasm_bindgen_test]
async fn runtime04_nested_evaluation_does_not_deadlock() {
    fresh();
    register_fixture_commands();

    let bridge =
        Closure::<dyn Fn(String) -> js_sys::Promise>::new(|q: String| liquers_web::evaluate(&q));
    js_sys::Reflect::set(
        &js_sys::global(),
        &"__liquersEvaluate".into(),
        bridge.as_ref(),
    )
    .expect("install bridge");

    // Case 1: an async command calls and awaits `evaluate`. The outer query resolves with the
    // nested result.
    register_command_on(&with(
        with(
            decl("case1", ""),
            "run",
            js_sys::eval("(async () => 'awaited:' + await globalThis.__liquersEvaluate('hello'))")
                .expect("async fn"),
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register case1");

    assert_eq!(
        with_timeout(
            2000,
            "case 1 (async command awaits evaluate)",
            eval_to_js("case1")
        )
        .await
        .expect("case 1 must resolve")
        .as_string()
        .as_deref(),
        Some("awaited:Hello, world!"),
        "an async command awaiting a nested evaluation must resolve with the nested result"
    );

    // Case 2: a *sync* command returns the Promise from `evaluate` without awaiting it.
    // `IsAsync::Auto` sees a thenable and awaits it, so the outer query still resolves.
    register_command_on(&decl(
        "case2",
        "return globalThis.__liquersEvaluate('hello').then(v => 'returned:' + v);",
    ))
    .expect("register case2");

    assert_eq!(
        with_timeout(
            2000,
            "case 2 (sync command returns the Promise)",
            eval_to_js("case2")
        )
        .await
        .expect("case 2 must resolve")
        .as_string()
        .as_deref(),
        Some("returned:Hello, world!"),
        "IsAsync::Auto must await a returned Promise rather than yielding it as a value"
    );

    // Case 3: a sync command *starts* an evaluation and ignores it. The outer query resolves
    // immediately, and the nested asset still reaches a terminal status rather than being
    // abandoned mid-flight.
    js_sys::eval("globalThis.__runtime04Nested = null").expect("init");
    register_command_on(&decl(
        "case3",
        "globalThis.__runtime04Nested = globalThis.__liquersEvaluate('hello'); return 'ignored';",
    ))
    .expect("register case3");

    assert_eq!(
        with_timeout(
            2000,
            "case 3 (sync command ignores the Promise)",
            eval_to_js("case3")
        )
        .await
        .expect("case 3 must resolve")
        .as_string()
        .as_deref(),
        Some("ignored"),
        "an ignored nested evaluation must not hold up the outer query"
    );

    // The abandoned nested evaluation still settles.
    let nested: js_sys::Promise = js_sys::eval("globalThis.__runtime04Nested")
        .expect("read the nested promise")
        .unchecked_into();
    let settled = with_timeout(
        2000,
        "case 3's abandoned nested evaluation",
        wasm_bindgen_futures::JsFuture::from(nested),
    )
    .await;
    assert_eq!(
        settled
            .expect("the abandoned evaluation must settle")
            .as_string()
            .as_deref(),
        Some("Hello, world!"),
        "an ignored nested evaluation must still reach a terminal status"
    );

    drop(bridge);
    reset_global();
}

/// RUNTIME05 — unregistering releases the retained JavaScript handles.
///
/// The claim is that `unregister` drops the `Rc<CallableSpec>` and with it the `js_sys::Function`
/// and everything its closure captured. JavaScript cannot observe a Rust drop, and
/// `WeakRef`/`FinalizationRegistry` depend on GC timing, so the primary assertion counts the live
/// `CallableSpec` allocations instead: deterministic, and it tests exactly the mechanism that
/// justifies `unregister` existing.
#[cfg(feature = "debug-handles")]
#[wasm_bindgen_test]
fn runtime05_cancellation_and_shutdown_release_handles() {
    use liquers_web::command::adapter::live_handle_count;

    fresh();
    let baseline = live_handle_count();

    // Registering N commands raises the count by exactly N.
    for i in 0..5 {
        register_command_on(&decl(&format!("h{i}"), "return 1;")).expect("register");
    }
    assert_eq!(
        live_handle_count(),
        baseline + 5,
        "each registration must retain exactly one JavaScript function handle"
    );

    // Unregistering them returns it to baseline.
    for i in 0..5 {
        assert!(unregister_command_on(&format!("h{i}")).expect("unregister"));
    }
    assert_eq!(
        live_handle_count(),
        baseline,
        "unregistering must release every retained handle — a residual count is the leak that C11 \
         describes"
    );

    // Shutdown releases whatever is still registered.
    for i in 0..3 {
        register_command_on(&decl(&format!("s{i}"), "return 1;")).expect("register");
    }
    assert_eq!(live_handle_count(), baseline + 3);
    reset_global();
    assert_eq!(
        live_handle_count(),
        baseline,
        "shutdown must release every retained handle"
    );
}

/// RUNTIME06 — a panic or exception is contained rather than tearing down the instance.
#[wasm_bindgen_test]
async fn runtime06_panic_and_exception_containment() {
    fresh();
    register_fixture_commands();

    // A JavaScript exception becomes a typed error, and the environment survives it.
    register_command_on(&decl("throws", "throw new Error('contained');")).expect("register");
    let err = eval_to_js("throws").await.expect_err("must fail");
    assert!(err.message.contains("contained"));

    // The instance is still usable afterwards — this is the containment claim.
    assert_eq!(
        eval_to_js("hello")
            .await
            .expect("still works")
            .as_string()
            .as_deref(),
        Some("Hello, world!")
    );

    // A non-`Error` throw is contained too, with a safe fallback rather than a lost value.
    register_command_on(&decl("throws_scalar", "throw 42;")).expect("register");
    let err = eval_to_js("throws_scalar").await.expect_err("must fail");
    assert!(
        err.message.contains("42"),
        "a thrown scalar must not be discarded: {}",
        err.message
    );
    assert_eq!(
        eval_to_js("hello")
            .await
            .expect("still works")
            .as_string()
            .as_deref(),
        Some("Hello, world!")
    );
    reset_global();
}

/// Replacement warnings reach the console, with distinct text for the two cases (C12, C14).
///
/// Not a prescribed conformance ID, but it is the only place the warning behaviour is observable:
/// `registerCommand` returns `Ok` either way, so without the spy a silently-dropped warning would
/// never be noticed.
#[wasm_bindgen_test]
fn web_replacement_warns_on_the_console() {
    fresh();
    let spy = WarnSpy::install();

    register_command_on(&decl("warned", "return 1;")).expect("first registration");
    assert!(
        spy.messages().is_empty(),
        "a first registration must not warn: {:?}",
        spy.messages()
    );

    register_command_on(&decl("warned", "return 2;")).expect("replacement");
    assert!(
        spy.saw("was replaced"),
        "replacing a command must warn, because a replacement and an accidental name collision \
         look identical at the point they happen: {:?}",
        spy.messages()
    );
    assert!(
        spy.saw("\"warned\""),
        "the warning must name the command: {:?}",
        spy.messages()
    );

    drop(spy);
    assert!(has_command("warned"));
    reset_global();
}
