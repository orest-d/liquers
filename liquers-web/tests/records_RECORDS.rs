//! `RECORDS*` — the wasm half of `specs/design/record-streams/phase3-tests.md` §6, and the records
//! bridge of `specs/design/liquers-web-records-bridge/` (sources, streaming, handles crossing back,
//! the `rec` commands in the web environment). The `RECORDS06` tests need the `debug-handles`
//! feature, as `RUNTIME05` does.
//!
//! All run under the Node loop (`cargo test -p liquers-web --target wasm32-unknown-unknown
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
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

mod common;

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

// ---------------------------------------------------------------------------
// The records bridge (`specs/design/liquers-web-records-bridge/`)
// ---------------------------------------------------------------------------

use liquers_lib::records::{InMemorySource, ManifestSource, ManifestSpec, RecordSource, RecordView};
use liquers_lib::value::{ExtValue, Value};

fn source_value(source: Arc<dyn RecordSource>) -> Value {
    Value::Extended(ExtValue::RecordSource { value: source })
}

fn source_to_js(source: Arc<dyn RecordSource>) -> JsValue {
    liquers_web::bridge::value_to_js(&source_value(source)).expect("value_to_js")
}

/// Three two-row views of one `Int` column `a`: 1..=6.
fn three_views() -> Arc<dyn RecordSource> {
    let views: Vec<Arc<dyn RecordView>> = [[1, 2], [3, 4], [5, 6]]
        .iter()
        .map(|pair| {
            json_batch(serde_json::json!([{ "a": pair[0] }, { "a": pair[1] }])) as Arc<dyn RecordView>
        })
        .collect();
    Arc::new(InMemorySource::new(views))
}

fn call(target: &JsValue, method: &str, args: &[JsValue]) -> JsValue {
    let function: Function = Reflect::get(target, &JsValue::from_str(method))
        .expect("method")
        .dyn_into()
        .expect("a function");
    function
        .apply(target, &args.iter().collect::<js_sys::Array>())
        .unwrap_or_else(|e| panic!("{method}() threw {e:?}"))
}

async fn awaited(promise: JsValue) -> Result<JsValue, JsValue> {
    JsFuture::from(js_sys::Promise::from(promise)).await
}

/// The `a` column of a batch handle, read through `columnCopy`.
fn int_column(batch: &JsValue) -> Vec<i64> {
    let copy = call(batch, "columnCopy", &[JsValue::from(0)]);
    copy.dyn_into::<js_sys::BigInt64Array>()
        .expect("an Int column copies to BigInt64Array")
        .to_vec()
}

/// Every batch an async iterable yields, driven through `Symbol.asyncIterator` as `for await` does.
async fn drain(iterable: &JsValue) -> Vec<JsValue> {
    let factory: Function = Reflect::get(iterable, &js_sys::Symbol::async_iterator())
        .expect("Symbol.asyncIterator")
        .dyn_into()
        .expect("a function");
    let iterator = factory.call0(iterable).expect("an iterator");
    let mut batches = Vec::new();
    loop {
        let step = awaited(call(&iterator, "next", &[])).await.expect("next()");
        if Reflect::get(&step, &"done".into()).expect("done").as_bool() == Some(true) {
            return batches;
        }
        batches.push(Reflect::get(&step, &"value".into()).expect("value"));
    }
}

fn free(handle: &JsValue) {
    call(handle, "free", &[]);
}

fn rust_value(js: &JsValue) -> Result<Value, liquers_core::error::Error> {
    liquers_web::bridge::js_to_value::<Value>(js, liquers_web::bridge::ConversionPolicy::Structural)
}

/// RECORDS01 — a batch handle crosses back as the very view it was made from, origin intact.
#[wasm_bindgen_test]
fn records01_a_batch_handle_crosses_back_as_the_same_view_with_origin_intact() {
    let chunk = liquers_core::parse::parse_query("part-1/ns-rec/from_json-records").expect("query");
    let origin = liquers_lib::records::ChunkOrigin {
        asset: chunk.clone(),
        chunk: chunk.clone(),
        info: None,
        locator: None,
    };
    let schema = Arc::new(
        RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Int).with_key(liquers_lib::records::KeyRole::Id),
            FieldSchema::new("x", FieldType::Float),
        ])
        .expect("schema"),
    );
    let batch = Arc::new(
        RecordBatch::new(
            schema,
            vec![
                Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) },
                Column::Float { validity: None, values: Buffer::from_slice(&[0.5, 1.5]) },
            ],
            Some(liquers_lib::records::ChunkId::Query(chunk)),
            None,
            vec![origin],
        )
        .expect("batch"),
    );
    let view: Arc<dyn RecordView> = batch.clone();
    let js = liquers_web::bridge::value_to_js(&Value::Extended(ExtValue::RecordView {
        value: view.clone(),
    }))
    .expect("value_to_js");

    assert_eq!(
        call(&js, "chunkIdJson", &[]).as_string(),
        Some(serde_json::to_string(&batch.chunk_id).expect("json"))
    );
    assert_eq!(
        call(&js, "sourcesJson", &[]).as_string(),
        Some(serde_json::to_string(&batch.sources).expect("json"))
    );

    match rust_value(&js).expect("the handle crosses back") {
        Value::Extended(ExtValue::RecordView { value }) => {
            assert!(Arc::ptr_eq(&value, &view), "the same Arc, not a copy");
            assert!(value.schema().id_field().is_some(), "the Id role survives");
        }
        other => panic!("expected a record view, got {other:?}"),
    }
    free(&js);
}

