use std::{default, sync::Arc};

use axum::{
    body::Body,
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::{media_type, value::{Value, ValueInterface}};

use crate::utils::CoreError;

pub struct ValueWrapper(pub Value);

impl From<Value> for ValueWrapper {
    fn from(value: Value) -> Self {
        ValueWrapper(value)
    }
}

pub fn json_response(value: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&value).unwrap().into())
        .unwrap()
}

pub fn default_value_response(value: Arc<Value>, media_type:Option<&str>) -> Response<Body> {
    match &*value {
        Value::None => json_response(value.try_into_json_value().unwrap()),
        Value::Bool(b) => json_response(value.try_into_json_value().unwrap()),
        Value::I32(_) => json_response(value.try_into_json_value().unwrap()),
        Value::I64(_) => json_response(value.try_into_json_value().unwrap()),
        Value::F64(_) => json_response(value.try_into_json_value().unwrap()),
        Value::Text(txt) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, media_type.unwrap_or("text/plain"))
            .body(txt.to_string().into())
            .unwrap(),
        Value::Array(vec) => {
            match value.try_into_json_value(){
                Ok(x) => json_response(x),
                Err(e) => CoreError(e).into_response(),
            }
        },
        Value::Object(_) => {
            match value.try_into_json_value(){
                Ok(x) => json_response(x),
                Err(e) => CoreError(e).into_response(),
            }
        },
        Value::Bytes(vec) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, media_type.unwrap_or("application/octet-stream"))
                .body(vec.to_vec().into())
                .unwrap()
        },
    }
}
