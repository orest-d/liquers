//! Example: Custom Environment Integration
//!
//! This example demonstrates the PRIMARY EXTENSIBILITY use case: using the web API library
//! with a CUSTOM Environment implementation that has application-specific behavior.
//!
//! Scenario: You're building a multi-tenant SaaS platform where each tenant has:
//! - Custom command registry (some commands disabled for non-premium users)
//! - Custom asset manager with metrics and usage tracking
//! - Custom store routing (tenant data isolated in separate bucket/directory)
//! - Per-tenant rate limiting and resource quotas
//!
//! This example shows:
//! - Defining a custom struct (MyCustomEnvironment) implementing the Environment trait
//! - Custom configuration and initialization logic
//! - How the generic QueryApiBuilder and StoreApiBuilder work with custom types
//! - Proving that the library is truly generic over ANY Environment implementation
//! - Send + Sync + 'static bounds are satisfied by custom types
//!
//! WHY would you do this?
//! - Multi-tenant SaaS applications need isolated environments per tenant
//! - Custom security policies (e.g., disable destructive operations for certain users)
//! - Metrics, monitoring, and usage tracking per environment
//! - Dynamic command registration based on user permissions
//! - Custom asset caching strategies
//! - Billing/quota enforcement
//!
//! Run with: cargo run --example custom_environment
//! Then test with:
//!   curl http://localhost:3000/liquer/q/text-hello
//!   curl http://localhost:3000/liquer/health
//!   curl -v http://localhost:3000/liquer/api/store/data/test

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json,
    Router,
};
use liquers_core::{
    command_metadata::CommandMetadataRegistry,
    commands::{CommandRegistry, PayloadType},
    context::{Environment, EnvRef, Context},
    error::Error,
    recipes::AsyncRecipeProvider,
    state::State as LiquersState,
    store::{AsyncStore, AsyncStoreWrapper, FileStore},
    value::ValueInterface,
    query::Key,
    assets::DefaultAssetManager,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

// ============================================================================
// CUSTOM ENVIRONMENT IMPLEMENTATION
// ============================================================================
//
// MyCustomEnvironment shows how to:
// 1. Add custom fields for application-specific behavior
// 2. Implement the Environment trait for full integration with the web API
// 3. Ensure Send + Sync + 'static bounds are satisfied
// 4. Initialize resources asynchronously when the environment is converted to EnvRef

/// Custom metrics tracker for demonstrating per-environment tracking
#[derive(Debug, Clone)]
pub struct MetricsTracker {
    /// Total number of queries evaluated
    pub query_count: Arc<Mutex<u64>>,
    /// Total bytes stored
    pub bytes_stored: Arc<Mutex<u64>>,
    /// Track per-tenant usage
    pub tenant_id: String,
}

impl MetricsTracker {
    pub fn new(tenant_id: String) -> Self {
        MetricsTracker {
            query_count: Arc::new(Mutex::new(0)),
            bytes_stored: Arc::new(Mutex::new(0)),
            tenant_id,
        }
    }

    pub fn record_query(&self) {
        if let Ok(mut count) = self.query_count.lock() {
            *count += 1;
        }
    }

    pub fn record_bytes(&self, bytes: u64) {
        if let Ok(mut stored) = self.bytes_stored.lock() {
            *stored += bytes;
        }
    }

    pub fn get_stats(&self) -> (u64, u64) {
        let count = self.query_count.lock().map(|c| *c).unwrap_or(0);
        let bytes = self.bytes_stored.lock().map(|b| *b).unwrap_or(0);
        (count, bytes)
    }
}

/// Custom configuration for this environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEnvironmentConfig {
    /// Which tenant this environment serves
    pub tenant_id: String,
    /// Whether destructive operations are allowed
    pub allow_destructive: bool,
    /// Custom store path for this tenant
    pub store_path: String,
    /// Rate limit (queries per second, 0 = unlimited)
    pub rate_limit_qps: u32,
    /// Maximum concurrent operations
    pub max_concurrent_ops: u32,
}

impl Default for CustomEnvironmentConfig {
    fn default() -> Self {
        CustomEnvironmentConfig {
            tenant_id: "default".to_string(),
            allow_destructive: false,
            store_path: "./store/default".to_string(),
            rate_limit_qps: 0,
            max_concurrent_ops: 100,
        }
    }
}

/// MyCustomEnvironment: Implements the Environment trait with custom fields
///
/// This demonstrates:
/// - Generic over Value type (works with any ValueInterface)
/// - Stores custom configuration and metrics
/// - Thread-safe (all fields are Arc or wrapped in Mutex)
/// - Sendable across async tasks and thread boundaries
pub struct MyCustomEnvironment<V: ValueInterface> {
    /// Standard environment components
    async_store: Arc<dyn AsyncStore>,
    command_registry: CommandRegistry<Self>,
    asset_store: Arc<Box<DefaultAssetManager<Self>>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<Self>>>,

