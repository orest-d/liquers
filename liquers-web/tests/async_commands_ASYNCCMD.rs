//! `ASYNCCMD*` conformance tests — JavaScript commands that return Promises.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{
    decl, decl_with_params, eval_to_js, fresh, register_fixture_commands, with, with_timeout,
};

use liquers_web::environment::{describe_command_on, register_command_on, reset_global};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

/// Declares an async command whose `run` returns a Promise resolving after `delay` ms.
fn async_decl(name: &str, delay: u32, expression: &str) -> JsValue {
    with(
        decl(
            name,
            &format!("return new Promise(r => setTimeout(() => r({expression}), {delay}));"),
        ),
        "async",
        JsValue::TRUE,
    )
}

/// ASYNCCMD01 — a command returning a Promise has its result awaited.
#[wasm_bindgen_test]
async fn asynccmd01_async_command_result() {
    fresh();
    register_fixture_commands();
    register_command_on(&async_decl("fetched", 10, "'from the network'")).expect("register");

    let value = with_timeout(2000, "async command", eval_to_js("fetched"))
        .await
        .expect("an async command must resolve");
    assert_eq!(
        value.as_string().as_deref(),
        Some("from the network"),
        "the Promise must be awaited, not returned as an opaque value"
    );

    // The same holds when the command is not *declared* async: `IsAsync::Auto` decides per call by
    // testing whether the result is thenable, so a plain function returning a Promise still works.
    register_command_on(&decl(
        "undeclared",
        "return new Promise(r => setTimeout(() => r('auto'), 5));",
    ))
    .expect("register undeclared");
    assert_eq!(
        with_timeout(2000, "auto-detected async", eval_to_js("undeclared"))
            .await
            .expect("resolve")
            .as_string()
            .as_deref(),
        Some("auto")
    );
    reset_global();
}

