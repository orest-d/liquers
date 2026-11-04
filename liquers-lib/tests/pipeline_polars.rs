//! Pipelines with the Polars commands: Examples 1 and 2 of
//! `specs/design/composite-command-definition/phase3-examples.md`.

#![cfg(feature = "polars")]

use liquers_core::{
    command_metadata::{ArgumentInfo, ArgumentType, CommandKey, CommandParameterValue},
    context::Environment,
    error::Error,
    metadata::Metadata,
    parse::parse_key,
    pipeline::Pipeline,
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
};
use liquers_lib::{
    environment::{DefaultEnvironment, PolarsCommandRegistration},
    value::{ExtValueInterface, Value},
};
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SALES: &str = "region,product,amount\n\
EU,a,10\nUS,b,30\nAS,c,50\nEU,d,40\nUS,e,20\nEU,f,5\nAS,g,60\nUS,h,15\n";

async fn make_env(recipes: Option<&str>) -> Result<DefaultEnvironment<Value>, Box<dyn std::error::Error>> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_default_recipe_provider();
    env.register_polars_commands()?;
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(&parse_key("data/sales.csv")?, SALES.as_bytes(), &Metadata::new())
        .await?;
    if let Some(recipes) = recipes {
        store
            .set(&parse_key("data/recipes.yaml")?, recipes.as_bytes(), &Metadata::new())
            .await?;
    }
    env.with_async_store(Box::new(store));
    Ok(env)
}

async fn frame(
    envref: &liquers_core::context::EnvRef<DefaultEnvironment<Value>>,
    query: &str,
) -> Result<polars::prelude::DataFrame, Error> {
    let state = envref.evaluate(query).await?.get().await?;
    Ok((*state.value()?.as_polars_dataframe()?).clone())
}

/// Example 1 — `sales/top_in` mixed with a query plans as the long query and gives its result.
#[tokio::test]
async fn top_in_matches_long_query() -> TestResult {
    let mut env = make_env(None).await?;
    let mut by = ArgumentInfo::any_argument("by");
    by.default = CommandParameterValue::Value(json!("amount"));
    let mut n = ArgumentInfo::any_argument("n").with_type(ArgumentType::Integer);
    n.default = CommandParameterValue::Value(json!(10));
    let mut regions = ArgumentInfo::string_argument("regions");
    regions.multiple = true;
    env.command_registry.register_pipeline(
        CommandKey::new("", "sales", "top_in"),
        Pipeline::from_json(&json!([
            {"action": "from_csv", "ns": "pl"},
            {"action": "is_in", "ns": "pl",
             "arguments": {"column": "region", "values": {"placeholder": "regions"}}},
            {"action": "sort", "ns": "pl",
             "arguments": {"column": {"placeholder": "by"}, "ascending": false}},
            {"action": "head", "ns": "pl", "arguments": {"n": {"placeholder": "n"}}}
        ]))?,
        vec![by, n, regions],
    )?;
    let envref = env.to_ref();
    let short = frame(&envref, "-R/data/sales.csv/-/ns-sales/top_in-amount-3-EU-US/ns-pl/tail-2").await?;
    let long = frame(
        &envref,
        "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-3/tail-2",
    )
    .await?;
    assert_eq!(short, long);
    assert_eq!(short.height(), 2);
    // EU/US by amount, descending: 40, 30, 20 — the last two of the top three.
    let amounts: Vec<String> = short
        .column("amount")?
        .as_materialized_series()
        .iter()
        .map(|v| v.to_string())
        .collect();
    assert_eq!(amounts, vec!["30", "20"]);
    Ok(())
}

/// Example 2 — a recipe pipeline with a list for a variadic argument, then the recipe's query.
#[tokio::test]
async fn recipe_pipeline_eu_csv() -> TestResult {
    let env = make_env(Some(
        r#"recipes:
  - pipeline:
      - "-R/data/sales.csv/-/ns-pl/from_csv"
      - {action: is_in, ns: pl, arguments: {column: region, values: [EU, US]}}
    query: ns-pl/sort-amount-f/head-10/eu.csv
"#,
    ))
    .await?;
    let envref = env.to_ref();
    let from_recipe = frame(&envref, "-R/data/eu.csv").await?;
    let long = frame(
        &envref,
        "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-10",
    )
    .await?;
    assert_eq!(from_recipe, long);
    assert_eq!(from_recipe.height(), 6);
    Ok(())
}
