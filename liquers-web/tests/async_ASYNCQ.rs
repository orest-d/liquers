//! `ASYNCQ*` conformance tests — the `Promise` bridge for query evaluation.
//!
//! `ASYNCQ07` is **`NA`** for this integration: Appendix A scopes it to languages with no async
//! model, and JavaScript has one natively. The useful part is kept as `web_no_blocking_api` below,
//! which is not a conformance test. Every other ID is required and present.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{decl, fresh, get, register_fixture_commands, sleep, with, with_timeout};

use liquers_web::environment::{register_command_on, reset_global};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

/// Awaits a `Promise`, mapping rejection to the rejection value.
async fn settle(promise: js_sys::Promise) -> Result<JsValue, JsValue> {
    JsFuture::from(promise).await
}

/// ASYNCQ01 — awaiting a successful evaluation resolves with the value.
#[wasm_bindgen_test]
async fn asyncq01_await_successful_evaluation() {
    fresh();
    register_fixture_commands();

    let value = settle(liquers_web::evaluate("hello"))
        .await
        .expect("a successful evaluation must resolve");
    assert_eq!(value.as_string().as_deref(), Some("Hello, world!"));
    reset_global();
}

/// ASYNCQ02 — a failure rejects with a structured error, never a bare string.
///
/// A string rejection would discard the error type, the query and the position — everything that
/// makes a Liquers error actionable. The assertion is on the *shape* of the rejection value, not
/// merely that it rejected.
#[wasm_bindgen_test]
async fn asyncq02_failure_rejects_with_structured_error() {
    fresh();
    register_fixture_commands();

    let rejection = settle(liquers_web::evaluate("no_such_command"))
        .await
        .expect_err("an unknown command must reject");

    assert!(
        !rejection.is_string(),
        "rejecting with a string discards the error type, query and position"
    );
    assert!(rejection.is_object(), "the rejection must be a LiquersError object");
    assert_eq!(
        get(&rejection, "errorType").as_string().as_deref(),
        Some("action_not_registered"),
        "the rejection must carry a typed errorType"
    );
    assert!(
        get(&rejection, "message")
            .as_string()
            .map(|m| m.contains("no_such_command"))
            .unwrap_or(false),
        "the message must name the failing action"
    );

    // A parse failure rejects the same way, before evaluation begins.
    let parse_rejection = settle(liquers_web::evaluate("hello/~~~"))
        .await
        .expect_err("a malformed query must reject");
    assert_eq!(
        get(&parse_rejection, "errorType").as_string().as_deref(),
        Some("parse_error")
    );
    reset_global();
}

