//! Tests of the backend-free editor core: every [`EditorCommand`], the tree it builds and the
//! write protocol. The scenarios are those of `specs/design/query-pipeline-editor/`.

use std::sync::Arc;

use liquers_core::accessor::Fingerprint;
use liquers_core::command_metadata::{
    ArgumentGUIInfo, ArgumentInfo, CommandKey, CommandMetadata, CommandMetadataRegistry,
    CommandParameterValue, CommandPreset, EnumArgument, ParameterPreset,
};
use liquers_core::metadata::Version;
use liquers_core::parse::parse_query;
use liquers_core::pipeline::{ElementKind, Located, Operand, Pipeline};
use liquers_core::value::ValueInterface;

use super::*;
use crate::ui::element::UIElement;
use crate::value::Value;

const PROBLEM: &str = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv";

fn registry() -> CommandMetadataRegistry {
    let mut cmr = CommandMetadataRegistry::new();
    let mut from_csv = CommandMetadata::new("from_csv");
    from_csv.with_namespace("pl").with_doc("Read a CSV file into a data frame");
    cmr.add_command(&from_csv);

    let mut is_in = CommandMetadata::new("is_in");
    is_in.with_namespace("pl").with_doc("Keep the rows whose column is one of the values");
    is_in
        .with_argument(
            ArgumentInfo::string_argument("column")
                .with_description("The column to test")
                .with_preset(ParameterPreset::new(
                    "Country",
                    CommandParameterValue::Value(serde_json::json!("country")),
                    "The country column",
                )),
        )
        .with_argument(ArgumentInfo::string_argument("values").set_multiple());
    cmr.add_command(&is_in);

    let mut head = CommandMetadata::new("head");
    head.with_namespace("pl").with_doc("Return the first N rows");
    let mut n = ArgumentInfo::integer_argument("n", false)
        .with_default(10)
        .with_description("Number of rows");
    n.gui_info = ArgumentGUIInfo::IntegerSlider { min: 0, max: 100, step: 1 };
    head.with_argument(n);
    head.presets.push(CommandPreset::new("head-20", "Twenty rows", "").expect("preset"));
    head.next.push(CommandPreset::new("sort", "Sort", "").expect("next"));
    cmr.add_command(&head);

    let mut rec_head = CommandMetadata::new("head");
    rec_head
        .with_namespace("rec")
        .with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
    cmr.add_command(&rec_head);

    let mut sort = CommandMetadata::new("sort");
    sort.with_namespace("pl")
        .with_argument(ArgumentInfo::boolean_argument("ascending").with_default(true))
        .with_argument(ArgumentInfo::float_argument("weight", false).with_default(1.0));
    cmr.add_command(&sort);

    let mut to_csv = CommandMetadata::new("to_csv");
    to_csv.with_namespace("pl").with_filename("data.csv");
    let mut sep = ArgumentInfo::string_argument("sep").with_default(",");
    sep.argument_type = liquers_core::command_metadata::ArgumentType::Enum(
        EnumArgument::new("sep").with_alternative("comma").with_alternative("tab"),
    );
    to_csv.with_argument(sep);
    cmr.add_command(&to_csv);
    cmr
}

fn editor(kind: EditorKind, query: &str) -> EditorState {
    let mut state = EditorState::new(kind, Arc::new(registry()));
    let q = parse_query(query).expect("query");
    state.on_snapshot(&Value::from_query(&q), Some(fp(1))).expect("snapshot");
    state
}

fn fp(n: u128) -> Fingerprint {
    Fingerprint { anchor: 0, content: Version::new(n) }
}

fn action<'a>(tree: &'a EditorTree, at: usize) -> &'a EditorNode {
    find_node(&tree.nodes, &NodePath::element(at)).expect("node")
}

fn field<'a>(tree: &'a EditorTree, at: usize, name: &str) -> &'a EditorField {
    action(tree, at).fields().iter().find(|f| f.name == name).expect("field")
}

fn set(path: usize, arg: &str, text: &str) -> EditorCommand {
    EditorCommand::SetField { path: NodePath::element(path), arg: arg.into(), text: text.into() }
}

fn is_write(effect: &EditorEffect) -> bool {
    matches!(effect, EditorEffect::Write { .. })
}

/// query-pipeline-editor AC-1
#[test]
fn tree_of_the_problem_example() {
    let state = editor(EditorKind::Query, PROBLEM);
    let tree = state.tree();
    assert_eq!(tree.text, PROBLEM);
    assert!(tree.text_editable);
    assert_eq!(tree.nodes.len(), 5, "{:?}", tree.nodes);
    assert!(matches!(&tree.nodes[0], EditorNode::Resource { key, header: Some(h), .. }
        if key == "data/sales.csv" && h.is_empty()));
    assert!(matches!(&tree.nodes[1], EditorNode::Namespaces { list, .. } if list == &vec!["pl".to_string()]));
    assert!(matches!(&tree.nodes[2], EditorNode::Action { command, .. } if command.name == "from_csv"));
    assert_eq!(
        field(tree, 2, "values").value,
        FieldValue::Items(vec![FieldValue::Text("EU".into()), FieldValue::Text("US".into())])
    );
    assert_eq!(field(tree, 3, "n").value, FieldValue::Text("10".into()));
    assert_eq!(tree.filename, "top.csv");
}

/// query-pipeline-editor AC-2
#[test]
fn effective_widgets_from_type_and_gui_info() {
    let state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/head/sort/to_csv/is_in-x");
    let tree = state.tree();
    assert_eq!(field(tree, 1, "n").widget, ArgumentGUIInfo::IntegerSlider { min: 0, max: 100, step: 1 });
    assert_eq!(field(tree, 2, "ascending").widget, ArgumentGUIInfo::Checkbox);
    assert_eq!(field(tree, 2, "weight").widget, ArgumentGUIInfo::FloatField);
    assert_eq!(field(tree, 3, "sep").widget, ArgumentGUIInfo::EnumSelector);
    assert_eq!(field(tree, 3, "sep").alternatives.len(), 2);
    assert_eq!(field(tree, 4, "column").widget, ArgumentGUIInfo::TextField(40));
    assert!(field(tree, 4, "values").multiple);
}

