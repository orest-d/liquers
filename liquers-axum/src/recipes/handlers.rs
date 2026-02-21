//! Recipes API Handlers - HTTP request handlers for recipe operations
//!
//! Part of the Recipes API implementation.
//! See specs/axum-assets-recipes-api/phase2-architecture.md for specifications.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
#[allow(unused_imports)]
use liquers_core::recipes::AsyncRecipeProvider; // Needed for trait method resolution
use liquers_core::{
    context::{EnvRef, Environment},
    parse::parse_key,
};

use crate::api_core::{error::error_to_detail, ApiResponse};

/// GET /listdir - List all available recipes (at root directory)
pub async fn listdir_handler<E: Environment>(State(env): State<EnvRef<E>>) -> Response {
    // Get RecipeProvider
    let recipe_provider = env.get_recipe_provider();

    // Use root key to list recipes
    use liquers_core::query::Key;
    let root_key = Key::new();

    // Check if root has recipes
    let has_recipes = match recipe_provider.has_recipes(&root_key, env.clone()).await {
        Ok(hr) => hr,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to check for recipes");
            return response.into_response();
        }
    };

    if !has_recipes {
        // No recipes at root
        let response: ApiResponse<Vec<String>> = ApiResponse::ok(vec![], "No recipes found");
        return response.into_response();
    }

    // List recipes at root
    let resource_names = match recipe_provider
        .assets_with_recipes(&root_key, env.clone())
        .await
    {
        Ok(names) => names,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to list recipes");
            return response.into_response();
        }
    };

    // Convert resource names to strings
    let recipe_names: Vec<String> = resource_names.iter().map(|rn| rn.name.clone()).collect();

    // Return as JSON
    let response: ApiResponse<Vec<String>> =
        ApiResponse::ok(recipe_names, "Recipes listed");
    response.into_response()
}

/// GET /data/{*key} - Get recipe definition (query string)
pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key from path
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get RecipeProvider from environment
    let recipe_provider = env.get_recipe_provider();

    // Get recipe definition
    let recipe = match recipe_provider.recipe(&key, env.clone()).await {
        Ok(r) => r,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get recipe");
            return response.into_response();
        }
    };

    // Serialize recipe to YAML string
    let recipe_yaml = recipe.to_string();

    // Return recipe as text/plain
    use axum::http::header::{CONTENT_TYPE, HeaderMap};
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, "text/plain".parse().unwrap());
    (headers, recipe_yaml).into_response()
}

/// GET /metadata/{*key} - Get recipe metadata
pub async fn get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get RecipeProvider
    let recipe_provider = env.get_recipe_provider();

    // Verify recipe exists
    match recipe_provider.recipe(&key, env.clone()).await {
        Ok(_) => {
            // Recipe exists, return empty metadata (placeholder)
            let response: ApiResponse<serde_json::Value> =
                ApiResponse::ok(serde_json::json!({}), "Recipe metadata retrieved");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get recipe");
            response.into_response()
        }
    }
}

/// GET /entry/{*key} - Get recipe entry (data + metadata)
pub async fn get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get RecipeProvider
    let recipe_provider = env.get_recipe_provider();

    // Get recipe definition
    let recipe = match recipe_provider.recipe(&key, env.clone()).await {
        Ok(r) => r,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get recipe");
            return response.into_response();
        }
    };

    // Serialize recipe to YAML and convert to bytes
    let recipe_yaml = recipe.to_string();
    let data = recipe_yaml.into_bytes();

    // Create DataEntry with recipe data and empty metadata
    use crate::api_core::response::DataEntry;
    let entry = DataEntry {
        data,
        metadata: serde_json::json!({}),
    };

    // Serialize entry to CBOR (default format)
    use crate::api_core::format::serialize_data_entry;
    let format = crate::SerializationFormat::Cbor;

    match serialize_data_entry(&entry, format) {
        Ok(bytes) => {
            // Return with appropriate Content-Type
            use axum::http::header::{CONTENT_TYPE, HeaderMap};
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, format.mime_type().parse().unwrap());
            (headers, bytes).into_response()
        }
        Err(e) => {
            let error_detail = crate::api_core::response::ErrorDetail {
                error_type: "SerializationError".to_string(),
                message: e,
                query: None,
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to serialize entry");
            response.into_response()
        }
    }
}

/// GET /resolve/{*key} - Resolve recipe to execution plan
pub async fn resolve_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get RecipeProvider
    let recipe_provider = env.get_recipe_provider();

    // Resolve recipe to plan
    let plan = match recipe_provider.recipe_plan(&key, env.clone()).await {
        Ok(p) => p,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to resolve recipe");
            return response.into_response();
        }
    };

    // Serialize plan to JSON
    let plan_json = match serde_json::to_value(&plan) {
        Ok(v) => v,
        Err(e) => {
            let error_detail = crate::api_core::response::ErrorDetail {
                error_type: "SerializationError".to_string(),
                message: format!("Failed to serialize plan: {}", e),
                query: None,
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to serialize plan");
            return response.into_response();
        }
    };

    // Return resolved plan as JSON
    #[derive(serde::Serialize)]
    struct ResolveResponse {
        key: String,
        query: String,
        plan: serde_json::Value,
    }

    let resolve_response = ResolveResponse {
        key: key.encode(),
        query: plan.query.encode(),
        plan: plan_json,
    };

    let response: ApiResponse<ResolveResponse> =
        ApiResponse::ok(resolve_response, "Recipe resolved");
    response.into_response()
}
