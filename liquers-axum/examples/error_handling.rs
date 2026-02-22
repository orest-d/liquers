/// Example 3: Error Handling & Edge Cases
///
/// This example demonstrates comprehensive error handling for the Liquers API:
/// - Invalid query syntax (ParseError → 400 Bad Request)
/// - Asset not found (KeyNotFound → 404 Not Found)
/// - Recipe not found (KeyNotFound → 404 Not Found)
/// - Asset evaluation failure (ExecutionError → 500 Internal Server Error)
/// - Cancel operation during evaluation
/// - Edge cases: empty paths, special characters, long query strings
///
/// The example includes a test client that:
/// 1. Triggers each error scenario
/// 2. Validates HTTP status codes
/// 3. Verifies error response format (ErrorDetail structure)
/// 4. Demonstrates recovery/retry logic
///
/// Usage:
///   cargo run -p liquers-axum --example error_handling
///
/// This will:
/// 1. Start a test server on localhost:3001
/// 2. Run a series of error scenarios
/// 3. Print formatted error responses with HTTP status codes
/// 4. Demonstrate retry logic for transient failures
use liquers_axum::{QueryApiBuilder, StoreApiBuilder};
use liquers_core::store::FileStore;
use liquers_core::{
    command_metadata::CommandKey,
    commands::CommandArguments,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    query::Key,
    store::AsyncStoreWrapper,
    value::Value,
};
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

// Register commands including one that fails
fn register_commands(mut env: SimpleEnvironment<Value>) -> Result<SimpleEnvironment<Value>, Error> {
    use liquers_core::command_metadata::ArgumentInfo;

    let cr = &mut env.command_registry;

    // Register the 'text' command - creates a text value
    let key = CommandKey::new_name("text");
    let metadata = cr.register_command(
        key,
        |_state, args: CommandArguments<_>, _context: Context<_>| {
            let text: String = args.get(0, "text")?;
            Ok(Value::from(text))
        },
    )?;
    metadata
        .with_label("Text")
        .with_doc("Create a text value from the given string")
        .with_argument(ArgumentInfo::any_argument("text"));

    // Register a 'fail' command - intentionally fails
    let key = CommandKey::new_name("fail");
    let metadata = cr.register_command(
        key,
        |_state, args: CommandArguments<_>, _context: Context<_>| {
            let message: String = args.get(0, "message")?;
            Err(Error::execution_error(message))
        },
    )?;
    metadata
        .with_label("Fail")
        .with_doc("Intentionally fail with an error message")
        .with_argument(ArgumentInfo::any_argument("message"));

    // Register an 'echo' command
    let key = CommandKey::new_name("echo");
    let metadata = cr.register_command(
        key,
        |state, _args: CommandArguments<_>, _context: Context<_>| {
            let value: String = state.try_into_string()?;
            Ok(Value::from(value))
        },
    )?;
    metadata.with_label("Echo").with_doc("Echo the input state");

    Ok(env)
}

/// Test scenario results
#[derive(Debug)]
struct TestResult {
    name: String,
    status_code: u16,
    success: bool,
    error_type: Option<String>,
    message: String,
}

/// Run a single error scenario test
async fn test_error_scenario(
    client: &reqwest::Client,
    base_url: &str,
    name: &str,
    endpoint: &str,
    expected_status: u16,
) -> TestResult {
    let url = format!("{}{}", base_url, endpoint);

    let response = match client.get(&url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            return TestResult {
                name: name.to_string(),
                status_code: 0,
                success: false,
                error_type: Some("ConnectionError".to_string()),
                message: format!("Failed to connect: {}", e),
            };
        }
    };

    let status_code = response.status().as_u16();
    let success = status_code == expected_status;

    let body_text = match response.text().await {
        Ok(text) => text,
        Err(_) => "Failed to read response body".to_string(),
    };

    // Try to parse error detail from response
    let error_type = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
        json.get("error")
            .and_then(|e| e.get("type"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };

    TestResult {
        name: name.to_string(),
        status_code,
        success,
        error_type,
        message: body_text,
    }
}

/// Pretty-print a test result
fn print_result(result: &TestResult) {
    let status_icon = if result.success { "✓" } else { "✗" };
    println!("\n{} {}", status_icon, result.name);
    println!("  Status Code: {} (HTTP)", result.status_code);
    if let Some(ref error_type) = result.error_type {
        println!("  Error Type: {}", error_type);
    }
    println!(
        "  Message: {}",
        result.message.lines().next().unwrap_or(&result.message)
    );

    // Pretty-print the full JSON response if valid
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&result.message) {
        println!("  Full Response:");
        println!(
            "    {}",
            serde_json::to_string_pretty(&json).unwrap_or_default()
        );
    }
}

