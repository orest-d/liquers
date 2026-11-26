//! Unit tests of the recipes panel's core (design `recipes-editor`, Phase 3).

use std::collections::BTreeSet;
use std::sync::Arc;

use liquers_core::accessor::Fingerprint;
use liquers_core::command_metadata::{ArgumentInfo, ArgumentType, CommandMetadata, CommandMetadataRegistry, NodeArgument};
use liquers_core::metadata::{AssetInfo, ProgressEntry, Status, Version};
use liquers_core::recipes::RecipeList;
use liquers_core::value::ValueInterface;

use super::*;
use crate::ui::editor::{EditorCommand, NodeContext};
use crate::value::Value;

const CAR: &str = r#"tree: {}
recipes:
  - {query: body.glb, title: Body, description: The car body}
  - {query: tyre.glb, title: Tyre}
  - {query: rubber.mat, title: Rubber}
  - {title: Wheel, query: "-R/./tyre.glb/-/material-rubber/wheel.glb"}
  - {title: Car, query: "-R/./body.glb/-/attach-wheel/car.glb"}
"#;

const FLAT: &str = "recipes:\n- {query: -R/data/x.csv/-/head/a.csv, title: A, description: First}\n- {query: -R/data/y.csv/-/head/b.csv}\n";

fn registry() -> Arc<CommandMetadataRegistry> {
    let mut cmr = CommandMetadataRegistry::new();
    let mut attach = CommandMetadata::new("attach");
    let mut part = ArgumentInfo::any_argument("part");
    part.argument_type = ArgumentType::Node(NodeArgument::new("glb"));
    attach.with_argument(part);
    cmr.add_command(&attach);
    let mut material = CommandMetadata::new("material");
    let mut m = ArgumentInfo::any_argument("m");
    m.argument_type = ArgumentType::Node(NodeArgument::new("mat"));
    material.with_argument(m);
    cmr.add_command(&material);
    cmr.add_command(&CommandMetadata::new("head"));
    Arc::new(cmr)
}

fn fp(n: u128) -> Fingerprint {
    Fingerprint { anchor: 1, content: Version::new(n) }
}

fn panel(text: &str) -> RecipesState {
    let mut state = RecipesState::new(registry()).with_folder(Some("scenes/car".into()));
    state.on_snapshot(&Value::from(text), Some(fp(1))).expect("snapshot");
    state
}

fn car() -> RecipesState {
    let mut state = panel(CAR);
    let list: RecipeList = serde_yaml::from_str(CAR).expect("list");
    state.on_context(NodeContext::new(
        "scenes/car",
        list,
        Some(vec!["body.glb".into(), "tyre.glb".into(), "rubber.mat".into(), "recipes.yaml".into()]),
    ));
    state
}

fn names(state: &RecipesState) -> Vec<String> {
    state.rows().iter().map(|r| r.name.clone()).collect()
}

fn written_text(effect: &RecipesEffect) -> String {
    match effect {
        RecipesEffect::Write { value, .. } => value.try_into_string().expect("text"),
        other => panic!("expected a write, got {other:?}"),
    }
}

/// AC-1: one row per recipe, in file order, showing filenames.
#[test]
fn outline_lists_recipes_in_file_order() {
    let state = panel(FLAT);
    assert_eq!(names(&state), vec!["a.csv", "b.csv"]);
    assert_eq!(state.rows()[0].title, "A");
    assert!(!state.is_tree());
}

/// AC-2: the icon comes from the asset's metadata, else from the extension.
#[test]
fn icon_from_metadata_else_extension() {
    let mut state = panel(FLAT);
    let by_extension = state.rows()[0].icon.clone();
    assert_eq!(by_extension, liquers_core::icons::file_extension_to_unicode_icon("csv").trim());
    let mut context = NodeContext::new("scenes/car", state.list().expect("list").clone(), None);
    context.assets.insert("a.csv".into(), AssetInfo { unicode_icon: "★".into(), ..Default::default() });
    state.on_context(context);
    assert_eq!(state.rows()[0].icon, "★");
    assert_eq!(state.rows()[1].icon, by_extension);
}

/// AC-3: a tree shows node names, marks the result and the sources.
#[test]
fn tree_rows_show_node_names_result_and_sources() {
    let mut state = car();
    assert!(state.is_tree());
    assert_eq!(names(&state), vec!["body", "tyre", "rubber", "wheel", "car"]);
    assert!(state.rows()[0].is_source);
    assert!(!state.rows()[4].is_source);
    state.apply(RecipesCommand::SetResult(Some("car".into()))).expect("result");
    assert!(state.rows()[4].is_result);
    assert_eq!(state.list().and_then(|l| l.tree.as_ref()).and_then(|t| t.result.clone()), Some("car.glb".into()));
}

