//! Recipes API Builder - Configurable router builder for Recipes API endpoints
//!
//! Part of the Recipes API implementation.
//! See specs/axum-assets-recipes-api/phase2-architecture.md for specifications.

use axum::{routing::get, Router};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

/// Builder for Recipes API endpoints (/api/recipes/*)
///
/// The Recipes API is read-only (HTTP GET only) and provides access to recipe definitions
/// via the AsyncRecipeProvider service.
pub struct RecipesApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> RecipesApiBuilder<E> {
    /// Create a new RecipesApiBuilder with the specified base path
    ///
    /// # Example
    /// ```ignore
    /// let builder = RecipesApiBuilder::new("/liquer/api/recipes");
    /// ```
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            _phantom: PhantomData,
        }
    }

    /// Build Axum router with all Recipes API endpoints
    ///
    /// Returns a Router that can be merged into your application
    ///
    /// All endpoints are read-only (HTTP GET)
    pub fn build(self) -> Router<EnvRef<E>> {
        let mut router = Router::new();

        // List all recipes - GET /listdir
        router = router.route(
            &format!("{}/listdir", self.base_path),
            get(crate::recipes::handlers::listdir_handler::<E>),
        );

        // Recipe data - GET /data/{*key}
        router = router.route(
            &format!("{}/data/{{*key}}", self.base_path),
            get(crate::recipes::handlers::get_data_handler::<E>),
        );

        // Recipe metadata - GET /metadata/{*key}
        router = router.route(
            &format!("{}/metadata/{{*key}}", self.base_path),
            get(crate::recipes::handlers::get_metadata_handler::<E>),
        );

        // Recipe entry (data + metadata) - GET /entry/{*key}
        router = router.route(
            &format!("{}/entry/{{*key}}", self.base_path),
            get(crate::recipes::handlers::get_entry_handler::<E>),
        );

        // Recipe resolution - GET /resolve/{*key}
        router = router.route(
            &format!("{}/resolve/{{*key}}", self.base_path),
            get(crate::recipes::handlers::resolve_handler::<E>),
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
    fn test_recipes_api_builder_new_creates_correct_structure() {
        let builder: RecipesApiBuilder<SimpleEnvironment<Value>> =
            RecipesApiBuilder::new("/liquer/api/recipes");
        assert_eq!(builder.base_path, "/liquer/api/recipes");
    }

    #[test]
    fn test_recipes_api_builder_has_no_websocket_path() {
        let builder: RecipesApiBuilder<SimpleEnvironment<Value>> =
            RecipesApiBuilder::new("/api/recipes");
        // Recipes API is HTTP-only, no WebSocket
        assert_eq!(builder.base_path, "/api/recipes");
    }

    #[test]
    fn test_recipes_api_builder_empty_path() {
        let builder: RecipesApiBuilder<SimpleEnvironment<Value>> = RecipesApiBuilder::new("");
        assert_eq!(builder.base_path, "");
    }
}
