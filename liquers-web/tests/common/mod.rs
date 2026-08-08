//! Shared test fixtures.
//!
//! Phase 3, "Test Infrastructure Requirements" #2: the fixture commands are defined **once** so
//! every suite exercises identical definitions. A suite that redefined `repeat` slightly
//! differently would be testing its own copy, not the thing under test.
//!
//! Also here: the `console.warn` spy (#1), which `wasm-bindgen-test` does not provide.

#![cfg(target_arch = "wasm32")]
#![allow(dead_code)] // Not every suite uses every helper.

use liquers_web::environment::{init_global, register_command_on, reset_global, shared_env};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Declaration helpers
// ---------------------------------------------------------------------------

pub fn obj() -> js_sys::Object {
    js_sys::Object::new()
}

pub fn set(o: &js_sys::Object, k: &str, v: &JsValue) {
    js_sys::Reflect::set(o, &JsValue::from_str(k), v).expect("set property");
}

pub fn get(v: &JsValue, k: &str) -> JsValue {
    js_sys::Reflect::get(v, &JsValue::from_str(k)).expect("get property")
}

/// `{ name, run }` where `run` is built from a body with the given parameter names.
pub fn decl_with_params(name: &str, params: &[&str], body: &str) -> JsValue {
    let spec = obj();
    set(&spec, "name", &JsValue::from_str(name));
    set(
        &spec,
        "run",
        &js_sys::Function::new_with_args(&params.join(","), body),
    );
    spec.into()
}

/// The minimal declaration: a source command with no parameters.
pub fn decl(name: &str, body: &str) -> JsValue {
    decl_with_params(name, &[], body)
}

/// Adds a property to an existing declaration.
pub fn with(spec: JsValue, key: &str, value: JsValue) -> JsValue {
    let o: js_sys::Object = spec.unchecked_into();
    set(&o, key, &value);
    o.into()
}

/// An `arguments` array entry.
pub fn arg(name: &str, ty: Option<&str>, default: Option<JsValue>) -> JsValue {
    let a = obj();
    set(&a, "name", &JsValue::from_str(name));
    if let Some(t) = ty {
        set(&a, "type", &JsValue::from_str(t));
    }
    if let Some(d) = default {
        set(&a, "default", &d);
    }
    a.into()
}

pub fn args(list: Vec<JsValue>) -> JsValue {
    let a = js_sys::Array::new();
    for item in list {
        a.push(&item);
    }
    a.into()
}

/// A function from source text, so the exact parameter text is under test.
///
/// Wrapped in parentheses so a `function (...)` expression is not parsed as a statement, which
/// would require a name.
pub fn func(source: &str) -> js_sys::Function {
    let value = js_sys::eval(&format!("({source})")).expect("eval a function literal");
    assert!(value.is_function(), "not a function: {source}");
    value.unchecked_into()
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// A clean, initialized global environment.
///
/// The whole test binary shares one wasm instance and therefore one thread-local singleton, so
/// every test must start from a known state rather than inherit whatever ran before it.
pub fn fresh() {
    reset_global();
    init_global().expect("init");
}

/// Evaluates a query on the global environment to a JavaScript value.
pub async fn eval_to_js(query: &str) -> Result<JsValue, liquers_core::error::Error> {
    let envref = shared_env()?;
    let parsed = liquers_core::parse::parse_query(query)?;
    liquers_web::eval::evaluate_query(envref, parsed).await
}

// ---------------------------------------------------------------------------
// Fixture commands (Phase 3's table)
// ---------------------------------------------------------------------------

/// Registers the four fixture commands on the global environment.
///
/// | Command | State | Arguments | Returns |
/// |---|---|---|---|
/// | `hello` | none (source) | — | `"Hello, world!"` |
/// | `number` | none (source) | `n: int` | that integer |
/// | `repeat` | text | `count: int = 2` | input repeated `count` times |
/// | `shout` | text | — | input uppercased |
///
/// `repeat` carries a default deliberately: inference refuses a defaulted parameter, so it is the
/// fixture that exercises the explicit-declaration path.
pub fn register_fixture_commands() {
    register_command_on(&decl("hello", "return 'Hello, world!';")).expect("hello");

    register_command_on(&with(
        decl_with_params("number", &["n"], "return n;"),
        "arguments",
        args(vec![arg("n", Some("int"), None)]),
    ))
    .expect("number");

    register_command_on(&with(
        with(
            decl_with_params("repeat", &["text", "count"], "return text.repeat(count);"),
            "state",
            JsValue::from_str("text"),
        ),
        "arguments",
        args(vec![arg("count", Some("int"), Some(JsValue::from_f64(2.0)))]),
    ))
    .expect("repeat");

    register_command_on(&with(
        decl_with_params("shout", &["text"], "return text.toUpperCase();"),
        "state",
        JsValue::from_str("text"),
    ))
    .expect("shout");
}

// ---------------------------------------------------------------------------
// console.warn spy (Phase 3 infrastructure #1)
// ---------------------------------------------------------------------------

/// Captures `console.warn` calls for the duration of its lifetime.
///
/// `wasm-bindgen-test` does not capture console output, and replacement warnings are asserted on
/// *content* — the two replacement variants carry different text. The original is restored on
/// drop, so it is restored even when a test fails part-way.
pub struct WarnSpy {
    original: JsValue,
    captured: js_sys::Array,
}

impl WarnSpy {
    pub fn install() -> WarnSpy {
        let console = get(&js_sys::global(), "console");
        let original = get(&console, "warn");
        let captured = js_sys::Array::new();

        // The replacement closure is created in JavaScript rather than as a Rust `Closure`, so
        // there is no handle to keep alive across the call and nothing to leak if a test panics.
        let installer: js_sys::Function = func(
            "(captured) => function (...a) { captured.push(a.map(String).join(' ')); }",
        );
        let spy = installer
            .call1(&JsValue::NULL, &captured)
            .expect("build the spy");
        js_sys::Reflect::set(&console, &"warn".into(), &spy).expect("install the spy");

        WarnSpy { original, captured }
    }

    /// Every message captured so far.
    pub fn messages(&self) -> Vec<String> {
        self.captured
            .iter()
            .filter_map(|v| v.as_string())
            .collect()
    }

    /// Whether any captured message contains `needle`.
    pub fn saw(&self, needle: &str) -> bool {
        self.messages().iter().any(|m| m.contains(needle))
    }
}

impl Drop for WarnSpy {
    fn drop(&mut self) {
        let console = get(&js_sys::global(), "console");
        let _ = js_sys::Reflect::set(&console, &"warn".into(), &self.original);
    }
}

// ---------------------------------------------------------------------------
// Timeouts
// ---------------------------------------------------------------------------

/// Awaits a future, failing after `millis` rather than hanging.
///
/// `RUNTIME04` guards against deadlock, and a deadlocked test that simply hangs reports nothing —
/// the harness times out with no indication of which case stalled. This turns a hang into a
/// named failure.
pub async fn with_timeout<F, T>(millis: i32, label: &str, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    use futures::future::{select, Either};
    futures::pin_mut!(future);
    let timer = sleep(millis);
    futures::pin_mut!(timer);
    match select(future, timer).await {
        Either::Left((value, _)) => value,
        Either::Right(((), _)) => panic!("{label} did not complete within {millis} ms"),
    }
}

/// A future that resolves after `millis`, via `setTimeout`.
pub async fn sleep(millis: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        let set_timeout: js_sys::Function = get(&global, "setTimeout").unchecked_into();
        set_timeout
            .call2(&global, &resolve, &JsValue::from_f64(millis as f64))
            .expect("setTimeout");
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