/// RECORDS02 — a column reads the same through its typed-array view and through its copy.
#[wasm_bindgen_test]
fn records02_a_column_reads_the_same_through_a_view_and_a_copy() {
    let values = [0.25, -1.0, 8.5];
    let handle = LiquersRecordBatch::from(float_batch(&values));
    let desc = handle.column(0).expect("descriptor");
    let view = Float64Array::new_with_byte_offset_and_length(
        &memory_buffer(),
        descriptor_field(&desc, "ptr") as u32,
        descriptor_field(&desc, "len") as u32,
    );
    let copy: Float64Array = handle.column_copy(0).expect("copy").dyn_into().expect("Float64Array");
    assert_eq!(view.to_vec(), copy.to_vec());

    let texts = LiquersRecordBatch::from(json_batch(serde_json::json!([{ "t": "x" }, { "t": "yz" }])));
    let copy: js_sys::Array = texts.column_copy(0).expect("copy").dyn_into().expect("Array");
    let read: Vec<Option<String>> = copy.iter().map(|v| v.as_string()).collect();
    assert_eq!(read, vec![Some("x".to_string()), Some("yz".to_string())]);
}

/// RECORDS04 — a column copy is detached: writing into it leaves the batch unchanged.
#[wasm_bindgen_test]
fn records04_a_column_copy_is_detached_from_the_batch() {
    let values = [1.0, 2.0];
    let handle = LiquersRecordBatch::from(float_batch(&values));
    let copy: Float64Array = handle.column_copy(0).expect("copy").dyn_into().expect("Float64Array");
    copy.set_index(0, 99.0);
    let desc = handle.column(0).expect("descriptor");
    let view = Float64Array::new_with_byte_offset_and_length(
        &memory_buffer(),
        descriptor_field(&desc, "ptr") as u32,
        descriptor_field(&desc, "len") as u32,
    );
    assert_eq!(view.to_vec(), values, "the batch's own buffer is untouched");
}

/// A JavaScript command `part` whose chunks count how often they are evaluated.
fn register_part_command() {
    use common::{arg, args, decl_with_params, with};
    liquers_web::environment::register_command_on(&with(
        decl_with_params(
            "part",
            &["n"],
            "globalThis.__liquersParts = (globalThis.__liquersParts || 0) + 1; \
             return [{ a: n }, { a: n * 10 }];",
        ),
        "arguments",
        args(vec![arg("n", Some("int"), None)]),
    ))
    .expect("register part");
}

