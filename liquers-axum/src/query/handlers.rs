//! Query Execution API handlers: `GET/POST {base}/{*query}` evaluate a query and serve its value.
//!
//! The value is read through `AssetRef::get_binary`, which waits for the evaluation, applies the
//! effective data format and refuses an expired, errored or cancelled result with that asset's own
//! error — the same path the Assets API's `data` routes use. The wait is bounded by
//! [`QueryApiConfig::timeout`] (`QueryApiBuilder::with_timeout`, default 30 s).

use std::time::Duration;

use crate::api_core::{error_to_detail, ApiResponse, BinaryResponse};
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension,
};
use liquers_core::{
    context::{EnvRef, Environment},
    parse::parse_query,
};
use serde_json::Value as JsonValue;

/// Settings of the Query API, handed to its handlers as an `Extension` by `QueryApiBuilder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryApiConfig {
    /// How long a request waits for the value before answering with an error.
    pub timeout: Duration,
}

impl Default for QueryApiConfig {
    fn default() -> Self {
        QueryApiConfig {
            timeout: Duration::from_secs(30),
        }
    }
}

fn error_response(e: &liquers_core::error::Error, message: &str) -> Response {
    let response: ApiResponse<()> = ApiResponse::error(error_to_detail(e), message);
    response.into_response()
}

/// Evaluate `query_path` and serve the value, or an error.
async fn serve_query<E: Environment>(
    env: &EnvRef<E>,
    query_path: &str,
    config: QueryApiConfig,
) -> Response {
    let query = match parse_query(query_path) {
        Ok(q) => q,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    // The timeout bounds the whole wait: with an inline manager (`EvalMode::Inline`),
    // `evaluate` itself runs the evaluation to completion before returning.
    let evaluation = async {
        let asset_ref = env
            .evaluate(&query)
            .await
            .map_err(|e| (e, "Query evaluation failed"))?;
        asset_ref
            .get_binary()
            .await
            .map_err(|e| (e, "Query execution failed"))
    };
    match tokio::time::timeout(config.timeout, evaluation).await {
        Ok(Ok((data, metadata))) => BinaryResponse {
            data: (*data).clone(),
            metadata: (*metadata).clone(),
        }
        .into_response(),
        Ok(Err((e, message))) => error_response(&e, message),
        Err(_) => {
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "ExecutionError".to_string(),
                message: format!(
                    "Query execution did not finish within {:?}. To follow a long evaluation, use \
                     the Assets API: POST {{assets}}/q/submit, then poll GET {{assets}}/q/info or \
                     subscribe on {{assets}}/ws/q",
                    config.timeout
                ),
                query: Some(query.encode()),
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Query execution timeout");
            response.into_response()
        }
    }
}

/// GET /q/{*query} - Execute query and return result
pub async fn get_query_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Extension(config): Extension<QueryApiConfig>,
    Path(query_path): Path<String>,
) -> Response {
    serve_query(&env, &query_path, config).await
}

/// POST /q/{*query} - Execute query with optional JSON body
/// Body format: {"parameters": ["arg1", "arg2", ...]} (optional, for future use)
/// Currently behaves the same as GET - body arguments are logged but not used
/// TODO: Implement parameter passing mechanism once query modification API is finalized
pub async fn post_query_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Extension(config): Extension<QueryApiConfig>,
    Path(query_path): Path<String>,
    body: Bytes,
) -> Response {
    // The body is optional: an empty POST (with or without a Content-Type) evaluates the query.
    if !body.is_empty() {
        match serde_json::from_slice::<JsonValue>(&body) {
            Ok(args) => {
                tracing::debug!("POST query received with body: {:?}", args);
                // TODO: Implement query parameter modification once API is designed
            }
            Err(e) => {
                let e = liquers_core::error::Error::from_error(
                    liquers_core::error::ErrorType::ParameterError,
                    format!("POST body must be JSON: {e}"),
                );
                return error_response(&e, "Invalid request body");
            }
        }
    }
    serve_query(&env, &query_path, config).await
}
