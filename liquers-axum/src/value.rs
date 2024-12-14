use std::default;

use axum::{
    body::Body,
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::value::{Value, ValueInterface};

pub struct ValueWrapper(pub Value);

impl From<Value> for ValueWrapper {
    fn from(value: Value) -> Self {
        ValueWrapper(value)
    }
}

pub fn json_respones(value: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&value).unwrap().into())
        .unwrap()
}

pub fn default_value_response(value: &Value) -> Response<Body> {
    match value {
        Value::None => json_respones(value.try_into_json_value().unwrap()),
        Value::Bool(b) => json_respones(value.try_into_json_value().unwrap()),
        Value::I32(_) => json_respones(value.try_into_json_value().unwrap()),
        Value::I64(_) => json_respones(value.try_into_json_value().unwrap()),
        Value::F64(_) => json_respones(value.try_into_json_value().unwrap()),
        Value::Text(txt) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(txt.to_string().into())
            .unwrap(),
        Value::Array(vec) => todo!(),
        Value::Object(btree_map) => todo!(),
        Value::Bytes(vec) => todo!(),
    }
}