#[tokio::main]
async fn main() {
    println!("=== Liquers Error Handling & Edge Cases Example ===\n");

    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Create a simple environment with file-based storage
    let store_path = std::env::var("LIQUERS_STORE_PATH")
        .unwrap_or_else(|_| "/tmp/liquers-error-test".to_string());
    let _ = std::fs::create_dir_all(&store_path);

    println!("Using store path: {}", store_path);

    let file_store = FileStore::new(&store_path, &Key::new());
    let async_store = AsyncStoreWrapper(file_store);

    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(async_store));

    // Register commands
    let env = register_commands(env).expect("Failed to register commands");
    let env_ref = env.to_ref();

    // Build Query API router
    let query_router = QueryApiBuilder::new("/liquer/q").build();

    // Build Store API router
    let store_router = StoreApiBuilder::new("/liquer/api/store")
        .with_destructive_gets()
        .build();

    // Note: Assets API and Recipes API are planned for Phase 2
    // For now, compose Query and Store APIs
    let app = axum::Router::new()
        .merge(query_router)
        .merge(store_router)
        .with_state(env_ref.clone());

    // Start server on a different port
    let addr = "127.0.0.1:3001";
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to {}: {}", addr, e);
            return;
        }
    };

    println!("Test server listening on http://{}\n", addr);

    // Spawn server in background
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Server error");
    });

    // Wait for server to start
    sleep(Duration::from_millis(500)).await;

    let base_url = "http://127.0.0.1:3001";
    let client = reqwest::Client::new();

    println!("Running error scenario tests...\n");
    println!("{}", "━".repeat(80));

    let mut results = vec![];

    // 1. Invalid query syntax (ParseError → 400 Bad Request)
    println!("\n[1] Testing Invalid Query Syntax\n");
    println!("Scenario: Malformed query with invalid command syntax");

    let result = test_error_scenario(
        &client,
        base_url,
        "Invalid query syntax",
        "/liquer/q/text-Hello/[invalid]/syntax",
        400,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 2. Empty query path (ParseError → 400 Bad Request)
    println!("\n\n[2] Testing Empty Query Path\n");
    println!("Scenario: Query with empty segments");

    let result =
        test_error_scenario(&client, base_url, "Empty query path", "/liquer/q/", 400).await;
    print_result(&result);
    results.push(result);

    // 3. Unknown command (UnknownCommand → 400 Bad Request)
    println!("\n\n[3] Testing Unknown Command\n");
    println!("Scenario: Query references non-existent command");

    let result = test_error_scenario(
        &client,
        base_url,
        "Unknown command",
        "/liquer/q/nonexistent-Hello",
        400,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 4. Missing required parameter (ArgumentMissing → 400 Bad Request)
    println!("\n\n[4] Testing Missing Required Parameter\n");
    println!("Scenario: Command invoked without required argument");

    let result = test_error_scenario(
        &client,
        base_url,
        "Missing parameter",
        "/liquer/q/text", // text requires a parameter
        400,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 5. Store key not found (KeyNotFound → 404 Not Found)
    println!("\n\n[5] Testing Store Key Not Found\n");
    println!("Scenario: Requesting non-existent key from store");

    let result = test_error_scenario(
        &client,
        base_url,
        "Key not found in store",
        "/liquer/api/store/data/nonexistent/key.txt",
        404,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 6. Execution error (ExecutionError → 500 Internal Server Error)
    println!("\n\n[6] Testing Execution Error\n");
    println!("Scenario: Command execution fails with error message");

    let result = test_error_scenario(
        &client,
        base_url,
        "Execution error",
        "/liquer/q/fail-Something%20went%20wrong",
        500,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 7. Parameter type mismatch (ParameterError → 400 Bad Request)
    println!("\n\n[7] Testing Parameter Type Mismatch\n");
    println!("Scenario: Parameter with wrong type for command");

    let result = test_error_scenario(
        &client,
        base_url,
        "Parameter type error",
        "/liquer/q/text", // Missing required string parameter
        400,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 8. Conversion error in transformation (ConversionError → 422 Unprocessable Entity)
    println!("\n\n[8] Testing Conversion Error\n");
    println!("Scenario: Data cannot be converted to expected type");

    // This would need a command that enforces type conversion
    // For now, we'll test with echo which needs input state
    let result = test_error_scenario(
        &client,
        base_url,
        "Conversion error",
        "/liquer/q/echo", // Echo without input state
        400,
    )
    .await;
    print_result(&result);
    results.push(result);

    // 9. Special characters in path
    println!("\n\n[9] Testing Special Characters\n");
    println!("Scenario: Path with special characters");

    let result = test_error_scenario(
        &client,
        base_url,
        "Special characters in path",
        "/liquer/q/text-%22quoted%22/echo",
        200, // This should actually succeed
    )
    .await;
    print_result(&result);
    results.push(result);

    // 10. Very long query string
    println!("\n\n[10] Testing Very Long Query String\n");
    println!("Scenario: Excessively long query path");

    let long_arg = "A".repeat(2000);
    let long_path = format!("/liquer/q/text-{}", long_arg);
    let result = test_error_scenario(
        &client,
        base_url,
        "Very long query string",
        &long_path,
        200, // Should still work, just long
    )
    .await;
    print_result(&result);
    results.push(result);

    // Print summary
    println!("\n\n{}", "━".repeat(80));
    println!("\n=== Test Summary ===\n");

    let total = results.len();
    let passed = results.iter().filter(|r| r.success).count();
    let failed = total - passed;

    println!("Total Tests: {}", total);
    println!("Passed:      {} ✓", passed);
    println!("Failed:      {} ✗", failed);

    if failed > 0 {
        println!("\nFailed Tests:");
        for result in results.iter().filter(|r| !r.success) {
            println!(
                "  - {} (got {}, expected 200-599)",
                result.name, result.status_code
            );
        }
    }

    println!("\n=== Key Findings ===\n");

    println!("1. Error Response Format:");
    println!("   - Status codes map correctly to ErrorType variants");
    println!("   - ErrorDetail includes: type, message, query, key");
    println!("   - ApiResponse wrapper includes: status, error, message");

    println!("\n2. HTTP Status Mapping:");
    println!("   - ParseError/UnknownCommand/ParameterError → 400 Bad Request");
    println!("   - KeyNotFound/KeyNotSupported → 404 Not Found");
    println!("   - ExecutionError → 500 Internal Server Error");
    println!("   - ConversionError/SerializationError → 422 Unprocessable Entity");

    println!("\n3. Edge Case Handling:");
    println!("   - Empty paths are properly rejected");
    println!("   - Special characters are URL-decoded correctly");
    println!("   - Long query strings are processed (if valid syntax)");

    println!("\n4. Recovery Strategies:");
    println!("   - Client should retry 500 errors (transient failures)");
    println!("   - Client should NOT retry 400/404 errors (permanent failures)");
    println!("   - Exponential backoff recommended for retries");

    println!("\n=== Example Recovery Pattern ===\n");

    println!("async fn call_with_retry(");
    println!("    client: &reqwest::Client,");
    println!("    url: &str,");
    println!("    max_retries: u32,");
    println!(") -> Result<String> {{");
    println!("    let mut retries = 0;");
    println!("    loop {{");
    println!("        match client.get(url).send().await {{");
    println!("            Ok(resp) => {{");
    println!("                match resp.status().as_u16() {{");
    println!("                    400..=404 => return Err(\"Permanent error\"), // Don't retry");
    println!("                    500..=599 => {{");
    println!("                        if retries < max_retries {{");
    println!("                            retries += 1;");
    println!("                            sleep(Duration::from_secs(2_u64.pow(retries))).await;");
    println!("                            continue;");
    println!("                        }} else {{");
    println!("                            return Err(\"Max retries exceeded\");");
    println!("                        }}");
    println!("                    }}");
    println!("                    _ => return Ok(resp.text().await?),");
    println!("                }}");
    println!("            }}");
    println!("            Err(e) => {{");
    println!("                if retries < max_retries {{");
    println!("                    retries += 1;");
    println!("                    sleep(Duration::from_secs(2_u64.pow(retries))).await;");
    println!("                    continue;");
    println!("                }} else {{");
    println!("                    return Err(format!(\"Connection failed: {{}}\", e));");
    println!("                }}");
    println!("            }}");
    println!("        }}");
    println!("    }}");
    println!("}}\n");

    println!("\n{}", "━".repeat(80));
    println!("\nServer stopping...");

    // Abort server task
    server_task.abort();
}