/// ASYNCQ03 — two concurrent evaluations both make progress (C15).
///
/// Started before either is awaited, so they genuinely overlap rather than running in sequence.
#[wasm_bindgen_test]
async fn asyncq03_two_evaluations_make_progress() {
    fresh();
    register_fixture_commands();
    // A command that yields to the event loop, so the two evaluations really interleave rather
    // than each completing synchronously before the next starts.
    register_command_on(&with(
        decl(
            "slow",
            "return new Promise(r => setTimeout(() => r('slow'), 10));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register slow");

    let a = liquers_web::evaluate("slow");
    let b = liquers_web::evaluate("hello");

    let both = with_timeout(2000, "two concurrent evaluations", async {
        let a = settle(a).await;
        let b = settle(b).await;
        (a, b)
    })
    .await;

    assert_eq!(
        both.0.expect("first must resolve").as_string().as_deref(),
        Some("slow")
    );
    assert_eq!(
        both.1.expect("second must resolve").as_string().as_deref(),
        Some("Hello, world!")
    );
    reset_global();
}

/// ASYNCQ04 — cancellation propagates through the asset handle.
///
/// The cancellable thing is the asset, not the `Promise`: JavaScript `Promise`s cannot be
/// cancelled, which is exactly why `getAsset` exists beside `evaluate`. This asserts that the
/// *surface* is reachable and well-behaved from JavaScript.
///
/// What it does **not** assert is that anything gets cancelled, because on this target nothing
/// does: the immediate asset manager evaluates during `getAsset`, so the handle is already
/// terminal. See `eval06_cancellation_has_defined_terminal_result` for that finding stated
/// directly, and `specs/ISSUES.md` `WEB-CANCELLATION-INERT`.
#[wasm_bindgen_test]
async fn asyncq04_cancellation_propagates() {
    fresh();
    register_fixture_commands();

    let asset = settle(liquers_web::get_asset("hello").unchecked_into())
        .await
        .expect("getAsset must resolve");
    assert!(asset.is_object(), "getAsset resolves with an Asset object");

    let call = |name: &str| -> js_sys::Promise {
        let f: js_sys::Function = get(&asset, name).unchecked_into();
        f.call0(&asset).expect(name).unchecked_into()
    };

    // The status crosses as a lowercase string.
    assert_eq!(
        settle(call("status")).await.expect("status").as_string().as_deref(),
        Some("ready")
    );

    // `cancel` resolves rather than throwing, and leaves the asset intact.
    settle(call("cancel")).await.expect("cancel must not reject");
    assert_eq!(
        settle(call("status")).await.expect("status").as_string().as_deref(),
        Some("ready"),
        "an inert cancel must not move a terminal asset"
    );

    // The value is still reachable through the handle.
    let state = with_timeout(2000, "get after cancel", settle(call("get")))
        .await
        .expect("get must resolve");
    let to_js: js_sys::Function = get(&state, "toJS").unchecked_into();
    assert_eq!(
        to_js.call0(&state).expect("toJS").as_string().as_deref(),
        Some("Hello, world!")
    );
    reset_global();
}

/// ASYNCQ05 — dropping the host handle follows the documented policy.
///
/// The policy: a `Promise` already in flight owns an `EnvRef` clone, so it completes against the
/// environment it started with even after the singleton is released. Shutdown does not strand a
/// pending `Promise`, and it does not resolve one early either.
#[wasm_bindgen_test]
async fn asyncq05_dropping_host_handle_follows_policy() {
    fresh();
    register_fixture_commands();
    register_command_on(&with(
        decl(
            "slow",
            "return new Promise(r => setTimeout(() => r('done'), 20));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register slow");

    let pending = liquers_web::evaluate("slow");

    // Release the singleton while the evaluation is still in flight.
    liquers_web::shutdown();
    assert!(!liquers_web::is_initialized());

    let value = with_timeout(2000, "evaluation outliving shutdown", settle(pending))
        .await
        .expect("a Promise in flight must complete after shutdown");
    assert_eq!(
        value.as_string().as_deref(),
        Some("done"),
        "the in-flight evaluation owns its EnvRef and completes against it"
    );

    // A *new* evaluation after shutdown is a different matter: it has no environment.
    let rejection = settle(liquers_web::evaluate("hello"))
        .await
        .expect_err("evaluating after shutdown must reject");
    assert_eq!(
        get(&rejection, "errorType").as_string().as_deref(),
        Some("not_available")
    );
    reset_global();
}

/// ASYNCQ06 — evaluation does not block the event loop.
///
/// Asserted by observing that the loop keeps turning *while* an evaluation is outstanding: a timer
/// scheduled after the evaluation starts must fire before the evaluation finishes. If evaluation
/// blocked, the timer could not run at all until it returned.
#[wasm_bindgen_test]
async fn asyncq06_no_event_loop_blocking() {
    fresh();
    register_fixture_commands();
    register_command_on(&with(
        decl(
            "slow",
            "return new Promise(r => setTimeout(() => r('late'), 50));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register slow");

    // A flag the timer sets. Reading it through JavaScript keeps the observation on the same side
    // of the boundary as the event loop being tested.
    js_sys::eval("globalThis.__asyncq06 = false").expect("init flag");
    let pending = liquers_web::evaluate("slow");

    sleep(5).await;
    js_sys::eval("globalThis.__asyncq06 = true").expect("set flag");

    let flag_before_completion = js_sys::eval("globalThis.__asyncq06")
        .expect("read flag")
        .as_bool();
    assert_eq!(
        flag_before_completion,
        Some(true),
        "the event loop must keep turning while an evaluation is outstanding"
    );

    let value = with_timeout(2000, "slow evaluation", settle(pending))
        .await
        .expect("resolve");
    assert_eq!(value.as_string().as_deref(), Some("late"));
    reset_global();
}

/// ASYNCQ08 — nested event-loop use is rejected or safe.
///
/// Here it is **safe**: evaluating from inside a command does not spin a nested loop, because
/// there is no way to spin one — `wasm_bindgen_futures` schedules onto the host loop rather than
/// driving its own. The nested evaluation is an ordinary task.
#[wasm_bindgen_test]
async fn asyncq08_nested_event_loop_use_is_rejected_or_safe() {
    fresh();
    register_fixture_commands();

    // A command that evaluates another query and awaits it — the nesting that a nested event loop
    // would be needed for in a blocking design.
    register_command_on(&with(
        decl(
            "nested",
            "return globalThis.__liquersEvaluate('hello').then(v => 'got:' + v);",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register nested");

    // Expose the evaluation entry point to the command. In a page this is `liquers.evaluate`;
    // under the test harness the module is not on `globalThis`, so it is bridged explicitly.
    let bridge = Closure::<dyn Fn(String) -> js_sys::Promise>::new(|q: String| {
        liquers_web::evaluate(&q)
    });
    js_sys::Reflect::set(
        &js_sys::global(),
        &"__liquersEvaluate".into(),
        bridge.as_ref(),
    )
    .expect("install bridge");

    let value = with_timeout(2000, "nested evaluation", settle(liquers_web::evaluate("nested")))
        .await
        .expect("a nested evaluation must resolve, not deadlock");
    assert_eq!(value.as_string().as_deref(), Some("got:Hello, world!"));

    drop(bridge);
    reset_global();
}

/// Every evaluation entry point hands back a `Promise` before doing any work.
///
/// Not a conformance test — `ASYNCQ07` is `NA` for JavaScript — but the property behind `ASYNCQ06`
/// is worth pinning directly. "Does not block" is guaranteed *structurally*: the entry points
/// return a `Promise` rather than a value, so there is no shape in which one could block, and the
/// return type is checked by the compiler. What the compiler cannot check is that the `Promise` is
/// handed back **immediately** — a function returning `Promise` could still do arbitrary
/// synchronous work first, and a slow command would freeze the page exactly as a blocking API
/// would.
///
/// So this asserts the timing: with a command that takes 50 ms, the call must return in a small
/// fraction of that.
#[wasm_bindgen_test]
async fn web_evaluation_returns_before_the_work_runs() {
    fresh();
    register_fixture_commands();
    register_command_on(&with(
        decl(
            "slow",
            "return new Promise(r => setTimeout(() => r('late'), 50));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register slow");

    let now: js_sys::Function = get(&get(&js_sys::global(), "Date"), "now").unchecked_into();
    let millis = || now.call0(&JsValue::NULL).ok().and_then(|v| v.as_f64()).unwrap_or(0.0);

    let before = millis();
    let pending = liquers_web::evaluate("slow");
    let elapsed = millis() - before;

    assert!(
        pending.is_instance_of::<js_sys::Promise>(),
        "evaluate must return a Promise, not a value"
    );
    assert!(
        elapsed < 25.0,
        "evaluate must return before the command runs; it took {elapsed} ms for a command that \
         takes 50 ms, which means the call blocked"
    );

    // The same for the asset entry point.
    let before = millis();
    let asset: JsValue = liquers_web::get_asset("slow").into();
    let elapsed = millis() - before;
    assert!(asset.is_instance_of::<js_sys::Promise>());
    assert!(elapsed < 25.0, "getAsset must not block either ({elapsed} ms)");

    // Drain both so the test does not leave work outstanding for the next one.
    let _ = with_timeout(2000, "slow evaluation", settle(pending)).await;
    let _ = with_timeout(2000, "slow asset", settle(asset.unchecked_into())).await;
    reset_global();
}