/// query-pipeline-editor AC-3
#[test]
fn fields_carry_hover_documentation() {
    let state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/head");
    let tree = state.tree();
    let hover = field(tree, 1, "n").hover();
    assert!(hover.contains("Number of rows"), "{hover}");
    assert!(hover.contains("Default: 10"), "{hover}");
    assert!(matches!(action(tree, 1), EditorNode::Action { doc, .. } if doc == "Return the first N rows"));
    assert_eq!(field(tree, 1, "n").value, FieldValue::Unset);
}

/// query-pipeline-editor AC-4
#[test]
fn text_edit_rebuilds_tree_or_marks_it_stale() {
    let mut state = editor(EditorKind::Query, PROBLEM);
    let effect = state.apply(EditorCommand::SetText("-R/data/b.csv/-/ns-pl/head-3".into())).expect("text");
    assert!(is_write(&effect));
    assert_eq!(state.tree().nodes.len(), 3);
    assert!(!state.tree().stale);

    let effect = state.apply(EditorCommand::SetText("-R/data/b.csv/-/head-~X~".into())).expect("typed");
    assert!(matches!(effect, EditorEffect::None));
    assert!(state.tree().stale);
    assert!(state.tree().text_note.is_some());
    assert_eq!(state.tree().text, "-R/data/b.csv/-/head-~X~");
    assert_eq!(state.tree().nodes.len(), 3, "the last good tree stays");
}

/// query-pipeline-editor AC-5
#[test]
fn refused_edit_changes_nothing() {
    let mut state = editor(EditorKind::Query, PROBLEM);
    assert!(state.apply(set(3, "n", "abc")).is_err());
    assert_eq!(state.tree().text, PROBLEM);
    assert!(!state.is_pending());
    let (path, message) = state.tree().message.clone().expect("message");
    assert_eq!(path, NodePath::element(3));
    assert!(!message.is_empty());
    assert!(state.apply(set(9, "n", "1")).is_err());
    assert_eq!(state.tree().text, PROBLEM);
}

/// query-pipeline-editor AC-6
#[test]
fn insert_remove_append_and_move() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/from_csv/head-5");
    state
        .apply(EditorCommand::Insert {
            path: NodePath::element(2),
            element: NewElement::Action(CommandKey::new("", "pl", "sort")),
        })
        .expect("insert");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/from_csv/sort/head-5");
    state.apply(EditorCommand::Move { path: NodePath::element(2), to: 3 }).expect("move");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/from_csv/head-5/sort");
    state.apply(EditorCommand::Remove { path: NodePath::element(3) }).expect("remove");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/from_csv/head-5");
    state.apply(EditorCommand::Append(NewElement::Volatile)).expect("append");
    assert!(state.tree().text.ends_with("/v"), "{}", state.tree().text);
    assert!(state.apply(EditorCommand::Move { path: NodePath::element(1), to: 9 }).is_err());
}

/// query-pipeline-editor AC-7
#[test]
fn resource_header_and_key() {
    let mut state = editor(EditorKind::Resource, PROBLEM);
    assert_eq!(state.tree().nodes.len(), 1, "a resource editor shows only the resource");
    state
        .apply(EditorCommand::SetHeader { path: NodePath::element(0), header: "dir".into() })
        .expect("header");
    assert!(state.tree().text.starts_with("-R-directory/data/sales.csv"), "{}", state.tree().text);
    state
        .apply(EditorCommand::SetKey { path: NodePath::element(0), key: "/data/other.csv".into() })
        .expect("key");
    assert!(state.tree().text.starts_with("-R-directory/data/other.csv/-/"), "{}", state.tree().text);
    assert!(state
        .apply(EditorCommand::SetHeader { path: NodePath::element(0), header: "nonsense".into() })
        .is_err());
    assert!(state.apply(EditorCommand::SetText("-R/x".into())).is_err(), "the text is read-only");
}

/// query-pipeline-editor AC-8
#[test]
fn action_kind_edits_one_element_pipeline() {
    let query = parse_query("ns-pl/head-5").expect("query");
    let pipeline = Pipeline::try_from(&query).expect("pipeline");
    let mut state = EditorState::new(EditorKind::Action, Arc::new(registry()));
    state.on_snapshot(&Value::from_pipeline(pipeline), None).expect("snapshot");
    assert!(matches!(action(state.tree(), 0), EditorNode::Action { command, .. } if command.name == "head"));
    let effect = state.apply(set(0, "n", "7")).expect("edit");
    let EditorEffect::Write { value, .. } = effect else { panic!("expected a write") };
    let written = value.try_into_pipeline().expect("pipeline");
    assert_eq!(written.elements.len(), 1);
    assert!(state.tree().text.contains("head-7"), "{}", state.tree().text);
}

/// query-pipeline-editor AC-9
#[test]
fn second_editor_conflict_reloads() {
    let mut a = editor(EditorKind::Query, PROBLEM);
    let mut b = editor(EditorKind::Query, PROBLEM);
    let EditorEffect::Write { value: a_value, expected } = a.apply(set(3, "n", "20")).expect("a") else {
        panic!("expected a write")
    };
    assert_eq!(expected, Some(fp(1)));
    a.on_written(fp(2));
    let EditorEffect::Write { expected, .. } = b.apply(set(3, "n", "30")).expect("b") else {
        panic!("expected a write")
    };
    assert_eq!(expected, Some(fp(1)), "b writes from the old fingerprint and is refused");
    b.on_conflict(&a_value, Some(fp(2)));
    assert!(b.tree().text.contains("head-20"), "{}", b.tree().text);
    let (_, message) = b.tree().message.clone().expect("message");
    assert!(message.starts_with("Conflict") && message.contains("head-30"), "{message}");
    assert!(!b.is_pending());
    // A snapshot of the value just written is not a change.
    a.on_snapshot(&a_value, Some(fp(2))).expect("own write");
    assert!(a.tree().text.contains("head-20"));
}

