//! The examples of the Rust user guide (`docs/guides/rust/`), compiled and run.
//!
//! Every ```rust block of the guide is a contiguous slice of this file or of
//! `liquers-axum/tests/user_guide_GUIDE.rs`, compared line by line with indentation ignored;
//! `liquers-docs/tests/rust_guide_examples.rs` checks that. A block whose first line starts with
//! `// Sketch` is exempt. Change an example here and in the chapter together.
//!
//! Two shapes: a complete program is a module whose `main` a test calls; every other example is
//! the body of an `async fn` returning `Result<(), Error>`, using the imports of `prelude` (shown
//! in section 2.5 of the guide) and `library_environment` (section 2.4).
#![cfg(all(
    feature = "polars",
    feature = "records",
    feature = "templates",
    feature = "image-support"
))]
#![allow(dead_code, unused_imports)]

mod ch01_first_query {
use liquers_core::{context::{Context, Environment}, error::Error, state::State};
use liquers_lib::{environment::{default_environment_builder, DefaultEnvironment}, value::Value};
use liquers_macro::register_command;

type CommandEnvironment = DefaultEnvironment<Value>;

fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}

fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    Ok(Value::from(format!("{greeting}, {text}!")))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut builder = default_environment_builder::<Value, ()>();
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;
    register_command!(cr, fn greet(state, greeting: String = "Hello") -> result)?;
    let env = builder.build()?;

    let state = env.evaluate("hello/greet").await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");
    let state = env.evaluate("hello/greet-Hi").await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hi, world!");
    Ok(())
}
#[test]
fn run() {
    main().unwrap()
}
}


/// Section 2.4: `src/library.rs` of a project.
mod library {
//! An environment with the Liquers command library.
use std::sync::Arc;

use liquers_core::{
    commands::CommandRegistry,
    context::{Context, EnvRef},
    environment_builder::EnvironmentBuilder,
    error::Error,
    store::AsyncStore,
};
use liquers_lib::{
    environment::{default_environment_builder, DefaultEnvironment, LibKind},
    value::Value,
};

/// The environment type. `register_command!` looks for a type of this name.
pub type CommandEnvironment = DefaultEnvironment<Value>;

/// Registers the command library. Leave out the groups you do not need.
pub fn register_library(cr: &mut CommandRegistry<CommandEnvironment>) -> Result<(), Error> {
    liquers_lib::register_core_commands!(cr)?; // to_text, commands_doc, ...
    liquers_lib::register_records_commands!(cr)?; // ns-rec
    liquers_lib::register_polars_commands!(cr)?; // ns-pl
    liquers_lib::register_image_commands!(cr)?; // ns-img
    liquers_lib::register_search_commands!(cr)?; // ns-search
    liquers_lib::register_template_commands!(cr)?; // ns-template
    Ok(())
}

/// A builder that reads and writes `store` and has the library's commands registered.
/// Register your own commands on it, then call `build()`.
pub fn library_builder(
    store: Arc<dyn AsyncStore>,
) -> Result<EnvironmentBuilder<Value, (), LibKind>, Error> {
    let mut builder = default_environment_builder::<Value, ()>().with_async_store(store);
    register_library(&mut builder.command_registry)?;
    Ok(builder)
}

/// The same, built: ready to evaluate queries.
pub fn library_environment(
    store: Arc<dyn AsyncStore>,
) -> Result<EnvRef<CommandEnvironment>, Error> {
    library_builder(store)?.build()
}
}

/// Section 2.4: `src/prelude.rs` of a project.
mod prelude {
pub use std::sync::Arc;

pub use liquers_core::{
    context::{Context, EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{Metadata, Status},
    parse::{parse_key, parse_query},
    query::{Key, Query},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::ValueInterface,
};
pub use liquers_lib::value::{ExtValueInterface, Value};
pub use liquers_macro::register_command;

pub use crate::library::{library_builder, library_environment, register_library, CommandEnvironment};
}
use prelude::*;

const SALES: &[u8] = b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n";

#[tokio::test]
async fn ch01_library() -> Result<(), Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let env = library_environment(store)?;

    let listing = env.evaluate("commands_doc").await?.get().await?.try_into_string()?;
    assert!(listing.starts_with("# Commands"));
    Ok(())
}

#[tokio::test]
async fn ch01_store() -> Result<(), Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store
        .set(
            &parse_key("data/sales.csv")?,
            b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n",
            &Metadata::new(),
        )
        .await?;
    let env = library_environment(store)?;

    let query = "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU";
    let table = env.evaluate(query).await?.get().await?;
    let csv = String::from_utf8_lossy(&table.as_bytes_with_data_format("csv")?).to_string();
    assert_eq!(csv, "region,month,amount\nEU,jan,120\nEU,feb,95\n");

    let count = env.evaluate(format!("{query}/nrows")).await?.get().await?;
    assert_eq!(count.try_into_string()?, "2");
    Ok(())
}

#[tokio::test]
async fn ch01_cache() -> Result<(), Error> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static RUNS: AtomicUsize = AtomicUsize::new(0);

    fn slow_answer() -> Result<Value, Error> {
        RUNS.fetch_add(1, Ordering::SeqCst); // stands in for a long computation
        Ok(Value::from(42))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn slow_answer() -> result)?;
    let env = builder.build()?;

    let first = env.evaluate("slow_answer").await?.get().await?;
    let second = env.evaluate("slow_answer").await?.get().await?;
    assert_eq!(first.try_into_string()?, "42");
    assert_eq!(second.try_into_string()?, "42");
    assert_eq!(RUNS.load(Ordering::SeqCst), 1); // the second call reused the first result
    Ok(())
}

#[tokio::test]
async fn ch01_recipes() -> Result<(), Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let sales = b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n";
    let recipes = b"
recipes:
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
    title: Sales in the EU
";
    store.set(&parse_key("data/sales.csv")?, sales, &Metadata::new()).await?;
    store.set(&parse_key("data/recipes.yaml")?, recipes, &Metadata::new()).await?;
    let env = library_environment(store)?;

    let eu = env.evaluate("-R/data/eu.csv").await?.get().await?;
    let csv = String::from_utf8_lossy(&eu.as_bytes_with_data_format("csv")?).to_string();
    assert_eq!(csv, "region,month,amount\nEU,jan,120\nEU,feb,95\n"); // the two EU rows
    Ok(())
}

#[tokio::test]
async fn ch01_templates() -> Result<(), Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let sales = b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n";
    let template = b"We have {{$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows}} EU sales.";
    store.set(&parse_key("data/sales.csv")?, sales, &Metadata::new()).await?;
    store.set(&parse_key("templates/summary.txt")?, template, &Metadata::new()).await?;
    let env = library_environment(store)?;

    let text = env.evaluate("-R/templates/summary.txt/-/ns-template/expand").await?.get().await?;
    assert_eq!(text.try_into_string()?, "We have 2 EU sales.");
    Ok(())
}