/// AC-4: opening a row twice keeps one window, focused last.
#[test]
fn open_twice_keeps_one_form() {
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    state.apply(RecipesCommand::Open("b.csv".into())).expect("open");
    state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    let ids: Vec<&str> = state.forms().iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec!["b.csv", "a.csv"]);
    state.apply(RecipesCommand::Close("a.csv".into())).expect("close");
    assert_eq!(state.forms().len(), 1);
}

/// AC-5: the form edits the title, the description, options and the query.
#[test]
fn form_edits_fields_and_query() {
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    let form = |command| RecipesCommand::Form { id: "a.csv".into(), command };
    state.apply(form(FormCommand::SetTitle("Alpha".into()))).expect("title");
    state.apply(form(FormCommand::SetDescription("The first".into()))).expect("description");
    state
        .apply(form(FormCommand::SetOption { field: "volatile".into(), text: "true".into() }))
        .expect("volatile");
    let effect = state
        .apply(form(FormCommand::Editor(EditorCommand::SetText("-R/data/c.csv/-/a.csv".into()))))
        .expect("query");
    let recipe = state.list().expect("list").get("a.csv").expect("recipe").clone();
    assert_eq!(recipe.title, "Alpha");
    assert_eq!(recipe.description, "The first");
    assert!(recipe.volatile);
    assert_eq!(recipe.query, "-R/data/c.csv/-/a.csv");
    assert!(matches!(effect, RecipesEffect::Write { .. } | RecipesEffect::None));
    assert!(state.apply(form(FormCommand::SetOption { field: "colour".into(), text: "red".into() })).is_err());
}

/// AC-6: a form follows its recipe by filename when the list is reordered.
#[test]
fn form_follows_its_recipe_after_reordering() {
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::Open("b.csv".into())).expect("open");
    let reordered = "recipes:\n- {query: -R/data/y.csv/-/head/b.csv, title: Bee}\n- {query: -R/data/x.csv/-/head/a.csv, title: A}\n";
    state.on_snapshot(&Value::from(reordered), Some(fp(2))).expect("snapshot");
    assert_eq!(state.form("b.csv").expect("form").recipe().title, "Bee");
}

/// AC-7: New appends a recipe with a free filename and opens it; the name is the first free
/// integer (design `strict-tree-editing`, AC-5).
#[test]
fn new_recipe_opens_its_form() {
    let mut state = panel(FLAT);
    let effect = state
        .apply(RecipesCommand::New { title: "Summary table".into(), extension: ".csv".into() })
        .expect("new");
    assert!(written_text(&effect).contains("1.csv"));
    let last = state.rows().last().expect("row").clone();
    assert_eq!(last.title, "Summary table");
    let filename = last.filename.expect("filename");
    assert_eq!(filename, "1.csv");
    assert!(state.form(&filename).is_some());
}

/// AC-8: a rename rewrites the references to the node and re-keys its open window.
#[test]
fn rename_rewrites_references_and_rekeys_the_form() {
    let mut state = panel(CAR);
    state.apply(RecipesCommand::Open("wheel.glb".into())).expect("open");
    let effect = state.apply(RecipesCommand::Rename { from: "wheel".into(), to: "rim".into() }).expect("rename");
    let text = written_text(&effect);
    assert!(text.contains("rim.glb"), "{text}");
    assert!(!text.contains("wheel.glb"), "{text}");
    assert!(state.form("rim.glb").is_some());
    assert!(state.form("wheel.glb").is_none());
    assert!(names(&state).contains(&"rim".to_owned()));
}

/// AC-8: renaming a node with stored data saves the tree, which moves the file.
#[test]
fn rename_with_stored_data_saves_tree() {
    let mut state = car();
    let effect = state.apply(RecipesCommand::Rename { from: "tyre".into(), to: "tire".into() }).expect("rename");
    match effect {
        RecipesEffect::SaveTree { folder, editor } => {
            assert_eq!(folder, "scenes/car");
            assert_eq!(editor.pending_data_renames().len(), 1);
        }
        other => panic!("expected a tree save, got {other:?}"),
    }
    assert!(!state.is_pending());
}

