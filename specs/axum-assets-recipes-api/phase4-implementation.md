# Phase 4: Implementation Plan - Axum Assets API and Recipes API

## Overview

This document provides a step-by-step implementation plan for the Assets API and Recipes API in liquers-axum. Each step includes exact file paths, function signatures, validation commands, and agent specifications.

**Total estimated steps:** 25 (across 7 implementation phases)

**Execution model:** Sequential with validation gates at phase boundaries

## Implementation Phases

1. **Module Structure Setup** (Steps 1-3) - Create directory structure and mod.rs files
2. **Builder Implementation** (Steps 4-5) - Implement AssetsApiBuilder and RecipesApiBuilder
3. **Assets API Handlers** (Steps 6-11) - Implement HTTP handlers for Assets API
4. **Recipes API Handlers** (Steps 12-16) - Implement HTTP handlers for Recipes API
5. **WebSocket Implementation** (Steps 17-19) - Implement WebSocket handler for Assets API
6. **Integration & Re-exports** (Steps 20-21) - Update lib.rs and wire everything together
7. **Testing & Validation** (Steps 22-25) - Run tests, fix issues, validate examples

---

## Step-by-Step Implementation

### Phase 1: Module Structure Setup

#### Step 1: Create Assets API module structure

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/mod.rs`

**Action:** Update module to export builder, handlers, and WebSocket modules

**Current state:**
```rust
// Placeholder module structure exists with commented-out exports
```

**Target state:**
```rust
pub mod builder;
pub mod handlers;
pub mod websocket;

pub use builder::AssetsApiBuilder;
```

**Validation:**
```bash
cargo check -p liquers-axum
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge: Phase 2 architecture (module structure section), existing liquers-axum/src/store/mod.rs pattern
- Task: Uncomment and organize module exports, ensure public API is minimal (only AssetsApiBuilder)

**Rollback:** Revert to commented-out state if compilation fails

---

#### Step 2: Create Recipes API module structure

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/mod.rs`

**Action:** Update module to export builder and handlers modules

**Current state:**
```rust
// Placeholder module structure exists with commented-out exports
```

**Target state:**
```rust
pub mod builder;
pub mod handlers;

pub use builder::RecipesApiBuilder;
```

**Validation:**
```bash
cargo check -p liquers-axum
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge: Phase 2 architecture (module structure section), existing liquers-axum/src/store/mod.rs pattern
- Task: Uncomment and organize module exports, ensure public API is minimal (only RecipesApiBuilder)

**Rollback:** Revert to commented-out state if compilation fails

---

#### Step 3: Create placeholder builder and handler files

**Files:**
- `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/builder.rs`
- `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`
- `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/websocket.rs`
- `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/builder.rs`
- `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/handlers.rs`

**Action:** Create empty files with module documentation

**Content for each file:**
```rust
//! [Module description]
//!
//! Part of the Assets API / Recipes API implementation.
//! See phase2-architecture.md for specifications.
```

**Validation:**
```bash
cargo check -p liquers-axum
```

**Agent specification:**
- Model: haiku
- Skills: None
- Knowledge: File paths from Phase 2
- Task: Create empty documented files to establish structure

**Rollback:** Remove created files if structure is incorrect

---

### Phase 2: Builder Implementation

#### Step 4: Implement AssetsApiBuilder

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/builder.rs`

**Action:** Implement complete AssetsApiBuilder with all methods

**Signature:**
```rust
use axum::{routing::{delete, get, post}, Router};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

pub struct AssetsApiBuilder<E: Environment> {
    base_path: String,
    websocket_path: Option<String>,
    _phantom: PhantomData<E>,
}

impl<E: Environment> AssetsApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn with_websocket_path(mut self, ws_path: impl Into<String>) -> Self;
    pub fn build(self) -> Router<EnvRef<E>>;
}
```

**Implementation details:**
- Default websocket_path to `Some(format!("{}/ws", base_path))`
- Build method creates routes following Phase 2 specifications:
  - `/data/{*query}` - GET, POST, DELETE
  - `/metadata/{*query}` - GET, POST
  - `/entry/{*query}` - GET, POST, DELETE
  - `/listdir/{*query}` - GET
  - `/cancel/{*query}` - POST
  - WebSocket route (if websocket_path is Some)
- Follow StoreApiBuilder pattern exactly (see liquers-axum/src/store/builder.rs)

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --lib assets::tests::test_assets_api_builder_new_creates_correct_structure
```

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (AssetsApiBuilder section, lines 120-162)
  - liquers-axum/src/store/builder.rs (pattern reference)
  - Phase 3 unit tests (assets/tests.rs, builder tests lines 26-92)
