use liquers_core::{
    context::{Context, Environment, SimpleEnvironment}, error::Error, interpreter::evaluate, state::State, value::Value
};
use liquers_macro::*;

#[tokio::test]
async fn test_async_hello_world() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register "hello" command
    fn world(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("world"))
    }
    async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
        let what = state.try_into_string()?;
        Ok(Value::from(format!("{greet}, {what}!")))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, fn world(state) -> result).expect("register_command failed");
    register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)
         .expect("register_command failed");

    let envref = env.to_ref();

    let state = evaluate(envref.clone(), "world/greet", None).await?;

    let value = state.try_into_string()?;
    assert_eq!(value, "Hello, world!");
    Ok(())
}

#[test]
fn try_to_query(){
    let query_str = "-R/config/config.yaml/-/from_yaml";
    let q = liquers_core::query::TryToQuery::try_to_query(query_str).unwrap();
}

#[tokio::test]
async fn test_q_instruction_evaluation() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::plan::{PlanBuilder, Step};
    use liquers_core::parse::parse_query;

    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Command that accepts a query as input and converts it to string
    fn query_to_string(state: &State<Value>) -> Result<Value, Error> {
        match state.data.as_ref() {
            Value::Query(q) => Ok(Value::from(q.encode())),
            _ => Err(Error::conversion_error("query", "string"))
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn query_to_string(state) -> result)?;

    // Build a plan with q instruction
    let query = parse_query("data/append-first/q/query_to_string")?;
    let plan = PlanBuilder::new(query, &cr.command_metadata_registry).build()?;

    // Verify the plan structure
    assert_eq!(plan.len(), 2);

    // First step should be UseQueryValue
    match &plan[0] {
        Step::UseQueryValue(q) => {
            assert_eq!(q.encode(), "data/append-first");
        }
        _ => panic!("Expected Step::UseQueryValue"),
    }

    // Second step should be Action
    match &plan[1] {
        Step::Action { action_name, .. } => {
            assert_eq!(action_name, "query_to_string");
        }
        _ => panic!("Expected Step::Action"),
    }

    // Execute the plan
    let envref = env.to_ref();
    let state = evaluate(envref, "data/append-first/q/query_to_string", None).await?;

    // The result should be the query string
    assert_eq!(state.try_into_string()?, "data/append-first");

    Ok(())
}

#[tokio::test]
async fn test_q_instruction_at_end() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Command that generates data
    fn data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("test-data"))
    }

    // Command that accepts a query as input and converts it to string
    fn query_to_string(state: &State<Value>) -> Result<Value, Error> {
        match state.data.as_ref() {
            Value::Query(q) => Ok(Value::from(q.encode())),
            _ => Err(Error::conversion_error("query", "string"))
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, fn query_to_string(state) -> result)?;

    let envref = env.to_ref();

    // Test query: data/q (q at the end)
    // Should create UseQueryValue("data") which creates Value::Query,
    // but we have no command to process it, so this test just verifies the query value
    let state = evaluate(envref, "data/q/query_to_string", None).await?;
    assert_eq!(state.try_into_string()?, "data");

    Ok(())
}

#[tokio::test]
async fn test_q_instruction_with_filename() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::plan::{PlanBuilder, Step};
    use liquers_core::parse::parse_query;

    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Command that generates data
    fn data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("test-data"))
    }

    // Command that accepts a query as input and converts it to string
    fn query_to_string(state: &State<Value>) -> Result<Value, Error> {
        match state.data.as_ref() {
            Value::Query(q) => Ok(Value::from(q.encode())),
            _ => Err(Error::conversion_error("query", "string"))
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, fn query_to_string(state) -> result)?;

    // Build a plan with q instruction and filename
    let query = parse_query("data/q/query_to_string/output.txt")?;
    let command_metadata_registry = &cr.command_metadata_registry;
    let plan = PlanBuilder::new(query, command_metadata_registry).build()?;

    // Verify the plan structure: UseQueryValue, Action, Filename
    assert_eq!(plan.len(), 3);

    // Step 1 should be UseQueryValue
    match &plan[0] {
        Step::UseQueryValue(q) => {
            assert_eq!(q.encode(), "data");
        }
        _ => panic!("Expected Step::UseQueryValue"),
    }

    // Step 2 should be Action
    match &plan[1] {
        Step::Action { action_name, .. } => {
            assert_eq!(action_name, "query_to_string");
        }
        _ => panic!("Expected Step::Action"),
    }

    // Step 3 should be Filename
    match &plan[2] {
        Step::Filename(name) => {
            assert_eq!(name.name, "output.txt");
        }
        _ => panic!("Expected Step::Filename"),
    }

    // Also test execution
    let envref = env.to_ref();
    let state = evaluate(envref, "data/q/query_to_string", None).await?;
    assert_eq!(state.try_into_string()?, "data");

    Ok(())
}