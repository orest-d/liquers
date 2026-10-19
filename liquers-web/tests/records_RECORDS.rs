//! `RECORDS05`/`RECORDS06` — the wasm half of `specs/design/record-streams/phase3-tests.md` §6.
//! `RECORDS06` needs the `debug-handles` feature, as `RUNTIME05` does.
//!
//! Both run under the Node loop (`cargo test -p liquers-web --target wasm32-unknown-unknown
//! --features debug-handles`): neither needs a browser API, since `WebAssembly.Memory` growth and
//! typed arrays behave the same under Node.

#![cfg(target_arch = "wasm32")]
#![cfg(feature = "records")]

use std::sync::Arc;

use js_sys::{Float64Array, Function, Reflect, WebAssembly};
// liquers-web reaches the records through liquers-lib's `records` feature, not a direct dependency.
use liquers_lib::records::{
    Bitmap, Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema,
};
use liquers_web::records::LiquersRecordBatch;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_test::*;

fn float_batch(values: &[f64]) -> Arc<RecordBatch> {
    let schema =
        Arc::new(RecordSchema::new(vec![FieldSchema::new("x", FieldType::Float)]).expect("schema"));
    let column = Column::Float {
        validity: None,
        values: Buffer::from_slice(values),
    };
    Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"))
}

fn memory_buffer() -> JsValue {
    wasm_bindgen::memory()
        .dyn_into::<WebAssembly::Memory>()
        .expect("wasm memory")
        .buffer()
}

fn descriptor_field(desc: &JsValue, name: &str) -> f64 {
    Reflect::get(desc, &JsValue::from_str(name))
        .expect("descriptor field")
        .as_f64()
        .expect("numeric descriptor field")
}

/// RECORDS05 — a view taken before the heap grows is detected as stale, and re-created at the
/// same pointer and length it reads the right values. This is the JS companion's refresh
/// (`view.buffer !== memory.buffer` → re-create), done by hand so the test does not depend on
/// the companion's packaging.
#[wasm_bindgen_test]
fn records05_a_column_view_is_detected_stale_after_memory_grow_and_refreshes_in_place() {
    let values = [1.5, -2.0, 3.25];
    let handle = LiquersRecordBatch::from(float_batch(&values));
    let desc = handle.column(0).expect("column descriptor");
    let ptr = descriptor_field(&desc, "ptr") as u32;
    let len = descriptor_field(&desc, "len") as u32;
    assert_eq!(len, 3);

    let before = memory_buffer();
    let view = Float64Array::new_with_byte_offset_and_length(&before, ptr, len);
    assert_eq!(view.to_vec(), values);

    // Grow linear memory by one page: the old ArrayBuffer is detached, the data does not move.
    let previous_pages = core::arch::wasm32::memory_grow::<0>(1);
    assert_ne!(previous_pages, usize::MAX, "memory.grow failed");

    let after = memory_buffer();
    assert!(
        !JsValue::eq(&view.buffer().into(), &after),
        "the growth must be detectable"
    );
    assert_eq!(
        view.length(),
        0,
        "a view over a detached buffer reads nothing, not garbage"
    );

    let refreshed = Float64Array::new_with_byte_offset_and_length(&after, ptr, len);
    assert_eq!(refreshed.to_vec(), values, "same pointer, same length, same values");

    // The always-safe fallback agrees.
    let copy: Float64Array = handle
        .column_copy(0)
        .expect("copy")
        .dyn_into()
        .expect("Float64Array");
    assert_eq!(copy.to_vec(), values);
}

/// Beyond Phase 3 §6: confirms the `records_column.js` local snippet actually loads and runs
/// under the Node test loop, not merely that it compiles — `columnView` is the one method that
/// calls into it. If this ever regresses (a wasm-bindgen upgrade refusing local `module = "/…"`
/// snippets under Node), `LiquersRecordBatch::column_view` is the piece to fall back out of; the
/// required RECORDS05/RECORDS06 above do not depend on it.
#[wasm_bindgen_test]
fn records_column_view_companion_loads_and_reads_under_node() {
    let values = [4.0, 5.0, 6.0];
    let handle = LiquersRecordBatch::from(float_batch(&values));
    let companion = handle.column_view(0).expect("column view");
    let view = Reflect::get(&companion, &JsValue::from_str("view")).expect("view getter");
    let view: Float64Array = view.dyn_into().expect("Float64Array view");
    assert_eq!(view.to_vec(), values);
}

/// An all-null `Vector` column has no width (`dim == 0`) and no data; its rows are counted by the
/// validity bitmap, so `columnCopy` gives one `null` per row rather than an empty array.
#[wasm_bindgen_test]
fn column_copy_of_an_all_null_vector_column_keeps_its_rows() {
    let schema =
        Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Vector)]).expect("schema"));
    let column = Column::Vector {
        validity: Some(Bitmap::from_bools(&[false, false, false])),
        dim: 0,
        data: Buffer::from_slice(&[]),
    };
    let batch = Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"));
    let handle = LiquersRecordBatch::from(batch);
    let copy: js_sys::Array = handle
        .column_copy(0)
        .expect("copy")
        .dyn_into()
        .expect("Array");
    assert_eq!(copy.length(), 3);
    assert!((0..3).all(|row| copy.get(row).is_null()));
}

