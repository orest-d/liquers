use axum::body::Body;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use liquers_core::metadata::Metadata;

pub struct CoreError(pub liquers_core::error::Error);

impl From<liquers_core::error::Error> for CoreError {
    fn from(e: liquers_core::error::Error) -> Self {
        CoreError(e)
    }
}

impl IntoResponse for CoreError {
    fn into_response(self) -> Response<Body> {
        // TODO: make error specific response
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(header::CONTENT_TYPE, "text/plain".to_owned())
            .body(format!("Error: {}", self.0).into())
            .unwrap()
    }
}

pub struct DataResultWrapper(pub Result<(Vec<u8>, Metadata), liquers_core::error::Error>);

impl From<Result<(Vec<u8>, Metadata), liquers_core::error::Error>> for DataResultWrapper {
    fn from(r: Result<(Vec<u8>, Metadata), liquers_core::error::Error>) -> Self {
        DataResultWrapper(r)
    }
}

impl IntoResponse for DataResultWrapper {
    fn into_response(self) -> Response<Body> {
        match self.0 {
            Ok((data, metadata)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, metadata.get_media_type())
                .body(data.into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        }
    }
}

/* 
pub struct StoreResultWrapper<T>(pub Result<T, liquers_core::error::Error>);

impl StoreResultWrapper<T> {
    fn to_json(&self, key:&str) -> String {
        match self.0 {
            Ok(data) => serde_json::to_string(&data).unwrap(),
            Err(e) => serde_json::to_string(&e).unwrap(),
        }
    }
}

impl<T> From<Result<T, liquers_core::error::Error> > for StoreResultWrapper<T> {
    fn from(r: Result<T, liquers_core::error::Error>) -> Self {
        StoreResultWrapper(r)
    }
 

impl<T> IntoResponse for StoreResultWrapper<T> where T: IntoResponse {
    fn into_response(self) -> Response<Body> {
        match self.0 {
            Ok(data) => data.into_response(),
            Err(e) => CoreError(e).into_response(),
        }
    }
}

*/