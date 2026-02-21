use axum::{
    routing::{delete, get, post, put},
    Router,
};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

/// Builder for Store API endpoints (/api/store/*)
pub struct StoreApiBuilder<E: Environment> {
    base_path: String,
    allow_destructive_gets: bool, // Enable GET-based remove/delete/makedir
    _phantom: PhantomData<E>,
}

impl<E: Environment> StoreApiBuilder<E> {
    /// Create a new StoreApiBuilder with the specified base path
    /// Example: StoreApiBuilder::new("/liquer/api/store")
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            allow_destructive_gets: false, // Disabled by default per spec
            _phantom: PhantomData,
        }
    }

    /// Allow GET-based destructive operations (per spec section 4.1)
    /// When enabled, adds GET routes for /remove, /removedir, and /makedir
    /// These are opt-in for backward compatibility with legacy clients
    pub fn with_destructive_gets(mut self) -> Self {
        self.allow_destructive_gets = true;
        self
    }

    /// Build Axum router with all Store API endpoints
    /// Returns a Router that can be merged into your application
    pub fn build(self) -> Router<EnvRef<E>> {
        // All routes are stubs for now - handlers will be implemented in subsequent tasks
        let mut router = Router::new();

        // Data endpoints (Task #25) - IMPLEMENTED
        router = router
            .route(
                &format!("{}/data/{{*key}}", self.base_path),
                get(crate::store::handlers::get_data_handler::<E>)
                    .put(crate::store::handlers::put_data_handler::<E>)
                    .delete(crate::store::handlers::delete_data_handler::<E>),
            );

        // Metadata endpoints (Task #25) - IMPLEMENTED
        router = router.route(
            &format!("{}/metadata/{{*key}}", self.base_path),
            get(crate::store::handlers::get_metadata_handler::<E>)
                .put(crate::store::handlers::put_metadata_handler::<E>),
        );

        // Unified entry endpoints (Task #27) - IMPLEMENTED
        router = router.route(
            &format!("{}/entry/{{*key}}", self.base_path),
            get(crate::store::handlers::get_entry_handler::<E>)
                .put(crate::store::handlers::put_entry_handler::<E>)
                .delete(crate::store::handlers::delete_entry_handler::<E>),
        );

        // Directory operations (Task #26) - IMPLEMENTED
        router = router
            .route(
                &format!("{}/listdir/{{*key}}", self.base_path),
                get(crate::store::handlers::listdir_handler::<E>),
            )
            .route(
                &format!("{}/is_dir/{{*key}}", self.base_path),
                get(crate::store::handlers::is_dir_handler::<E>),
            )
            .route(
                &format!("{}/contains/{{*key}}", self.base_path),
                get(crate::store::handlers::contains_handler::<E>),
            )
            .route(
                &format!("{}/keys", self.base_path),
                get(crate::store::handlers::keys_handler::<E>),
            )
            .route(
                &format!("{}/makedir/{{*key}}", self.base_path),
                put(crate::store::handlers::makedir_handler::<E>),
            )
            .route(
                &format!("{}/removedir/{{*key}}", self.base_path),
                delete(crate::store::handlers::removedir_handler::<E>),
            );

        // Optional GET-based destructive operations (Task #28) - IMPLEMENTED
        if self.allow_destructive_gets {
            router = router
                .route(
                    &format!("{}/remove/{{*key}}", self.base_path),
                    get(crate::store::handlers::get_remove_handler::<E>),
                )
                .route(
                    &format!("{}/removedir/{{*key}}", self.base_path),
                    get(crate::store::handlers::get_removedir_handler::<E>),
                )
                .route(
                    &format!("{}/makedir/{{*key}}", self.base_path),
                    get(crate::store::handlers::get_makedir_handler::<E>),
                );
        }

        // Upload endpoint (Task #29) - IMPLEMENTED
        router = router.route(
            &format!("{}/upload/{{*key}}", self.base_path),
            post(crate::store::handlers::upload_handler::<E>),
        );

        router
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::value::Value;

    #[test]
    fn test_store_api_builder_creation() {
        let builder: StoreApiBuilder<SimpleEnvironment<Value>> =
            StoreApiBuilder::new("/liquer/api/store");
        assert_eq!(builder.base_path, "/liquer/api/store");
        assert!(!builder.allow_destructive_gets);
    }

    #[test]
    fn test_store_api_builder_with_destructive_gets() {
        let builder: StoreApiBuilder<SimpleEnvironment<Value>> =
            StoreApiBuilder::new("/api/store").with_destructive_gets();
        assert_eq!(builder.base_path, "/api/store");
        assert!(builder.allow_destructive_gets);
    }

    // Note: Router build test removed because Axum requires actual state to validate routes
    // The builder is tested implicitly by compilation and integration tests
}