#[tokio::test]
async fn ch03_running() -> Result<(), Error> {
    fn hello() -> Result<Value, Error> {
        Ok(Value::from("world"))
    }
    fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
        Ok(Value::from(format!("{greeting}, {}!", state.try_into_string()?)))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;
    register_command!(cr, fn greet(state, greeting: String = "Hello") -> result)?;
    let env = builder.build()?;

    // 1. Ask for the asset, then wait for its state.
    let asset = env.evaluate("hello/greet-Hi").await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "Hi, world!");
    assert_eq!(asset.status().await, Status::Ready);

    // 2. A parsed query works wherever text does.
    let query = parse_query("hello/greet")?;
    let state = env.evaluate(&query).await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");

    // 3. One-off evaluation that keeps nothing.
    let state = liquers_core::interpreter::evaluate(env.clone(), "hello/greet-Hey", None).await?;
    assert_eq!(state.try_into_string()?, "Hey, world!");
    Ok(())
}

#[tokio::test]
async fn ch03_inside() -> Result<(), Error> {
    let query = parse_query("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv")?;
    assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv");
    assert_eq!(query.filename().map(|name| name.name), Some("eu.csv".to_string()));
    assert_eq!(query.extension(), Some("csv".to_string()));
    assert_eq!(query.segments.len(), 2);

    let key = parse_key("data/sales.csv")?;
    assert_eq!(key.len(), 2);
    assert_eq!(key.extension(), Some("csv".to_string()));
    assert_eq!(key.parent().encode(), "data");
    assert_eq!(key.parent().join("eu.csv").encode(), "data/eu.csv");
    assert_eq!(Query::from(key).encode(), "-R/data/sales.csv");

    let error = parse_query("hello/greet-~").unwrap_err();
    assert_eq!(error.error_type, ErrorType::ParseError);
    assert_eq!(error.position.column, 13); // where in the text
    Ok(())
}

#[tokio::test]
async fn ch03_building() -> Result<(), Error> {
    use liquers_core::pipeline::QueryEditor;
    use liquers_core::query::{ActionParameter, ActionRequest};

    let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;

    let region = "North-West / 2"; // a value you do not control
    let mut editor = QueryEditor::new(env.command_metadata_snapshot());
    editor.resource("data/sales.csv", Default::default())?;
    editor.action("to_record")?.ns("rec")?;
    let mut filter = editor.action("eq")?;
    filter.ns("rec")?.push("region")?.push(region)?;
    assert!(filter.set("no_such_argument", 1).is_err()); // refused, and nothing changed
    assert_eq!(
        editor.query().encode(),
        "-R/data/sales.csv/-/ns-rec/to_record/eq-region-North~_West~.~/~.2"
    );

    // --- action
    let action = ActionRequest::new("eq".to_owned()).with_parameters(vec![
        ActionParameter::new_string("region".to_owned()),
        ActionParameter::new_string(region.to_owned()),
    ]);
    assert_eq!(action.encode(), "eq-region-North~_West~.~/~.2");
    Ok(())
}

#[tokio::test]
async fn ch03_pipelines() -> Result<(), Error> {
    use liquers_core::interpreter::evaluate_pipeline;
    use liquers_core::pipeline::Pipeline;
    use serde_json::json;

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    let env = library_environment(store)?;

    let pipeline = Pipeline::from_json(&json!([
        "-R/data/sales.csv/-/ns-rec/to_record",
        {"action": "eq", "ns": "rec", "parameters": ["region", "EU"]},
        {"action": "nrows", "ns": "rec"}
    ]))?;
    let state = evaluate_pipeline(env.clone(), &pipeline, None).await?;
    assert_eq!(state.try_into_string()?, "2");

    // --- convert
    let cmr = env.command_metadata_snapshot();
    let query = pipeline.to_query_with(&cmr)?;
    assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows");
    let back = Pipeline::try_from(&query)?;
    assert_eq!(back.to_query_with(&cmr)?.encode(), query.encode());
    Ok(())
}

#[tokio::test]
async fn ch03_checking() -> Result<(), Error> {
    use liquers_core::plan::PlanBuilder;

    let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cmr = env.command_metadata_snapshot();

    let plan = PlanBuilder::new(parse_query("-R/data/sales.csv/-/ns-rec/to_record/nrows")?, &cmr).build()?;
    assert_eq!(plan.len(), 3); // read the file, to_record, nrows

    let error = PlanBuilder::new(parse_query("ns-rec/nrows-1-2-3")?, &cmr).build().unwrap_err();
    assert_eq!(error.error_type, ErrorType::TooManyParameters);
    let error = PlanBuilder::new(parse_query("no_such_command")?, &cmr).build().unwrap_err();
    assert_eq!(error.error_type, ErrorType::ActionNotRegistered);
    assert!(error.to_string().contains("no_such_command"));
    Ok(())
}

#[tokio::test]
async fn ch04_values() -> Result<(), Error> {
    // Building values.
    let text = Value::from("hello");
    let number = Value::from(42);
    let ratio = Value::from(0.5);
    let flag = Value::from(true);
    let bytes = Value::from_bytes(vec![1, 2, 3]);
    let nothing = Value::none();
    let record = Value::try_from_json_value(&serde_json::json!({"name": "Ada", "scores": [1, 2.5]}))?;

    // Every value knows its type.
    assert_eq!(text.identifier(), "Text");
    assert_eq!(number.identifier(), "I32");
    assert_eq!(ratio.identifier(), "F64");
    assert_eq!(flag.identifier(), "Bool");
    assert_eq!(bytes.identifier(), "Bytes");
    assert_eq!(record.identifier(), "Object");
    assert!(nothing.is_none());

    // Reading them back. A conversion that loses nothing succeeds; the rest are errors.
    assert_eq!(number.try_into_i64()?, 42);
    assert_eq!(number.try_into_string()?, "42");
    assert!(Value::from("42").try_into_i64().is_err()); // text is not parsed for you
    assert_eq!(record.try_into_json_value()?["name"], "Ada");
    Ok(())
}

#[tokio::test]
async fn ch04_state() -> Result<(), Error> {
    fn hello() -> Result<Value, Error> {
        Ok(Value::from("world"))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;
    let env = builder.build()?;

    let state = env.evaluate("hello/greeting.txt").await?.get().await?;

    // The value.
    let value: Arc<Value> = state.value()?;
    assert_eq!(value.try_into_string()?, "world");
    assert_eq!(state.try_into_string()?, "world"); // a shortcut for the same

    // The metadata.
    assert_eq!(state.status(), Status::Ready);
    assert_eq!(state.type_identifier(), "Text");
    assert_eq!(state.metadata.query()?.encode(), "hello/greeting.txt");
    assert_eq!(state.metadata.filename(), Some("greeting.txt".to_string()));
    assert_eq!(state.get_data_format(), "txt");
    assert_eq!(state.metadata.get_media_type(), "text/plain");
    assert!(!state.metadata.get_dependencies().is_empty()); // what it was made from

    // A state is two reference-counted pointers: cloning it copies neither value nor metadata.
    let copy = state.clone();
    assert!(Arc::ptr_eq(state.data_unchecked(), copy.data_unchecked()));
    Ok(())
}

#[tokio::test]
async fn ch04_files() -> Result<(), Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store.set(&parse_key("data/note.txt")?, b"hello", &Metadata::new()).await?;
    store.set(&parse_key("data/config.json")?, b"{\"retries\": 3}", &Metadata::new()).await?;
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    let env = library_environment(store)?;

    let note = env.evaluate("-R/data/note.txt").await?.get().await?;
    assert_eq!(note.type_identifier(), "Text");
    let config = env.evaluate("-R/data/config.json").await?.get().await?;
    assert_eq!(config.type_identifier(), "Object");
    assert_eq!(config.value()?.try_into_json_value()?["retries"], 3);
    let sales = env.evaluate("-R/data/sales.csv").await?.get().await?;
    assert_eq!(sales.type_identifier(), "Bytes"); // not parsed until a command asks
    assert_eq!(sales.get_data_format(), "csv");
    let table = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record").await?.get().await?;
    assert_eq!(table.type_identifier(), "RecordView");

    // --- registry
    let info = env.get_type_registry().get("RecordView").expect("a registered type");
    assert_eq!(info.default_extension, "csv");
    assert!(info.supported_data_formats.iter().any(|format| format == "json"));
    Ok(())
}

