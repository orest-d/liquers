use axum::{routing::get, Extension, Router};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;
use std::time::Duration;

use crate::query::handlers::QueryApiConfig;

/// Builder for Query Execution API endpoints (GET/POST /q/{*query})
pub struct QueryApiBuilder<E: Environment> {
    base_path: String,
    config: QueryApiConfig,
    _phantom: PhantomData<E>,
}

impl<E: Environment> QueryApiBuilder<E> {
    /// Create a new QueryApiBuilder with the specified base path
    /// Example: QueryApiBuilder::new("/liquer/q")
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            config: QueryApiConfig::default(),
            _phantom: PhantomData,
        }
    }

    /// How long `GET/POST {base}/{*query}` waits for a value before answering with an
    /// `ExecutionError` (default 30 s). A longer evaluation is better followed through the
    /// Assets API (`q/submit`, then `q/info` or `ws/q`), which the timeout message points to.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Build Axum router with query execution endpoints
    /// Returns a Router that can be merged into your application
    pub fn build(self) -> Router<EnvRef<E>> {
        Router::new()
            // GET /q/{*query} - Execute query via GET
            .route(
                &format!("{}{{*query}}", self.base_path),
                get(crate::query::handlers::get_query_handler::<E>)
                    .post(crate::query::handlers::post_query_handler::<E>),
            )
            .layer(Extension(self.config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::value::Value;

    #[test]
    fn test_query_api_builder_creation() {
        let builder: QueryApiBuilder<SimpleEnvironment<Value>> = QueryApiBuilder::new("/liquer/q");
        assert_eq!(builder.base_path, "/liquer/q");
        assert_eq!(builder.config.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_query_api_builder_with_timeout() {
        let builder: QueryApiBuilder<SimpleEnvironment<Value>> =
            QueryApiBuilder::new("/q").with_timeout(Duration::from_millis(100));
        assert_eq!(builder.config.timeout, Duration::from_millis(100));
    }

    #[test]
    fn test_query_api_builder_with_custom_path() {
        let builder: QueryApiBuilder<SimpleEnvironment<Value>> = QueryApiBuilder::new("/api/query");
        assert_eq!(builder.base_path, "/api/query");
    }
}
