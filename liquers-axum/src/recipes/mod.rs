/// Recipes API module - HTTP REST API for recipe management
///
/// Provides HTTP endpoints for:
/// - Listing available recipes
/// - Getting recipe definitions and metadata
/// - Getting unified recipe entries (data+metadata)
/// - Resolving recipes to execution plans
///
/// The Recipes API wraps the `AsyncRecipeProvider` service and exposes it via HTTP.
/// This API is read-only (no POST/PUT/DELETE operations).

pub mod builder;
pub mod handlers;

pub use builder::RecipesApiBuilder;

#[cfg(test)]
mod tests;
