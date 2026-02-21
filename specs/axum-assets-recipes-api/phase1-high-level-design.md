# Phase 1: High-Level Design - Assets API and Recipes API

## Feature Name

Axum Assets API and Recipes API

## Purpose

Implement HTTP REST endpoints for asset lifecycle management and recipe operations in liquers-axum, completing the web API specification. The Assets API provides access to computed/cached data with real-time progress tracking via WebSocket, while the Recipes API exposes recipe definitions and resolution for client-driven query construction.

## Core Interactions

### Query System
Assets API uses full query syntax (not just keys) - queries may include commands and are evaluated on-demand.
Recipes API uses keys to identify recipe definitions, which resolve to query strings.
Example: `GET /api/assets/data/-R/path/to/-/cmd-arg` vs `GET /api/recipes/data/my-recipe`

### Store System
Assets API operates through AssetManager (thin HTTP wrapper around AssetManager methods).
Recipes API operates through AsyncRecipeProvider (thin HTTP wrapper around AsyncRecipeProvider methods).
Both APIs expose existing service interfaces via HTTP; no new store implementations needed.

### Command System
No new commands introduced - these are pure HTTP API endpoints.
Assets API triggers command execution when assets are accessed (via AssetManager.get).
Recipes API exposes recipe query strings that reference existing commands.

### Asset System
Assets API is the HTTP interface to AssetManager (exposes AssetManager methods as REST endpoints):
- Lifecycle: None → Recipe → Submitted → Processing → Ready/Error
- Progress tracking: Primary/secondary progress updates
- Real-time notifications via WebSocket (StatusChanged, ProgressUpdated, etc.)
- Cancel operation support for in-flight evaluations

Recipes API is the HTTP interface to AsyncRecipeProvider (exposes AsyncRecipeProvider methods as REST endpoints):
- `listdir` → `assets_with_recipes()`
- `data/{*key}` → `recipe()` / `recipe_opt()`
- `metadata/{*key}` → `recipe()` + extract metadata fields
- `entry/{*key}` → `recipe()` + combined data/metadata response
- `resolve/{*key}` → `recipe_plan()`

### Value Types
No new ExtValue variants.
Uses existing DataEntry structure from api_core for unified data/metadata responses.
Asset values converted to bytes for HTTP responses.

### Web/API
**Assets API** (`/liquer/api/assets/*`):
- GET/POST/DELETE `/data/{*query}`, `/metadata/{*query}`, `/entry/{*query}`
- GET `/listdir/{*query}` - list assets in directory
- POST `/cancel/{*query}` - cancel running evaluation
- WebSocket `/ws/assets/{*query}` - real-time notifications

**Recipes API** (`/liquer/api/recipes/*`):
- GET `/listdir` - list all recipes
- GET `/data/{*key}`, `/metadata/{*key}`, `/entry/{*key}` - recipe details
- GET `/resolve/{*key}` - resolve recipe to execution plan

### UI
Not applicable - server-side HTTP API only.

## Crate Placement

**liquers-axum** - All implementation in new modules:
- `src/assets/mod.rs`, `src/assets/builder.rs`, `src/assets/handlers.rs`
- `src/assets/websocket.rs` - WebSocket notification handler
- `src/recipes/mod.rs`, `src/recipes/builder.rs`, `src/recipes/handlers.rs`

Rationale: Web API belongs in liquers-axum; follows existing Store API pattern.
Reuses `api_core` types (ApiResponse, DataEntry, ErrorDetail, SerializationFormat).

No changes to liquers-core, liquers-store, or liquers-lib.

## Design Decisions

1. **WebSocket multiplexing**: Single WebSocket connection supports subscribing to multiple assets via subscribe/unsubscribe messages
2. **Asset deletion semantics**: Deleting an asset with a recipe preserves the recipe (status returns to Recipe, allowing re-computation)
3. **API responsibility**: Both APIs are thin HTTP wrappers:
   - Assets API → AssetManager methods (get, get_metadata, cancel, subscribe, etc.)
   - Recipes API → AsyncRecipeProvider methods (recipe, assets_with_recipes, recipe_plan, etc.)
4. **Query parsing in Assets API**: Invalid queries return 400 Bad Request with ParseError (fail fast before evaluation)
5. **WebSocket auth/session**: Delegate authentication to axum/tower middleware layers (investigate Layer pattern in Phase 2)

## Open Questions

(To be resolved in Phase 2 architecture)

1. **WebSocket session management**: How to pass authenticated session context through the WebSocket upgrade?
2. **Asset notification fan-out**: How to efficiently broadcast notifications to multiple WebSocket clients subscribed to the same asset?
3. **Recipe serialization format**: Should recipes be returned as JSON-serialized Recipe struct or custom format?

## References

- WEB_API_SPECIFICATION.md - Sections 5 (Assets API) and 6 (Recipes API)
- liquers-axum/src/store/ - Existing Store API implementation for pattern reference
- liquers-core/src/assets.rs - AssetManager trait and AssetRef lifecycle
- liquers-core/src/recipes.rs - AsyncRecipeProvider trait