/// AC-9: Delete is refused while something references the node; Duplicate appends a copy.
#[test]
fn delete_is_refused_while_referenced_and_duplicate_appends() {
    let mut state = panel(CAR);
    assert!(state.apply(RecipesCommand::Delete("wheel".into())).is_err());
    assert!(state.message().expect("message").contains("car"));
    assert_eq!(state.rows().len(), 5);
    state.apply(RecipesCommand::Duplicate("wheel".into())).expect("duplicate");
    assert_eq!(state.rows().len(), 6);
    let copy = state.rows().last().expect("row").clone();
    assert_ne!(copy.filename.as_deref(), Some("wheel.glb"));
    assert_eq!(copy.title, "Wheel");
    state.apply(RecipesCommand::Delete(copy.filename.expect("filename"))).expect("delete the copy");
    assert_eq!(state.rows().len(), 5);
}

/// AC-10: check findings are attached to the rows they are about.
#[test]
fn findings_attach_to_rows() {
    let state = panel("tree: {}\nrecipes:\n  - {query: \"-R/./missing.glb/-/attach-wheel/car.glb\", title: Car}\n");
    let car = &state.rows()[0];
    assert!(!car.findings.is_empty(), "{:?}", state.report());
    assert!(!car.findings_tooltip().is_empty());
}

/// AC-11: the dependencies view puts roots on top, repeats shared nodes and stops at a cycle.
#[test]
fn dependencies_view_repeats_shared_nodes_and_stops_cycles() {
    let shared = "tree: {}\nrecipes:\n  - {query: tyre.glb}\n  - {query: \"-R/./tyre.glb/-/front.glb\"}\n  - {query: \"-R/./tyre.glb/-/back.glb\"}\n  - {query: \"-R/./front.glb/-/attach-back/car.glb\"}\n";
    let mut state = panel(shared).with_display(RecipeLabel::Name, RecipesView::Dependencies);
    let rows: Vec<(String, usize, bool)> = state.rows().iter().map(|r| (r.name.clone(), r.depth, r.repeated)).collect();
    assert_eq!(rows[0], ("car".to_owned(), 0, false));
    assert_eq!(rows.iter().filter(|(n, _, _)| n == "tyre").count(), 2);
    assert!(rows.iter().any(|(n, _, repeated)| n == "tyre" && *repeated));
    // Collapsing the root hides its children.
    let root = state.rows()[0].id.clone();
    state.apply(RecipesCommand::Toggle(root)).expect("toggle");
    assert_eq!(state.rows().len(), 1);
    assert_eq!(state.rows()[0].expandable, Some(false));

    let cycle = "tree: {}\nrecipes:\n  - {query: \"-R/./b.txt/-/a.txt\"}\n  - {query: \"-R/./a.txt/-/b.txt\"}\n";
    let mut state = RecipesState::new(registry())
        .with_folder(Some("x".into()))
        .with_display(RecipeLabel::Name, RecipesView::Dependencies);
    // A cycle is a finding of the check, and the document still loads.
    let _ = state.on_snapshot(&Value::from(cycle), Some(fp(1)));
    assert!(state.rows().iter().any(|r| r.cycle));
    assert!(state.rows().len() <= 4);
    let _ = BTreeSet::<String>::new();
}

/// AC-12: the YAML window applies only text that parses.
#[test]
fn yaml_apply_refuses_invalid_text() {
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::OpenYaml).expect("open");
    assert!(state.yaml().expect("yaml").text.contains("a.csv"));
    state.apply(RecipesCommand::SetYaml("recipes:\n- {query: [".into())).expect("typed");
    assert!(state.yaml().expect("yaml").error.is_some());
    assert!(state.apply(RecipesCommand::ApplyYaml).is_err());
    assert_eq!(state.rows().len(), 2);
    state
        .apply(RecipesCommand::SetYaml("recipes:\n- {query: -R/data/c.csv}\n".into()))
        .expect("typed");
    let effect = state.apply(RecipesCommand::ApplyYaml).expect("apply");
    assert!(written_text(&effect).contains("c.csv"));
    assert_eq!(names(&state), vec!["c.csv"]);
}

