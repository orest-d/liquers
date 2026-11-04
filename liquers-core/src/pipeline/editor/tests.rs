//! Tests for the pipeline editors.

use std::sync::Arc;

use serde_json::json;

use super::super::*;
use crate::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
use crate::error::Error;
use crate::parse::parse_query;
use crate::query::Query;

fn registry() -> Arc<CommandMetadataRegistry> {
    let mut cmr = CommandMetadataRegistry::new();
    let mut add = |ns: &str, name: &str, args: Vec<ArgumentInfo>| {
        let mut cm = CommandMetadata::new(name);
        cm.with_namespace(ns);
        for a in args {
            cm.with_argument(a);
        }
        cmr.add_command(&cm);
    };
    add("", "text", vec![]);
    add(
        "pl",
        "is_in",
        vec![
            ArgumentInfo::string_argument("column"),
            ArgumentInfo::string_argument("values").set_multiple(),
        ],
    );
    add(
        "pl",
        "sort",
        vec![
            ArgumentInfo::string_argument("column"),
            ArgumentInfo::boolean_argument("ascending").with_default(true),
        ],
    );
    add("", "head", vec![ArgumentInfo::integer_argument("n", false).with_default(10)]);
    let mut env = ArgumentInfo::any_argument("env");
    env.injected = true;
    add(
        "",
        "new_action",
        vec![
            ArgumentInfo::integer_argument("a", false),
            ArgumentInfo::string_argument("b"),
            ArgumentInfo::any_argument("c"),
            env,
        ],
    );
    Arc::new(cmr)
}

/// Phase 3 Example 3, and AC-26: building, refusing, and the round trip.
#[test]
fn query_editor_example() -> Result<(), Error> {
    let cmr = registry();
    let mut q = QueryEditor::new(cmr.clone());
    q.resource("data/sales.csv", Read::Asset)?;
    q.action("is_in")?.ns("pl")?.set("column", "region")?.push("EU")?.push("US")?;
    assert_eq!(q.query().encode(), "-R/data/sales.csv/-/ns-pl/is_in-region-EU-US");
    assert!(q.action("sort")?.ns("pl")?.placeholder("column", "by").is_err());
    let p = PipelineEditor::from_query(&q.query(), cmr)?;
    assert_eq!(Query::try_from(&p)?.encode(), q.query().encode());
    Ok(())
}

/// The chain the maintainer asked for, in a command definition (placeholders allowed).
#[test]
fn action_editor_chain() -> Result<(), Error> {
    let cmr = registry();
    let args = [ArgumentInfo::string_argument("bplaceholder")];
    let mut editor = PipelineEditor::for_definition(cmr, &args);
    editor
        .action("new_action")?
        .set("a", 1)?
        .placeholder("b", "bplaceholder")?
        .link("c", "-R/text.txt")?;
    let ElementKind::Action(a) = &editor.pipeline().elements[0].item else {
        panic!("not an action");
    };
    assert_eq!(a.arguments["a"].item, Source::Value(json!(1)));
    assert_eq!(a.arguments["b"].item, Source::Placeholder("bplaceholder".into()));
    assert_eq!(a.arguments["c"].item, Source::Link("-R/text.txt".into()));
    Ok(())
}

/// `.ns(…)` can follow `.action(…)` for a command outside the default namespaces.
#[test]
fn action_editor_resolves_after_ns() -> Result<(), Error> {
    let mut editor = PipelineEditor::new(registry());
    let mut action = editor.action("sort")?;
    let err = action.set("column", "x").expect_err("not resolvable without ns");
    assert!(err.message.contains("not registered"), "{}", err.message);
    action.ns("pl")?.set("column", "x")?;
    assert_eq!(editor.command_key(0)?.namespace, "pl");
    // An ns that does not resolve the command is refused, and the action keeps its ns.
    let err = editor.edit_action(0)?.ns("img").expect_err("not in img");
    assert!(err.message.contains("not registered"), "{}", err.message);
    assert_eq!(editor.command_key(0)?.namespace, "pl");
    Ok(())
}

/// AC-31 — each edit is checked; a refused edit leaves the pipeline unchanged.
#[test]
fn editor_validates_each_edit() -> Result<(), Error> {
    let mut editor = PipelineEditor::new(registry());
    editor.action("head")?.set("n", 3)?;
    let before = editor.pipeline().clone();
    for result in [
        editor.edit_action(0)?.set("n", "three").map(|_| ()),
        editor.edit_action(0)?.set("nope", 1).map(|_| ()),
        editor.edit_action(0)?.placeholder("n", "x").map(|_| ()),
        editor.edit_action(0)?.state("n").map(|_| ()),
        editor.edit_action(0)?.link("n", "a b").map(|_| ()),
        editor.action("two words").map(|_| ()),
        editor.action("ns").map(|_| ()),
        editor.set_realm("Upper").map(|_| ()),
        editor.set_filename(Some("noext")).map(|_| ()),
    ] {
        assert!(result.is_err());
        assert_eq!(editor.pipeline(), &before);
    }
    // Injected and doubly given arguments.
    let mut editor = PipelineEditor::new(registry());
    let err = editor.action("new_action")?.set("env", 1).expect_err("injected");
    assert!(err.message.contains("injected"), "{}", err.message);
    let mut editor = PipelineEditor::new(registry());
    editor.action("new_action")?.push(1)?.set("a", 2)?;
    let ElementKind::Action(a) = &editor.pipeline().elements[0].item else {
        panic!("not an action");
    };
    // Setting by name an argument given by position replaces it there.
    assert_eq!(a.parameters[0].item, Source::Value(json!(2)));
    assert!(a.arguments.is_empty());
    Ok(())
}