- Task: Implement builder following StoreApiBuilder pattern, ensure all routes use handler stubs (to be implemented in next steps)
- Rationale: Sonnet for builder complexity with route configuration

**Rollback:** Clear file contents, restore placeholder

---

#### Step 5: Implement RecipesApiBuilder

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/builder.rs`

**Action:** Implement complete RecipesApiBuilder with all methods

**Signature:**
```rust
use axum::{routing::get, Router};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

pub struct RecipesApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> RecipesApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn build(self) -> Router<EnvRef<E>>;
}
```

**Implementation details:**
- Build method creates routes following Phase 2 specifications:
  - `/listdir` - GET only
  - `/data/{*key}` - GET only
  - `/metadata/{*key}` - GET only
  - `/entry/{*key}` - GET only
  - `/resolve/{*key}` - GET only
- All routes are read-only (HTTP GET)
- No WebSocket support
- Follow StoreApiBuilder pattern

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --lib recipes::tests::test_recipes_api_builder_new_creates_correct_structure
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (RecipesApiBuilder section, lines 163-195)
  - liquers-axum/src/store/builder.rs (pattern reference)
  - Phase 3 unit tests (recipes/tests.rs, builder tests lines 25-85)
- Task: Implement builder following StoreApiBuilder pattern (simpler than Assets - no WebSocket, read-only)
- Rationale: Haiku sufficient for simpler read-only builder

**Rollback:** Clear file contents, restore placeholder

---

### Phase 3: Assets API Handlers

#### Step 6: Implement Assets API helper functions

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`

**Action:** Implement helper functions for format selection and error conversion

**Functions to implement:**
```rust
use liquers_core::error::Error;
use liquers_axum::api_core::{ErrorDetail, SerializationFormat};

/// Convert liquers_core::error::Error to api_core::ErrorDetail
pub fn error_to_detail(error: &Error) -> ErrorDetail;

/// Select serialization format from query param or Accept header
pub fn select_format(
    query_format: Option<String>,
    accept_header: Option<String>
) -> SerializationFormat;
```

**Implementation details:**
- `error_to_detail`: Reuse existing `liquers_axum::api_core::error::error_to_detail` function
- `select_format`: Reuse existing `liquers_axum::api_core::format::select_format` function
- These are re-exports or thin wrappers for consistency

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --lib assets::tests -- error_conversion
cargo test -p liquers-axum --lib assets::tests -- format_selection
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Error Handling section, lines 305-344)
  - liquers-axum/src/api_core/error.rs (existing implementation)
  - liquers-axum/src/api_core/format.rs (existing implementation)
  - Phase 3 unit tests (assets/tests.rs, lines 186-391)
- Task: Re-export or wrap existing api_core helper functions for Assets API namespace
- Rationale: Haiku for simple wrapper/re-export functions

**Rollback:** Remove helper functions if compilation fails

---

#### Step 7: Implement get_data_handler

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`

**Action:** Implement GET /data/{*query} handler

**Signature:**
```rust
use axum::{
    extract::{Path, State, Query as AxumQuery},
    response::Response,
};
use liquers_core::{
    context::{EnvRef, Environment},
    parse::parse_query,
};
use liquers_axum::api_core::{ApiResponse, BinaryResponse};
use std::collections::HashMap;

pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response
```

**Implementation details:**
1. Parse query from path using `parse_query(&query_path)`
2. Get AssetManager from environment: `env.get_asset_manager()`
3. Call `asset_manager.get(&query).await`
4. On success: Return BinaryResponse with data and metadata
5. On error: Convert to ErrorDetail and return ApiResponse::error

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_assets_api_get_data
```

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Assets API Handlers section, lines 196-243)
  - liquers-axum/src/store/handlers.rs (existing handler pattern, lines 13-46)
  - liquers-core/src/assets.rs (AssetManager trait)
  - Phase 3 integration tests (lines 100-168)
- Task: Implement handler following Store API pattern, use AssetManager.get() method
- Rationale: Sonnet for main data handler with AssetManager integration

**Rollback:** Comment out handler implementation

---

#### Step 8: Implement post_data_handler

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`

**Action:** Implement POST /data/{*query} handler

**Signature:**
```rust
use axum::body::Bytes;

pub async fn post_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    body: Bytes,
) -> Response
```

**Implementation details:**
1. Parse query from path
2. Get AssetManager from environment
3. Call `asset_manager.set(&query, body.to_vec()).await`
4. On success: Return ApiResponse::ok with empty result
5. On error: Convert to ErrorDetail and return ApiResponse::error

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_assets_api_post_data
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (POST /data handler, lines 244-267)
  - liquers-axum/src/store/handlers.rs (put_data_handler pattern)
  - liquers-core/src/assets.rs (AssetManager::set method)
- Task: Implement POST handler for asset data, similar to Store API put_data_handler
- Rationale: Haiku for straightforward POST handler

**Rollback:** Comment out handler implementation

---

#### Step 9: Implement delete_data_handler

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`

