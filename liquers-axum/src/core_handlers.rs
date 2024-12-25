use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::value::Value;




/*
#[axum::debug_handler]
pub async fn evaluate_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    
    let env_access = env.read().await;
    let envref = (*env_access).to_ref();
    match async_evaluate::<ServerEnvironment<Value>,_>(envref, query).await{
        Ok(state) => {
            default_value_response(&state.data)
        },
        Err(e) => CoreError(e).into_response(),
    }
}
*/