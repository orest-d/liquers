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

// ---------------------------------------------------------------------------------------------
// query-pipeline-editor (Phase 3, core tests)
// ---------------------------------------------------------------------------------------------

fn q(text: &str) -> Result<QueryEditor, Error> {
    QueryEditor::from_query(&parse_query(text)?, registry())
}

/// query-pipeline-editor AC-18: the last argument is removed, a middle one gets its default.
#[test]
fn reset_removes_last_and_writes_default_in_middle() -> Result<(), Error> {
    let mut editor = q("-R/a.csv/-/ns-pl/sort-x-f")?;
    editor.edit_action(1)?.reset("ascending")?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/sort-x");
    // A middle argument with a default: its default is written in its place.
    let mut editor = q("new_action-5-b-c")?;
    editor.edit_action(0)?.reset("b").expect_err("b has no default and c is given");
    assert_eq!(editor.query().encode(), "new_action-5-b-c");
    let mut editor = q("-R/a.csv/-/head-3")?;
    editor.edit_action(1)?.reset("n")?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/head");
    // Resetting an argument that is not given changes nothing.
    editor.edit_action(1)?.reset("n")?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/head");
    Ok(())
}

/// query-pipeline-editor AC-2: a variadic argument is an ordered list of items.
#[test]
fn variadic_items_insert_remove_move() -> Result<(), Error> {
    let mut editor = q("-R/a.csv/-/ns-pl/is_in-region-EU-US")?;
    editor.edit_action(1)?.insert_item("values", 2, Source::Value(json!("APAC")))?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/is_in-region-EU-US-APAC");
    editor.edit_action(1)?.move_item("values", 2, 0)?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/is_in-region-APAC-EU-US");
    editor.edit_action(1)?.remove_item("values", 1)?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/is_in-region-APAC-US");
    assert!(editor.edit_action(1)?.remove_item("values", 5).is_err());
    assert!(editor.edit_action(1)?.insert_item("column", 0, Source::Value(json!("x"))).is_err());
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/is_in-region-APAC-US");
    Ok(())
}

/// query-pipeline-editor AC-19: an argument switches between a value and a link.
#[test]
fn set_source_switches_between_value_and_link() -> Result<(), Error> {
    let mut editor = q("-R/a.csv/-/head-3")?;
    editor.edit_action(1)?.set_source("n", Source::Link("-R/config/limit.txt".into()))?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/head-~X~-R/config/limit.txt~E");
    editor.edit_action(1)?.set_source("n", Source::Value(json!(10)))?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/head-10");
    assert!(editor.edit_action(1)?.set_source("n", Source::Value(json!("abc"))).is_err());
    Ok(())
}

/// query-pipeline-editor AC-23 and G6: a query with excess parameters opens and keeps them.
#[test]
fn excess_parameters_are_kept_and_listed() -> Result<(), Error> {
    let mut editor = q("-R/a.csv/-/head-3-4-5")?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/head-3-4-5");
    let excess = editor.edit_action(1)?.excess()?;
    assert_eq!(excess, vec![Source::Value(json!("4")), Source::Value(json!("5"))]);
    editor.edit_action(1)?.set("n", 7)?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/head-7-4-5");
    // The strict conversion still refuses them.
    assert!(Query::try_from(editor.editor()).is_err());
    Ok(())
}

fn namespaced_registry() -> Arc<CommandMetadataRegistry> {
    let mut cmr = CommandMetadataRegistry::new();
    let mut add = |ns: &str, name: &str, args: Vec<ArgumentInfo>| {
        let mut cm = CommandMetadata::new(name);
        cm.with_namespace(ns);
        for a in args {
            cm.with_argument(a);
        }
        cmr.add_command(&cm);
    };
    add("pl", "from_csv", vec![]);
    add("pl", "head", vec![ArgumentInfo::integer_argument("n", false).with_default(5)]);
    add("rec", "head", vec![ArgumentInfo::integer_argument("n", false).with_default(5)]);
    add("rec", "first", vec![]);
    add("img", "head", vec![ArgumentInfo::boolean_argument("flip")]);
    Arc::new(cmr)
}