fn parts_evaluated() -> f64 {
    Reflect::get(&js_sys::global(), &"__liquersParts".into())
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// RECORDS07 — chunk ids are data: listing them evaluates nothing, and evaluating them one at a
/// time yields the source's rows.
#[wasm_bindgen_test]
async fn records07_chunk_ids_are_data_and_evaluate_one_at_a_time() {
    common::fresh();
    register_part_command();
    let _ = Reflect::set(&js_sys::global(), &"__liquersParts".into(), &JsValue::from(0));
    let spec: ManifestSpec = serde_json::from_value(serde_json::json!({
        "manifest": "record-stream",
        "chunks": [
            { "query": "part-1/ns-rec/from_json-records" },
            { "query": "part-2/ns-rec/from_json-records" },
        ],
    }))
    .expect("manifest");
    let source: Arc<dyn RecordSource> = Arc::new(ManifestSource::new(spec, None).expect("source"));
    let js = source_to_js(source);

    assert_eq!(Reflect::get(&js, &"bounded".into()).expect("bounded").as_bool(), Some(true));
    let chunks: Vec<JsValue> = js_sys::Array::from(&call(&js, "chunks", &[])).iter().collect();
    assert_eq!(chunks.len(), 2);
    assert_eq!(parts_evaluated(), 0.0, "listing chunks evaluates nothing");

    let mut one_at_a_time = Vec::new();
    for chunk in &chunks {
        assert_eq!(Reflect::get(chunk, &"kind".into()).expect("kind").as_string().as_deref(), Some("query"));
        let query = Reflect::get(chunk, &"query".into()).expect("query").as_string().expect("text");
        let batch = common::eval_to_js(&query).await.expect("the chunk query evaluates");
        one_at_a_time.extend(int_column(&batch));
    }
    assert_eq!(one_at_a_time, vec![1, 10, 2, 20]);

    let all = awaited(call(&js, "materialize", &[JsValue::from(100)]))
        .await
        .expect("materialize");
    assert_eq!(int_column(&all), one_at_a_time, "the stream's rows, in order");
    liquers_web::environment::reset_global();
}

/// RECORDS08 — the async iterator yields one batch per view, with the rows `materialize` gives.
#[wasm_bindgen_test]
async fn records08_the_async_iterator_yields_the_same_rows_as_materialize() {
    common::fresh();
    let js = source_to_js(three_views());
    let iterable = awaited(call(&js, "stream", &[])).await.expect("stream()");
    let batches = drain(&iterable).await;
    assert_eq!(batches.len(), 3, "one batch per view");
    let streamed: Vec<i64> = batches.iter().flat_map(int_column).collect();

    let all = awaited(call(&js, "materialize", &[JsValue::from(1000)]))
        .await
        .expect("materialize");
    assert_eq!(streamed, int_column(&all));
    assert_eq!(streamed, vec![1, 2, 3, 4, 5, 6]);
    liquers_web::environment::reset_global();
}

/// `materialize(maxRows)` refuses a larger source and accepts one that fits.
#[wasm_bindgen_test]
async fn records_source_materialize_is_bounded_by_max_rows() {
    common::fresh();
    let js = source_to_js(three_views());
    let refused = awaited(call(&js, "materialize", &[JsValue::from(5)]))
        .await
        .expect_err("six rows exceed five");
    let message = Reflect::get(&refused, &"message".into()).expect("message").as_string().unwrap_or_default();
    assert!(message.contains("max_rows"), "{message}");

    let all = awaited(call(&js, "materialize", &[JsValue::from(6)]))
        .await
        .expect("six rows fit six");
    assert_eq!(js_number(&all, "numRows"), 6.0);
    liquers_web::environment::reset_global();
}

/// A freed handle passed back is refused, naming what it was.
#[wasm_bindgen_test]
fn records_a_freed_handle_is_refused_by_name() {
    let batch = to_js(float_batch(&[1.0, 2.0]));
    free(&batch);
    let error = rust_value(&batch).expect_err("a freed batch must be refused");
    assert!(error.message.contains("freed RecordBatch"), "{}", error.message);

    let source = source_to_js(three_views());
    free(&source);
    let error = rust_value(&source).expect_err("a freed source must be refused");
    assert!(error.message.contains("freed RecordSource"), "{}", error.message);
}

/// The `rec` commands run in the web environment: a page query can produce records.
#[wasm_bindgen_test]
async fn records_ns_rec_commands_run_in_the_web_environment() {
    common::fresh();
    liquers_web::environment::register_command_on(&common::decl(
        "rows",
        "return [{ a: 1 }, { a: 2 }, { a: 3 }];",
    ))
    .expect("register rows");
    let count = common::eval_to_js("rows/ns-rec/from_json-records/nrows")
        .await
        .expect("evaluates");
    assert_eq!(count.as_f64(), Some(3.0));
    liquers_web::environment::reset_global();
}

/// A command handed a batch that returns it gives Rust the same view, and the query continues.
#[wasm_bindgen_test]
async fn records_a_command_returning_its_batch_hands_rust_the_same_view() {
    common::fresh();
    liquers_web::environment::register_command_on(&common::decl(
        "rows",
        "return [{ a: 1 }, { a: 2 }, { a: 3 }];",
    ))
    .expect("register rows");
    liquers_web::environment::register_command_on(&common::with(
        common::decl_with_params("keep", &["batch"], "if (typeof batch.numRows !== 'number') throw new Error('not a batch'); return batch;"),
        "state",
        JsValue::from_str("value"),
    ))
    .expect("register keep");
    let count = common::eval_to_js("rows/ns-rec/from_json-records/keep/ns-rec/nrows")
        .await
        .expect("evaluates");
    assert_eq!(count.as_f64(), Some(3.0));
    liquers_web::environment::reset_global();
}

/// RECORDS06 — freeing a source handle releases the source: the registry forgets its id and the
/// source itself is dropped.
#[wasm_bindgen_test]
fn records06_releasing_a_source_handle_releases_the_source() {
    let source = three_views();
    let weak = Arc::downgrade(&source);
    let js = source_to_js(source);
    let id = js_number(&js, "__liquersRecordHandle") as u32;
    assert!(liquers_web::records::handle_is_live(id));
    free(&js);
    assert!(!liquers_web::records::handle_is_live(id), "the registry forgets a freed handle");
    assert!(weak.upgrade().is_none(), "free() must drop the last Arc of the source");
}

/// Data carrying the marker is data, not a handle: an own property cannot name a live record.
#[wasm_bindgen_test]
fn records_an_object_copying_the_marker_is_not_a_handle() {
    let batch = to_js(float_batch(&[1.0, 2.0]));
    let id = Reflect::get(&batch, &"__liquersRecordHandle".into()).expect("a live id");
    let forged = js_sys::Object::new();
    Reflect::set(&forged, &"__liquersRecordHandle".into(), &id).expect("set");
    match rust_value(&forged.into()).expect("plain data converts") {
        Value::Extended(ExtValue::RecordView { .. }) | Value::Extended(ExtValue::RecordSource { .. }) => {
            panic!("an object with a copied marker must not become a record value")
        }
        Value::Base(_) | Value::Extended(_) => {}
    }
    assert!(
        matches!(rust_value(&batch), Ok(Value::Extended(ExtValue::RecordView { .. }))),
        "the real handle still crosses back"
    );
    free(&batch);
}

/// Views held in memory list as `memory` chunks, which name no query to evaluate.
#[wasm_bindgen_test]
fn records_in_memory_chunks_are_not_offered_as_queries() {
    let js = source_to_js(three_views());
    let kinds: Vec<Option<String>> = js_sys::Array::from(&call(&js, "chunks", &[]))
        .iter()
        .map(|chunk| Reflect::get(&chunk, &"kind".into()).expect("kind").as_string())
        .collect();
    assert_eq!(kinds, vec![Some("memory".to_string()); 3]);
    free(&js);
}

fn parts_manifest() -> Arc<dyn RecordSource> {
    let spec: ManifestSpec = serde_json::from_value(serde_json::json!({
        "manifest": "record-stream",
        "chunks": [{ "query": "part-1/ns-rec/from_json-records" }],
    }))
    .expect("manifest");
    Arc::new(ManifestSource::new(spec, None).expect("source"))
}

/// A source from an explicit environment resolves through it: it needs no singleton, and it does
/// not see the singleton's commands.
#[wasm_bindgen_test]
async fn records_a_source_resolves_through_the_environment_that_produced_it() {
    liquers_web::environment::reset_global();
    let explicit = liquers_web::environment::build_environment().expect("an explicit environment");

    let in_memory = liquers_web::bridge::value_to_js_in(&explicit, &source_value(three_views()))
        .expect("converts");
    let all = awaited(call(&in_memory, "materialize", &[JsValue::from(6)]))
        .await
        .expect("an uninitialized singleton is not needed");
    assert_eq!(js_number(&all, "numRows"), 6.0);

    common::fresh();
    register_part_command();
    let manifest = liquers_web::bridge::value_to_js_in(&explicit, &source_value(parts_manifest()))
        .expect("converts");
    awaited(call(&manifest, "materialize", &[JsValue::from(10)]))
        .await
        .expect_err("`part` is registered on the singleton only, so the explicit environment fails");
    let global = source_to_js(parts_manifest());
    let rows = awaited(call(&global, "materialize", &[JsValue::from(10)]))
        .await
        .expect("the singleton has `part`");
    assert_eq!(int_column(&rows), vec![1, 10]);
    liquers_web::environment::reset_global();
}

/// A source from the singleton keeps following it: a command registered after the source was
/// produced, which rebuilds the singleton, is visible when the source streams.
#[wasm_bindgen_test]
async fn records_a_singleton_source_sees_commands_registered_later() {
    common::fresh();
    let envref = liquers_web::environment::shared_env().expect("share");
    let js = liquers_web::bridge::value_to_js_in(&envref, &source_value(parts_manifest()))
        .expect("converts");
    register_part_command();
    let rows = awaited(call(&js, "materialize", &[JsValue::from(10)]))
        .await
        .expect("the rebuilt singleton has `part`");
    assert_eq!(int_column(&rows), vec![1, 10]);
    liquers_web::environment::reset_global();
}