/// query-pipeline-editor AC-10
#[test]
fn pipeline_text_is_read_only() {
    let mut pipeline = Pipeline::try_from(&parse_query("ns-pl/head-5").expect("query")).expect("pipeline");
    pipeline
        .elements
        .insert(0, Located::new(ElementKind::Value(Operand::Literal(serde_json::json!([1, 2])))));
    let mut state = EditorState::new(EditorKind::Query, Arc::new(registry()));
    state.on_snapshot(&Value::from_pipeline(pipeline), None).expect("snapshot");
    let tree = state.tree();
    assert!(!tree.text_editable);
    assert!(tree.text_note.is_some());
    assert!(matches!(&tree.nodes[0], EditorNode::Value { .. }));
    let effect = state.apply(set(1, "n", "8")).expect("tree edit");
    let EditorEffect::Write { value, .. } = effect else { panic!("expected a write") };
    assert!(value.try_into_pipeline().is_ok());
}

/// query-pipeline-editor AC-12
#[test]
fn element_round_trips_and_old_console_state_loads() {
    let element = EditorElement::new("E", EditorKind::Action, EditorBinding::Memory("ns-pl/head".into()))
        .with_continuous(false);
    let boxed: Box<dyn UIElement> = Box::new(element);
    let json = serde_json::to_string(&boxed).expect("serialize");
    let back: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
    let back = back.as_any().and_then(|a| a.downcast_ref::<EditorElement>()).expect("editor");
    assert_eq!(back.kind(), EditorKind::Action);
    assert!(!back.continuous());
    assert_eq!(back.binding(), &EditorBinding::Memory("ns-pl/head".into()));

    // An element saved without the optional fields loads with "Update immediately" on.
    let old = r#"{"EditorElement":{"handle":null,"title":"E","kind":"Query","binding":{"Accessor":"-R/x"}}}"#;
    let old: Box<dyn UIElement> = serde_json::from_str(old).expect("old");
    let old = old.as_any().and_then(|a| a.downcast_ref::<EditorElement>()).expect("editor");
    assert!(old.continuous());

    // A query console saved before the Parameters panel loads with it closed.
    let console = crate::ui::widgets::QueryConsoleElement::new("C".to_string(), "-R/x".to_string());
    let mut value = serde_json::to_value(&console).expect("console");
    if let Some(map) = value.as_object_mut() {
        map.remove("show_parameters");
    }
    let console: crate::ui::widgets::QueryConsoleElement = serde_json::from_value(value).expect("old console");
    assert!(!console.show_parameters());
}

/// query-pipeline-editor AC-13
#[test]
fn numeric_controller_scales_into_the_argument() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/head-5");
    assert_eq!(field(state.tree(), 1, "n").numeric_index, Some(0));
    state.apply(EditorCommand::SetNumeric { index: 0, value: 64.0, max: 127.0 }).expect("controller");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/head-50");
    assert!(state.apply(EditorCommand::SetNumeric { index: 1, value: 1.0, max: 1.0 }).is_err());
    let mut sort = ArgumentInfo::float_argument("x", false);
    sort.gui_info = ArgumentGUIInfo::FloatSlider { min: 0.0, max: 2.0, step: 0.1 };
    assert_eq!(scale_numeric(&sort, 5.0, 10.0).expect("float"), serde_json::json!(1.0));
}

/// query-pipeline-editor AC-14
#[test]
fn browse_lists_directory_and_accepts_any_key() {
    let mut state = editor(EditorKind::Query, PROBLEM);
    let effect = state
        .apply(EditorCommand::Browse { path: NodePath::element(0), folder: "/data/".into() })
        .expect("browse");
    assert!(matches!(effect, EditorEffect::ListDirectory(ref f) if f == "data"));
    state.on_directory(DirectoryListing {
        folder: "data".into(),
        entries: vec![],
        error: Some("store unavailable".into()),
    });
    let browser = state.tree().browser.clone().expect("browser");
    assert_eq!(browser.listing.and_then(|l| l.error), Some("store unavailable".to_string()));
    state
        .apply(EditorCommand::SetKey { path: NodePath::element(0), key: "data/new.csv".into() })
        .expect("any key");
    assert!(state.tree().browser.is_none());
    assert!(state.tree().text.starts_with("-R/data/new.csv/-/"));
}

/// query-pipeline-editor AC-15
#[test]
fn key_kind_and_key_links() {
    let mut state = EditorState::new(EditorKind::Key, Arc::new(registry()));
    state.on_snapshot(&Value::from("data/a.csv".to_string()), None).expect("snapshot");
    assert!(matches!(&state.tree().nodes[..], [EditorNode::Resource { header: None, key, .. }] if key == "data/a.csv"));
    let effect = state
        .apply(EditorCommand::SetKey { path: NodePath::default(), key: "data/b.csv".into() })
        .expect("key");
    let EditorEffect::Write { value, .. } = effect else { panic!("expected a write") };
    assert_eq!(value.try_into_string().expect("text"), "data/b.csv");
    assert!(state
        .apply(EditorCommand::SetHeader { path: NodePath::default(), header: "dir".into() })
        .is_err());

    let state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/is_in-~X~-R-key/data/x.csv~E");
    assert!(matches!(&field(state.tree(), 1, "column").value, FieldValue::Link { is_key: true, .. }));
}

/// query-pipeline-editor AC-16
#[test]
fn directives_are_nodes() {
    let state = editor(EditorKind::Query, "-R/a.csv/-/q/ns-pl/head-5/v");
    let tree = state.tree();
    let directives: Vec<&str> = tree
        .nodes
        .iter()
        .filter_map(|n| match n {
            EditorNode::Directive { name, .. } => Some(name.as_str()),
            EditorNode::Resource { .. }
            | EditorNode::Namespaces { .. }
            | EditorNode::Action { .. }
            | EditorNode::Value { .. }
            | EditorNode::Nested { .. }
            | EditorNode::Opaque { .. } => None,
        })
        .collect();
    assert_eq!(directives, vec!["q", "v"]);
    // The `ns` node offers the namespaces not yet listed.
    assert!(tree.nodes.iter().any(|n| matches!(n, EditorNode::Namespaces { list, unused, .. }
        if list == &vec!["pl".to_string()] && unused == &vec!["rec".to_string()])), "{:?}", tree.nodes);
}

