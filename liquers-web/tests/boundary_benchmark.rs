//! Boundary cost: structural conversion versus opaque retention.
//!
//! **Reported, not asserted.** There is no threshold and no CI to enforce one; timings vary by
//! machine, browser and build profile, and a benchmark that fails the suite on a slow day teaches
//! nothing. It runs with the tests so the numbers stay reachable, and prints a table.
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --release \
//!     --test boundary_benchmark -- --nocapture
//! ```
//!
//! **Run it `--release`.** A debug build measures the absence of optimization, not the cost of the
//! boundary; the ratio between the two paths is roughly preserved, but the absolute numbers are
//! not worth quoting.
//!
//! # What the answer is for
//!
//! Phase 1 decision 2 chose structural conversion as the default and `opaque()` as an explicit
//! opt-in, and the discussion at the time treated opacity as partly a *performance* answer. That
//! framing should only survive if the numbers support it — see the note this test prints.

#![cfg(target_arch = "wasm32")]

use liquers_lib::value::Value;
use liquers_web::bridge::{js_to_value, opaque_value, value_to_js, ConversionPolicy};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

/// `performance.now()`, in milliseconds, with sub-millisecond resolution.
fn now() -> f64 {
    let global = js_sys::global();
    let performance = js_sys::Reflect::get(&global, &"performance".into()).expect("performance");
    let now: js_sys::Function =
        js_sys::Reflect::get(&performance, &"now".into()).expect("now").unchecked_into();
    now.call0(&performance).ok().and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn log(line: &str) {
    // Diagnostics go to the console, never to stdout — the project rule holds in tests too.
    web_sys::console::log_1(&JsValue::from_str(line));
}

/// An object with `n` properties, each a small string.
fn object_with(n: usize) -> JsValue {
    let obj = js_sys::Object::new();
    for i in 0..n {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str(&format!("k{i}")),
            &JsValue::from_str("value"),
        )
        .expect("set");
    }
    obj.into()
}

/// Median of `iterations` timed runs, in milliseconds. Median rather than mean: one GC pause in
/// the middle of a run should not become the reported number.
fn median_ms<F: FnMut()>(iterations: usize, mut body: F) -> f64 {
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = now();
        body();
        samples.push(now() - start);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    samples[samples.len() / 2]
}

#[wasm_bindgen_test]
fn web_boundary_benchmark() {
    log("");
    log("Boundary cost — structural conversion vs opaque retention");
    log("(median of N runs, one full round trip: JavaScript -> Value -> JavaScript)");
    log("");
    log("  input                     iters   structural     opaque    ratio");
    log("  ----------------------- -------  -----------  ---------  -------");

    for (n, iterations) in [(10usize, 200usize), (100, 100), (1_000, 50), (10_000, 10)] {
        let js = object_with(n);

        let structural = median_ms(iterations, || {
            let value: Value =
                js_to_value(&js, ConversionPolicy::Structural).expect("structural conversion");
            let _ = value_to_js(&value).expect("back to JavaScript");
        });

        let opaque = median_ms(iterations, || {
            let value: Value = opaque_value(js.clone()).expect("opaque retention");
            let _ = value_to_js(&value).expect("back to JavaScript");
        });

        let ratio = if opaque > 0.0 { structural / opaque } else { f64::INFINITY };
        log(&format!(
            "  object, {n:>6} properties  {iterations:>5}  {structural:>9.3}ms  {opaque:>7.3}ms  {ratio:>6.0}x"
        ));
    }

    // A 1 MB byte array: the case where structural conversion copies the most and gains the least,
    // since bytes have no structure to interpret.
    let bytes = js_sys::Uint8Array::new_with_length(1_048_576);
    let js_bytes: JsValue = bytes.into();

    let structural = median_ms(20, || {
        let value: Value =
            js_to_value(&js_bytes, ConversionPolicy::Structural).expect("structural conversion");
        let _ = value_to_js(&value).expect("back to JavaScript");
    });
    let opaque = median_ms(20, || {
        let value: Value = opaque_value(js_bytes.clone()).expect("opaque retention");
        let _ = value_to_js(&value).expect("back to JavaScript");
    });
    let ratio = if opaque > 0.0 { structural / opaque } else { f64::INFINITY };
    log(&format!(
        "  Uint8Array, 1 MB             20  {structural:>9.3}ms  {opaque:>7.3}ms  {ratio:>6.0}x"
    ));

    log("");
    log("  Opaque retention is O(1): it stores a handle and hands the same object back.");
    log("  Structural conversion is O(size) in both directions.");
    log("");
    log("  Feed this back into Phase 1 decision 2: `opaque()` is justified by *identity*");
    log("  — the same object comes back, and a value that only passes between two JavaScript");
    log("  commands need not be understood by Rust at all. Whether it is also the right");
    log("  answer for performance depends on whether the sizes above are realistic for the");
    log("  page in question. At small sizes the absolute cost is negligible either way.");
    log("");
}
