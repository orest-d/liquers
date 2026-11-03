//! `ns-md` end to end through `EnvRef::evaluate`
//! (`specs/design/markdown-navigation/phase3-examples.md` §Test Plan).
#![cfg(feature = "markdown")]

use std::sync::Arc;

use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::value::ValueInterface;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::records::{FieldValue, RecordView};
use liquers_lib::register_markdown_commands;
use liquers_lib::value::{ExtValueInterface, Value};

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value>;
type Res = Result<(), Box<dyn std::error::Error>>;

const SERVICE: &str = "---
title: Service
---
# Agent Memory Service

Intro with a [guide](../guides/GUIDE.md#part).

## 1. Layers

Layers text.

## 8. Storage

Storage text, see [store config][sc], <https://example.org> and
[a query](-R/data/a_b.csv/-/replace-a~~b-x_y).

### Details

More.

[sc]: ../guides/STORE_CONFIG_GUIDE.md

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | First | design |
| 2026-11-02 | Second | review |

# Appendix

## Notes

A.

Phase 2
-------

## Notes

B.
";

async fn env() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let key = parse_key("specs/reference/SERVICE.md")?;
    let mut metadata = Metadata::new();
    metadata.set_filename("SERVICE.md")?;
    metadata.set_status(Status::Source)?;
    store.set(&key, SERVICE.as_bytes(), &metadata).await?;
    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        register_markdown_commands!(cr)?;
    }
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

async fn eval(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<State<Value>, Error> {
    let state = envref.evaluate(query).await?.get().await?;
    state.error_result()?;
    Ok(state)
}

async fn view_of(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<Arc<dyn RecordView>, Error> {
    eval(envref, query).await?.value()?.as_record_view()
}

async fn text_of(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<String, Error> {
    eval(envref, query).await?.value()?.try_into_string()
}

fn texts(view: &Arc<dyn RecordView>, name: &str) -> Vec<String> {
    let index = view.schema().index_of(name).unwrap_or_else(|| panic!("no column '{name}'"));
    (0..view.len())
        .map(|row| match view.value(row, index).expect("cell") {
            FieldValue::Text(t) => t.to_string(),
            FieldValue::Null => String::new(),
            FieldValue::Int(i) => i.to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

const DOC: &str = "-R/specs/reference/SERVICE.md/-/ns-md";

#[tokio::test]
async fn outline_lists_headings() -> Res {
    let env = env().await?;
    let view = view_of(&env, &format!("{DOC}/outline")).await?;
    assert_eq!(
        texts(&view, "heading"),
        vec!["Agent Memory Service", "1. Layers", "8. Storage", "Details", "History", "Appendix", "Notes", "Phase 2", "Notes"]
    );
    assert_eq!(texts(&view, "address"), vec!["1", "1.1", "1.2", "1.2.1", "1.3", "2", "2.1", "2.2", "2.3"]);
    assert_eq!(texts(&view, "number")[2], "8");
    assert_eq!(texts(&view, "path")[3], "Agent Memory Service > 8. Storage > Details");
    assert_eq!(texts(&view, "line")[0], "4");
    assert_eq!(texts(&view, "level")[7], "2", "a setext heading counts");
    Ok(())
}

#[tokio::test]
async fn section_by_number_text_and_placeholder() -> Res {
    let env = env().await?;
    let by_number = text_of(&env, &format!("{DOC}/section-1.2")).await?;
    assert!(by_number.starts_with("## 8. Storage\n") && by_number.contains("### Details"), "{by_number}");
    assert_eq!(text_of(&env, &format!("{DOC}/section-_.storage")).await?, by_number);
    assert_eq!(text_of(&env, &format!("{DOC}/section-1.storage")).await?, by_number);
    assert_eq!(text_of(&env, &format!("{DOC}/section-appendix.phase~.2")).await?, "Phase 2\n-------\n\n");
    Ok(())
}

#[tokio::test]
async fn section_flags() -> Res {
    let env = env().await?;
    let own = text_of(&env, &format!("{DOC}/section-_.storage-f")).await?;
    assert!(own.starts_with("## 8. Storage") && !own.contains("Details"), "{own}");
    let body = text_of(&env, &format!("{DOC}/section-_.storage-f-f")).await?;
    assert!(body.starts_with("\nStorage text"), "{body:?}");
    Ok(())
}

#[tokio::test]
async fn section_errors_name_candidates() -> Res {
    let env = env().await?;
    let ambiguous = eval(&env, &format!("{DOC}/section-_.notes")).await;
    let message = format!("{:?}", ambiguous.err().ok_or("expected an error")?);
    assert!(message.contains("2.1 (Notes") && message.contains("2.3 (Notes"), "{message}");
    let missing = eval(&env, &format!("{DOC}/section-storage")).await;
    let message = format!("{:?}", missing.err().ok_or("expected an error")?);
    assert!(message.contains("deeper sections match: 1.2 (8. Storage"), "{message}");
    let reserved = eval(&env, &format!("{DOC}/section-my_var")).await;
    assert!(reserved.is_err());
    Ok(())
}

#[tokio::test]
async fn sections_level_filter() -> Res {
    let env = env().await?;
    let all = view_of(&env, &format!("{DOC}/sections")).await?;
    assert_eq!(all.len(), 9);
    let level2 = view_of(&env, &format!("{DOC}/sections--2")).await?;
    assert_eq!(texts(&level2, "heading"), vec!["1. Layers", "8. Storage", "History", "Notes", "Phase 2", "Notes"]);
    let under = view_of(&env, &format!("{DOC}/sections-_.storage")).await?;
    assert_eq!(texts(&under, "heading"), vec!["8. Storage", "Details"]);
    assert!(texts(&under, "text")[1].starts_with("### Details"));
    Ok(())
}

#[tokio::test]
async fn links_resolve_against_the_input_key() -> Res {
    let env = env().await?;
    let view = view_of(&env, &format!("{DOC}/links")).await?;
    assert_eq!(texts(&view, "target_kind"), vec!["relative", "relative", "url", "query"]);
    assert_eq!(texts(&view, "key")[0], "specs/guides/GUIDE.md");
    assert_eq!(texts(&view, "anchor")[0], "part");
    assert_eq!(texts(&view, "kind")[1], "reference");
    assert_eq!(texts(&view, "key")[1], "specs/guides/STORE_CONFIG_GUIDE.md");
    assert_eq!(texts(&view, "target")[3], "-R/data/a_b.csv/-/replace-a~~b-x_y");
    assert_eq!(texts(&view, "section"), vec!["1", "1.2", "1.2", "1.2"]);
    let storage = view_of(&env, &format!("{DOC}/links-1.1")).await?;
    assert_eq!(storage.len(), 0);
    Ok(())
}

#[tokio::test]
async fn table_under_heading() -> Res {
    let env = env().await?;
    let view = view_of(&env, &format!("{DOC}/table-_.history")).await?;
    assert_eq!(view.len(), 2);
    assert_eq!(texts(&view, "change"), vec!["First", "Second"]);
    Ok(())
}

#[tokio::test]
async fn table_missing_is_an_error() -> Res {
    let env = env().await?;
    let message = format!("{:?}", eval(&env, &format!("{DOC}/table-_.storage")).await.err().ok_or("expected an error")?);
    assert!(message.contains("has 0 table(s)"), "{message}");
    let second = eval(&env, &format!("{DOC}/table-_.history-2")).await;
    assert!(second.is_err());
    Ok(())
}
