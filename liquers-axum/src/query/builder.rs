use axum::{routing::get, Router};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

/// Builder for Query Execution API endpoints (GET/POST /q/{*query})
pub struct QueryApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> QueryApiBuilder<E> {
    /// Create a new QueryApiBuilder with the specified base path
    /// Example: QueryApiBuilder::new("/liquer/q")
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            _phantom: PhantomData,
        }
    }

    /// Build Axum router with query execution endpoints
    /// Returns a Router that can be merged into your application
    pub fn build(self) -> Router<EnvRef<E>> {
        Router::new()
            // GET /q/{*query} - Execute query via GET
            .route(
                &format!("{}/*query", self.base_path),
                get(crate::query::handlers::get_query_handler::<E>)
                    .post(crate::query::handlers::post_query_handler::<E>),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::value::Value;

    #[test]
    fn test_query_api_builder_creation() {
        let builder: QueryApiBuilder<SimpleEnvironment<Value>> =
            QueryApiBuilder::new("/liquer/q");
        assert_eq!(builder.base_path, "/liquer/q");
    }

    #[test]
    fn test_query_api_builder_with_custom_path() {
        let builder: QueryApiBuilder<SimpleEnvironment<Value>> = QueryApiBuilder::new("/api/query");
        assert_eq!(builder.base_path, "/api/query");
    }
}