/// query-pipeline-editor AC-25: an `ns` edit that keeps, changes or breaks the meaning.
#[test]
fn namespace_impact_same_changed_invalid() -> Result<(), Error> {
    let editor =
        QueryEditor::from_query(&parse_query("-R/a.csv/-/ns-pl/from_csv/head-5")?, namespaced_registry())?;
    assert_eq!(editor.namespace_run(1)?, 1..3);
    let pl_rec = vec!["pl".to_owned(), "rec".to_owned()];
    assert_eq!(editor.namespace_impact(1, &pl_rec)?, NamespaceImpact::Same);
    let rec_pl = vec!["rec".to_owned(), "pl".to_owned()];
    match editor.namespace_impact(1, &rec_pl)? {
        NamespaceImpact::Changed(changes) => {
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].at, 2);
            assert_eq!(changes[0].before.namespace, "pl");
            assert_eq!(changes[0].after.namespace, "rec");
        }
        other => panic!("expected Changed, got {other:?}"),
    }
    assert!(matches!(
        editor.namespace_impact(1, &["rec".to_owned()])?,
        NamespaceImpact::Invalid(_)
    ));
    // `img/head` takes a boolean, not a number: the arguments no longer fit.
    assert!(matches!(
        editor.namespace_impact(1, &["img".to_owned(), "pl".to_owned()])?,
        NamespaceImpact::Invalid(_)
    ));
    Ok(())
}

/// query-pipeline-editor AC-25, AC-16: a changed meaning is applied only when accepted.
#[test]
fn set_namespaces_needs_acceptance_for_changed() -> Result<(), Error> {
    let mut editor =
        QueryEditor::from_query(&parse_query("-R/a.csv/-/ns-pl/from_csv/head-5")?, namespaced_registry())?;
    let rec_pl = vec!["rec".to_owned(), "pl".to_owned()];
    assert!(matches!(
        editor.set_namespaces(1, rec_pl.clone(), false)?,
        NamespaceImpact::Changed(_)
    ));
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/from_csv/head-5");
    editor.set_namespaces(1, rec_pl, true)?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-rec-pl/from_csv/head-5");
    assert!(editor.set_namespaces(1, vec!["rec".to_owned()], true).is_err());
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-rec-pl/from_csv/head-5");
    Ok(())
}

/// query-pipeline-editor AC-16: the namespaces offered for an `ns` list.
#[test]
fn available_and_unused_namespaces() -> Result<(), Error> {
    let editor =
        QueryEditor::from_query(&parse_query("-R/a.csv/-/ns-pl/from_csv")?, namespaced_registry())?;
    assert_eq!(editor.available_namespaces(1)?, vec!["img", "pl", "rec"]);
    assert_eq!(editor.unused_namespaces(1)?, vec!["img", "rec"]);
    assert!(editor.available_namespaces(0).is_err(), "element 0 is the resource");
    Ok(())
}

/// query-pipeline-editor AC-20, AC-21: a preset's action request becomes an action element.
#[test]
fn action_from_preset_request() -> Result<(), Error> {
    let preset = crate::command_metadata::CommandPreset::new("sort-region-f", "By region", "")?;
    let element = ActionElement::from_action_request(&preset.action, Some(Namespaces::from("pl")));
    let mut editor = q("-R/a.csv/-/ns-pl/is_in-region-EU")?;
    editor.replace(1, Located::new(ElementKind::Action(element)))?;
    assert_eq!(editor.query().encode(), "-R/a.csv/-/ns-pl/sort-region-f");
    assert_eq!(
        Source::from_parameter_value(&crate::command_metadata::CommandParameterValue::None),
        None
    );
    Ok(())
}
