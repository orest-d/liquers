pub mod api_core;
pub mod assets;
pub mod axum_integration;
pub mod query;
pub mod recipes;
pub mod store;

// Re-exports for convenience
pub use api_core::{ApiResponse, BinaryResponse, DataEntry, ErrorDetail, SerializationFormat};
pub use assets::AssetsApiBuilder;
pub use query::QueryApiBuilder;
pub use recipes::RecipesApiBuilder;
pub use store::StoreApiBuilder;

// Re-export EnvRef from liquers-core
pub use liquers_core::context::EnvRef;