/// query-pipeline-editor AC-17
#[test]
fn command_catalog_filters_and_inserts_ns() {
    let cmr = registry();
    let catalog = command_catalog(&cmr, "", "sor");
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].namespace, "pl");
    assert_eq!(catalog[0].commands[0].key.name, "sort");
    assert_eq!(command_catalog(&cmr, "", "").len(), 2);

    let mut state = editor(EditorKind::Query, "-R/a.csv");
    state
        .apply(EditorCommand::SearchCommands { insert_at: None, filter: "sor".into() })
        .expect("picker");
    assert_eq!(state.tree().picker.as_ref().map(|p| p.catalog.len()), Some(1));
    state.apply(EditorCommand::PickCommand(CommandKey::new("", "pl", "sort"))).expect("pick");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/sort");
    assert!(state.tree().picker.is_none());
}

/// query-pipeline-editor AC-18
#[test]
fn reset_field_to_default() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/head-3");
    state.apply(EditorCommand::Reset { path: NodePath::element(1), arg: "n".into() }).expect("reset");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/head");
    assert_eq!(field(state.tree(), 1, "n").value, FieldValue::Unset);
}

/// query-pipeline-editor AC-19
#[test]
fn link_argument_is_a_subtree() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/head-~X~-R/config/limit.txt~E");
    let FieldValue::Link { nodes, is_key, .. } = &field(state.tree(), 1, "n").value else {
        panic!("expected a link")
    };
    assert!(!is_key);
    let inner = NodePath::element(1).then(PathStep::Argument("n".into())).then(PathStep::Element(0));
    assert_eq!(nodes[0].path(), &inner);
    state
        .apply(EditorCommand::SetKey { path: inner, key: "config/other.txt".into() })
        .expect("edit inside the link");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/head-~X~-R/config/other.txt~E");
    state.apply(EditorCommand::ToValue { path: NodePath::element(1), arg: "n".into() }).expect("to value");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/head-10");
    state.apply(EditorCommand::ToLink { path: NodePath::element(1), arg: "n".into() }).expect("to link");
    assert!(matches!(field(state.tree(), 1, "n").value, FieldValue::Link { .. }));
}

/// query-pipeline-editor AC-20, AC-21
#[test]
fn presets_replace_and_next_inserts() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/head-3");
    assert!(matches!(action(state.tree(), 1), EditorNode::Action { presets, next, .. }
        if presets[0].label == "Twenty rows" && next[0].label == "Sort"));
    state.apply(EditorCommand::ApplyPreset { path: NodePath::element(1), index: 0 }).expect("preset");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/head-20");
    state.apply(EditorCommand::ApplyNext { path: NodePath::element(1), index: 0 }).expect("next");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/head-20/sort");
    assert!(state.apply(EditorCommand::ApplyPreset { path: NodePath::element(1), index: 5 }).is_err());
}

/// query-pipeline-editor AC-22
#[test]
fn filename_button_field_and_default() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/from_csv/to_csv");
    assert!(state.tree().has_filename);
    assert_eq!(state.tree().filename_default.as_deref(), Some("data.csv"));
    state.apply(EditorCommand::DefaultFilename).expect("default");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/from_csv/to_csv/data.csv");
    state.apply(EditorCommand::SetFilename("out.csv".into())).expect("typed");
    assert_eq!(state.tree().filename, "out.csv");
    state.apply(EditorCommand::SetFilename(String::new())).expect("cleared");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/from_csv/to_csv");
    state.apply(EditorCommand::FilenameFrom { path: NodePath::element(2) }).expect("from command");
    assert_eq!(state.tree().filename, "data.csv");
    assert!(state.apply(EditorCommand::FilenameFrom { path: NodePath::element(1) }).is_err());
}

/// query-pipeline-editor AC-23
#[test]
fn unknown_command_and_excess_are_shown_read_only() {
    let state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/unknown-1/head-5-6");
    let tree = state.tree();
    assert!(matches!(action(tree, 1), EditorNode::Opaque { text, .. } if text.starts_with("unknown")));
    assert!(matches!(action(tree, 2), EditorNode::Action { excess, .. } if excess == &vec!["6".to_string()]));
    assert_eq!(tree.text, "-R/a.csv/-/ns-pl/unknown-1/head-5-6");
}

/// query-pipeline-editor AC-24
#[test]
fn argument_preset_menu() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/is_in-region-EU");
    assert_eq!(field(state.tree(), 1, "column").presets[0].label, "Country");
    state
        .apply(EditorCommand::ApplyArgumentPreset { path: NodePath::element(1), arg: "column".into(), index: 0 })
        .expect("preset");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/is_in-country-EU");
    assert!(state
        .apply(EditorCommand::ApplyArgumentPreset { path: NodePath::element(1), arg: "column".into(), index: 3 })
        .is_err());
}

/// query-pipeline-editor AC-25
#[test]
fn namespace_change_awaits_confirmation() {
    let mut state = editor(EditorKind::Query, "-R/a.csv/-/ns-pl/from_csv/head-5");
    let ns = |list: &[&str]| EditorCommand::SetNamespaces {
        path: NodePath::element(1),
        namespaces: list.iter().map(|s| s.to_string()).collect(),
    };
    let effect = state.apply(ns(&["pl", "rec"])).expect("same meaning");
    assert!(is_write(&effect));
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl-rec/from_csv/head-5");

    let effect = state.apply(ns(&["rec", "pl"])).expect("changed meaning");
    assert!(matches!(effect, EditorEffect::None));
    let confirmation = state.tree().confirmation.clone().expect("confirmation");
    assert!(confirmation.changes[0].contains("rec"), "{:?}", confirmation.changes);
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl-rec/from_csv/head-5", "not applied yet");
    state.apply(EditorCommand::ConfirmNamespaces).expect("confirm");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-rec-pl/from_csv/head-5");

    assert!(state.apply(ns(&["rec"])).is_err(), "from_csv is not in rec");
    assert_eq!(state.tree().text, "-R/a.csv/-/ns-rec-pl/from_csv/head-5");
}