**Action:** Implement DELETE /data/{*query} handler

**Signature:**
```rust
pub async fn delete_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response
```

**Implementation details:**
1. Parse query from path
2. Get AssetManager from environment
3. Call `asset_manager.delete(&query).await`
4. On success: Return ApiResponse::ok with empty result
5. On error: Convert to ErrorDetail and return ApiResponse::error
6. Note: Deleting an asset with a recipe preserves the recipe (Phase 1 design decision)

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_assets_api_delete_data
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (DELETE /data handler, lines 268-285)
  - liquers-axum/src/store/handlers.rs (delete_data_handler pattern)
  - Phase 1 design (asset deletion semantics, line 77)
- Task: Implement DELETE handler, ensure recipe preservation semantics
- Rationale: Haiku for simple DELETE handler

**Rollback:** Comment out handler implementation

---

#### Step 10: Implement metadata and entry handlers

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`

**Action:** Implement handlers for metadata and entry endpoints

**Handlers to implement:**
```rust
pub async fn get_metadata_handler<E: Environment>(...) -> Response;
pub async fn post_metadata_handler<E: Environment>(...) -> Response;
pub async fn get_entry_handler<E: Environment>(...) -> Response;
pub async fn post_entry_handler<E: Environment>(...) -> Response;
pub async fn delete_entry_handler<E: Environment>(...) -> Response;
```

**Implementation details:**
- `get_metadata_handler`: Call `asset_manager.get_metadata(&query).await`, return as JSON
- `post_metadata_handler`: Call `asset_manager.set_metadata(&query, metadata).await`
- `get_entry_handler`: Call both get_data and get_metadata, return DataEntry with format negotiation
- `post_entry_handler`: Extract data and metadata from request body, call both set methods
- `delete_entry_handler`: Same as delete_data_handler (deletes asset, preserves recipe)

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration -- test_assets_api_metadata
cargo test -p liquers-axum --test assets_recipes_integration -- test_assets_api_entry
```

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Assets API Handlers, lines 196-304)
  - liquers-axum/src/store/handlers.rs (metadata and entry handler patterns)
  - liquers-axum/src/api_core/format.rs (serialize_data_entry, deserialize_data_entry)
  - Phase 3 integration tests (lines 173-311)
- Task: Implement 5 handlers following Store API patterns, handle format negotiation for entry endpoints
- Rationale: Sonnet for complex entry handlers with format negotiation

**Rollback:** Comment out handler implementations

---

#### Step 11: Implement directory and cancel handlers

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/handlers.rs`

**Action:** Implement handlers for listdir and cancel operations

**Handlers to implement:**
```rust
pub async fn listdir_handler<E: Environment>(...) -> Response;
pub async fn cancel_handler<E: Environment>(...) -> Response;
```

**Implementation details:**
- `listdir_handler`: Call `asset_manager.list_assets(&query_prefix).await`, return Vec<String> as JSON
- `cancel_handler`: Call `asset_manager.cancel(&query).await`, return ApiResponse::ok on success

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_assets_api_listdir
cargo test -p liquers-axum --test assets_recipes_integration test_assets_api_cancel
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Assets API Handlers, listdir and cancel sections)
  - liquers-core/src/assets.rs (AssetManager::list_assets, cancel methods)
  - Phase 3 integration tests (lines 320-357)
- Task: Implement 2 simpler handlers (listdir and cancel operations)
- Rationale: Haiku for straightforward operations

**Rollback:** Comment out handler implementations

---

### Phase 4: Recipes API Handlers

#### Step 12: Implement Recipes API helper functions

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/handlers.rs`

**Action:** Implement helper functions for key parsing and error conversion

**Functions to implement:**
```rust
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::error::Error;

/// Parse recipe key from path string
pub fn parse_recipe_key(key_path: &str) -> Result<Key, Error>;

/// Convert liquers_core::error::Error to api_core::ErrorDetail
pub fn error_to_detail(error: &Error) -> ErrorDetail;
```

**Implementation details:**
- `parse_recipe_key`: Wrapper around `parse_key`, handles empty paths
- `error_to_detail`: Reuse `liquers_axum::api_core::error::error_to_detail`

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --lib recipes::tests -- key_parsing
cargo test -p liquers-axum --lib recipes::tests -- error_conversion
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Recipes API Handlers section, lines 345-384)
  - liquers-core/src/parse.rs (parse_key function)
  - Phase 3 unit tests (recipes/tests.rs, lines 96-329)
- Task: Implement simple helper functions for Recipes API
- Rationale: Haiku for simple wrappers

