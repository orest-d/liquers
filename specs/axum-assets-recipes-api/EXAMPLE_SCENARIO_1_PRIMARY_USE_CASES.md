# Example Scenario 1: Primary Use Cases

## Overview

This example demonstrates the primary use cases for the **Assets API** and **Recipes API**. It shows:

1. **Assets API**: HTTP interface for computed/cached asset lifecycle (retrieval, metadata checks, unified data+metadata)
2. **Recipes API**: HTTP interface for recipe definitions and resolution

The example is a fully functional Axum server that you can run locally to test both APIs.

## Code

**File:** `liquers-axum/examples/assets_recipes_basic.rs`

This example provides:
- A working server with Query API and Store API (existing functionality)
- Clear comments explaining Assets API and Recipes API endpoints
- Registration of example commands (`text`, `upper`, `reverse`, `count`)
- Detailed help text showing usage examples
- Realistic demonstrations of primary use cases

**Key features:**

1. **Environment setup** - Creates a SimpleEnvironment with file-based store
2. **Command registration** - Registers 4 example commands for transforming text
3. **Router composition** - Shows how to integrate Query API, Store API, and future Assets/Recipes APIs
4. **Comprehensive documentation** - Inline comments and help text for each API

## Usage

### Run the example:

```bash
cargo run -p liquers-axum --example assets_recipes_basic
```

Expected output:
```
================================================================================
Assets API and Recipes API - Primary Use Cases Example
================================================================================

Store path: .

Server listening on http://0.0.0.0:3000

--------------------------------------------------------------------------------
USAGE EXAMPLES
--------------------------------------------------------------------------------

[... detailed usage examples ...]

Press Ctrl+C to stop
```

### Test the APIs:

The example server supports Query API and Store API (existing APIs). The Assets API and Recipes API endpoints are documented but not yet implemented.

#### Query API (existing - fully functional):

```bash
# Simple text command
curl http://localhost:3000/liquer/q/text-hello
# Output: hello

# Chained commands - uppercase transformation
curl http://localhost:3000/liquer/q/text-world/upper
# Output: WORLD

# Multiple transformations
curl http://localhost:3000/liquer/q/text-liquers/reverse/upper
# Output: SREUCIL

# Count characters
curl http://localhost:3000/liquer/q/text-hello/count
# Output: 5
```

#### Store API (existing - fully functional):

```bash
# List all keys in store
curl http://localhost:3000/liquer/api/store/keys
# Output: {"status": "ok", "result": [], "message": "..."}

# Store binary data
curl -X PUT --data-binary 'Hello, World!' \
  http://localhost:3000/liquer/api/store/data/greeting.txt
# Output: {"status": "ok", "result": "greeting.txt", "message": "Data stored successfully"}

# Retrieve data
curl http://localhost:3000/liquer/api/store/data/greeting.txt
# Output: Hello, World! (binary response with metadata in headers)

# Get metadata only
curl http://localhost:3000/liquer/api/store/metadata/greeting.txt
# Output: {"status": "ok", "result": {...metadata...}, "message": "..."}
```

## Assets API - Primary Use Cases

Once the Assets API is implemented, the following endpoints will be available:

### 1. GET /api/assets/data/{query}

**Purpose:** Retrieve computed asset data (triggers evaluation if not cached)

**Use case:** Get the result of a query computation, with automatic caching and re-computation on demand

```bash
curl http://localhost:3000/liquer/api/assets/data/text-hello/upper
```

**Response:**
- Status: 200 OK
- Body: Binary data (the computed result)
- Headers: Metadata fields (created_at, modified_at, status, etc.)

**Expected behavior:**
- First request: Triggers evaluation of the query, caches result, returns data
- Subsequent requests: Returns cached data (until asset is deleted/invalidated)
- If query fails: Returns 500 with error details

### 2. GET /api/assets/metadata/{query}

**Purpose:** Check asset status without retrieving data

**Use case:** Monitor computation progress, check if asset is ready/processing/error

```bash
curl http://localhost:3000/liquer/api/assets/metadata/text-hello/upper
```

**Response:**
- Status: 200 OK
- Body: JSON with asset metadata:
  ```json
  {
    "status": "ok",
    "result": {
      "status": "Ready",
      "created_at": "2026-02-21T10:30:00Z",
      "modified_at": "2026-02-21T10:30:01Z",
      "query": "text-hello/upper",
      "key": "text-hello_upper",
      "size": 5,
      "metadata": {...}
    },
    "message": "Asset metadata retrieved"
  }
  ```