/// RECORDS06 — freeing the JS handle releases the batch. Observed two ways: the live batch-handle
/// count returns to its baseline, and the batch itself is dropped (its `Weak` no longer upgrades).
#[cfg(feature = "debug-handles")]
#[wasm_bindgen_test]
fn records06_freeing_the_handle_releases_the_batch() {
    use liquers_web::records::live_batch_handle_count;

    let baseline = live_batch_handle_count();
    let batch = float_batch(&[1.0, 2.0]);
    let weak = Arc::downgrade(&batch);

    let js_handle: JsValue = LiquersRecordBatch::from(batch).into();
    assert_eq!(live_batch_handle_count(), baseline + 1);
    assert!(weak.upgrade().is_some(), "the handle keeps the batch alive");

    // `free()` is what JavaScript calls; call it the same way.
    let free: Function = Reflect::get(&js_handle, &JsValue::from_str("free"))
        .expect("free")
        .dyn_into()
        .expect("free is a function");
    free.call0(&js_handle).expect("free() succeeds");

    assert_eq!(live_batch_handle_count(), baseline);
    assert!(weak.upgrade().is_none(), "free() must drop the last Arc<RecordBatch>");
}

fn view_value(batch: Arc<RecordBatch>) -> liquers_lib::value::Value {
    let view: Arc<dyn liquers_lib::records::RecordView> = batch;
    liquers_lib::value::Value::Extended(liquers_lib::value::ExtValue::RecordView { value: view })
}

fn json_batch(rows: serde_json::Value) -> Arc<RecordBatch> {
    Arc::new(
        liquers_lib::records::from_json(
            &rows,
            liquers_lib::records::JsonOrient::Records,
            liquers_lib::records::ReadSchema::Infer,
        )
        .expect("batch from JSON"),
    )
}

fn to_js(batch: Arc<RecordBatch>) -> JsValue {
    liquers_web::bridge::value_to_js(&view_value(batch)).expect("value_to_js")
}

fn js_number(obj: &JsValue, name: &str) -> f64 {
    Reflect::get(obj, &JsValue::from_str(name))
        .expect("property")
        .as_f64()
        .expect("numeric property")
}

/// RECORDS10 — a single-cell view reaches the language as its own scalar.
#[wasm_bindgen_test]
fn records10_a_single_cell_view_reaches_javascript_as_its_scalar() {
    assert_eq!(to_js(float_batch(&[1.5])).as_f64(), Some(1.5));
    assert_eq!(to_js(json_batch(serde_json::json!([{ "n": 7 }]))).as_f64(), Some(7.0));
    assert_eq!(
        to_js(json_batch(serde_json::json!([{ "t": "a" }]))).as_string(),
        Some("a".to_string())
    );
    assert_eq!(to_js(json_batch(serde_json::json!([{ "b": true }]))).as_bool(), Some(true));

    // No payload field: the `Id` field is the value.
    let schema = Arc::new(
        RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int)
            .with_key(liquers_lib::records::KeyRole::Id)])
        .expect("schema"),
    );
    let ids = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[42i64]),
    };
    let batch = Arc::new(RecordBatch::new(schema, vec![ids], None, None, vec![]).expect("batch"));
    assert_eq!(to_js(batch).as_f64(), Some(42.0));
}

/// RECORDS10 — a larger view crosses as a batch whose shape is readable.
#[wasm_bindgen_test]
fn records10_a_larger_view_reaches_javascript_as_a_batch_naming_its_shape() {
    let tall = to_js(float_batch(&[1.0, 2.0]));
    assert!(tall.is_object());
    assert_eq!(js_number(&tall, "numRows"), 2.0);
    assert_eq!(js_number(&tall, "numColumns"), 1.0);

    let wide = to_js(json_batch(serde_json::json!([{ "a": 1.0, "b": 2.0 }])));
    assert!(wide.is_object());
    assert_eq!(js_number(&wide, "numRows"), 1.0);
    assert_eq!(js_number(&wide, "numColumns"), 2.0);
}

/// RECORDS10 — a null cell is `null` and a binary cell a `Uint8Array`, as for a bare value.
#[wasm_bindgen_test]
fn records10_null_and_binary_cells_keep_the_value_mapping() {
    let schema =
        Arc::new(RecordSchema::new(vec![FieldSchema::new("x", FieldType::Float)]).expect("schema"));
    let column = Column::Float {
        validity: Some(Bitmap::from_bytes(&[0u8], 1)),
        values: Buffer::from_slice(&[0.0]),
    };
    let batch = Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"));
    assert!(to_js(batch).is_null());

    let schema = Arc::new(
        RecordSchema::new(vec![FieldSchema::new("blob", FieldType::Binary)]).expect("schema"),
    );
    let column = Column::Binary {
        validity: None,
        offsets: Buffer::from_slice(&[0i32, 3]),
        data: liquers_lib::records::AlignedBuffer::from_slice(b"abc"),
    };
    let batch = Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"));
    let js = to_js(batch);
    assert!(js.is_instance_of::<js_sys::Uint8Array>());
    assert_eq!(js_sys::Uint8Array::from(js).to_vec(), b"abc".to_vec());
}