**Rollback:** Remove helper functions

---

#### Step 13: Implement get_data_handler (Recipes)

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/handlers.rs`

**Action:** Implement GET /data/{*key} handler for recipes

**Signature:**
```rust
pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response
```

**Implementation details:**
1. Parse key from path using `parse_recipe_key(&key_path)`
2. Get AsyncRecipeProvider from environment: `env.get_recipe_provider()`
3. Call `recipe_provider.recipe(&key).await`
4. On success: Extract query string from Recipe, return as ApiResponse::ok
5. On error: Convert to ErrorDetail and return ApiResponse::error

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_recipes_api_get_data
```

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Recipes API Handlers, GET /data, lines 418-442)
  - liquers-core/src/recipes.rs (AsyncRecipeProvider trait, recipe method)
  - Phase 3 integration tests (lines 497-538)
- Task: Implement handler to fetch recipe definition from AsyncRecipeProvider
- Rationale: Sonnet for main recipe handler with AsyncRecipeProvider integration

**Rollback:** Comment out handler implementation

---

#### Step 14: Implement get_metadata_handler (Recipes)

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/handlers.rs`

**Action:** Implement GET /metadata/{*key} handler for recipes

**Signature:**
```rust
pub async fn get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response
```

**Implementation details:**
1. Parse key from path
2. Get AsyncRecipeProvider from environment
3. Call `recipe_provider.recipe(&key).await`
4. On success: Extract metadata fields (name, description, created_at, etc.) from Recipe
5. Return as ApiResponse::ok with Metadata object
6. On error: Convert to ErrorDetail

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_recipes_api_get_metadata
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Recipes API Handlers, GET /metadata, lines 443-466)
  - liquers-core/src/recipes.rs (Recipe struct fields)
  - Phase 3 integration tests (lines 543-565)
- Task: Implement handler to extract metadata from Recipe object
- Rationale: Haiku for metadata extraction

**Rollback:** Comment out handler implementation

---

#### Step 15: Implement entry, listdir, and resolve handlers (Recipes)

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/recipes/handlers.rs`

**Action:** Implement remaining Recipes API handlers

**Handlers to implement:**
```rust
pub async fn get_entry_handler<E: Environment>(...) -> Response;
pub async fn listdir_handler<E: Environment>(...) -> Response;
pub async fn resolve_handler<E: Environment>(...) -> Response;
```

**Implementation details:**
- `get_entry_handler`: Combine data (query string) and metadata (Recipe fields) into DataEntry
- `listdir_handler`: Call `recipe_provider.assets_with_recipes().await`, return Vec<Key> as JSON
- `resolve_handler`: Call `recipe_provider.recipe_plan(&key).await`, return execution plan as JSON

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration -- test_recipes_api_entry
cargo test -p liquers-axum --test assets_recipes_integration -- test_recipes_api_listdir
cargo test -p liquers-axum --test assets_recipes_integration -- test_recipes_api_resolve
```

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (Recipes API Handlers, lines 467-530)
  - liquers-core/src/recipes.rs (AsyncRecipeProvider trait methods)
  - liquers-axum/src/api_core/response.rs (DataEntry structure)
  - Phase 3 integration tests (lines 570-620)
- Task: Implement 3 handlers exposing AsyncRecipeProvider methods via HTTP
- Rationale: Sonnet for entry handler complexity and recipe resolution

**Rollback:** Comment out handler implementations

---

#### Step 16: Validate Recipes API handlers compilation

**Action:** Ensure all Recipes API handlers compile together

**Validation:**
```bash
cargo check -p liquers-axum
cargo clippy -p liquers-axum -- -D warnings
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge: Phase 2 architecture (complete Recipes API specification)
- Task: Fix any compilation errors, clippy warnings, or type mismatches
- Rationale: Haiku for simple compilation validation

**Rollback:** Revert to last working state if unfixable errors

---

### Phase 5: WebSocket Implementation

#### Step 17: Implement WebSocket message types

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/websocket.rs`

**Action:** Implement WebSocket message enums and serialization

**Types to implement:**
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum ClientMessage {
    Subscribe { query: String },
    Unsubscribe { query: String },
    UnsubscribeAll,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationMessage {
    Initial { asset_id: u64, query: String, timestamp: String, metadata: Option<serde_json::Value> },
    StatusChanged { asset_id: u64, query: String, status: String, timestamp: String },
    ProgressUpdated { asset_id: u64, query: String, primary_progress: Option<f64>, secondary_progress: Option<f64>, timestamp: String },
    ValueProduced { asset_id: u64, query: String, timestamp: String },
    ErrorOccurred { asset_id: u64, query: String, error: String, timestamp: String },
    RecipeDetected { asset_id: u64, query: String, recipe_query: String, timestamp: String },
    Submitted { asset_id: u64, query: String, timestamp: String },
    Processing { asset_id: u64, query: String, timestamp: String },
    Ready { asset_id: u64, query: String, timestamp: String },
    Cancelled { asset_id: u64, query: String, timestamp: String },
    Pong { timestamp: String },
    UnsubscribedAll { timestamp: String },
}
```

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --lib assets::tests -- notification_message
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (WebSocket Messages section, lines 78-119)
  - Phase 3 unit tests (assets/tests.rs, NotificationMessage tests, lines 104-175)