/// ASYNCCMD02 — a rejected Promise crosses the boundary as a typed error carrying its class and
/// message.
#[wasm_bindgen_test]
async fn asynccmd02_async_exception() {
    fresh();
    register_fixture_commands();

    // Rejection with an Error.
    register_command_on(&with(
        decl(
            "rejects",
            "return new Promise((_, reject) => setTimeout(() => reject(new TypeError('bad input')), 5));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register rejects");

    let err = with_timeout(2000, "rejecting command", eval_to_js("rejects"))
        .await
        .expect_err("a rejected Promise must fail the evaluation");
    assert_eq!(
        err.error_type,
        liquers_core::error::ErrorType::ExecutionError
    );
    assert!(
        err.message.contains("bad input"),
        "the rejection message must survive: {}",
        err.message
    );
    assert!(
        err.message.contains("TypeError"),
        "the error class must survive: {}",
        err.message
    );

    // A throw *inside* an async function is a rejection too, and must behave identically.
    register_command_on(&with(
        with(
            decl_with_params("throws_async", &[], "throw new Error('inside');"),
            "run",
            js_sys::eval("(async () => { throw new Error('inside'); })").expect("async fn"),
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register throws_async");

    let err = with_timeout(2000, "throwing async command", eval_to_js("throws_async"))
        .await
        .expect_err("an async throw must fail the evaluation");
    assert!(
        err.message.contains("inside"),
        "a throw inside an async function must surface: {}",
        err.message
    );
    reset_global();
}

/// ASYNCCMD03 — cancellation in both directions.
///
/// **Rust → JavaScript is not possible, and that is the finding.** There is no way to abort a
/// JavaScript `Promise` from Rust — a property of the language, not a gap here — and on this
/// target there is nothing to abort anyway, because the immediate asset manager has already run
/// the command to completion before the caller holds the asset. So the assertion in that direction
/// is that an async command *runs to completion* regardless of a cancel, rather than being left
/// half-finished or hanging.
///
/// **JavaScript → Rust does work**: a command that fails, for whatever reason including its own
/// cancellation, surfaces as a failure of the evaluation rather than being swallowed.
///
/// See `specs/issues/WEB-CANCELLATION-INERT.md`.
#[wasm_bindgen_test]
async fn asynccmd03_cancellation_in_both_directions() {
    fresh();
    register_fixture_commands();
    register_command_on(&async_decl("slow", 200, "'finished'")).expect("register slow");

    let envref = liquers_web::environment::shared_env().expect("env");
    let query = liquers_core::parse::parse_query("slow").expect("parse");
    let asset = liquers_web::asset::get_asset_for(envref, query)
        .await
        .expect("get asset");
    asset.cancel().await.expect("cancel must not error");

    let state = with_timeout(2000, "async command after cancel", asset.get())
        .await
        .expect("the command must have run to completion, not been left half-finished");
    assert_eq!(
        liquers_web::asset::LiquersState::from_state(state)
            .to_js()
            .expect("value")
            .as_string()
            .as_deref(),
        Some("finished"),
        "an inert cancel must leave the async command's result intact"
    );

    // JavaScript → Rust: a command that reports its own cancellation surfaces as a failure.
    register_command_on(&with(
        decl(
            "self_cancel",
            "return Promise.reject(new Error('aborted by the caller'));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register self_cancel");

    let err = with_timeout(2000, "self-cancelling command", eval_to_js("self_cancel"))
        .await
        .expect_err("a self-cancelling command must fail the evaluation");
    assert!(
        err.message.contains("aborted by the caller"),
        "the reason must reach the caller: {}",
        err.message
    );
    reset_global();
}

/// ASYNCCMD04 — an async command may evaluate a nested query (C2).
#[wasm_bindgen_test]
async fn asynccmd04_nested_async_evaluation() {
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

    register_command_on(&with(
        with(
            decl("outer", ""),
            "run",
            js_sys::eval(
                "(async () => { const inner = await globalThis.__liquersEvaluate('hello/shout'); \
                 return 'outer saw ' + inner; })",
            )
            .expect("async fn"),
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register outer");

    let value = with_timeout(2000, "nested async evaluation", eval_to_js("outer"))
        .await
        .expect("a nested evaluation must resolve, not deadlock");
    assert_eq!(
        value.as_string().as_deref(),
        Some("outer saw HELLO, WORLD!")
    );

    drop(bridge);
    reset_global();
}

/// ASYNCCMD05 — concurrent calls to the same command do not corrupt state.
///
/// Each call gets its own arguments and its own result; interleaving must not let one call's
/// arguments reach another's invocation.
#[wasm_bindgen_test]
async fn asynccmd05_concurrent_calls_do_not_corrupt_state() {
    fresh();
    register_fixture_commands();

    // The delay is inversely proportional to the argument, so the calls finish in the *opposite*
    // order from which they started. If arguments leaked between invocations, the results would
    // pair up with the wrong inputs — which is exactly what this ordering makes visible.
    register_command_on(&with(
        with(
            decl_with_params(
                "delayed",
                &["n"],
                "return new Promise(r => setTimeout(() => r('n=' + n), 60 - n * 10));",
            ),
            "arguments",
            common::args(vec![common::arg("n", Some("int"), None)]),
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register delayed");

    let a = liquers_web::evaluate("delayed-1");
    let b = liquers_web::evaluate("delayed-2");
    let c = liquers_web::evaluate("delayed-3");

    let results = with_timeout(3000, "three concurrent calls", async {
        let a = wasm_bindgen_futures::JsFuture::from(a).await;
        let b = wasm_bindgen_futures::JsFuture::from(b).await;
        let c = wasm_bindgen_futures::JsFuture::from(c).await;
        (a, b, c)
    })
    .await;

    assert_eq!(
        results.0.expect("first").as_string().as_deref(),
        Some("n=1")
    );
    assert_eq!(
        results.1.expect("second").as_string().as_deref(),
        Some("n=2")
    );
    assert_eq!(
        results.2.expect("third").as_string().as_deref(),
        Some("n=3")
    );
    reset_global();
}

/// ASYNCCMD06 — sync and async commands are distinguishable, and the distinction is enforced.
///
/// The enforcement is the substantive half: a command declared synchronous that returns a Promise
/// is **refused**, rather than having the un-awaited Promise silently become its value — which
/// would surface much later as a baffling opaque result.
#[wasm_bindgen_test]
async fn asynccmd06_sync_and_async_metadata_differ() {
    fresh();
    register_fixture_commands();

    register_command_on(&with(
        decl("sync_cmd", "return 1;"),
        "async",
        JsValue::FALSE,
    ))
    .expect("register sync");
    register_command_on(&async_decl("async_cmd", 5, "2")).expect("register async");

    // Both are registered and describable.
    assert!(!describe_command_on("sync_cmd")
        .expect("describe sync")
        .is_null());
    assert!(!describe_command_on("async_cmd")
        .expect("describe async")
        .is_null());

    // Both evaluate correctly.
    assert_eq!(
        eval_to_js("sync_cmd").await.expect("sync").as_f64(),
        Some(1.0)
    );
    assert_eq!(
        with_timeout(2000, "async_cmd", eval_to_js("async_cmd"))
            .await
            .expect("async")
            .as_f64(),
        Some(2.0)
    );

    // Declared sync but returning a Promise: refused with a message that says what to do.
    register_command_on(&with(
        decl("lying", "return Promise.resolve(3);"),
        "async",
        JsValue::FALSE,
    ))
    .expect("register lying");

    let err = eval_to_js("lying")
        .await
        .expect_err("a sync command returning a Promise must be refused");
    assert!(
        err.message.contains("declared synchronous") && err.message.contains("async"),
        "the error must say how to fix it: {}",
        err.message
    );
    reset_global();
}

/// An async command may await work that itself takes several event-loop turns.
///
/// Not a prescribed conformance test — it guards the case where the awaited Promise is not
/// resolved by a single `setTimeout`, which is what a real `fetch` looks like.
#[wasm_bindgen_test]
async fn web_async_command_awaiting_multiple_turns() {
    fresh();
    register_command_on(&with(
        with(
            decl("chained", ""),
            "run",
            js_sys::eval(
                "(async () => { let n = 0; for (let i = 0; i < 3; i++) { \
                 await new Promise(r => setTimeout(r, 2)); n += i; } return n; })",
            )
            .expect("async fn"),
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register chained");

    assert_eq!(
        with_timeout(2000, "multi-turn command", eval_to_js("chained"))
            .await
            .expect("resolve")
            .as_f64(),
        Some(3.0)
    );
    reset_global();
}