/// AC-12: keys, punctuation and the query of a YAML text are coloured.
#[test]
fn yaml_spans_mark_keys_and_queries() {
    use crate::ui::theme::SyntaxRole;
    let text = "# recipes\n- {query: -R/a.csv/-/head, title: A}\n";
    let spans = yaml_spans(text);
    assert_eq!(spans.first().map(|(r, _)| r.start), Some(0));
    assert_eq!(spans.last().map(|(r, _)| r.end), Some(text.len()));
    for pair in spans.windows(2) {
        assert_eq!(pair[0].0.end, pair[1].0.start, "spans cover the text");
    }
    let role_of = |needle: &str| {
        let at = text.find(needle).expect("needle");
        spans.iter().find(|(r, _)| r.contains(&at)).map(|(_, role)| *role).expect("span")
    };
    assert_eq!(role_of("# recipes"), SyntaxRole::Separator);
    assert_eq!(role_of("query"), SyntaxRole::Resource);
    assert_eq!(role_of("head"), SyntaxRole::Action);
    assert!(parse_recipes("recipes:\n- {query: [").is_err());
    assert!(parse_recipes("").expect("empty").recipes.is_empty());
}

/// AC-13: on demand, edits wait for Update; Revert reads the file again.
#[test]
fn on_demand_holds_edits_until_update() {
    let mut state = panel(FLAT).with_continuous(false);
    state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    let effect = state
        .apply(RecipesCommand::Form { id: "a.csv".into(), command: FormCommand::SetTitle("Alpha".into()) })
        .expect("title");
    assert!(matches!(effect, RecipesEffect::None));
    assert!(state.is_pending());
    assert!(written_text(&state.apply(RecipesCommand::Update).expect("update")).contains("Alpha"));
    assert!(!state.is_pending());
    assert!(matches!(state.apply(RecipesCommand::Revert).expect("revert"), RecipesEffect::Reload));
}

/// AC-13: applied immediately, each edit is written, one write at a time.
#[test]
fn apply_immediately_writes_each_edit() {
    let mut state = panel(FLAT);
    let first = state.apply(RecipesCommand::Duplicate("a.csv".into())).expect("duplicate");
    assert!(matches!(first, RecipesEffect::Write { expected: Some(_), .. }));
    // A second edit while the first write is in flight waits for it.
    let second = state.apply(RecipesCommand::Duplicate("b.csv".into())).expect("duplicate");
    assert!(matches!(second, RecipesEffect::None));
    let next = state.on_written(fp(2));
    assert!(written_text(&next).contains("b"));
    assert!(matches!(state.on_written(fp(3)), RecipesEffect::None));
}

/// AC-14: a refused edit changes nothing and is reported.
#[test]
fn refused_edit_changes_nothing() {
    let mut state = panel(CAR);
    let before = state.document_text();
    // Renaming to a name that is no filename is refused (a used name is made unique instead).
    assert!(state.apply(RecipesCommand::Rename { from: "wheel".into(), to: "a/b".into() }).is_err());
    assert_eq!(state.document_text(), before);
    assert!(state.message().is_some());
    // A query edit that would add a cycle is refused and the form is restored.
    state.apply(RecipesCommand::Open("body.glb".into())).expect("open");
    let cycle = state.apply(RecipesCommand::Form {
        id: "body.glb".into(),
        command: FormCommand::Editor(EditorCommand::SetText("-R/./car.glb/-/body.glb".into())),
    });
    assert!(cycle.is_err());
    assert_eq!(state.document_text(), before);
    assert_eq!(state.form("body.glb").expect("form").recipe().query, "body.glb");
}

/// AC-15: a conflicting write loads the live file and says the edits were not written.
#[test]
fn conflict_reloads_and_reports() {
    let mut state = panel(FLAT);
    let _ = state.apply(RecipesCommand::Duplicate("a.csv".into())).expect("duplicate");
    state.on_conflict(&Value::from("recipes:\n- {query: -R/data/z.csv}\n"), Some(fp(9)));
    assert_eq!(names(&state), vec!["z.csv"]);
    assert!(state.message().expect("message").starts_with("Conflict"));
}

/// AC-16: rows show the name, the title or both.
#[test]
fn label_setting_chooses_name_title_or_both() {
    let mut state = panel(FLAT);
    let a = state.rows()[0].clone();
    let b = state.rows()[1].clone();
    assert_eq!(a.label(RecipeLabel::Name), ("a.csv".to_owned(), None));
    assert_eq!(a.label(RecipeLabel::Title), ("A".to_owned(), None));
    assert_eq!(a.label(RecipeLabel::Both), ("a.csv".to_owned(), Some("A".to_owned())));
    assert_eq!(b.label(RecipeLabel::Title), ("b.csv".to_owned(), None));
    state.apply(RecipesCommand::SetLabel(RecipeLabel::Both)).expect("label");
    assert_eq!(state.label(), RecipeLabel::Both);
    assert_eq!(RecipeLabel::parse(RecipeLabel::Title.as_str()), Some(RecipeLabel::Title));
}

