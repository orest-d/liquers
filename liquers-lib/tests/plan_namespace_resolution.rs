//! Tests for plan building with namespace resolution in multi-segment queries.
//!
//! Demonstrates that `ns-lui` namespace specification should be respected
//! even when the query contains both resource and transform segments.

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::parse::parse_query;
use liquers_core::plan::{PlanBuilder, Step};
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello"))
}

fn yyy(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("yyy"))
}

/// Create a test environment with `hello`, `yyy`, and `lui` commands registered.
fn setup_env() -> DefaultEnvironment<Value, SimpleUIPayload> {
    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let env = rt.block_on(async {
        let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
        env.with_trivial_recipe_provider();
        register_commands(&mut env).expect("register commands");
        env
    });
    // Leak runtime to keep it alive (test only)
    std::mem::forget(rt);
    env
}

fn register_commands(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result)?;
    register_command!(cr, fn yyy(state) -> result)?;
    liquers_lib::register_lui_commands!(cr)?;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// A pure transform query with ns-lui should correctly resolve query_console.
/// This is the baseline: single-segment transform query works.
#[test]
fn test_plan_single_segment_ns_lui_query_console() {
    let env = setup_env();
    let cr = env.get_command_metadata_registry();

    // Pure transform query (single segment): yyy/q/ns-lui/query_console
    let query = parse_query("yyy/q/ns-lui/query_console").expect("parse query");
    let plan = PlanBuilder::new(query, cr)
        .build()
        .expect("plan should build successfully");

    println!("Plan steps for 'yyy/q/ns-lui/query_console':");
    for (i, step) in plan.steps.iter().enumerate() {
        println!("  Step {}: {:?}", i, step);
    }

    let has_query_console_action = plan.steps.iter().any(|step| {
        if let Step::Action {
            ns, action_name, ..
        } = step
        {
            ns == "lui" && action_name == "query_console"
        } else {
            false
        }
    });

    assert!(
        has_query_console_action,
        "Plan should contain an Action step calling query_console in the lui namespace"
    );
}

/// Multi-segment query (resource + transform) with ns-lui should also
/// correctly resolve query_console in the lui namespace.
///
/// Query: -R/xxx/-/yyy/q/ns-lui/query_console
/// Expected plan steps:
///   1. UseQueryValue(-R/xxx/-/yyy) — predecessor wrapped by /q/
///   2. Action { ns: "lui", action_name: "query_console" }
#[test]
fn test_plan_multi_segment_ns_lui_query_console() {
    let env = setup_env();
    let cr = env.get_command_metadata_registry();

    let query = parse_query("-R/xxx/-/yyy/q/ns-lui/query_console").expect("parse query");
    let plan = PlanBuilder::new(query, cr)
        .build()
        .expect("plan should build successfully for multi-segment query with ns-lui");

    println!("Plan steps for '-R/xxx/-/yyy/q/ns-lui/query_console':");
    for (i, step) in plan.steps.iter().enumerate() {
        println!("  Step {}: {:?}", i, step);
    }

    // Verify the plan has a UseQueryValue step for the predecessor
    let has_use_query_value = plan.steps.iter().any(|step| {
        matches!(step, Step::UseQueryValue(_))
    });
    assert!(
        has_use_query_value,
        "Plan should contain a UseQueryValue step for the /q/ predecessor"
    );

    // Verify the plan has an Action step calling query_console in lui namespace
    let has_query_console_action = plan.steps.iter().any(|step| {
        if let Step::Action {
            ns, action_name, ..
        } = step
        {
            ns == "lui" && action_name == "query_console"
        } else {
            false
        }
    });

    assert!(
        has_query_console_action,
        "Plan should contain an Action step calling query_console in the lui namespace.\n\
         Bug: Query::last_ns() delegates to transform_query() which returns None \
         for multi-segment queries (Resource + Transform), so the 'lui' namespace is lost."
    );
}
