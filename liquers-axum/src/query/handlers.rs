//! Query Execution API handlers: `GET/POST {base}/{*query}` evaluate a query and serve its value.
//!
//! The value is read through `AssetRef::get_binary`, which waits for the evaluation, applies the
//! effective data format and refuses an expired, errored or cancelled result with that asset's own
//! error — the same path the Assets API's `data` routes use. The wait is bounded by
//! [`QueryApiConfig::timeout`] (`QueryApiBuilder::with_timeout`, default 30 s).
//!
//! Both methods take **argument overrides** for the query's last action — URL query parameters
//! and, on `POST`, a JSON object body (`specs/design/axum-query-named-arguments/`). A request
//! with none is evaluated as before; one with any is evaluated ad hoc, as an unmanaged asset.
//! Every answer about an asset carries `x-liquers-managed`.

use std::time::Duration;

use crate::api_core::overrides::{OverrideRequest, RouteOptions};
use crate::api_core::{error_to_detail, ApiResponse, BinaryResponse};
use crate::assets::common::{address_is_managed, is_managed, request_with_overrides, with_managed};
use axum::{
    body::Bytes,
    extract::{Path, Query as AxumQuery, State},
    response::{IntoResponse, Response},
    Extension,
};
use liquers_core::{
    context::{EnvRef, Environment},
    parse::parse_query,
};

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

/// Evaluate `query_path` with the request's overrides and serve the value, or an error.
async fn serve_query<E: Environment>(
    env: &EnvRef<E>,
    query_path: &str,
    pairs: Vec<(String, String)>,
    body: &[u8],
    config: QueryApiConfig,
) -> Response {
    let query = match parse_query(query_path) {
        Ok(q) => q,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    let overrides = match OverrideRequest::parse(pairs, body, RouteOptions::None) {
        Ok(overrides) => overrides,
        Err(e) => return error_response(&e.with_query(&query), "Invalid argument overrides"),
    };
    // The timeout bounds the whole wait: with an inline manager (`EvalMode::Inline`),
    // `evaluate` itself runs the evaluation to completion before returning.
    let evaluation = async {
        let (asset_ref, overridden) = if overrides.is_empty() {
            let asset_ref = env
                .evaluate(&query)
                .await
                .map_err(|e| (e, "Query evaluation failed"))?;
            (asset_ref, false)
        } else {
            let requested = request_with_overrides(env, &query, &overrides)
                .await
                .map_err(|e| (e, "Query evaluation failed"))?;
            (requested.asset, requested.overridden)
        };
        let value = asset_ref
            .get_binary()
            .await
            .map_err(|e| (e, "Query execution failed"));
        let managed = is_managed(env, &query, &asset_ref, overridden).await;
        Ok::<_, (liquers_core::error::Error, &str)>((value, managed))
    };
    match tokio::time::timeout(config.timeout, evaluation).await {
        Ok(Ok((Ok((data, metadata)), managed))) => with_managed(
            BinaryResponse {
                data: (*data).clone(),
                metadata: (*metadata).clone(),
            }
            .into_response(),
            managed,
        ),
        Ok(Ok((Err((e, message)), managed))) => with_managed(error_response(&e, message), managed),
        Ok(Err((e, message))) => error_response(&e, message),
        Err(_) => {
            // The answer still says whether the asset can be found again: one with overrides is
            // unmanaged by construction; otherwise ask the manager about the address.
            let (managed, advice) = if overrides.is_empty() {
                (
                    address_is_managed(env, &query).await,
                    "To follow a long evaluation, use the Assets API: POST {assets}/q/submit, then \
                     poll GET {assets}/q/info or subscribe on {assets}/ws/q",
                )
            } else {
                (
                    false,
                    "This request carries argument overrides, so its evaluation is unmanaged and \
                     cannot be polled: follow it with a WebSocket subscribe with arguments on \
                     {assets}/ws/q",
                )
            };
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "ExecutionError".to_string(),
                message: format!(
                    "Query execution did not finish within {:?}. {advice}",
                    config.timeout
                ),
                query: Some(query.encode()),
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Query execution timeout");
            with_managed(response.into_response(), managed)
        }
    }
}

/// `GET {base}{*query}` — evaluate the query and return the value. URL query parameters are
/// argument overrides of the last action (`?limit=20`).
pub async fn get_query_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Extension(config): Extension<QueryApiConfig>,
    Path(query_path): Path<String>,
    AxumQuery(pairs): AxumQuery<Vec<(String, String)>>,
) -> Response {
    serve_query(&env, &query_path, pairs, &[], config).await
}

/// `POST {base}{*query}` — as `GET`, and an optional JSON object body carries further argument
/// overrides (`{"query": "expiration safety", "limit": 20}`). An empty body, or `{}`, carries
/// none. A body that is not a JSON object is a 400 `ParameterError`.
pub async fn post_query_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Extension(config): Extension<QueryApiConfig>,
    Path(query_path): Path<String>,
    AxumQuery(pairs): AxumQuery<Vec<(String, String)>>,
    body: Bytes,
) -> Response {
    serve_query(&env, &query_path, pairs, &body, config).await
}