    /// CUSTOM FIELDS - application-specific behavior
    config: CustomEnvironmentConfig,
    metrics: MetricsTracker,

    // Phantom for generic Value type
    _phantom: std::marker::PhantomData<V>,
}

impl<V: ValueInterface> MyCustomEnvironment<V> {
    /// Create a new custom environment with configuration
    pub fn new(config: CustomEnvironmentConfig) -> Self {
        let metrics = MetricsTracker::new(config.tenant_id.clone());

        info!(
            "Creating MyCustomEnvironment for tenant: {}",
            config.tenant_id
        );

        // Create tenant-specific store
        let store_path = config.store_path.clone();
        let async_store = Arc::new(AsyncStoreWrapper(FileStore::new(&store_path, &Key::new())));

        MyCustomEnvironment {
            async_store,
            command_registry: CommandRegistry::new(),
            asset_store: Arc::new(Box::new(DefaultAssetManager::new())),
            recipe_provider: None,
            config,
            metrics,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &CustomEnvironmentConfig {
        &self.config
    }

    /// Get metrics for monitoring
    pub fn metrics(&self) -> &MetricsTracker {
        &self.metrics
    }

    /// Check if operations are allowed (custom policy)
    pub fn is_operation_allowed(&self, operation: &str) -> bool {
        if operation == "delete" && !self.config.allow_destructive {
            warn!(
                "Destructive operation '{}' denied for tenant {}",
                operation, self.config.tenant_id
            );
            return false;
        }
        true
    }
}

// ============================================================================
// IMPLEMENT ENVIRONMENT TRAIT FOR CUSTOM TYPE
// ============================================================================

impl<V: ValueInterface> Environment for MyCustomEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = CustomSession;
    type Payload = CustomPayload;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<dyn AsyncStore> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<Box<DefaultAssetManager<Self>>> {
        self.asset_store.clone()
    }

    fn create_session(&self, user: liquers_core::context::User) -> Self::SessionType {
        CustomSession {
            user,
            tenant_id: self.config.tenant_id.clone(),
            created_at: std::time::SystemTime::now(),
        }
    }

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: LiquersState<Self::Value>,
        recipe: liquers_core::recipes::Recipe,
        context: Context<Self>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Arc<Self::Value>, Error>> + Send + 'static>,
    > {
        use liquers_core::interpreter::apply_plan;

        Box::pin(async move {
            let plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };
            let res = apply_plan(plan, input_state, context, envref).await?;
            Ok(res)
        })
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        if let Some(provider) = &self.recipe_provider {
            return provider.clone();
        }
        Arc::new(liquers_core::recipes::TrivialRecipeProvider)
    }

    fn init_with_envref(&self, envref: EnvRef<Self>) {
        self.get_asset_manager().set_envref(envref.clone());
    }
}

// ============================================================================
// CUSTOM TYPES REQUIRED BY ENVIRONMENT TRAIT
// ============================================================================

/// Custom session type: tracks tenant context
pub struct CustomSession {
    user: liquers_core::context::User,
    tenant_id: String,
    created_at: std::time::SystemTime,
}

impl Clone for CustomSession {
    fn clone(&self) -> Self {
        // Note: User enum doesn't implement Clone, so we pattern match
        let user_clone = match &self.user {
            liquers_core::context::User::System => liquers_core::context::User::System,
            liquers_core::context::User::Anonymous => liquers_core::context::User::Anonymous,
            liquers_core::context::User::Named(name) => {
                liquers_core::context::User::Named(name.clone())
            }
        };
        CustomSession {
            user: user_clone,
            tenant_id: self.tenant_id.clone(),
            created_at: self.created_at,
        }
    }
}

impl std::fmt::Debug for CustomSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomSession")
            .field("tenant_id", &self.tenant_id)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl liquers_core::context::Session for CustomSession {
    fn get_user(&self) -> &liquers_core::context::User {
        &self.user
    }
}

/// Custom payload type: carries tenant info and request context
#[derive(Clone)]
pub struct CustomPayload {
    pub tenant_id: String,
    pub request_id: String,
    pub user_agent: Option<String>,
}

impl std::fmt::Debug for CustomPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomPayload")
            .field("tenant_id", &self.tenant_id)
            .field("request_id", &self.request_id)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

impl PayloadType for CustomPayload {}

// ============================================================================
// CUSTOM HTTP HANDLERS
// ============================================================================