/// AC-17: a status circle, a percentage while in progress, the error on hover.
#[test]
fn status_progress_and_errors() {
    assert_eq!(RowStatus::of(None), RowStatus::None);
    let ready = AssetInfo { status: Status::Ready, ..Default::default() };
    assert_eq!(RowStatus::of(Some(&ready)).tone(), "ok");
    let working = AssetInfo {
        status: Status::Processing,
        progress: ProgressEntry { message: "rows".into(), done: 1, total: 4, ..Default::default() },
        ..Default::default()
    };
    let status = RowStatus::of(Some(&working));
    assert_eq!(status, RowStatus::Progress { percent: 25, message: "rows".into() });
    assert!(status.is_busy());
    assert!(status.tooltip().contains("25%"));
    let failed = AssetInfo { status: Status::Error, is_error: true, message: "no such column".into(), ..Default::default() };
    let status = RowStatus::of(Some(&failed));
    assert_eq!(status.tone(), "error");
    assert!(status.tooltip().contains("no such column"));

    let mut state = panel(FLAT);
    let mut context = NodeContext::new("scenes/car", state.list().expect("list").clone(), None);
    context.assets.insert("a.csv".into(), working);
    state.on_context(context);
    assert!(state.busy());
}

/// AC-18: the name's tooltip gives the title and description, the title's the description.
#[test]
fn tooltips_give_title_and_description() {
    let state = panel(FLAT);
    let a = &state.rows()[0];
    assert_eq!(a.name_tooltip(), "A\nFirst");
    assert_eq!(a.title_tooltip(), "First");
    let state = panel(CAR);
    let body = &state.rows()[0];
    assert_eq!(body.name, "body");
    assert_eq!(body.name_tooltip(), "Body\nThe car body\nbody.glb");
}

/// AC-19: the context menu's items and the effects of View, Metadata, Evaluate and Save as.
#[test]
fn menu_actions_produce_their_effects() {
    let mut state = car();
    let row = state.rows()[3].clone();
    let items = menu_items(&row, true, true, true);
    let actions: Vec<MenuAction> = items.iter().map(|i| i.action.clone()).collect();
    for expected in [MenuAction::View, MenuAction::Metadata, MenuAction::Edit, MenuAction::Evaluate, MenuAction::Rename, MenuAction::AddChild, MenuAction::SaveAs, MenuAction::Delete] {
        assert!(actions.contains(&expected), "{expected:?}");
    }
    // Export waits for the formats; a read-only panel cannot rename.
    assert!(items.iter().any(|i| matches!(i.action, MenuAction::Export(_)) && i.disabled.is_some()));
    let read_only = menu_items(&row, true, false, false);
    assert!(read_only.iter().find(|i| i.action == MenuAction::Rename).and_then(|i| i.disabled.clone()).is_some());
    assert!(read_only.iter().find(|i| i.action == MenuAction::SaveAs).and_then(|i| i.disabled.clone()).is_some());

    let submit = |effect| match effect {
        RecipesEffect::Submit(q) => q,
        other => panic!("expected a query, got {other:?}"),
    };
    assert_eq!(submit(state.apply(RecipesCommand::View("wheel.glb".into())).expect("view")), "-R/scenes/car/wheel.glb/-/ns-lui/add-child");
    assert_eq!(
        submit(state.apply(RecipesCommand::Metadata("wheel.glb".into())).expect("metadata")),
        "-R-meta/scenes/car/wheel.glb/-/ns-lui/add-child"
    );
    assert!(matches!(
        state.apply(RecipesCommand::Evaluate("wheel.glb".into())).expect("evaluate"),
        RecipesEffect::Evaluate(q) if q == "-R/scenes/car/wheel.glb"
    ));
    assert!(matches!(
        state.apply(RecipesCommand::SaveAs { filename: "wheel.glb".into(), format: Some("glb".into()), path: "/tmp/w.glb".into() }).expect("save as"),
        RecipesEffect::SaveAs { query, format: Some(f), path } if query == "-R/scenes/car/wheel.glb" && f == "glb" && path == "/tmp/w.glb"
    ));
    assert!(state.apply(RecipesCommand::SaveAs { filename: "wheel.glb".into(), format: None, path: " ".into() }).is_err());
    assert_eq!(MenuAction::Export("csv".into()).event("a.csv"), "rc:export:csv:a.csv");
    // A tree's Add child lists the Node arguments of the parent.
    let slots = state.child_slots("car.glb");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].extension, "glb");
}

