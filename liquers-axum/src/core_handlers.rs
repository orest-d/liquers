use crate::utils::AssetDataResultWrapper;

use axum::{
    body::Body,
    extract::{Path, State},
    http::Response,
    response::IntoResponse,
};

use crate::{
    environment::{ServerEnvRef},
    utils::CoreError,
};

#[axum::debug_handler]
pub async fn evaluate_handler(
    Path(query): Path<String>,
    State(envref): State<ServerEnvRef>,
) -> Response<Body> {
    match envref.evaluate(query).await {
        Ok(asset) => {
            let dw: AssetDataResultWrapper = asset.get_binary().await.into();
            dw.into_response()
        },
        Err(e) => CoreError(e).into_response(),
    }
}
