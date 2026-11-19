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