**Status values:**
- `Recipe` - Asset has a recipe but hasn't been computed yet
- `Submitted` - Computation submitted, waiting to start
- `Processing` - Currently being evaluated
- `Ready` - Computation complete, data is available
- `Error` - Computation failed
- `Cancelled` - Computation was cancelled

### 3. GET /api/assets/entry/{query}?format=json

**Purpose:** Unified data+metadata response in multiple formats

**Use case:** Get both computed data and its metadata in a single request

```bash
# JSON format
curl "http://localhost:3000/liquer/api/assets/entry/text-hello/upper?format=json"

# CBOR format (default, most efficient)
curl "http://localhost:3000/liquer/api/assets/entry/text-hello/upper"

# Bincode format
curl "http://localhost:3000/liquer/api/assets/entry/text-hello/upper?format=bincode"
```

**Response (JSON format):**
```json
{
  "status": "ok",
  "result": {
    "data": "HELLO",
    "metadata": {
      "status": "Ready",
      "created_at": "2026-02-21T10:30:00Z",
      "query": "text-hello/upper",
      "size": 5
    }
  },
  "message": "Asset entry retrieved"
}
```

**Response (CBOR/Bincode):**
- Binary format containing serialized DataEntry (more efficient)
- Format selected via `?format=` query parameter or `Accept` header
- Default: CBOR (most efficient binary format)

## Recipes API - Primary Use Cases

Once the Recipes API is implemented, the following endpoints will be available:

### 1. GET /api/recipes/listdir

**Purpose:** List all available recipes

**Use case:** Discover what recipes are registered in the system

```bash
curl http://localhost:3000/liquer/api/recipes/listdir
```

**Response:**
```json
{
  "status": "ok",
  "result": [
    "greeting",
    "uppercase_greeting",
    "reverse_text",
    "count_chars"
  ],
  "message": "Recipes listed successfully"
}
```

**Expected behavior:**
- Returns list of all recipe keys registered in AsyncRecipeProvider
- Empty array if no recipes are registered
- Recipes are defined by the RecipeProvider implementation

### 2. GET /api/recipes/data/{key}

**Purpose:** Get recipe definition (the query string for this recipe)

**Use case:** Retrieve the command pipeline that a recipe executes

```bash
curl http://localhost:3000/liquer/api/recipes/data/uppercase_greeting
```

**Response:**
```json
{
  "status": "ok",
  "result": "text-Hello/upper",
  "message": "Recipe retrieved successfully"
}
```

**Expected behavior:**
- Returns the query string that represents this recipe
- Query string can be executed directly via Query API
- If recipe doesn't exist: Returns 404 with KeyNotFound error

### 3. GET /api/recipes/resolve/{key}

**Purpose:** Resolve recipe to execution plan (dependency tree)

**Use case:** Understand the step-by-step execution plan before running a recipe

```bash
curl http://localhost:3000/liquer/api/recipes/resolve/uppercase_greeting
```

**Response:**
```json
{
  "status": "ok",
  "result": {
    "steps": [
      {
        "type": "Literal",
        "command": "text",
        "args": ["Hello"],
        "key": "text-Hello"
      },
      {
        "type": "Command",
        "command": "upper",
        "depends_on": ["text-Hello"],
        "key": "text-Hello_upper"
      }
    ],
    "execution_order": [0, 1]
  },
  "message": "Recipe plan resolved"
}
```

**Expected behavior:**
- Returns structured plan showing all steps and dependencies
- Plan shows in what order steps will be executed
- Each step shows its input, command, and output key
- Useful for understanding recipe structure and dependencies

## Primary Use Case Workflows

### Workflow 1: Simple Data Retrieval

**Goal:** Get the result of a text transformation

```bash
# Step 1: Request the computed asset
curl http://localhost:3000/liquer/api/assets/data/text-hello/upper

# Server:
# - Checks if asset is cached
# - If not: Triggers evaluation via environment.evaluate()
# - Polls until Ready or Error
# - Returns binary data

# Step 2: Client receives binary data
# Output: HELLO
```

### Workflow 2: Progress Monitoring

**Goal:** Check if a long-running computation is complete

```bash
# Step 1: Check metadata
curl http://localhost:3000/liquer/api/assets/metadata/expensive-query

# Response: {"status": "Processing", ...}

# Step 2: Wait and retry
sleep 1
curl http://localhost:3000/liquer/api/assets/metadata/expensive-query

# Response: {"status": "Ready", ...}

# Step 3: Retrieve data
curl http://localhost:3000/liquer/api/assets/data/expensive-query
```

### Workflow 3: Recipe Discovery and Execution

**Goal:** Find available recipes, understand what they do, and execute one