#[tokio::test]
async fn ch04_extended() -> Result<(), Error> {
    use image::DynamicImage;

    let picture = Value::from_image(Arc::new(DynamicImage::new_rgb8(4, 3)));
    assert_eq!(picture.identifier(), "Image");
    assert!(picture.is_extended());
    assert_eq!(picture.as_image()?.width(), 4);
    assert_eq!(picture.default_extension(), "png");

    // Asking a value for something it is not is a conversion error, never a panic.
    let error = Value::from("not a picture").as_image().err().expect("text is not an image");
    assert_eq!(error.error_type, ErrorType::ConversionError);
    assert!(picture.try_into_string().is_err());
    Ok(())
}

#[tokio::test]
async fn ch05_first() -> Result<(), Error> {
    // 1. Ordinary functions. Each returns `Result<Value, Error>`.
    fn hello() -> Result<Value, Error> {
        Ok(Value::from("world"))
    }

    fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
        let text = state.try_into_string()?;
        Ok(Value::from(format!("{greeting}, {text}!")))
    }

    // 2. Registration: the signature again, in the macro's own notation.
    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;
    register_command!(cr,
        fn greet(state, greeting: String = "Hello") -> result
        label: "Greet"
        doc: "Greets the input text"
    )?;
    let env = builder.build()?;

    // 3. Use.
    let state = env.evaluate("hello/greet-Hi").await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hi, world!");
    Ok(())
}

#[tokio::test]
async fn ch05_input() -> Result<(), Error> {
    fn hello() -> Result<Value, Error> {
        Ok(Value::from("world"))
    }

    // `state`: the value with its metadata, by reference.
    fn describe(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(format!("{} from {}", state.type_identifier(), state.metadata.query()?.encode())))
    }

    // `text`: the value converted to text.
    fn shout(text: &str) -> Result<Value, Error> {
        Ok(Value::from(text.to_uppercase()))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;
    register_command!(cr, fn describe(state) -> result)?;
    register_command!(cr, fn shout(text) -> result)?;
    let env = builder.build()?;

    let text = |query: &'static str| {
        let env = env.clone();
        async move { env.evaluate(query).await?.get().await?.try_into_string() }
    };
    assert_eq!(text("hello/describe").await?, "Text from hello");
    assert_eq!(text("hello/shout").await?, "WORLD");
    Ok(())
}

#[tokio::test]
async fn ch05_arguments() -> Result<(), Error> {
    fn repeat(text: &str, count: usize, separator: String) -> Result<Value, Error> {
        Ok(Value::from(vec![text; count].join(&separator)))
    }

    fn first(text: &str, n: Option<usize>) -> Result<Value, Error> {
        Ok(Value::from(match n {
            Some(n) => text.chars().take(n).collect::<String>(),
            None => text.to_string(),
        }))
    }

    fn join(words: Vec<String>) -> Result<Value, Error> {
        Ok(Value::from(words.join(" ")))
    }

    fn scale(text: &str, unit: String) -> Result<Value, Error> {
        let number: i64 = text.parse().map_err(|e| Error::general_error(format!("{e}")))?;
        Ok(Value::from(number * if unit == "k" { 1000 } else { 1 }))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr,
        fn repeat(text, count: usize = 2 (label: "Count", description: "How many copies"),
                  separator: String = "") -> result
    )?;
    register_command!(cr, fn first(text, n: Option<usize>) -> result)?;
    register_command!(cr, fn join(words: Vec<String> multiple) -> result)?;
    register_command!(cr,
        fn scale(text, unit: String = "one" (enum: ["one", "k"])) -> result
    )?;
    let env = builder.build()?;

    let text = |query: &'static str| {
        let env = env.clone();
        async move { env.evaluate(query).await?.get().await?.try_into_string() }
    };
    assert_eq!(text("join-ab/repeat").await?, "abab"); // both defaults
    assert_eq!(text("join-ab/repeat-3").await?, "ababab");
    assert_eq!(text("join-ab/repeat--~.").await?, "ab ab"); // count left empty: its default
    assert_eq!(text("join-abcdef/first-2").await?, "ab");
    assert_eq!(text("join-abcdef/first").await?, "abcdef"); // None
    assert_eq!(text("join-a-b-c").await?, "a b c"); // three arguments
    assert_eq!(text("join-a~_b").await?, "a-b"); // one argument with a dash in it
    assert_eq!(text("join-3/scale-k").await?, "3000");


    // --- refused
    // Wrong arguments are refused when the query is planned, before any command runs.
    let error = env.evaluate("join-ab/repeat-x").await.err().expect("not a number");
    assert_eq!(error.error_type, ErrorType::ConversionError);
    let error = env.evaluate("join-3/scale-M").await.err().expect("not a unit");
    assert!(error.to_string().contains("Valid values: one, k"));
    let error = env.evaluate("join-ab/repeat-1-x-y").await.err().expect("one too many");
    assert_eq!(error.error_type, ErrorType::TooManyParameters);
    Ok(())
}

