use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::{
    context::Environment,
    metadata::{Metadata, MetadataRecord},
    parse::parse_key, value::Value,
};
use tokio::sync::RwLock;

use crate::{
    environment::{async_evaluate, ServerEnvironment, SharedEnvironment},
    utils::{CoreError, DataResultWrapper},
};


/*
#[axum::debug_handler]
pub async fn evaluate_handler(
    Path(query): Path<String>,
    State(env): State<SharedEnvironment>,
) -> Response<Body> {
    let envref = env.read().await.to_ref();
    match async_evaluate::<ServerEnvironment<Value>,_>(envref, query).await{
        Ok(state) => {
            default_value_response(&state.data)
        },
        Err(e) => CoreError(e).into_response(),
    }
}
*/