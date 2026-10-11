//! `Option<T>` arguments declared through `register_command!`.
//!
//! See `specs/design/register-command-option-value/`. Each command returns its argument formatted
//! with `{:?}`, so a test reads `None` / `Some(5)` directly.

use liquers_core::{
    command_metadata::{ArgumentType, CommandKey},
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::{Value, ValueInterface},
};
use liquers_macro::*;

fn opt_i(x: Option<i64>) -> Result<Value, Error> {
    Ok(Value::from(format!("{x:?}")))
}
fn opt_i_d(x: Option<i64>) -> Result<Value, Error> {
    Ok(Value::from(format!("{x:?}")))
}
fn opt_b(x: Option<bool>) -> Result<Value, Error> {
    Ok(Value::from(format!("{x:?}")))
}
fn opt_b_d(x: Option<bool>) -> Result<Value, Error> {
    Ok(Value::from(format!("{x:?}")))
}
fn five(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(5i64))
}
fn yes(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(true))
}
fn nothing(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::none())
}

fn environment() -> Result<SimpleEnvironment<Value>, Error> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn opt_i(x: Option<i64>) -> result)?;
    register_command!(cr, fn opt_i_d(x: Option<i64> = 7) -> result)?;
    register_command!(cr, fn opt_b(x: Option<bool>) -> result)?;
    register_command!(cr, fn opt_b_d(x: Option<bool> = true) -> result)?;
    register_command!(cr, fn five(state) -> result)?;
    register_command!(cr, fn yes(state) -> result)?;
    register_command!(cr, fn nothing(state) -> result)?;
    Ok(env)
}

async fn run(query: &str) -> Result<String, Error> {
    let envref = environment()?.to_ref();
    evaluate(envref, query, None).await?.try_into_string()
}

#[tokio::test]
async fn option_i64_absent_and_present() -> Result<(), Error> {
    assert_eq!(run("opt_i").await?, "None");
    assert_eq!(run("opt_i-").await?, "None");
    assert_eq!(run("opt_i-5").await?, "Some(5)");
    Ok(())
}

#[tokio::test]
async fn option_i64_default_on_empty() -> Result<(), Error> {
    assert_eq!(run("opt_i_d").await?, "Some(7)");
    assert_eq!(run("opt_i_d-").await?, "Some(7)");
    Ok(())
}

#[test]
fn option_widths_register() -> Result<(), Error> {
    fn widths(
        a: Option<i8>,
        b: Option<i16>,
        c: Option<i32>,
        d: Option<isize>,
        e: Option<u8>,
        f: Option<u16>,
        g: Option<u32>,
        h: Option<u64>,
        i: Option<usize>,
        j: Option<f32>,
        k: Option<f64>,
    ) -> Result<Value, Error> {
        Ok(Value::from(format!(
            "{a:?}{b:?}{c:?}{d:?}{e:?}{f:?}{g:?}{h:?}{i:?}{j:?}{k:?}"
        )))
    }
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr,
        fn widths(
            a: Option<i8>,
            b: Option<i16>,
            c: Option<i32>,
            d: Option<isize>,
            e: Option<u8>,
            f: Option<u16>,
            g: Option<u32>,
            h: Option<u64>,
            i: Option<usize>,
            j: Option<f32>,
            k: Option<f64>
        ) -> result
    )?;
    let metadata = cr
        .command_metadata_registry
        .get(CommandKey::new_name("widths"))
        .ok_or(Error::general_error("widths not registered".to_string()))?;
    let types: Vec<&ArgumentType> = metadata
        .arguments
        .iter()
        .map(|a| &a.argument_type)
        .collect();
    for (index, argument_type) in types.iter().enumerate() {
        let expected = if index < 9 {
            ArgumentType::IntegerOption
        } else {
            ArgumentType::FloatOption
        };
        assert_eq!(**argument_type, expected, "argument {index}");
    }
    Ok(())
}

#[tokio::test]
async fn option_widths_convert() -> Result<(), Error> {
    fn narrow(a: Option<i8>, b: Option<u16>, c: Option<f32>) -> Result<Value, Error> {
        Ok(Value::from(format!("{a:?} {b:?} {c:?}")))
    }
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn narrow(a: Option<i8>, b: Option<u16>, c: Option<f32>) -> result)?;
    let envref = env.to_ref();
    let result = evaluate(envref.clone(), "narrow-3-4-1.5", None).await?;
    assert_eq!(result.try_into_string()?, "Some(3) Some(4) Some(1.5)");
    let result = evaluate(envref, "narrow", None).await?;
    assert_eq!(result.try_into_string()?, "None None None");
    Ok(())
}

#[tokio::test]
async fn option_bool_spellings() -> Result<(), Error> {
    for spelling in ["t", "TRUE", "yes", "y", "1"] {
        assert_eq!(
            run(&format!("opt_b-{spelling}")).await?,
            "Some(true)",
            "{spelling}"
        );
    }
    for spelling in ["f", "No", "n", "0"] {
        assert_eq!(
            run(&format!("opt_b-{spelling}")).await?,
            "Some(false)",
            "{spelling}"
        );
    }
    for query in ["opt_b-none", "opt_b-NONE", "opt_b"] {
        assert_eq!(run(query).await?, "None", "{query}");
    }
    Ok(())
}

#[tokio::test]
async fn option_bool_default_on_empty() -> Result<(), Error> {
    assert_eq!(run("opt_b_d").await?, "Some(true)");
    assert_eq!(run("opt_b_d-").await?, "Some(true)");
    assert_eq!(run("opt_b_d-none").await?, "None");
    Ok(())
}

#[tokio::test]
async fn option_bool_rejects_other_text() -> Result<(), Error> {
    let error = match run("opt_b-maybe").await {
        Ok(value) => panic!("opt_b-maybe must fail, got {value}"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("boolean or none"),
        "error: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn option_from_link_none_and_value() -> Result<(), Error> {
    assert_eq!(run("opt_i-~X~nothing~E").await?, "None");
    assert_eq!(run("opt_i-~X~five~E").await?, "Some(5)");
    assert_eq!(run("opt_b-~X~nothing~E").await?, "None");
    assert_eq!(run("opt_b-~X~yes~E").await?, "Some(true)");
    Ok(())
}