#[tokio::test]
async fn ch05_context() -> Result<(), Error> {
    fn hello() -> Result<Value, Error> {
        Ok(Value::from("world"))
    }

    async fn report(state: State<Value>, context: Context<CommandEnvironment>) -> Result<Value, Error> {
        context.info("building the report")?;
        let greeting = context.get_dependency_state(&parse_query("hello")?).await?;
        if state.is_none() {
            context.warning("no input")?;
        }
        context.set_title("Report").await?;
        Ok(Value::from(format!("Hello, {}!", greeting.try_into_string()?)))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;
    register_command!(cr, async fn report(state, context) -> result)?;
    let env = builder.build()?;

    let state = env.evaluate("report").await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");
    assert_eq!(state.metadata.title(), "Report");
    let record = state.metadata.metadata_record().expect("a metadata record");
    assert!(record.log.iter().any(|entry| entry.message == "building the report"));
    let dependencies = state.metadata.get_dependencies();
    assert!(dependencies.iter().any(|dependency| dependency.key.to_string() == "hello"));
    Ok(())
}

#[tokio::test]
async fn ch05_cwd() -> Result<(), Error> {
    async fn read_sibling(
        _state: State<Value>,
        dir: Key,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let sibling = Query::from(dir.join("hello.txt")); // absolute, so it is accepted
        let state = context.get_dependency_state(&sibling).await?;
        Ok(Value::from(state.try_into_string()?))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store.set(&parse_key("proj/a/hello.txt")?, b"hello from a", &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, async fn read_sibling(state, dir: Key = query "-R-key/.", context) -> result)?;
    let env = builder.build()?;

    let state = env.evaluate("-R-cwd/proj/a/-/read_sibling").await?.get().await?;
    assert_eq!(state.try_into_string()?, "hello from a");
    Ok(())
}

#[tokio::test]
async fn ch05_options() -> Result<(), Error> {
    #[liquers_macro::command_version]
    fn now() -> Result<Value, Error> {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::general_error(e.to_string()))?;
        Ok(Value::from(format!("{}", since_epoch.as_nanos())))
    }

    #[liquers_macro::command_version]
    fn shout(text: &str) -> Result<Value, Error> {
        Ok(Value::from(text.to_uppercase()))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr,
        fn now() -> result
        namespace: "clock"
        label: "Now"
        doc: "Nanoseconds since 1970, as text"
        volatile: true
    )?;
    register_command!(cr,
        fn shout(text) -> result
        namespace: "words"
        label: "Shout"
        doc: "The input text in capitals"
        expires: "in 5 min"
        version: auto
    )?;
    let env = builder.build()?;

    let first = env.evaluate("ns-clock/now").await?.get().await?.try_into_string()?;
    let second = env.evaluate("ns-clock/now").await?.get().await?.try_into_string()?;
    assert_ne!(first, second); // volatile: computed on every request

    // Volatility is contagious: whatever is made from a volatile value is volatile too.
    let state = env.evaluate("ns-clock/now/ns-words/shout").await?.get().await?;
    assert_eq!(state.status(), Status::Volatile);
    Ok(())
}

#[tokio::test]
async fn ch05_later() -> Result<(), Error> {
    async fn later(state: State<Value>, millis: u64) -> Result<Value, Error> {
        tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
        Ok(Value::from(format!("{} later", state.try_into_string()?)))
    }

    fn word(text: String) -> Result<Value, Error> {
        Ok(Value::from(text))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, async fn later(state, millis: u64 = 10) -> result)?;
    register_command!(cr, fn word(text: String) -> result)?;
    let env = builder.build()?;

    let state = env.evaluate("word-soon/later").await?.get().await?;
    assert_eq!(state.try_into_string()?, "soon later");
    Ok(())
}

#[tokio::test]
async fn ch05_errors() -> Result<(), Error> {
    fn checked(text: &str) -> Result<Value, Error> {
        let number: i64 = text
            .parse()
            .map_err(|e| Error::general_error(format!("'{text}' is not a number: {e}")))?;
        Ok(Value::from(number))
    }

    fn word(text: String) -> Result<Value, Error> {
        Ok(Value::from(text))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn checked(text) -> result)?;
    register_command!(cr, fn word(text: String) -> result)?;
    let env = builder.build()?;

    let asset = env.evaluate("word-abc/checked").await?; // planned and started: no error yet
    let state = asset.get().await?; // finished: still no error
    assert_eq!(state.status(), Status::Error);
    let error = state.value().err().expect("the command's error");
    assert_eq!(error.error_type, ErrorType::General);
    assert!(error.message.starts_with("'abc' is not a number"));
    Ok(())
}

#[tokio::test]
async fn ch05_no_function() -> Result<(), Error> {
    use liquers_core::command_metadata::{ArgumentInfo, ArgumentType, CommandKey, CommandParameterValue};
    use liquers_core::pipeline::Pipeline;
    use serde_json::json;

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    store.set(&parse_key("templates/count.txt")?, b"{{.}} rows", &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;

    // An alias: `sales/first_rows` is `rec/head` with a default of one row.
    cr.register_alias(
        CommandKey::new("", "sales", "first_rows"),
        CommandKey::new("", "rec", "head"),
        vec![],
        vec![ArgumentInfo::integer_argument("n", false).with_default(1)],
    )?
    .with_label("First rows");

    // A pipeline command: several steps under one name, with an argument.
    cr.register_pipeline(
        CommandKey::new("", "sales", "count_region"),
        Pipeline::from_json(&json!([
            {"action": "to_record", "ns": "rec"},
            {"action": "eq", "ns": "rec", "parameters": ["region", {"placeholder": "region"}]},
            {"action": "nrows", "ns": "rec"}
        ]))?,
        vec![ArgumentInfo::any_argument("region").with_type(ArgumentType::String)],
    )?;

    // A template command: an alias of `template/render` with the template fixed.
    cr.register_alias(
        CommandKey::new("", "sales", "count_text"),
        CommandKey::new("", "template", "render"),
        vec![CommandParameterValue::Query(parse_query("-R/templates/count.txt")?)],
        vec![
            ArgumentInfo::any_argument("data").with_default(""),
            ArgumentInfo::string_argument("errors").with_default("strict"),
            ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?),
        ],
    )?;
    let env = builder.build()?;

    let count = env.evaluate("-R/data/sales.csv/-/ns-sales/count_region-EU").await?.get().await?;
    assert_eq!(count.try_into_string()?, "2");
    let text = env
        .evaluate("-R/data/sales.csv/-/ns-sales/count_region-EU/count_text")
        .await?
        .get()
        .await?;
    assert_eq!(text.try_into_string()?, "2 rows");
    let rows = env
        .evaluate("-R/data/sales.csv/-/ns-rec/to_record/ns-sales/first_rows/ns-rec/nrows")
        .await?
        .get()
        .await?;
    assert_eq!(rows.try_into_string()?, "1");
    Ok(())
}

#[tokio::test]
async fn ch05_manual() -> Result<(), Error> {
    use liquers_core::command_metadata::CommandKey;

    let prefix = String::from(">> "); // captured by the closure
    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    cr.register_command(CommandKey::new_name("answer"), |_state, _arguments, _context| {
        Ok(Value::from(42))
    })?
    .with_label("The answer");
    cr.register_command(CommandKey::new_name("quote"), move |state, _arguments, _context| {
        Ok(Value::from(format!("{prefix}{}", state.try_into_string()?)))
    })?;
    let env = builder.build()?;

    let state = env.evaluate("answer/quote").await?.get().await?;
    assert_eq!(state.try_into_string()?, ">> 42");
    Ok(())
}

#[tokio::test]
async fn ch06_store() -> Result<(), Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let key = parse_key("data/note.txt")?;
    let folder = parse_key("data")?;

    store.set(&key, b"remember the milk", &Metadata::new()).await?;

    let (data, metadata) = store.get(&key).await?;
    assert_eq!(data, b"remember the milk");
    assert_eq!(metadata.get_media_type(), "text/plain"); // from the extension of the key
    assert!(store.contains(&key).await?);
    assert!(store.is_dir(&folder).await?);
    assert_eq!(store.listdir(&folder).await?, vec!["note.txt"]);

    store.remove(&key).await?;
    let error = store.get(&key).await.err().expect("nothing there any more");
    assert_eq!(error.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn ch06_config() -> Result<(), Error> {
    use liquers_core::environment_config::EnvironmentConfig;
    use liquers_core::store_factory::default_store_factory;
    use liquers_lib::environment::default_environment_builder;

    let io = |e: std::io::Error| Error::from_error(ErrorType::General, e);
    let root = std::env::temp_dir().join(format!("liquers-guide-{}", std::process::id()));
    std::fs::create_dir_all(root.join("data")).map_err(io)?;
    std::fs::write(root.join("data/sales.csv"), "region,amount\nEU,120\nUS,80\n").map_err(io)?;
    std::env::set_var("LIQUERS_DATA", &root);

    let config = EnvironmentConfig::from_yaml(
        "
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
    - type: memory
      prefix: scratch
",
    )?;
    let mut builder = default_environment_builder::<Value, ()>()
        .with_config(config, Box::new(default_store_factory()));
    register_library(&mut builder.command_registry)?;
    let env = builder.build()?;

    let rows = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/nrows").await?.get().await?;
    assert_eq!(rows.try_into_string()?, "2");

    let store = env.get_async_store();
    store.set(&parse_key("data/note.txt")?, b"kept", &Metadata::new()).await?;
    assert_eq!(std::fs::read_to_string(root.join("data/note.txt")).map_err(io)?, "kept");
    store.set(&parse_key("scratch/note.txt")?, b"not kept", &Metadata::new()).await?;
    assert!(!root.join("scratch").exists());
    std::fs::remove_dir_all(&root).map_err(io)?;
    Ok(())
}

#[tokio::test]
async fn ch06_custom() -> Result<(), Error> {
    use async_trait::async_trait;
    use liquers_core::environment_config::EnvironmentConfig;
    use liquers_core::store_factory::{
        default_store_factory, StoreArgumentInfo, StoreArgumentType, StoreTypeInfo, StoreTypeMap,
    };
    use liquers_lib::environment::default_environment_builder;

    /// A read-only store with one file, `<prefix>/hello.txt`.
    struct Notes {
        prefix: Key,
        greeting: String,
    }

    #[async_trait]
    impl AsyncStore for Notes {
        fn key_prefix(&self) -> Key {
            self.prefix.clone()
        }

        fn is_supported(&self, key: &Key) -> bool {
            key.has_key_prefix(&self.prefix)
        }

        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            key.as_absolute()?;
            if *key != self.prefix.join("hello.txt") {
                return Err(Error::key_not_found(key));
            }
            let data = self.greeting.clone().into_bytes();
            let mut metadata = Metadata::MetadataRecord(self.default_metadata(key, false));
            self.finalize_metadata(&mut metadata, key, &data, false);
            Ok((data, metadata))
        }

        async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
            Err(Error::key_not_supported(key, &self.store_name()))
        }

        async fn contains(&self, key: &Key) -> Result<bool, Error> {
            Ok(*key == self.prefix || *key == self.prefix.join("hello.txt"))
        }

        async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
            Ok(*key == self.prefix)
        }

        async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
            Ok(if *key == self.prefix { vec!["hello.txt".to_string()] } else { vec![] })
        }
    }

    // --- factory
    let notes = StoreTypeMap::new().with_store_type(
        StoreTypeInfo::new("notes")
            .with_label("Notes")
            .with_doc("One greeting, read-only.")
            .with_argument(
                StoreArgumentInfo::new("greeting", StoreArgumentType::String)
                    .with_doc("The text of hello.txt."),
            ),
        Box::new(|config| {
            Ok(Box::new(Notes {
                prefix: config.key_prefix()?,
                greeting: config.require_config_string_expanded("greeting")?,
            }))
        }),
    );
    let factory = default_store_factory().chain(Box::new(notes));

    let config = EnvironmentConfig::from_yaml(
        "
store:
  stores:
    - {type: notes, prefix: notes, config: {greeting: Hello}}
    - {type: memory, prefix: scratch}
",
    )?;
    let mut builder =
        default_environment_builder::<Value, ()>().with_config(config, Box::new(factory));
    register_library(&mut builder.command_registry)?;
    let env = builder.build()?;

    let state = env.evaluate("-R/notes/hello.txt").await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hello");
    let store = env.get_async_store();
    assert_eq!(store.listdir(&parse_key("notes")?).await?, vec!["hello.txt"]);
    let refused = store.set(&parse_key("notes/new.txt")?, b"x", &Metadata::new()).await;
    assert_eq!(refused.err().expect("read-only").error_type, ErrorType::KeyNotSupported);
    Ok(())
}