- Task: Implement message enums with serde serialization, ensure externally tagged format
- Rationale: Haiku for straightforward enum definitions

**Rollback:** Clear file contents

---

#### Step 18: Implement WebSocket connection handler

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/websocket.rs`

**Action:** Implement WebSocket upgrade and connection management

**Handler signature:**
```rust
use axum::{
    extract::{ws::WebSocket, Path, State, WebSocketUpgrade},
    response::Response,
};
use liquers_core::context::{EnvRef, Environment};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn websocket_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(query_path): Path<String>,
    State(env): State<EnvRef<E>>,
) -> Response
```

**Implementation details:**
1. Parse query from path (optional - may be empty for multiplexed connections)
2. Upgrade WebSocket connection
3. Spawn task to handle WebSocket messages
4. Maintain subscription map: `Arc<RwLock<HashMap<String, AssetRef<E>>>>`
5. Handle ClientMessage::Subscribe - subscribe to asset notifications
6. Handle ClientMessage::Unsubscribe - unsubscribe from asset
7. Handle ClientMessage::UnsubscribeAll - clear all subscriptions
8. Handle ClientMessage::Ping - respond with Pong
9. Forward asset notifications to client as NotificationMessage

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_websocket_integration
```

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (WebSocket Handler section, lines 531-601)
  - Axum WebSocket examples (https://docs.rs/axum/latest/axum/extract/ws/index.html)
  - Phase 3 integration tests (lines 938-996)
- Task: Implement WebSocket handler with subscription management and notification forwarding
- Rationale: Sonnet for complex async WebSocket handling with subscription tracking

**Rollback:** Comment out WebSocket handler

---

#### Step 19: Integrate WebSocket route into AssetsApiBuilder

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/assets/builder.rs`

**Action:** Wire WebSocket handler into builder's build() method

**Update build() method:**
```rust
pub fn build(self) -> Router<EnvRef<E>> {
    let mut router = Router::new();

    // ... existing routes ...

    // WebSocket route (if enabled)
    if let Some(ws_path) = self.websocket_path {
        router = router.route(
            &format!("{}/*query", ws_path),
            get(crate::assets::websocket::websocket_handler::<E>)
        );
    }

    router
}
```

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum --test assets_recipes_integration test_websocket_integration
```

**Agent specification:**
- Model: haiku
- Skills: rust-best-practices
- Knowledge:
  - Phase 2 architecture (AssetsApiBuilder section, lines 120-162)
  - Step 4 implementation (AssetsApiBuilder)
  - Step 18 implementation (WebSocket handler)
- Task: Integrate WebSocket route into builder conditionally
- Rationale: Haiku for simple route addition

**Rollback:** Remove WebSocket route addition

---

### Phase 6: Integration & Re-exports

#### Step 20: Update lib.rs with Assets and Recipes API exports

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/src/lib.rs`

**Action:** Add public re-exports for Assets and Recipes APIs

**Add to lib.rs:**
```rust
pub mod assets;
pub mod recipes;

pub use assets::AssetsApiBuilder;
pub use recipes::RecipesApiBuilder;
```

**Validation:**
```bash
cargo check -p liquers-axum
cargo doc -p liquers-axum --no-deps
```

**Agent specification:**
- Model: haiku
- Skills: None
- Knowledge: Phase 2 architecture (Integration Points section, lines 428-429)
- Task: Add module declarations and re-exports to lib.rs
- Rationale: Haiku for simple additions

**Rollback:** Remove added lines

---

#### Step 21: Update examples to use Assets and Recipes APIs

**File:** `/home/orest/zlos/rust/liquers/liquers-axum/examples/assets_recipes_basic.rs`

**Action:** Uncomment Assets and Recipes API router initialization

**Update router composition:**
```rust
let query_router = QueryApiBuilder::new("/liquer/q").build();
let store_router = StoreApiBuilder::new("/liquer/api/store").build();
let assets_router = AssetsApiBuilder::new("/liquer/api/assets")
    .with_websocket_path("/liquer/api/assets/ws")
    .build();
let recipes_router = RecipesApiBuilder::new("/liquer/api/recipes").build();

let app = axum::Router::new()
    .route("/", axum::routing::get(|| async { help_text() }))
    .merge(query_router)
    .merge(store_router)
    .merge(assets_router)
    .merge(recipes_router)
    .with_state(env_ref);
```

**Validation:**
```bash
cargo run -p liquers-axum --example assets_recipes_basic
# In another terminal:
curl http://localhost:3000/liquer/api/assets/metadata/text-hello
curl http://localhost:3000/liquer/api/recipes/listdir
```

**Agent specification:**
- Model: haiku
- Skills: None
- Knowledge: Phase 3 example (assets_recipes_basic.rs, lines 153-167)
- Task: Uncomment builder usage in example, verify server runs
- Rationale: Haiku for example update

**Rollback:** Re-comment API usage

---

### Phase 7: Testing & Validation

#### Step 22: Run unit tests

**Action:** Execute all unit tests for Assets and Recipes APIs

**Commands:**
```bash
cargo test -p liquers-axum --lib assets::tests
cargo test -p liquers-axum --lib recipes::tests
```

**Expected results:**
- Assets API: 34 tests pass
- Recipes API: 34 tests pass
- Total: 68 unit tests pass

**Agent specification:**
- Model: haiku
- Skills: liquers-unittest
- Knowledge: Phase 3 unit tests (assets/tests.rs, recipes/tests.rs)
- Task: Run tests, investigate any failures, fix issues
- Rationale: Haiku for test execution and simple fixes

**Failure handling:**
- If tests fail: Identify root cause, fix implementation, re-run
- If unfixable: Escalate to sonnet agent for debugging

---

#### Step 23: Run integration tests

**Action:** Execute all integration tests

**Commands:**
```bash
cargo test -p liquers-axum --test assets_recipes_integration
```

**Expected results:**
- 45 integration tests pass
- All test categories succeed (server setup, HTTP operations, WebSocket, concurrency, errors)

**Agent specification:**
- Model: sonnet
- Skills: liquers-unittest
- Knowledge:
  - Phase 3 integration tests (tests/assets_recipes_integration.rs)
  - All handler implementations (Steps 6-15)
  - WebSocket implementation (Steps 17-19)
- Task: Run full integration test suite, debug failures, ensure all 45 tests pass
- Rationale: Sonnet for complex integration test debugging

**Failure handling:**
- Categorize failures by test type
- Fix handler implementations
- Adjust WebSocket logic if needed
- Re-run until all tests pass

---

#### Step 24: Manual validation with example server

**Action:** Start example server and manually test all endpoints

**Test plan:**
```bash
# Terminal 1: Start server
cargo run -p liquers-axum --example assets_recipes_basic

# Terminal 2: Test Assets API
curl http://localhost:3000/liquer/api/assets/data/text-hello
curl http://localhost:3000/liquer/api/assets/metadata/text-hello/upper
curl "http://localhost:3000/liquer/api/assets/entry/text-world?format=json"
curl http://localhost:3000/liquer/api/assets/listdir/

# Terminal 3: Test Recipes API
curl http://localhost:3000/liquer/api/recipes/listdir
curl http://localhost:3000/liquer/api/recipes/data/example-recipe
curl http://localhost:3000/liquer/api/recipes/resolve/example-recipe

# Terminal 4: Test WebSocket (using websocat or wscat)
websocat ws://localhost:3000/liquer/api/assets/ws/text-hello
# Send: {"action": "Subscribe", "query": "text-hello"}
# Expect: NotificationMessage stream
```

**Agent specification:**
- Model: haiku
- Skills: None
- Knowledge: Phase 3 example (assets_recipes_basic.rs usage examples)
- Task: Execute manual tests, verify responses match expectations
- Rationale: Haiku for manual validation

**Success criteria:**
- All endpoints return valid responses
- WebSocket connection establishes and receives notifications
- Error responses have correct HTTP status codes
- Format negotiation works (CBOR, bincode, JSON)

---

#### Step 25: Final validation and documentation

**Action:** Run full test suite and update documentation

**Commands:**
```bash
# Full test suite
cargo test -p liquers-axum --all-targets

# Check examples compile
cargo test -p liquers-axum --examples

# Clippy validation
cargo clippy -p liquers-axum -- -D warnings

# Documentation generation
cargo doc -p liquers-axum --no-deps --open
```

**Documentation updates:**
- Update `liquers-axum/README.md` with Assets and Recipes API usage
- Add module-level documentation to `src/assets/mod.rs` and `src/recipes/mod.rs`
- Ensure all public types have doc comments

**Agent specification:**
- Model: sonnet
- Skills: rust-best-practices
- Knowledge:
  - Phase 1-3 documents (all design specifications)
  - All implemented code
  - Existing liquers-axum documentation patterns
- Task: Run full validation, generate documentation, ensure completeness
- Rationale: Sonnet for comprehensive final validation

**Success criteria:**
- All tests pass (68 unit + 45 integration = 113 tests)
- All examples compile and run
- No clippy warnings
- Documentation is complete and accurate

---

## Rollback Strategy

### Per-Step Rollback

Each step includes a specific rollback action. If a step fails:
1. Execute the rollback action (comment out code, delete files, revert changes)
2. Run `cargo check -p liquers-axum` to verify clean state
3. Investigate failure cause
4. Retry step with fixes

### Phase-Level Rollback

If an entire phase fails (e.g., all Assets API handlers fail):
1. Revert all changes in that phase (Steps N through M)
2. Return to last successful validation gate
3. Re-plan the phase with adjusted approach
4. Resume from failed phase

### Full Rollback

If implementation is fundamentally blocked:
1. Revert all changes (git reset or manual deletion)
2. Return to Phase 3 for design review
3. Identify architectural issue
4. Update Phase 2/3 documents
5. Restart Phase 4 with corrected design

---

## Testing Plan

### Unit Test Schedule

**After Step 4 (AssetsApiBuilder):**
- Run: `cargo test -p liquers-axum --lib assets::tests -- builder`
- Expect: 4 builder tests pass

**After Step 5 (RecipesApiBuilder):**
- Run: `cargo test -p liquers-axum --lib recipes::tests -- builder`
- Expect: 4 builder tests pass

**After Step 6 (Assets helpers):**
- Run: `cargo test -p liquers-axum --lib assets::tests -- error_conversion`
- Run: `cargo test -p liquers-axum --lib assets::tests -- format_selection`
- Expect: 10 helper tests pass

**After Step 12 (Recipes helpers):**
- Run: `cargo test -p liquers-axum --lib recipes::tests -- key_parsing`
- Run: `cargo test -p liquers-axum --lib recipes::tests -- error_conversion`
- Expect: 10 helper tests pass

**After Step 17 (WebSocket messages):**
- Run: `cargo test -p liquers-axum --lib assets::tests -- notification_message`
- Expect: 6 serialization tests pass

### Integration Test Schedule

**After Step 11 (All Assets handlers complete):**
- Run: `cargo test -p liquers-axum --test assets_recipes_integration -- test_assets_api`
- Expect: 20 Assets API integration tests pass

**After Step 15 (All Recipes handlers complete):**
- Run: `cargo test -p liquers-axum --test assets_recipes_integration -- test_recipes_api`
- Expect: 10 Recipes API integration tests pass

**After Step 18 (WebSocket handler):**
- Run: `cargo test -p liquers-axum --test assets_recipes_integration test_websocket_integration`
- Expect: 5 WebSocket tests pass

**After Step 21 (Example updated):**
- Run: `cargo run -p liquers-axum --example assets_recipes_basic`
- Manual testing: Verify all endpoints work

**After Step 23 (Full integration):**
- Run: `cargo test -p liquers-axum --test assets_recipes_integration`
- Expect: All 45 integration tests pass

### Continuous Validation

After every step:
```bash
cargo check -p liquers-axum
```

After every phase:
```bash
cargo clippy -p liquers-axum -- -D warnings
```

---

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | rust-best-practices | Simple module setup |
| 2 | haiku | rust-best-practices | Simple module setup |
| 3 | haiku | None | File creation |
| 4 | sonnet | rust-best-practices | Complex builder with route configuration |
| 5 | haiku | rust-best-practices | Simpler read-only builder |
| 6 | haiku | rust-best-practices | Simple wrapper functions |
| 7 | sonnet | rust-best-practices | Main data handler with AssetManager |
| 8 | haiku | rust-best-practices | Straightforward POST handler |
| 9 | haiku | rust-best-practices | Simple DELETE handler |
| 10 | sonnet | rust-best-practices | Complex entry handlers with format negotiation |
| 11 | haiku | rust-best-practices | Straightforward operations |
| 12 | haiku | rust-best-practices | Simple wrappers |
| 13 | sonnet | rust-best-practices | Main recipe handler with AsyncRecipeProvider |
| 14 | haiku | rust-best-practices | Metadata extraction |
| 15 | sonnet | rust-best-practices | Entry handler and recipe resolution |
| 16 | haiku | rust-best-practices | Compilation validation |
| 17 | haiku | rust-best-practices | Enum definitions |
| 18 | sonnet | rust-best-practices | Complex async WebSocket handling |
| 19 | haiku | rust-best-practices | Simple route addition |
| 20 | haiku | None | Simple additions |
| 21 | haiku | None | Example update |
| 22 | haiku | liquers-unittest | Test execution |
| 23 | sonnet | liquers-unittest | Integration test debugging |
| 24 | haiku | None | Manual validation |
| 25 | sonnet | rust-best-practices | Comprehensive final validation |

**Model distribution:**
- Haiku: 15 steps (simple, focused tasks)
- Sonnet: 10 steps (complex handlers, integration, testing)
- Opus: 0 steps (reserved for Phase 4 review)

---

## Validation Gates

### Gate 1: After Module Setup (Step 3)
- `cargo check -p liquers-axum` passes
- Module structure mirrors Store API structure
- All placeholder files exist

### Gate 2: After Builders (Step 5)
- Both builders compile
- Builder unit tests pass (8 tests)
- Routes are properly configured

### Gate 3: After Assets Handlers (Step 11)
- All Assets API handlers compile
- Assets API integration tests pass (20 tests)
- Example server starts and responds to Assets API requests

### Gate 4: After Recipes Handlers (Step 15)
- All Recipes API handlers compile
- Recipes API integration tests pass (10 tests)
- Example server responds to Recipes API requests

### Gate 5: After WebSocket (Step 19)
- WebSocket handler compiles
- WebSocket integration tests pass (5 tests)
- WebSocket connections work in example server

### Gate 6: After Integration (Step 21)
- Example server runs with all APIs enabled
- Manual testing succeeds for all endpoints
- No compilation warnings

### Gate 7: Final Validation (Step 25)
- All 113 tests pass (68 unit + 45 integration)
- All examples compile and run
- No clippy warnings
- Documentation complete

---

## Success Criteria

### Functional Requirements

✅ All Assets API endpoints functional:
- GET/POST/DELETE /data/{*query}
- GET/POST /metadata/{*query}
- GET/POST/DELETE /entry/{*query}
- GET /listdir/{*query}
- POST /cancel/{*query}
- WebSocket /ws/assets/{*query}

✅ All Recipes API endpoints functional:
- GET /listdir
- GET /data/{*key}
- GET /metadata/{*key}
- GET /entry/{*key}
- GET /resolve/{*key}

✅ All tests pass:
- 68 unit tests
- 45 integration tests
- Examples compile and run

### Quality Requirements

✅ No compilation warnings
✅ All clippy lints pass
✅ Documentation complete for public API
✅ Error handling uses liquers_core::error::Error exclusively
✅ Follows Rust best practices (no unwrap/expect in library code)
✅ Follows existing liquers-axum patterns (builder, handlers, api_core)

### Performance Requirements

✅ WebSocket connections handle subscriptions efficiently
✅ Concurrent requests don't cause data races
✅ Large data handling (10MB) works without memory issues

---

## Estimated Timeline

| Phase | Steps | Estimated Time | Agent Model |
|-------|-------|----------------|-------------|
| 1. Module Setup | 1-3 | 30 minutes | Haiku |
| 2. Builders | 4-5 | 1 hour | Sonnet + Haiku |
| 3. Assets Handlers | 6-11 | 3 hours | Sonnet + Haiku |
| 4. Recipes Handlers | 12-16 | 2 hours | Sonnet + Haiku |
| 5. WebSocket | 17-19 | 2 hours | Sonnet + Haiku |
| 6. Integration | 20-21 | 30 minutes | Haiku |
| 7. Testing | 22-25 | 2 hours | Sonnet + Haiku |
| **Total** | **25 steps** | **~11 hours** | Mixed |

**Parallel execution opportunities:**
- Builders (Steps 4-5): Can run in parallel → Save 30 minutes
- Handlers (Steps 6-15): Assets and Recipes can run in parallel → Save 2 hours
- **Adjusted total: ~8.5 hours**

---

## References

### Phase Documents
- Phase 1: `/home/orest/zlos/rust/liquers/specs/axum-assets-recipes-api/phase1-high-level-design.md`
- Phase 2: `/home/orest/zlos/rust/liquers/specs/axum-assets-recipes-api/phase2-architecture.md`
- Phase 3: `/home/orest/zlos/rust/liquers/specs/axum-assets-recipes-api/phase3-examples.md`

### Specification
- WEB_API_SPECIFICATION.md: Sections 5 (Assets API) and 6 (Recipes API)

### Codebase References
- Store API Builder: `liquers-axum/src/store/builder.rs`
- Store API Handlers: `liquers-axum/src/store/handlers.rs`
- API Core: `liquers-axum/src/api_core/`
- AssetManager: `liquers-core/src/assets.rs`
- AsyncRecipeProvider: `liquers-core/src/recipes.rs`

---

## Conclusion

This Phase 4 implementation plan provides a detailed, step-by-step roadmap for implementing the Assets API and Recipes API in liquers-axum. With 25 concrete steps organized into 7 phases, the implementation can proceed incrementally with validation gates to catch issues early.

**Ready for multi-agent review and user approval.**
