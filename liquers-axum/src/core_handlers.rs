use crate::value::default_value_response;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::value::Value;

use crate::{
    environment::{async_evaluate, ServerEnvRef},
    utils::CoreError,
};

#[axum::debug_handler]
pub async fn evaluate_handler(
    Path(query): Path<String>,
    State(envref): State<ServerEnvRef>,
) -> Response<Body> {
    match async_evaluate(envref, query).await {
        Ok(state) => default_value_response(&state.data, Some(&state.metadata.get_media_type())),
        Err(e) => CoreError(e).into_response(),
    }
}