#[tokio::test]
async fn ch07_asset() -> Result<(), Error> {
    use liquers_core::assets::AssetNotificationMessage;

    async fn slow_report(_state: State<Value>, context: Context<CommandEnvironment>) -> Result<Value, Error> {
        context.info("collecting")?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Ok(Value::from("the report"))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, async fn slow_report(state, context) -> result)?;
    let env = builder.build()?;

    let asset = env.evaluate("slow_report/report.txt").await?; // returns at once
    assert!(!asset.status().await.is_finished());
    let mut notifications = asset.subscribe_to_notifications().await;

    let state = asset.get().await?; // waits
    assert_eq!(state.try_into_string()?, "the report");
    assert_eq!(asset.status().await, Status::Ready);
    assert!(matches!(
        *notifications.borrow_and_update(),
        AssetNotificationMessage::JobFinished | AssetNotificationMessage::StatusChanged(Status::Ready)
    ));

    // The same query gives the same asset, not a second computation.
    let again = env.evaluate("slow_report/report.txt").await?;
    assert_eq!(again.id(), asset.id());

    // The value as bytes with its metadata, as a server would send it.
    let (bytes, metadata) = asset.get_binary().await?;
    assert_eq!(bytes.as_slice(), b"the report");
    assert_eq!(metadata.get_media_type(), "text/plain");
    Ok(())
}

#[tokio::test]
async fn ch07_fresh() -> Result<(), Error> {
    use liquers_core::assets::AssetManager;

    fn shout(text: &str) -> Result<Value, Error> {
        Ok(Value::from(text.to_uppercase()))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn shout(text) -> result)?;
    let env = builder.build()?;
    let assets = env.get_asset_manager();
    let key = parse_key("data/note.txt")?;

    assets.set_state(&key, State::new().with_data(Value::from("milk"))).await?;
    let first = env.evaluate("-R/data/note.txt/-/shout").await?;
    assert_eq!(first.get().await?.try_into_string()?, "MILK");

    // A write through the asset manager makes everything made from the key out of date.
    assets.set_state(&key, State::new().with_data(Value::from("bread"))).await?;
    assert_eq!(first.status().await, Status::Expired);
    let second = env.evaluate("-R/data/note.txt/-/shout").await?;
    assert_eq!(second.get().await?.try_into_string()?, "BREAD");

    // Throwing a result away by hand.
    second.expire().await?;
    assert!(second.is_expired().await);
    Ok(())
}