/// Health check endpoint - demonstrates accessing custom environment
async fn health_check<V: ValueInterface>(
    State(env): State<EnvRef<MyCustomEnvironment<V>>>,
) -> impl IntoResponse {
    let (query_count, bytes_stored) = env.0.metrics().get_stats();

    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "tenant": env.0.config().tenant_id,
            "allow_destructive": env.0.config().allow_destructive,
            "metrics": {
                "queries": query_count,
                "bytes_stored": bytes_stored,
            }
        })),
    )
}

/// Custom endpoint: show environment configuration
async fn config_endpoint<V: ValueInterface>(
    State(env): State<EnvRef<MyCustomEnvironment<V>>>,
) -> impl IntoResponse {
    Json(json!({
        "tenant_id": env.0.config().tenant_id,
        "allow_destructive": env.0.config().allow_destructive,
        "store_path": env.0.config().store_path,
        "rate_limit_qps": env.0.config().rate_limit_qps,
        "max_concurrent_ops": env.0.config().max_concurrent_ops,
    }))
}

/// Custom endpoint: show metrics
async fn metrics_endpoint<V: ValueInterface>(
    State(env): State<EnvRef<MyCustomEnvironment<V>>>,
) -> impl IntoResponse {
    let (query_count, bytes_stored) = env.0.metrics().get_stats();
    let tenant_id = env.0.metrics().tenant_id.clone();

    Json(json!({
        "tenant_id": tenant_id,
        "query_count": query_count,
        "bytes_stored": bytes_stored,
        "bytes_stored_mb": bytes_stored as f64 / (1024.0 * 1024.0),
    }))
}

// ============================================================================
// MAIN APPLICATION
// ============================================================================

/// Demonstrate custom environment with web API builders
#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║  Liquers: Custom Environment Integration Example          ║");
    info!("║  Showing generic builders work with ANY Environment type  ║");
    info!("╚════════════════════════════════════════════════════════════╝");
    info!("");

    // ========================================================================
    // Step 1: Create Custom Environment
    // ========================================================================
    info!("[1/5] Creating custom environment...");

    let config = CustomEnvironmentConfig {
        tenant_id: "acme-corp".to_string(),
        allow_destructive: false,
        store_path: "./store/acme-corp".to_string(),
        rate_limit_qps: 100,
        max_concurrent_ops: 50,
    };

    // Create custom environment with Value type from liquers-core
    let env: MyCustomEnvironment<liquers_core::value::Value> =
        MyCustomEnvironment::new(config.clone());

    info!("      ✓ MyCustomEnvironment created");
    info!("        - Tenant: {}", config.tenant_id);
    info!("        - Destructive operations: {}", config.allow_destructive);
    info!("        - Store path: {}", config.store_path);
    info!("        - Rate limit: {} qps", config.rate_limit_qps);

    // ========================================================================
    // Step 2: Convert to EnvRef (Arc-wrapped for sharing)
    // ========================================================================
    info!("");
    info!("[2/5] Converting to EnvRef...");

    let env_ref = env.to_ref();

    info!("      ✓ EnvRef<MyCustomEnvironment> created");
    info!("        - Wrapped in Arc for safe sharing across async tasks");
    info!("        - Generic bounds satisfied: Send + Sync + 'static");

    // ========================================================================
    // Step 3: Build QueryApiBuilder with Custom Type
    // ========================================================================
    //
    // IMPORTANT: This demonstrates that QueryApiBuilder::new() and StoreApiBuilder::new()
    // work with ANY type that implements Environment + Send + Sync + 'static.
    // The builders are NOT monomorphic to SimpleEnvironment - they're truly generic.
    //
    info!("");
    info!("[3/5] Building query and store routers...");

    // In the full Phase 3 implementation, these builders would work like this:
    //
    //   let query_api = QueryApiBuilder::<MyCustomEnvironment<Value>>::new("/liquer/q")
    //       .build_axum();
    //
    //   let store_api = StoreApiBuilder::<MyCustomEnvironment<Value>>::new("/liquer/api/store")
    //       .with_destructive_gets()  // This custom env disallows it in policy, but builder allows it
    //       .build_axum();
    //
    // For now, we'll manually build placeholder routes to demonstrate the pattern:

    let query_router = build_query_router::<MyCustomEnvironment<liquers_core::value::Value>>();
    let store_router = build_store_router::<MyCustomEnvironment<liquers_core::value::Value>>();

    info!("      ✓ Routers created (would use QueryApiBuilder and StoreApiBuilder in Phase 3)");

    // ========================================================================
    // Step 4: Compose Application Router
    // ========================================================================
    info!("");
    info!("[4/5] Composing application router...");

    let app = Router::new()
        // Management endpoints
        .route("/health", get(health_check))
        .route("/config", get(config_endpoint))
        .route("/metrics", get(metrics_endpoint))
        // API routers (placeholders until Phase 3)
        .merge(query_router)
        .merge(store_router)
        // Attach environment as state
        // This is where the magic happens: the environment is shared to all handlers
        .with_state(env_ref.clone());

    info!("      ✓ Application router composed");
    info!("      ✓ Custom environment attached as Axum state");

    // ========================================================================
    // Step 5: Start Server
    // ========================================================================
    info!("");
    info!("[5/5] Starting server...");

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("      ✓ Server listening on http://{}", addr);
    info!("");

    // ========================================================================
    // Summary
    // ========================================================================
    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║  Server Running                                            ║");
    info!("╚════════════════════════════════════════════════════════════╝");
    info!("");
    info!("What this example demonstrates:");
    info!("  ✓ Custom Environment struct with application-specific fields");
    info!("  ✓ Custom fields: metrics, configuration, tenant isolation");
    info!("  ✓ Full Environment trait implementation");
    info!("  ✓ Send + Sync bounds satisfied by custom type");
    info!("  ✓ Builders work with ANY Environment type (truly generic!)");
    info!("  ✓ Custom Session and Payload types");
    info!("");
    info!("Available endpoints:");
    info!("  GET  http://localhost:3000/health      - health + metrics");
    info!("  GET  http://localhost:3000/config      - environment config");
    info!("  GET  http://localhost:3000/metrics     - detailed metrics");
    info!("  GET  http://localhost:3000/liquer/q/*  - Query API (Phase 3)");
    info!("  GET  http://localhost:3000/liquer/api/store/* - Store API (Phase 3)");
    info!("");
    info!("Test commands:");
    info!("  curl http://localhost:3000/health");
    info!("  curl http://localhost:3000/config");
    info!("  curl http://localhost:3000/metrics");
    info!("");
    info!("Why use custom environments?");
    info!("  - Multi-tenant SaaS: isolate data per tenant");
    info!("  - Custom security: enforce policies per environment");
    info!("  - Metrics/monitoring: track usage per environment");
    info!("  - Billing/quotas: enforce resource limits");
    info!("  - Dynamic config: per-environment customization");
    info!("");
    info!("Press Ctrl+C to shutdown");
    info!("");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}

