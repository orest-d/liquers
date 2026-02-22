/// Assets API module - HTTP REST API for asset management
///
/// Provides HTTP endpoints for:
/// - Getting/setting asset data and metadata
/// - Listing directory contents
/// - Canceling asset evaluations
/// - Real-time asset notifications via WebSocket
///
/// The Assets API wraps the `AssetManager` service and exposes it via HTTP.
pub mod builder;
pub mod handlers;
pub mod websocket;

pub use builder::AssetsApiBuilder;

#[cfg(test)]
mod tests;