#[tokio::test]
async fn ch07_recipes() -> Result<(), Error> {
    use liquers_core::assets::AssetManager;

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let recipes = b"
recipes:
  - query: -R/./sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
    title: Sales in the EU
    description: The rows of sales.csv with region EU
  - query: -R/./sales.csv/-/ns-rec/to_record/nrows/count.txt
    title: Number of sales
    expires: in 1 h
";
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    store.set(&parse_key("data/recipes.yaml")?, recipes, &Metadata::new()).await?;
    let env = library_environment(store.clone())?;
    let assets = env.get_asset_manager();

    // The folder lists what is stored and what can be made.
    let names = assets.listdir(&parse_key("data")?).await?;
    assert!(names.contains(&"eu.csv".to_string()) && names.contains(&"count.txt".to_string()));
    let info = assets.get_asset_info(&parse_key("data/eu.csv")?).await?;
    assert_eq!(info.status, Status::Recipe); // known, not made yet
    assert_eq!(info.title, "Sales in the EU");

    // Asking for the key makes it and stores it.
    let count = assets.get(&parse_key("data/count.txt")?).await?.get().await?;
    assert_eq!(count.try_into_string()?, "3");
    assert!(store.contains(&parse_key("data/count.txt")?).await?);

    // The recipe behind a key.
    let recipe = assets.recipe_opt(&parse_key("data/eu.csv")?).await?.expect("a recipe");
    assert_eq!(recipe.title, "Sales in the EU");
    Ok(())
}

#[tokio::test]
async fn ch07_cancel() -> Result<(), Error> {
    async fn forever(_state: State<Value>) -> Result<Value, Error> {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        Ok(Value::none())
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, async fn forever(state) -> result)?;
    let env = builder.build()?;

    let asset = env.evaluate("forever").await?;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await; // let it start
    asset.cancel().await?;
    assert_eq!(asset.status().await, Status::Cancelled);
    let state = asset.get().await?;
    let error = state.value().err().expect("no value");
    assert_eq!(error.error_type, ErrorType::Cancelled);
    Ok(())
}

#[tokio::test]
async fn ch08_builder() -> Result<(), Error> {
    use liquers_core::assets::AssetManager;
    use liquers_core::cache_strategy::CacheStrategy;
    use liquers_core::environment_builder::AssetManagerOptions;
    use liquers_lib::environment::default_environment_builder;

    fn hello() -> Result<Value, Error> {
        Ok(Value::from("world"))
    }

    let options = AssetManagerOptions::default()
        .with_job_capacity(4) // at most four evaluations at a time
        .with_query_cache_strategy(CacheStrategy::Result); // keep results, not intermediate steps

    let mut builder = default_environment_builder::<Value, ()>()
        .with_async_store(Arc::new(AsyncMemoryStore::new(&Key::new())))
        .with_asset_manager_options(options)
        .with_default_namespaces(["rec"]);
    register_library(&mut builder.command_registry)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn hello() -> result)?;

    // Look at the command metadata before building; build() refuses it if it has errors.
    let report = builder.validate();
    assert!(!report.has_errors());

    let env = builder.build()?; // started and ready
    assert!(env.get_asset_manager().is_started());
    assert!(env.command_metadata_snapshot().commands.iter().any(|c| c.name == "hello"));
    Ok(())
}

#[tokio::test]
async fn ch08_document() -> Result<(), Error> {
    use liquers_core::environment_config::ConfigDocument;
    use liquers_core::store_factory::default_store_factory;
    use liquers_lib::environment::default_environment_builder;

    #[derive(serde::Deserialize)]
    struct Report {
        title: String,
    }

    let mut doc = ConfigDocument::from_yaml(
        "
store:
  stores:
    - {type: memory, prefix: data}
recipes: default
assets:
  job_capacity: 8
default_namespaces: [rec]
report:
  title: Monthly sales
",
    )?;
    let report: Option<Report> = doc.take_section("report")?; // a section of your own
    let config = doc.finish()?; // an unknown section would fail here

    let mut builder = default_environment_builder::<Value, ()>()
        .with_config(config, Box::new(default_store_factory()));
    register_library(&mut builder.command_registry)?;
    let env = builder.build()?;

    env.get_async_store().set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    // `default_namespaces: [rec]` lets the query leave out `ns-rec`.
    let rows = env.evaluate("-R/data/sales.csv/-/to_record/nrows").await?.get().await?;
    assert_eq!(rows.try_into_string()?, "3");
    assert_eq!(report.expect("the section").title, "Monthly sales");
    Ok(())
}

mod ch08_inline {
use liquers_core::{
    context::Environment,
    environment_builder::{EnvironmentBuilder, Inline},
    error::Error,
};
use liquers_lib::value::Value;

// No Tokio runtime: the Inline kind evaluates in the caller's task and spawns nothing.
fn main() -> Result<(), Error> {
    let mut builder = EnvironmentBuilder::<Value, (), Inline>::new();
    builder.command_registry.register_command(
        liquers_core::command_metadata::CommandKey::new_name("hello"),
        |_state, _arguments, _context| Ok(Value::from("world")),
    )?;
    let env = builder.build()?;

    let state = futures::executor::block_on(async { env.evaluate("hello").await?.get().await })?;
    assert_eq!(state.try_into_string()?, "world");
    Ok(())
}
#[test]
fn run() {
    main().unwrap()
}
}

#[tokio::test]
async fn ch09_table() -> Result<(), Error> {
    use liquers_records::FieldValue;

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store
        .set(
            &parse_key("data/sales.csv")?,
            b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n",
            &Metadata::new(),
        )
        .await?;
    let env = library_environment(store)?;

    let state = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record").await?.get().await?;
    let table = state.value()?.as_record_view()?; // Arc<dyn RecordView>
    assert_eq!(table.len(), 3);
    let names: Vec<&str> = table.schema().fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(names, ["region", "month", "amount"]);
    assert_eq!(table.value(0, 2)?, FieldValue::Int(120)); // row 0, column 2
    assert_eq!(table.column(0)?.get(1)?, FieldValue::Text(Arc::from("US")));

    let query = "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-amount/eu.csv";
    let eu = env.evaluate(query).await?.get().await?;
    assert_eq!(
        String::from_utf8_lossy(&eu.as_bytes()?),
        "region,month,amount\nEU,feb,95\nEU,jan,120\n"
    );
    Ok(())
}

#[tokio::test]
async fn ch09_command() -> Result<(), Error> {
    use liquers_records::{
        CompareOp, FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView,
        RecordViewMut,
    };

    // A command that makes a table.
    fn sales() -> Result<Value, Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("region", FieldType::Text),
            FieldSchema::new("amount", FieldType::Float),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 3);
        batch.append_row(&[FieldValue::Text(Arc::from("north")), FieldValue::Float(120.0)])?;
        batch.append_row(&[FieldValue::Text(Arc::from("south")), FieldValue::Float(80.5)])?;
        batch.append_row(&[FieldValue::Text(Arc::from("north")), FieldValue::Float(30.0)])?;
        let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
        Ok(Value::from_record_view(view))
    }

    // A command that reads one: the sum of `amount` over the rows of one region.
    fn total(state: &State<Value>, region: String) -> Result<Value, Error> {
        let table = state.value()?.as_record_view()?;
        let mask = table.column(0)?.compare(CompareOp::Eq, &FieldValue::Text(Arc::from(region)))?;
        let rows = table.filter(&mask)?; // a view: no data is copied
        let mut sum = 0.0;
        for row in 0..rows.len() {
            if let FieldValue::Float(amount) = rows.value(row, 1)? {
                sum += amount;
            }
        }
        Ok(Value::from(sum))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn sales() -> result)?;
    register_command!(cr, fn total(state, region: String) -> result)?;
    let env = builder.build()?;

    let text = |query: &'static str| {
        let env = env.clone();
        async move { env.evaluate(query).await?.get().await?.try_into_string() }
    };
    assert_eq!(text("sales/total-north").await?, "150");
    assert_eq!(text("sales/ns-rec/nrows").await?, "3"); // a library command on your table
    assert_eq!(text("sales/ns-rec/gt-amount-100/total-north").await?, "120");
    Ok(())
}