/// AC-15: a missing file is an empty list that can be added to.
#[test]
fn missing_document_is_an_empty_list() {
    let mut state = RecipesState::new(registry()).with_folder(Some("new".into()));
    state.on_snapshot(&Value::none(), None).expect("snapshot");
    assert!(state.rows().is_empty());
    assert!(state.read_only().is_none());
    let effect = state.apply(RecipesCommand::New { title: "First".into(), extension: "txt".into() }).expect("new");
    assert!(written_text(&effect).contains("1.txt"));
}

/// AC-15: a file that does not parse loads read-only, with the YAML window open to repair it.
#[test]
fn invalid_document_loads_read_only() {
    let mut state = RecipesState::new(registry()).with_folder(Some("x".into()));
    assert!(state.on_snapshot(&Value::from("recipes:\n- {query: ["), Some(fp(1))).is_err());
    assert!(state.read_only().expect("read only").contains("does not parse"));
    assert!(state.yaml().expect("yaml").error.is_some());
    assert!(state.apply(RecipesCommand::New { title: "A".into(), extension: "txt".into() }).is_err());
    state.apply(RecipesCommand::SetYaml("recipes:\n- {query: -R/a.txt}\n".into())).expect("typed");
    state.apply(RecipesCommand::ApplyYaml).expect("repaired");
    assert!(state.read_only().is_none());
    assert_eq!(names(&state), vec!["a.txt"]);
}

/// AC-1: a recipe without a filename is listed by its position.
#[test]
fn recipe_without_filename_is_listed() {
    let mut state = panel("recipes:\n- {query: -R/data/a.csv/-/ns-pl/from_csv, title: Untitled}\n");
    assert_eq!(state.rows()[0].id, "#0");
    assert!(state.rows()[0].filename.is_none());
    state.apply(RecipesCommand::Open("#0".into())).expect("open");
    let effect = state
        .apply(RecipesCommand::Form { id: "#0".into(), command: FormCommand::SetTitle("Named".into()) })
        .expect("title");
    assert!(written_text(&effect).contains("Named"));
    let menu = menu_items(&state.rows()[0], false, true, true);
    assert!(menu.iter().find(|i| i.action == MenuAction::View).and_then(|i| i.disabled.clone()).is_some());
}

/// The panel decodes browser events into commands.
#[test]
fn browser_events_decode_to_commands() {
    use crate::ui::event::{UIEvent, UIEventKind};
    let state = panel(FLAT);
    let event = UIEvent::new(UIEventKind::Named("rc:open:a.csv".into()));
    assert_eq!(panel_commands(&event, &state), vec![RecipesCommand::Open("a.csv".into())]);
    let event = UIEvent::new(UIEventKind::Named("rc:mode:dependencies".into()));
    assert_eq!(panel_commands(&event, &state), vec![RecipesCommand::SetView(RecipesView::Dependencies)]);
    assert_eq!(recipes_folder("-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor"), Some("scenes/car".into()));
    assert_eq!(recipes_folder("-R/recipes.yaml/-/ns-acc/accessor"), Some(String::new()));
}

/// Review fix: a stored-file rename pending while the file changed elsewhere is not saved over it.
#[test]
fn pending_rename_with_stored_data_conflicts_after_a_remote_change() {
    let mut state = car().with_continuous(false);
    let effect = state.apply(RecipesCommand::Rename { from: "tyre".into(), to: "tire".into() }).expect("rename");
    assert!(matches!(effect, RecipesEffect::None));
    let changed = format!("{CAR}  - {{query: extra.glb, title: Extra}}\n");
    state.on_snapshot(&Value::from(changed.as_str()), Some(fp(7))).expect("snapshot");
    assert!(matches!(state.apply(RecipesCommand::Update).expect("update"), RecipesEffect::Reload));
    assert!(state.message().expect("message").starts_with("Conflict"));
}

