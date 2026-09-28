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
pub mod common;
pub mod key_handlers;
pub mod query_handlers;
pub mod value_description;
pub mod websocket;

pub use builder::AssetsApiBuilder;
pub use value_description::ValueDescription;
pub use websocket::WebSocketLimits;