/// AC-31 — a key segment with a space is refused, and the editor is unchanged.
#[test]
fn editor_refuses_invalid_key_segment() {
    let mut editor = PipelineEditor::new(registry());
    let err = editor.resource("data/my file.csv", Read::Asset).expect_err("space");
    assert!(err.message.contains("'my file.csv'"), "{}", err.message);
    assert!(editor.pipeline().elements.is_empty());
}

/// AC-41 — an error on an element from a query carries its position and the query.
#[test]
fn editor_errors_carry_position_and_query() -> Result<(), Error> {
    let q = parse_query("text/head-5")?;
    let mut editor = PipelineEditor::from_query(&q, registry())?;
    let err = editor.edit_action(1)?.set("n", "five").expect_err("type");
    assert_eq!(err.position.column, 6, "{err:?}");
    assert_eq!(err.query.as_deref(), Some("text/head-5"));
    Ok(())
}

/// AC-41 — after an edit the query editor re-parses, so positions point into the current query.
#[test]
fn edited_element_position_from_reparse() -> Result<(), Error> {
    let q = parse_query("head-5")?;
    let mut editor = QueryEditor::from_query(&q, registry())?;
    editor.insert(0, Located::new(ElementKind::Action(ActionElement::new("text"))))?;
    assert_eq!(editor.query().encode(), "text/head-5");
    let pos = editor.editor().pipeline().elements[1].pos.clone().expect("position");
    assert_eq!(pos.column, 6);
    let err = editor.edit_action(1)?.set("n", "five").expect_err("type");
    assert_eq!(err.position.column, 6);
    assert_eq!(err.query.as_deref(), Some("text/head-5"));
    Ok(())
}

/// AC-32 — the query editor refuses what a query cannot say, and stays unchanged.
#[test]
fn query_editor_refuses_non_representable_edit() -> Result<(), Error> {
    let mut editor = QueryEditor::new(registry());
    editor.action("text")?;
    let err = editor
        .push(Located::new(ElementKind::Value(Operand::Literal(json!({"a": 1})))))
        .expect_err("a value as state");
    assert!(err.message.contains("(P1)"), "{}", err.message);
    let err = editor.action("head")?.set("n", json!([1, 2])).expect_err("a list");
    assert!(err.message.contains("is not an integer") || err.message.contains("(S4)"), "{}", err.message);
    assert_eq!(editor.query().encode(), "text/head");
    Ok(())
}

/// AC-32 — `query()` and `Query::from(&editor)` always succeed.
#[test]
fn query_editor_query_is_infallible() -> Result<(), Error> {
    let mut editor = QueryEditor::new(registry());
    assert_eq!(Query::from(&editor).encode(), "");
    editor.action("sort")?; // not resolvable yet: written as it stands
    assert_eq!(editor.query().encode(), "sort");
    editor.edit_action(0)?.ns("pl")?.set("column", "x")?;
    assert_eq!(Query::from(&editor).encode(), "ns-pl/sort-x");
    Ok(())
}

/// AC-37 — the editors answer registry questions.
#[test]
fn editor_exposes_metadata_and_argument_info() -> Result<(), Error> {
    let mut editor = PipelineEditor::new(registry());
    editor.action("new_action")?.push(1)?;
    assert_eq!(editor.command_metadata(0)?.name, "new_action");
    assert_eq!(editor.argument_info(0, 1)?.name, "b");
    assert_eq!(editor.argument_info(0, "c")?.name, "c");
    let unset: Vec<_> = editor.unset_arguments(0)?.iter().map(|a| a.name.clone()).collect();
    assert_eq!(unset, vec!["b", "c"]);
    assert_eq!(editor.resolved_namespaces(0)?, vec!["".to_owned(), "root".to_owned()]);
    let mut action = editor.edit_action(0)?;
    assert_eq!(action.metadata()?.arguments.len(), 4);
    assert_eq!(action.argument_info("env")?.injected, true);
    action.set("b", "x")?;
    assert!(editor.argument_info(0, "zzz").is_err());
    Ok(())
}

/// AC-37 — an editor keeps the registry snapshot it was opened with.
#[test]
fn editor_keeps_registry_snapshot() -> Result<(), Error> {
    let cmr = registry();
    let mut editor = PipelineEditor::new(cmr.clone());
    let mut changed = (*cmr).clone();
    changed.add_command(&CommandMetadata::new("later"));
    // The editor still uses its own snapshot.
    let err = editor.action("later")?.set(0, 1).expect_err("not in the snapshot");
    assert!(err.message.contains("not registered"), "{}", err.message);
    assert!(changed.find_command("", "", "later").is_some());
    Ok(())
}

/// AC-36 — an empty parameter is accepted and encodes as the empty token.
#[test]
fn editor_accepts_empty_parameter() -> Result<(), Error> {
    let mut editor = QueryEditor::new(registry());
    editor.action("new_action")?.push("")?.push("")?;
    assert_eq!(editor.query().encode(), "new_action--");
    let q = parse_query(&editor.query().encode())?;
    assert_eq!(q.action().map(|a| a.parameters.len()), Some(2));
    Ok(())
}