#[tokio::test]
async fn ch09_polars() -> Result<(), Error> {
    use polars::prelude::*;

    fn numbers() -> Result<Value, Error> {
        let frame = df!("a" => [1, 2, 3]).map_err(|e| Error::from_error(ErrorType::General, e))?;
        Ok(Value::from_polars_dataframe(frame))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn numbers() -> result)?;
    let env = builder.build()?;

    let state = env.evaluate("-R/data/sales.csv/-/ns-pl/from_csv/gt-amount-90").await?.get().await?;
    assert_eq!(state.type_identifier(), "polars.DataFrame");
    let frame = state.value()?.as_polars_dataframe()?; // Arc<DataFrame>
    assert_eq!(frame.height(), 2);

    let head = env.evaluate("numbers/ns-pl/head-2").await?.get().await?;
    assert_eq!(head.value()?.as_polars_dataframe()?.height(), 2);

    // The two table types convert at the namespace border.
    let count = env.evaluate("numbers/ns-rec/nrows").await?.get().await?;
    assert_eq!(count.try_into_string()?, "3");
    Ok(())
}

#[tokio::test]
async fn ch09_images() -> Result<(), Error> {
    use image::{DynamicImage, Rgb, RgbImage};

    fn red() -> Result<Value, Error> {
        let picture = RgbImage::from_pixel(40, 20, Rgb([255, 0, 0]));
        Ok(Value::from_image(Arc::new(DynamicImage::ImageRgb8(picture))))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn red() -> result)?;
    let env = builder.build()?;

    let small = env.evaluate("red/ns-img/resize-20-10").await?.get().await?;
    assert_eq!(small.value()?.as_image()?.width(), 20);
    let dims = env.evaluate("red/ns-img/dims").await?.get().await?;
    assert_eq!(dims.try_into_string()?, "40x20");
    let png = env.evaluate("red/ns-img/gray/picture.png").await?.get().await?;
    assert_eq!(&png.as_bytes()?[..4], b"\x89PNG"); // the file name chose the format
    Ok(())
}

#[tokio::test]
async fn ch09_chunks() -> Result<(), Error> {
    use liquers_records::{FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView, RecordViewMut};

    fn part(i: i64) -> Result<Value, Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Int(10 * i)])?;
        batch.append_row(&[FieldValue::Int(10 * i + 1)])?;
        let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
        Ok(Value::from_record_view(view))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let manifest = b"
manifest: record-stream
chunks:
  - query: part-0
  - query: part-1
  - query: part-2
";
    store.set(&parse_key("data/parts.manifest.yaml")?, manifest, &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn part(i: i64 = 0) -> result)?;
    let env = builder.build()?;

    // The source: nothing is computed yet.
    let source = env.evaluate("-R/data/parts.manifest.yaml/-/ns-rec/to_record_source").await?.get().await?;
    assert_eq!(source.type_identifier(), "RecordSource");

    // Narrowed chunk by chunk, then gathered into one view.
    let query = "-R/data/parts.manifest.yaml/-/ns-rec/to_record_source/gt-n-10/materialize";
    let rows = env.evaluate(query).await?.get().await?.value()?.as_record_view()?;
    assert_eq!(rows.len(), 3); // 11, 20, 21
    Ok(())
}