```bash
# Step 1: List all recipes
curl http://localhost:3000/liquer/api/recipes/listdir
# Output: ["greeting", "uppercase_greeting", ...]

# Step 2: Get recipe definition
curl http://localhost:3000/liquer/api/recipes/data/uppercase_greeting
# Output: "text-Hello/upper"

# Step 3: Resolve to understand execution plan
curl http://localhost:3000/liquer/api/recipes/resolve/uppercase_greeting
# Output: step-by-step execution plan

# Step 4: Execute via Assets API
curl http://localhost:3000/liquer/api/assets/data/uppercase_greeting
# Output: HELLO (computed result)
```

### Workflow 4: Batch Operations

**Goal:** Retrieve multiple assets with metadata

```bash
# Sequential requests to get multiple assets
for asset in "query1" "query2" "query3"; do
  curl "http://localhost:3000/liquer/api/assets/entry/$asset?format=json"
done

# Each request returns both data and metadata
```

## Expected Output

When you run the example server, you should see:

```
================================================================================
Assets API and Recipes API - Primary Use Cases Example
================================================================================

Store path: .

Server listening on http://0.0.0.0:3000

--------------------------------------------------------------------------------
USAGE EXAMPLES
--------------------------------------------------------------------------------

1. QUERY API - Execute commands directly:
   # Simple text command:
   curl http://localhost:3000/liquer/q/text-hello
   # Expected output: 'hello'

2. QUERY API - Chained commands:
   curl http://localhost:3000/liquer/q/text-world/upper
   # Expected output: 'WORLD'

[... more examples ...]

Press Ctrl+C to stop
```

## Testing the Example

### Prerequisites

```bash
cd /home/orest/zlos/rust/liquers
```

### Build and run

```bash
cargo run -p liquers-axum --example assets_recipes_basic
```

### In another terminal, test Query API

```bash
# Test basic command
curl http://localhost:3000/liquer/q/text-hello

# Test chained commands
curl http://localhost:3000/liquer/q/text-hello/upper/reverse

# Test character count
curl http://localhost:3000/liquer/q/text-example/count
```

### Test Store API

```bash
# Store data
curl -X PUT --data-binary 'test content' \
  http://localhost:3000/liquer/api/store/data/mydata.txt

# Retrieve data
curl http://localhost:3000/liquer/api/store/data/mydata.txt

# List all keys
curl http://localhost:3000/liquer/api/store/keys
```

### Verify Assets API documentation

```bash
# View help text with all documented endpoints
curl http://localhost:3000/
```

## Implementation Notes

### Assets API Implementation (Phase 2)

When implementing the Assets API handlers, follow this pattern:

1. **Parse query:** Use `parse_query()` from `liquers_core::parse`
2. **Get AssetManager:** Via `env.get_asset_manager()`
3. **Retrieve asset:** Call `asset_manager.get()` to trigger evaluation
4. **Convert response:** Use `api_core` types (`ApiResponse`, `BinaryResponse`, `DataEntry`)
5. **Error handling:** Convert `Error` to `ErrorDetail` via `error_to_detail()`

### Recipes API Implementation (Phase 2)

When implementing the Recipes API handlers, follow this pattern:

1. **Parse key:** Use `parse_key()` from `liquers_core::parse`
2. **Get RecipeProvider:** Via `env.get_recipe_provider()`
3. **Retrieve recipe:** Call appropriate RecipeProvider method
4. **Convert response:** Use `api_core` types for consistent responses
5. **Error handling:** Use same pattern as Assets API

### Router Integration (Phase 2)

Once implemented, integrate into an Axum application:

```rust
use liquers_axum::{QueryApiBuilder, StoreApiBuilder, AssetsApiBuilder, RecipesApiBuilder};

let query_router = QueryApiBuilder::new("/liquer/q").build();
let store_router = StoreApiBuilder::new("/liquer/api/store").build();
let assets_router = AssetsApiBuilder::new("/liquer/api/assets").build();
let recipes_router = RecipesApiBuilder::new("/liquer/api/recipes").build();

let app = axum::Router::new()
    .merge(query_router)
    .merge(store_router)
    .merge(assets_router)
    .merge(recipes_router)
    .with_state(env_ref);
```

## References

- **Phase 1 Design:** `/specs/axum-assets-recipes-api/phase1-high-level-design.md`
- **Phase 2 Architecture:** `/specs/axum-assets-recipes-api/phase2-architecture.md`
- **WEB_API_SPECIFICATION:** `/specs/WEB_API_SPECIFICATION.md` (sections 5 & 6)
- **Store API Pattern:** `/liquers-axum/src/store/` (reference implementation)
- **Query API Pattern:** `/liquers-axum/src/query/` (reference implementation)