/// Review fix: a recipe window's file selector asks the runner for a listing and gets it back.
#[test]
fn recipe_window_browse_lists_the_store() {
    use crate::ui::editor::{DirectoryEntry, DirectoryListing, NodePath};
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    let effect = state
        .apply(RecipesCommand::Form {
            id: "a.csv".into(),
            command: FormCommand::Editor(EditorCommand::Browse { path: NodePath::element(0), folder: "data".into() }),
        })
        .expect("browse");
    assert!(matches!(effect, RecipesEffect::ListDirectory(ref f) if f == "data"), "{effect:?}");
    state.on_directory(DirectoryListing {
        folder: "data".into(),
        entries: vec![DirectoryEntry { name: "x.csv".into(), is_dir: false }],
        error: None,
    });
}

/// strict-tree-editing AC-5, AC-2: New adds `1.node` with an empty query, and the tree written is
/// valid, with the new node as its result.
#[test]
fn new_adds_an_empty_node_and_writes_a_valid_tree() {
    let mut state = RecipesState::new(registry()).with_folder(Some("p".into()));
    state.on_snapshot(&Value::from("tree: {dependencies: strict}\nrecipes: []\n"), Some(fp(1))).expect("snapshot");
    let effect = state.apply(RecipesCommand::New { title: String::new(), extension: String::new() }).expect("new");
    let written = written_text(&effect);
    let list: RecipeList = serde_yaml::from_str(&written).expect("list");
    assert_eq!(list.recipes[0].query, "1.node");
    assert_eq!(list.tree.and_then(|t| t.result).as_deref(), Some("1.node"));
    assert!(state.report().errors.is_empty(), "{:?}", state.report().errors);
    assert!(state.tree_notice().is_none());
    assert!(state.form("1.node").is_some());
}

/// strict-tree-editing AC-14: a query typed without a filename keeps the node's filename.
#[test]
fn query_edit_keeps_the_node_filename() {
    let mut state = RecipesState::new(registry()).with_folder(Some("p".into()));
    state.on_snapshot(&Value::from("tree: {}\nrecipes: []\n"), Some(fp(1))).expect("snapshot");
    state.apply(RecipesCommand::New { title: String::new(), extension: String::new() }).expect("new");
    state.on_written(fp(2));
    let effect = state
        .apply(RecipesCommand::Form {
            id: "1.node".into(),
            command: FormCommand::Editor(EditorCommand::SetText("-R/data/x.csv/-/head".into())),
        })
        .expect("query");
    assert!(written_text(&effect).contains("-R/data/x.csv/-/head/1.node"), "{}", written_text(&effect));
    assert_eq!(state.form("1.node").expect("form").recipe().query, "-R/data/x.csv/-/head/1.node");
}

/// strict-tree-editing AC-14: a filename set in the form or typed into the query renames the
/// node, made unique, and its window follows.
#[test]
fn filename_edit_renames_uniquely() {
    let mut state = panel(CAR);
    state.apply(RecipesCommand::Open("car.glb".into())).expect("open");
    state
        .apply(RecipesCommand::Form { id: "car.glb".into(), command: FormCommand::SetFilename("tyre".into()) })
        .expect("rename");
    assert!(state.list().expect("list").get("tyre1.glb").is_some());
    assert!(state.form("tyre1.glb").is_some());
    state.on_written(fp(2));
    state
        .apply(RecipesCommand::Form {
            id: "tyre1.glb".into(),
            command: FormCommand::Editor(EditorCommand::SetText("-R/./body.glb/-/attach-wheel/body.glb".into())),
        })
        .expect("query rename");
    assert!(state.list().expect("list").get("body1.glb").is_some(), "{}", state.document_text());
    assert!(state.form("body1.glb").is_some());
}

/// strict-tree-editing AC-15: plain recipes get no duplicate filename either.
#[test]
fn plain_recipes_refuse_duplicate_filenames() {
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::Rename { from: "b.csv".into(), to: "a".into() }).expect("rename");
    assert!(!state.is_tree());
    let list = state.list().expect("list");
    assert!(list.get("a.csv").is_some() && list.get("a1.csv").is_some(), "{}", state.document_text());
}

/// strict-tree-editing AC-9: a `tree:` section that fails its check is shown as plain recipes.
#[test]
fn invalid_tree_shows_a_notice() {
    assert!(panel(CAR).tree_notice().is_none());
    let state = panel("tree: {dependencies: strict}\nrecipes:\n  - {query: \"-R/../x/y.glb/-/attach-y/bad.glb\"}\n");
    let notice = state.tree_notice().expect("notice");
    assert!(notice.starts_with("Not a valid recipe tree: plain recipes."), "{notice}");
    assert!(state.read_only().is_none(), "still editable");
}