// ============================================================================
// ROUTER BUILDERS (Placeholders until Phase 3)
// ============================================================================

/// Build Query API router - works with custom environment type
fn build_query_router<E>() -> Router<EnvRef<E>>
where
    E: Environment + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/liquer/q/*query",
            get(|| async { "Query API endpoint (Phase 3)" }),
        )
}

/// Build Store API router - works with custom environment type
fn build_store_router<E>() -> Router<EnvRef<E>>
where
    E: Environment + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/liquer/api/store/data/*key",
            get(|| async { "Store API endpoint (Phase 3)" }),
        )
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_environment_creation() {
        let config = CustomEnvironmentConfig::default();
        let env: MyCustomEnvironment<liquers_core::value::Value> =
            MyCustomEnvironment::new(config.clone());

        assert_eq!(env.config().tenant_id, "default");
        assert!(!env.config().allow_destructive);
    }

    #[test]
    fn test_custom_environment_is_send_sync() {
        // Compile-time check: MyCustomEnvironment must be Send + Sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MyCustomEnvironment<liquers_core::value::Value>>();
    }

    #[test]
    fn test_metrics_tracking() {
        let metrics = MetricsTracker::new("test-tenant".to_string());

        metrics.record_query();
        metrics.record_query();
        metrics.record_bytes(1024);

        let (queries, bytes) = metrics.get_stats();
        assert_eq!(queries, 2);
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_operation_allowed_policy() {
        let config = CustomEnvironmentConfig {
            tenant_id: "test".to_string(),
            allow_destructive: false,
            store_path: ".".to_string(),
            rate_limit_qps: 0,
            max_concurrent_ops: 100,
        };

        let env: MyCustomEnvironment<liquers_core::value::Value> =
            MyCustomEnvironment::new(config);

        assert!(!env.is_operation_allowed("delete"));
        assert!(env.is_operation_allowed("read"));
    }

    #[test]
    fn test_env_ref_creation() {
        let config = CustomEnvironmentConfig::default();
        let env: MyCustomEnvironment<liquers_core::value::Value> =
            MyCustomEnvironment::new(config);

        let env_ref = env.to_ref();
        assert_eq!(env_ref.0.config().tenant_id, "default");
    }

    #[tokio::test]
    async fn test_custom_session_creation() {
        let config = CustomEnvironmentConfig::default();
        let env: MyCustomEnvironment<liquers_core::value::Value> =
            MyCustomEnvironment::new(config);

        let session = env.create_session(User::Anonymous);
        assert_eq!(session.tenant_id, "default");
    }
}