/// query-pipeline-editor AC-26
#[test]
fn continuous_and_on_demand_writes() {
    let mut state = EditorState::new(EditorKind::Query, Arc::new(registry())).with_continuous(false);
    state
        .on_snapshot(&Value::from_query(&parse_query(PROBLEM).expect("query")), Some(fp(1)))
        .expect("snapshot");
    let effect = state.apply(set(3, "n", "4")).expect("edit");
    assert!(matches!(effect, EditorEffect::None));
    assert!(state.tree().pending);
    assert!(is_write(&state.apply(EditorCommand::Update).expect("update")));
    state.on_written(fp(2));
    assert!(!state.tree().pending);
    state.apply(set(3, "n", "5")).expect("edit");
    assert!(matches!(state.apply(EditorCommand::Revert).expect("revert"), EditorEffect::Reload));
    state.apply(set(3, "n", "6")).expect("edit");
    assert!(is_write(&state.apply(EditorCommand::SetContinuous(true)).expect("continuous")));
    assert!(state.continuous());
}

/// query-pipeline-editor AC-26
#[test]
fn one_write_in_flight_coalesces() {
    let mut state = editor(EditorKind::Query, PROBLEM);
    assert!(is_write(&state.apply(set(3, "n", "1")).expect("first")));
    assert!(matches!(state.apply(set(3, "n", "2")).expect("second"), EditorEffect::None));
    assert!(matches!(state.apply(set(3, "n", "3")).expect("third"), EditorEffect::None));
    let EditorEffect::Write { value, expected } = state.on_written(fp(2)) else {
        panic!("the coalesced edits are written next")
    };
    assert_eq!(expected, Some(fp(2)));
    assert!(value.try_into_query().expect("query").encode().contains("head-3"));
    assert!(matches!(state.on_written(fp(3)), EditorEffect::None));
}

/// query-pipeline-editor AC-5
#[test]
fn editor_without_registry_shows_text_only() {
    let mut state = EditorState::new(EditorKind::Query, Arc::new(CommandMetadataRegistry::new()));
    assert_eq!(state.tree().text_note.as_deref(), Some("no command registry"));
    state
        .on_snapshot(&Value::from_query(&parse_query("-R/a.csv/-/head-5").expect("query")), None)
        .expect("snapshot");
    assert_eq!(state.tree().text, "-R/a.csv/-/head-5");
    assert!(matches!(action(state.tree(), 1), EditorNode::Opaque { .. }));
    let element = EditorElement::new("E", EditorKind::Query, EditorBinding::Memory("-R/a.csv".into()));
    assert!(element.tree().is_none());
}

// ---------------------------------------------------------------------------------------------
// query-editor-ergonomics (`specs/design/query-editor-ergonomics/phase3-examples.md`)
// ---------------------------------------------------------------------------------------------

/// The problem example `P` of the ergonomics design.
const P: &str = "-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20";
/// `P` with a link argument.
const L: &str = "-R/data/sales.csv/-/ns-pl/gt-amount-~X~-R/config/limit.txt~E/head-20";

fn ergonomics_registry() -> CommandMetadataRegistry {
    let mut cmr = CommandMetadataRegistry::new();
    let mut gt = CommandMetadata::new("gt");
    gt.with_namespace("pl")
        .with_label("Greater than")
        .with_hint("icon", serde_json::json!("ph:funnel-simple"))
        .with_argument(ArgumentInfo::string_argument("column"))
        .with_argument(ArgumentInfo::string_argument("value"));
    cmr.add_command(&gt);

    let mut sort = CommandMetadata::new("sort");
    sort.with_namespace("pl")
        .with_label("Sort by column")
        .with_hint("icon", serde_json::json!(42)) // malformed: no icon
        .with_argument(ArgumentInfo::string_argument("column"))
        .with_argument(ArgumentInfo::boolean_argument("ascending").with_default(true));
    cmr.add_command(&sort);

    let mut head = CommandMetadata::new("head");
    head.with_namespace("pl").with_label("Get first rows");
    head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5).with_description("Number of rows"));
    for (q, label) in [("head-5", "Top 5"), ("head-20", "Top 20"), ("head-100", "Top 100")] {
        head.presets.push(CommandPreset::new(q, label, "").expect("preset"));
    }
    cmr.add_command(&head);

    let mut select = CommandMetadata::new("select_columns");
    select
        .with_namespace("pl")
        .with_label("Select columns")
        .with_argument(ArgumentInfo::string_argument("columns").set_multiple());
    cmr.add_command(&select);
    cmr
}

fn ergonomics_editor(query: &str) -> EditorState {
    let mut state = EditorState::new(EditorKind::Query, Arc::new(ergonomics_registry()));
    state.on_snapshot(&Value::from_query(&parse_query(query).expect("query")), Some(fp(1))).expect("snapshot");
    state
}

fn chip_labels(chips: &[StepChip]) -> Vec<&str> {
    chips.iter().map(|c| c.label.as_str()).collect()
}

// Test 3 (AC-1).
#[test]
fn simple_layout_of_the_problem_example() {
    let state = ergonomics_editor(P);
    let layout = editor_layout(state.tree(), EditorMode::Simple);
    assert_eq!(chip_labels(&layout.before), ["sales.csv", "Greater than", "Sort by column"]);
    assert!(layout.after.is_empty());
    assert_eq!(layout.groups.len(), 1);
    assert_eq!(layout.groups[0].kind, CardGroupKind::QueryPart);
    assert!(matches!(layout.groups[0].cards[0].node, EditorNode::Action { label, .. } if label == "Get first rows"));
    assert!(!layout.outline);

    // A link argument stays its field's text: `gt` is a chip, its link no card.
    let state = ergonomics_editor(L);
    let layout = editor_layout(state.tree(), EditorMode::Simple);
    assert_eq!(chip_labels(&layout.before), ["sales.csv", "Greater than"]);
    assert_eq!(layout.groups[0].cards.len(), 1);
}

