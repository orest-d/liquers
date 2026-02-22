//! Assets API Builder - Configurable router builder for Assets API endpoints
//!
//! Part of the Assets API implementation.
//! See specs/axum-assets-recipes-api/phase2-architecture.md for specifications.

use axum::{
    routing::{get, post},
    Router,
};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

/// Builder for Assets API endpoints (/api/assets/*)
pub struct AssetsApiBuilder<E: Environment> {
    base_path: String,
    websocket_path: Option<String>,
    _phantom: PhantomData<E>,
}

impl<E: Environment> AssetsApiBuilder<E> {
    /// Create a new AssetsApiBuilder with the specified base path
    ///
    /// WebSocket endpoint defaults to `{base_path}/ws`
    ///
    /// # Example
    /// ```ignore
    /// let builder = AssetsApiBuilder::new("/liquer/api/assets");
    /// ```
    pub fn new(base_path: impl Into<String>) -> Self {
        let base_path = base_path.into();
        let websocket_path = Some(format!("{}/ws", base_path));
        Self {
            base_path,
            websocket_path,
            _phantom: PhantomData,
        }
    }

    /// Set a custom WebSocket endpoint path
    ///
    /// # Example
    /// ```ignore
    /// let builder = AssetsApiBuilder::new("/api/assets")
    ///     .with_websocket_path("/api/assets/notifications");
    /// ```
    pub fn with_websocket_path(mut self, ws_path: impl Into<String>) -> Self {
        self.websocket_path = Some(ws_path.into());
        self
    }

    /// Disable WebSocket endpoint
    pub fn without_websocket(mut self) -> Self {
        self.websocket_path = None;
        self
    }

    /// Build Axum router with all Assets API endpoints
    ///
    /// Returns a Router that can be merged into your application
    pub fn build(self) -> Router<EnvRef<E>> {
        let mut router = Router::new();

        // Data endpoints - GET, POST, DELETE /data/{*query}
        router = router.route(
            &format!("{}/data/{{*query}}", self.base_path),
            get(crate::assets::handlers::get_data_handler::<E>)
                .post(crate::assets::handlers::post_data_handler::<E>)
                .delete(crate::assets::handlers::delete_data_handler::<E>),
        );

        // Metadata endpoints - GET, POST /metadata/{*query}
        router = router.route(
            &format!("{}/metadata/{{*query}}", self.base_path),
            get(crate::assets::handlers::get_metadata_handler::<E>)
                .post(crate::assets::handlers::post_metadata_handler::<E>),
        );

        // Unified entry endpoints - GET, POST, DELETE /entry/{*query}
        router = router.route(
            &format!("{}/entry/{{*query}}", self.base_path),
            get(crate::assets::handlers::get_entry_handler::<E>)
                .post(crate::assets::handlers::post_entry_handler::<E>)
                .delete(crate::assets::handlers::delete_entry_handler::<E>),
        );

        // Directory listing - GET /listdir/{*query}
        router = router.route(
            &format!("{}/listdir/{{*query}}", self.base_path),
            get(crate::assets::handlers::listdir_handler::<E>),
        );

        // Cancel operation - POST /cancel/{*query}
        router = router.route(
            &format!("{}/cancel/{{*query}}", self.base_path),
            post(crate::assets::handlers::cancel_handler::<E>),
        );

        // WebSocket endpoint (if enabled) - GET /ws/{*query}
        if let Some(ws_path) = self.websocket_path {
            router = router.route(
                &format!("{}/*query", ws_path),
                get(crate::assets::websocket::websocket_handler::<E>),
            );
        }

        router
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::value::Value;

    #[test]
    fn test_assets_api_builder_new_creates_correct_structure() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/liquer/api/assets");
        assert_eq!(builder.base_path, "/liquer/api/assets");
        assert_eq!(
            builder.websocket_path,
            Some("/liquer/api/assets/ws".to_string())
        );
    }

    #[test]
    fn test_assets_api_builder_with_websocket_path_sets_custom_path() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/api/assets").with_websocket_path("/api/ws/assets");
        assert_eq!(builder.websocket_path, Some("/api/ws/assets".to_string()));
    }

    #[test]
    fn test_assets_api_builder_without_websocket_disables_ws() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/api/assets").without_websocket();
        assert_eq!(builder.websocket_path, None);
    }

    #[test]
    fn test_assets_api_builder_method_chaining() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/api/assets")
                .with_websocket_path("/ws")
                .without_websocket()
                .with_websocket_path("/api/notifications");
        assert_eq!(builder.base_path, "/api/assets");
        assert_eq!(
            builder.websocket_path,
            Some("/api/notifications".to_string())
        );
    }
}