/// strict-tree-editing AC-4: a failed write keeps the edits and says they are unsaved.
#[test]
fn failed_write_marks_unsaved() {
    let mut state = panel(FLAT);
    state.apply(RecipesCommand::Duplicate("a.csv".into())).expect("duplicate");
    state.on_write_failed(&liquers_core::error::Error::general_error("disk full".to_owned()));
    assert!(state.unsaved().is_some_and(|e| e.contains("disk full")));
    assert!(state.message().is_some_and(|m| m.starts_with("Unsaved edits:")));
    let retry = state.apply(RecipesCommand::Update).expect("update");
    assert!(matches!(retry, RecipesEffect::Write { .. }));
    state.on_written(fp(2));
    assert!(state.unsaved().is_none());
}

// Design `recipes-editor-open-action`.

fn with_open_command(text: &str) -> RecipesState {
    let mut state = RecipesState::new(registry())
        .with_folder(Some("scenes/car".into()))
        .with_open_command(Some(OpenCommand::new("dpl", "edit_node")));
    state.on_snapshot(&Value::from(text), Some(fp(1))).expect("snapshot");
    state
}

/// AC-1: with no open command, a click opens the recipe form as before.
#[test]
fn open_without_open_command_opens_the_form() {
    let mut state = panel(FLAT);
    let effect = state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    assert!(matches!(effect, RecipesEffect::None));
    assert!(state.form("a.csv").is_some());
}

/// AC-2, AC-7: a click submits the open command on the node's key; no form opens.
#[test]
fn open_with_open_command_submits_the_key_query() {
    let mut state = with_open_command(FLAT);
    let effect = state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    match effect {
        RecipesEffect::Submit(query) => assert_eq!(query, "-R-key/scenes/car/a.csv/-/ns-dpl/edit_node"),
        other => panic!("expected a submit, got {other:?}"),
    }
    assert!(state.form("a.csv").is_none());
    assert!(liquers_core::parse::parse_query("-R-key/scenes/car/a.csv/-/ns-dpl/edit_node").is_ok());
}

/// AC-3: Edit opens the form whatever the open command.
#[test]
fn edit_opens_the_form_with_an_open_command() {
    let mut state = with_open_command(FLAT);
    let effect = state.apply(RecipesCommand::Edit("b.csv".into())).expect("edit");
    assert!(matches!(effect, RecipesEffect::None));
    assert!(state.form("b.csv").is_some());
}

/// AC-4: the row clicked last is marked, and the mark follows a rename.
#[test]
fn clicked_row_is_marked() {
    let mut state = with_open_command(FLAT);
    assert!(!state.is_marked("a.csv"));
    state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
    assert!(state.is_marked("a.csv") && !state.is_marked("b.csv"));
    state.apply(RecipesCommand::Open("b.csv".into())).expect("open");
    assert!(state.is_marked("b.csv") && !state.is_marked("a.csv"));
    state.apply(RecipesCommand::Rename { from: "b.csv".into(), to: "c".into() }).expect("rename");
    assert!(state.is_marked("c.csv"), "{:?}", names(&state));
}

/// AC-5: the open command is saved with the element; an element saved before loads with none.
#[test]
fn open_command_survives_serialization() {
    let element = RecipesElement::new("Recipes", crate::ui::editor::EditorBinding::Memory(String::new()))
        .with_open_command("dpl", "edit_node");
    let json = serde_json::to_string(&element).expect("serialize");
    let loaded: RecipesElement = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(loaded.open_command(), Some(&OpenCommand::new("dpl", "edit_node")));
    let mut old: serde_json::Value = serde_json::from_str(&json).expect("json");
    old.as_object_mut().expect("object").remove("open_command");
    let loaded: RecipesElement = serde_json::from_value(old).expect("old form");
    assert_eq!(loaded.open_command(), None);
}

/// AC-3: the row menu's Edit sends `rc:edit`, decoded to `Edit`; a click stays `rc:open`.
#[test]
fn menu_edit_sends_rc_edit() {
    use crate::ui::event::{UIEvent, UIEventKind};
    let state = panel(FLAT);
    assert_eq!(MenuAction::Edit.event("a.csv"), "rc:edit:a.csv");
    let event = UIEvent::new(UIEventKind::Named("rc:edit:a.csv".into()));
    assert_eq!(panel_commands(&event, &state), vec![RecipesCommand::Edit("a.csv".into())]);
}