#[tokio::test]
async fn ch10_queries() -> Result<(), Error> {
    fn number(n: i64) -> Result<Value, Error> {
        Ok(Value::from(n))
    }
    fn place(region: String) -> Result<Value, Error> {
        Value::try_from_json_value(&serde_json::json!({ "region": region }))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let report = b"# Sales in {{region}}
{{#$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
- {{month}}: {{amount}}
{{/}}
{{^$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
No sales.
{{/}}
";
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    store.set(&parse_key("templates/greet.md")?, b"Hello, {{.}}!", &Metadata::new()).await?;
    store.set(&parse_key("templates/report.md")?, report, &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn number(n: i64) -> result)?;
    register_command!(cr, fn place(region: String) -> result)?;
    let env = builder.build()?;

    let text = |query: &'static str| {
        let env = env.clone();
        async move { env.evaluate(query).await?.get().await?.try_into_string() }
    };

    // render: the template is an argument, the input is what `{{.}}` and plain names read.
    assert_eq!(
        text("number-7/ns-template/render-~X~-R/templates/greet.md~E").await?,
        "Hello, 7!"
    );
    assert_eq!(
        text("place-EU/ns-template/render-~X~-R/templates/report.md~E").await?,
        "# Sales in EU\n- jan: 120\n- feb: 95\n"
    );

    // expand: the template is the input, the data is an argument.
    assert_eq!(
        text("-R/templates/report.md/-/ns-template/expand-~X~place-ASIA~E").await?,
        "# Sales in ASIA\nNo sales.\n"
    );
    Ok(())
}

#[tokio::test]
async fn ch10_errors() -> Result<(), Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let draft = b"line one\n{{$ no_such_command}} end";
    store.set(&parse_key("templates/draft.md")?, draft, &Metadata::new()).await?;
    let env = library_environment(store)?;

    // strict, the default: the render fails with the error of the tag.
    let strict = env.evaluate("-R/templates/draft.md/-/ns-template/expand").await?.get().await?;
    let error = strict.value().err().expect("a failing tag");
    assert_eq!(error.error_type, ErrorType::ActionNotRegistered);
    assert!(error.message.contains("no_such_command"));

    // silent: the failing tag renders as nothing, and the log has a warning.
    let silent = env.evaluate("-R/templates/draft.md/-/ns-template/expand--silent").await?.get().await?;
    assert_eq!(silent.try_into_string()?, "line one\n end");

    // validate: checks every tag and runs nothing.
    let problems = env.evaluate("-R/templates/draft.md/-/ns-template/validate").await?.get().await?;
    let problems = problems.value()?.try_into_json_value()?; // a list, one entry for each problem
    assert!(problems[0]["message"].to_string().contains("no_such_command"));
    Ok(())
}

#[tokio::test]
async fn ch10_rust() -> Result<(), Error> {
    use liquers_lib::template::{
        render_top_level, ErrorMode, FrameData, FrameKind, RenderRequest, TemplateSource,
    };

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store.set(&parse_key("templates/header.md")?, b"Report for {{name}}", &Metadata::new()).await?;
    let page = b"{{> ./header.md}}: {{$ -R/data/sales.csv/-/ns-rec/to_record/nrows}} rows";
    store.set(&parse_key("templates/page.md")?, page, &Metadata::new()).await?;
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    let env = library_environment(store)?;

    let data = State::new().with_data(Value::try_from_json_value(&serde_json::json!({"name": "Ann"}))?);
    let request = RenderRequest {
        template: TemplateSource::Link(parse_query("-R/templates/page.md")?),
        frames: vec![(FrameData::State(data), FrameKind::Data)],
        errors: ErrorMode::Strict,
        cwd: parse_key("templates")?, // where `./header.md` is looked up
        html_escape: false,
    };
    let render = render_top_level(&env, request)?; // a parse error fails here
    let rendered = render.run().await?.expect("not cancelled");
    assert_eq!(rendered.text, "Report for Ann: 3 rows");
    assert!(rendered.warnings.is_empty());
    Ok(())
}

#[tokio::test]
async fn ch10_standalone() -> Result<(), Error> {
    use liquers_core::data_node::AnyNode;
    use liquers_template::{
        parse_template, render, ContextStack, FrameKind, MapHost, RenderOptions, TemplateSyntax,
    };

    let template = parse_template("Hello {{name}}{{#items}}, {{.}}{{/items}}", TemplateSyntax::Mustache)?;
    let mut stack = ContextStack::new();
    stack.push(
        AnyNode::from_json(serde_json::json!({"name": "world", "items": [1, 2]})),
        FrameKind::Data,
    );
    let text = render(&template, stack, &MapHost::new(), &RenderOptions::default()).await?;
    assert_eq!(text, "Hello world, 1, 2");
    Ok(())
}

#[tokio::test]
async fn ch13_search() -> Result<(), Error> {
    use liquers_core::assets::AssetManager;
    use liquers_core::metadata::MetadataRecord;
    use liquers_core::query::{ActionParameter, ActionRequest};
    use liquers_records::FieldValue;

    // A Markdown document as a state: the text, and the format it is written in.
    fn document(text: &str) -> State<Value> {
        let mut record = MetadataRecord::new();
        record.data_format = Some("md".to_string());
        State::from_value_and_metadata(Value::from(text), Arc::new(Metadata::MetadataRecord(record)))
    }

    let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let assets = env.get_asset_manager();
    let shopping = "---\ntitle: Shopping\nstatus: open\n---\n# Shopping\n\nRemember the milk and the bread.\n";
    let trip = "---\ntitle: Trip to Prague\nstatus: done\n---\n# Trip\n\nBook the train to Prague.\n";
    assets.set_state(&parse_key("notes/shopping.md")?, document(shopping)).await?;
    assets.set_state(&parse_key("notes/trip.md")?, document(trip)).await?;

    // The values of one column of a table, as text.
    let column = |query: String, name: &'static str| {
        let env = env.clone();
        async move {
            let table = env.evaluate(query).await?.get().await?.value()?.as_record_view()?;
            let index = table.schema().fields.iter().position(|field| field.name == name);
            let index = index.ok_or_else(|| Error::general_error(format!("no column {name}")))?;
            let mut cells = Vec::new();
            for row in 0..table.len() {
                match table.value(row, index)? {
                    FieldValue::Text(text) => cells.push(text.to_string()),
                    other => cells.push(format!("{other:?}")),
                }
            }
            Ok::<_, Error>(cells)
        }
    };

    // A folder as a table: one row for each file.
    let files = column("-R-key/notes/-/ns-search/catalog".to_string(), "filename").await?;
    assert_eq!(files, ["shopping.md", "trip.md"]);

    // Full-text search in the files.
    let hits = column("-R-key/notes/-/ns-search/catalog-t-t/search-milk".to_string(), "key").await?;
    assert_eq!(hits, ["notes/shopping.md"]);

    // --- fields
    // Search by a field of the front matter. The expression has a colon: let the encoder escape it.
    let search = ActionRequest::new("search".to_owned()).with_parameters(
        ["status:open", "f", "f", "0"]
            .iter()
            .map(|text| ActionParameter::new_string(text.to_string()))
            .collect(),
    );
    assert_eq!(search.encode(), "search-status~ncolon~open-f-f-0");
    let query = format!("-R-key/notes/-/ns-search/field_extract/{}", search.encode());
    assert_eq!(column(query, "title").await?, ["Shopping"]);
    Ok(())
}

/// Section 14.2: the shape of a test module.
mod ch14_tests {
use crate::prelude::*;

// The command under test.
fn shout(text: &str) -> Result<Value, Error> {
    if text.is_empty() {
        return Err(Error::general_error("nothing to shout".to_string()));
    }
    Ok(Value::from(text.to_uppercase()))
}

pub fn register(cr: &mut liquers_core::commands::CommandRegistry<CommandEnvironment>) -> Result<(), Error> {
    register_command!(cr, fn shout(text) -> result)?;
    Ok(())
}

/// A fresh environment for one test: its own memory store, the library, the commands under test.
async fn test_environment(files: &[(&str, &[u8])]) -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    for (key, data) in files {
        store.set(&parse_key(*key)?, data, &Metadata::new()).await?;
    }
    let mut builder = library_builder(store)?;
    register(&mut builder.command_registry)?;
    builder.build()
}

#[test]
fn shout_is_a_function() {
    assert_eq!(shout("abc").unwrap().try_into_string().unwrap(), "ABC");
    assert!(shout("").is_err());
}

#[tokio::test]
async fn shout_in_a_query() -> Result<(), Error> {
    let env = test_environment(&[("data/note.txt", b"milk")]).await?;
    let state = env.evaluate("-R/data/note.txt/-/shout").await?.get().await?;
    assert_eq!(state.try_into_string()?, "MILK");
    Ok(())
}

#[tokio::test]
async fn shout_reports_an_empty_input() -> Result<(), Error> {
    let env = test_environment(&[("data/empty.txt", b"")]).await?;
    let state = env.evaluate("-R/data/empty.txt/-/shout").await?.get().await?;
    let error = state.value().err().expect("an error");
    assert_eq!(error.error_type, ErrorType::General);
    assert!(error.message.contains("nothing to shout"));
    Ok(())
}

#[tokio::test]
async fn a_wrong_query_fails_before_it_runs() -> Result<(), Error> {
    let env = test_environment(&[]).await?;
    let error = env.evaluate("shout-1-2-3").await.err().expect("too many arguments");
    assert_eq!(error.error_type, ErrorType::TooManyParameters);
    Ok(())
}
}

#[tokio::test]
async fn ch15_errors() -> Result<(), Error> {
    fn fail() -> Result<Value, Error> {
        Err(Error::general_error("the disk is on fire".to_string()))
    }

    let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn fail() -> result)?;
    let env = builder.build()?;

    // 1. The text is not a query.
    let error = env.evaluate("fail/-x-/").await.err().expect("a parse error");
    assert_eq!(error.error_type, ErrorType::ParseError);

    // 2. The query cannot be planned.
    let error = env.evaluate("no_such_command").await.err().expect("a planning error");
    assert_eq!(error.error_type, ErrorType::ActionNotRegistered);

    // 3. A command fails: the asset ends in the status Error and keeps the report.
    let asset = env.evaluate("fail").await?;
    let state = asset.get().await?;
    assert_eq!(state.status(), Status::Error);
    let error = state.value().err().expect("the command's error");
    assert_eq!(error.error_type, ErrorType::General);
    assert_eq!(error.message, "the disk is on fire");
    assert!(error.to_string().contains("fail")); // the text names the command
    assert!(state.metadata.is_error()?);

    // 4. Data is missing.
    let state = env.evaluate("-R/data/missing.csv").await?.get().await?;
    let error = state.value().err().expect("no such file");
    assert_eq!(error.error_type, ErrorType::AssetNotFound);
    Ok(())
}