// Test 4 (AC-2).
#[test]
fn advanced_layout_groups_key_and_query_parts() {
    let state = ergonomics_editor(L);
    let layout = editor_layout(state.tree(), EditorMode::Advanced);
    assert!(layout.outline);
    assert!(layout.before.is_empty() && layout.after.is_empty());
    let kinds: Vec<_> = layout.groups.iter().map(|g| g.kind).collect();
    assert_eq!(kinds, [CardGroupKind::KeyPart, CardGroupKind::QueryPart]);
    assert_eq!(layout.groups[0].cards.len(), 1);
    let gt = layout.groups[1]
        .cards
        .iter()
        .find(|c| matches!(c.node, EditorNode::Action { command, .. } if command.name == "gt"))
        .expect("gt card");
    assert!(gt.marks.link);
    let value = gt.node.fields().iter().find(|f| f.name == "value").expect("value");
    assert!(matches!(&value.value, FieldValue::Link { .. }), "{:?}", value.value);
}

// Test 5 (AC-1).
#[test]
fn simple_layout_keeps_trailing_directives_as_chips() {
    let state = ergonomics_editor("-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20/q");
    let layout = editor_layout(state.tree(), EditorMode::Simple);
    assert_eq!(chip_labels(&layout.after), ["q"]);
    assert!(matches!(layout.groups[0].cards[0].node, EditorNode::Action { label, .. } if label == "Get first rows"));

    // `ns-pl` directly before the last action is no chip.
    let state = ergonomics_editor("-R/data/sales.csv/-/ns-pl/head-20");
    let layout = editor_layout(state.tree(), EditorMode::Simple);
    assert_eq!(chip_labels(&layout.before), ["sales.csv"]);

    // No action: the resource is the card.
    let state = ergonomics_editor("-R/data/sales.csv");
    let layout = editor_layout(state.tree(), EditorMode::Simple);
    assert_eq!(layout.groups[0].kind, CardGroupKind::KeyPart);
}

// Test 6 (AC-3).
#[test]
fn mode_switch_keeps_text_edits_and_focus() {
    let mut state = ergonomics_editor(P);
    let head = state.tree().nodes[state.tree().last_action().expect("head")].path().clone();
    state
        .apply(EditorCommand::SetField { path: head.clone(), arg: "n".into(), text: "30".into() })
        .expect("edit");
    let mut view = EditorView { focus: Some(FocusTarget::Node(head.clone())), ..EditorView::default() };
    let simple = editor_layout(state.tree(), EditorMode::Simple);
    let advanced = editor_layout(state.tree(), EditorMode::Advanced);
    assert!(simple.groups[0].cards[0].node.path() == &head);
    assert!(advanced.groups.iter().flat_map(|g| &g.cards).any(|c| c.node.path() == &head));
    assert!(state.tree().text.ends_with("head-30"));
    view.collapsed.insert(head.id());
    assert_eq!(view.focus, Some(FocusTarget::Node(head)));

    let mut element = EditorElement::new("Query", EditorKind::Query, EditorBinding::Memory(P.into()));
    assert_eq!(element.mode(), EditorMode::Simple);
    element.set_mode(element.mode().toggled());
    let json = serde_json::to_value(&element).expect("json");
    assert_eq!(json["mode"], "advanced");
    let back: EditorElement = serde_json::from_value(json.clone()).expect("back");
    assert_eq!(back.mode(), EditorMode::Advanced);

    // A saved element without `mode` / `density` loads as Simple, density from the theme.
    let mut old = json;
    old.as_object_mut().expect("object").remove("mode");
    let old: EditorElement = serde_json::from_value(old).expect("old");
    assert_eq!(old.mode(), EditorMode::Simple);
    assert_eq!(old.density(), None);
    let theme = crate::ui::theme::ThemeState::default();
    assert_eq!(old.effective_density(&theme), theme.density);
}

// Test 7 (AC-4, AC-8).
#[test]
fn card_header_marks_and_realm() {
    let mut state = ergonomics_editor(L);
    let tree = state.tree();
    assert!(!tree.show_realm);
    let gt = tree.nodes.iter().find(|n| matches!(n, EditorNode::Action { command, .. } if command.name == "gt")).expect("gt");
    let marks = card_marks(tree, gt);
    assert!(marks.link && marks.modified && !marks.error);
    match gt {
        EditorNode::Action { hint_icon, .. } => assert!(hint_icon.is_some()),
        other => panic!("{other:?}"),
    }
    let head = &tree.nodes[tree.last_action().expect("head")];
    let marks = card_marks(tree, head);
    assert!(marks.modified, "20 differs from the default 5");
    assert!(!marks.link);

    // A refused edit marks its card.
    let at = state.tree().last_action().expect("head");
    let path = state.tree().nodes[at].path().clone();
    assert!(state
        .apply(EditorCommand::SetField { path, arg: "n".into(), text: "abc".into() })
        .is_err());
    let tree = state.tree();
    assert!(card_marks(tree, &tree.nodes[at]).error);

    let state = ergonomics_editor(P);
    let sort = state
        .tree()
        .nodes
        .iter()
        .find(|n| matches!(n, EditorNode::Action { command, .. } if command.name == "sort"))
        .expect("sort");
    assert!(matches!(sort, EditorNode::Action { hint_icon: None, .. }), "a malformed hint gives no icon");

    // A second realm shows the realm on the headers.
    let mut cmr = ergonomics_registry();
    let mut gui = CommandMetadata::new("draw");
    gui.with_realm("gui");
    cmr.add_command(&gui);
    let mut state = EditorState::new(EditorKind::Query, Arc::new(cmr));
    state.on_snapshot(&Value::from_query(&parse_query(P).expect("q")), Some(fp(1))).expect("snapshot");
    assert!(state.tree().show_realm);
}

// Test 10 (AC-7).
#[test]
fn active_preset_matches_the_action() {
    let mut state = ergonomics_editor(P);
    let at = state.tree().last_action().expect("head");
    let path = state.tree().nodes[at].path().clone();
    let active = |state: &EditorState| match &state.tree().nodes[at] {
        EditorNode::Action { presets, .. } => {
            presets.iter().filter(|p| p.active).map(|p| p.label.clone()).collect::<Vec<_>>()
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(active(&state), ["Top 20"]);
    state.apply(EditorCommand::ApplyPreset { path: path.clone(), index: 2 }).expect("preset");
    assert!(state.tree().text.ends_with("head-100"));
    assert_eq!(active(&state), ["Top 100"]);
    state
        .apply(EditorCommand::SetField { path, arg: "n".into(), text: "7".into() })
        .expect("edit");
    assert!(active(&state).is_empty());
}

/// Every `shortcut:` of the menus of the bundled UI specs (examples).
fn bundled_menu_shortcuts() -> Vec<String> {
    let sources = [
        include_str!("../../../examples/ui_query_console_app.rs"),
        include_str!("../../../examples/ui_spec_interactive.rs"),
        include_str!("../../../examples-web/ui_query_console_app/src/lib.rs"),
        include_str!("../widgets/ui_spec_element.rs"),
    ];
    let mut out = Vec::new();
    for source in sources {
        for line in source.lines() {
            let Some(rest) = line.trim().strip_prefix("shortcut:") else {
                continue;
            };
            let rest = rest.trim().trim_end_matches(',');
            let text = rest
                .strip_prefix("Some(\"")
                .and_then(|r| r.split('"').next())
                .unwrap_or(rest)
                .trim();
            if !text.is_empty() && text != "None" && !text.starts_with("Option<") {
                out.push(text.to_string());
            }
        }
    }
    out
}

// Test 12 (AC-9).
#[test]
fn keymap_parses_and_has_no_duplicates() {
    use crate::ui::shortcuts::KeyboardShortcut;
    let table: Vec<_> = keymap_table().collect();
    let map = editor_keymap();
    assert_eq!(map.len(), table.len(), "every entry parses");
    for (i, (a, _, scope_a)) in map.iter().enumerate() {
        for (b, _, scope_b) in &map[i + 1..] {
            assert!(!(a == b && scope_a == scope_b), "{a} twice in {scope_a:?}");
        }
    }
    assert_eq!(shortcut_text(EditorKey::Jump), Some("Ctrl+J"));
    assert!(map.iter().any(|(_, k, _)| *k == EditorKey::ApplyPreset(1)));

    let menus = bundled_menu_shortcuts();
    assert!(menus.iter().any(|m| m == "Ctrl+Q"), "{menus:?}");
    for text in menus {
        let menu: KeyboardShortcut = text.parse().expect("menu shortcut parses");
        for (key, what, _) in map {
            assert!(key != &menu, "{text} is both a menu shortcut and {what:?}");
        }
    }
}

// Test 14 (AC-10).
#[test]
fn spans_of_the_problem_example() {
    let state = ergonomics_editor(P);
    let tree = state.tree();
    let text = |r: &std::ops::Range<usize>| &tree.text[r.clone()];
    let resource = tree.span_of(&NodePath::element(0), &SpanTarget::Element).expect("resource");
    assert_eq!(text(&resource), "-R/data/sales.csv");
    let gt = NodePath::element(1);
    assert_eq!(text(&tree.span_of(&gt, &SpanTarget::Namespaces).expect("ns")), "ns-pl");
    assert_eq!(text(&tree.span_of(&gt, &SpanTarget::Element).expect("gt")), "gt-amount-1000");
    let value = SpanTarget::Argument { arg: "value".into(), item: None };
    assert_eq!(text(&tree.span_of(&gt, &value).expect("value")), "1000");
    let head = tree.nodes[tree.last_action().expect("head")].path().clone();
    let n = SpanTarget::Argument { arg: "n".into(), item: None };
    assert_eq!(text(&tree.span_of(&head, &n).expect("n")), "20");

    // Variadic items.
    let state = ergonomics_editor("-R/data/sales.csv/-/ns-pl/select_columns-region-amount/head-5");
    let tree = state.tree();
    let at = NodePath::element(1);
    let second = SpanTarget::Argument { arg: "columns".into(), item: Some(1) };
    assert_eq!(&tree.text[tree.span_of(&at, &second).expect("item")], "amount");

    // A link: its own tree's spans hang under the argument.
    let state = ergonomics_editor(L);
    let tree = state.tree();
    let at = tree.text.find("limit.txt").expect("link");
    let span = tree.locate(at).expect("inner span");
    assert_eq!(span.path.0[..2], [PathStep::Element(1), PathStep::Argument("value".into())]);

    // Stale text: no spans.
    let mut state = ergonomics_editor(P);
    let _ = state.apply(EditorCommand::SetText("-R/data/sales.csv/-/ns-pl/gt-".into()));
    if state.tree().stale {
        assert!(state.tree().spans.is_empty());
    }
}

// Test 15 (AC-10).
#[test]
fn jump_round_trip_and_simple_offer() {
    let mut state = ergonomics_editor(P);
    let tree = state.tree().clone();
    let at = tree.text.rfind("20").expect("20");
    let span = tree.locate(at).expect("span").clone();
    assert_eq!(span.target, SpanTarget::Argument { arg: "n".into(), item: None });
    assert_eq!(tree.span_of(&span.path, &span.target), Some(span.range.clone()));

    let mut view = EditorView::default();
    assert!(view.jump_from_text(&tree, EditorMode::Simple, at).is_none());
    assert_eq!(
        view.focus,
        Some(FocusTarget::Field { path: span.path.clone(), arg: "n".into(), item: None })
    );
    assert_eq!(view.jump_to_text(&tree), Some(span.range.clone()));
    assert_eq!(view.focus, Some(FocusTarget::Text));

    // `1000` is outside the Simple card: offer Advanced, keep focus.
    let at = tree.text.find("1000").expect("1000");
    view.jump_from_text(&tree, EditorMode::Simple, at);
    assert_eq!(view.simple_offer, Some(NodePath::element(1)));
    assert_eq!(view.focus, Some(FocusTarget::Text));
    view.jump_from_text(&tree, EditorMode::Advanced, at);
    assert!(view.simple_offer.is_none());
    assert!(matches!(&view.focus, Some(FocusTarget::Field { arg, .. }) if arg == "value"));

    // Typed but not committed: commit first, jump after the rebuild.
    let typed = tree.text.replace("head-20", "head-30");
    view.text = Some((typed.clone(), tree.text.clone()));
    let at = typed.rfind("30").expect("30");
    let command = view.jump_from_text(&tree, EditorMode::Simple, at).expect("commit");
    assert_eq!(view.pending_jump, Some(at));
    state.apply(command).expect("set text");
    view.resume_jump(state.tree(), EditorMode::Simple);
    assert!(view.pending_jump.is_none());
    assert!(matches!(&view.focus, Some(FocusTarget::Field { arg, .. }) if arg == "n"));
}

// --- Node arguments (design `recipe-tree`) ---

fn tree_registry() -> Arc<CommandMetadataRegistry> {
    use liquers_core::command_metadata::{ArgumentType, NodeArgument};
    let mut cmr = CommandMetadataRegistry::new();
    let mut attach = CommandMetadata::new("attach");
    let mut part = ArgumentInfo::any_argument("part");
    part.argument_type = ArgumentType::Node(NodeArgument::new("glb"));
    attach.with_argument(part);
    cmr.add_command(&attach);
    Arc::new(cmr)
}

fn car_context() -> NodeContext {
    let recipes: liquers_core::recipes::RecipeList = serde_yaml::from_str(
        "recipes:\n  - {query: tyre.glb, title: Tyre}\n  - {query: \"-R/./tyre.glb/-/wheel.glb\", title: Wheel}\n  - {query: \"-R/./body.glb/-/attach-wheel/car.glb\", title: Car}\n",
    )
    .expect("recipes");
    NodeContext {
        folder: "scenes/car".into(),
        recipes,
        listing: Some(vec!["body.glb".into(), "tyre.glb".into(), "recipes.yaml".into()]),
    }
}

fn node_editor(query: &str) -> EditorState {
    let mut state = EditorState::new(EditorKind::Query, tree_registry());
    let q = parse_query(query).expect("query");
    state.on_snapshot(&Value::from_query(&q), Some(fp(1))).expect("snapshot");
    state
}

/// AC-18: a Node field lists the folder's nodes of its extension, with filename and title.
#[test]
fn node_selector_lists_filename_and_title() {
    let mut state = node_editor("-R/./body.glb/-/attach-wheel/car.glb");
    assert!(field(state.tree(), 1, "part").node_choices.is_empty(), "no folder yet");
    assert_eq!(field(state.tree(), 1, "part").widget, ArgumentGUIInfo::NodeSelector);
    state.set_node_context(Some(car_context()));
    let f = field(state.tree(), 1, "part");
    let choices: Vec<(&str, &str, &str)> = f
        .node_choices
        .iter()
        .map(|c| (c.name.as_str(), c.filename.as_str(), c.title.as_str()))
        .collect();
    assert_eq!(
        choices,
        vec![
            ("body", "body.glb", ""),
            ("car", "car.glb", "Car"),
            ("tyre", "tyre.glb", "Tyre"),
            ("wheel", "wheel.glb", "Wheel")
        ]
    );
    assert_eq!(f.value, FieldValue::Text("wheel".into()));
    assert_eq!(
        recipe_folder("-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor/recipe-2/query").as_deref(),
        Some("scenes/car")
    );
    assert_eq!(recipe_folder("-R/data/x.csv"), None);
}

/// AC-18: a link to a node of the folder is shown as the node's name.
#[test]
fn node_selector_shows_link_as_name() {
    let state = node_editor("-R/./body.glb/-/attach-~X~-R/./wheel.glb~E/car.glb");
    assert_eq!(field(state.tree(), 1, "part").value, FieldValue::Text("wheel".into()));
    let other = node_editor("-R/./body.glb/-/attach-~X~-R/data/wheel.glb~E/car.glb");
    assert!(matches!(field(other.tree(), 1, "part").value, FieldValue::Link { .. }));
}

/// AC-25: Rename rewrites the whole document and asks for the stored file's rename.
#[test]
fn node_selector_rename_writes_document_and_data() {
    let mut state = node_editor("-R/./body.glb/-/attach-wheel/car.glb");
    assert!(state
        .apply(EditorCommand::RenameNode { from: "wheel".into(), to: "w".into() })
        .is_err(), "no folder: refused");
    state.set_node_context(Some(car_context()));
    match state
        .apply(EditorCommand::RenameNode { from: "tyre".into(), to: "tire".into() })
        .expect("rename")
    {
        EditorEffect::RenameData { folder, editor } => {
            assert_eq!(folder, "scenes/car");
            let queries: Vec<&str> =
                editor.recipes().recipes.iter().map(|r| r.query.as_str()).collect();
            assert_eq!(queries, vec!["tire.glb", "-R/./tire.glb/-/wheel.glb", "-R/./body.glb/-/attach-wheel/car.glb"]);
            assert_eq!(editor.pending_data_renames().len(), 1);
            assert_eq!(editor.pending_data_renames()[0].to, "tire.glb");
        }
        other => panic!("expected RenameData, got {other:?}"),
    }
    let names: Vec<String> = field(state.tree(), 1, "part")
        .node_choices
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(names.contains(&"tire".to_string()), "{names:?}");
    let error = state
        .apply(EditorCommand::RenameNode { from: "wheel".into(), to: "car".into() })
        .expect_err("car is taken");
    assert!(error.message.contains("car.glb"), "{error}");
}
