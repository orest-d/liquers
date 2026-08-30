//! Integration tests for the command declaration pipeline.
//!
//! The unit tests in `liquers-core::command_declaration` cover the merge laws, the conventions and
//! the defaults. These cover the things that need the whole crate, or a file on disk.

use liquers_core::command_declaration::CommandDeclaration;
use liquers_core::command_metadata::{CommandMetadata, CommandMetadataRegistry};
use serde_json::{json, Value};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("liquers-core has a parent directory")
        .to_path_buf()
}

/// The hard constraint: the committed registry must not move. Compared byte-for-byte rather than
/// by value equality, because a `Serialize` change that happens to round-trip is still a change to
/// a committed file.
///
/// This is what proves the deserialize-only serde work in `command_metadata.rs` really is
/// deserialize-only.
#[test]
fn int01_command_registry_yaml_is_byte_identical_after_parse_and_re_serialize() {
    let path = repo_root().join("specs/command_registry.yaml");
    let original = std::fs::read_to_string(&path).expect("the committed registry is readable");
    let registry: CommandMetadataRegistry =
        serde_yaml::from_str(&original).expect("the committed registry parses");
    let again = serde_yaml::to_string(&registry).expect("it re-serializes");

    // The exporter carries a hand-maintained comment block that serde does not round-trip.
    let without_comments: String = original
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| format!("{line}\n"))
        .collect();

    assert_eq!(
        without_comments, again,
        "specs/command_registry.yaml changed on a parse and re-serialize"
    );
}

/// The two label rules must stay apart. A JavaScript command keeps its name verbatim; a document
/// command gets the derived label. Collapsing them would re-version every underscored JavaScript
/// command and re-expire its dependent assets.
#[test]
fn int03_label_parity_between_the_two_paths() {
    let document = CommandDeclaration::from_document(json!({ "name": "foo_bar" }))
        .finish()
        .expect("builds");
    assert_eq!(document.label, "Foo bar", "the document path derives");

    // What `liquers-web` does: the name verbatim, set explicitly rather than derived.
    let javascript = CommandDeclaration::from_document(json!({ "name": "foo_bar",
                                                               "label": "foo_bar" }))
    .finish()
    .expect("builds");
    assert_eq!(
        javascript.label, "foo_bar",
        "the JavaScript path keeps the name"
    );
}

/// The same declaration in YAML and in JSON must build the same metadata. The fixture is Example 2
/// of Phase 3, so the documented example is the tested input.
#[test]
fn int04_yaml_and_json_agree() {
    let yaml = include_str!("fixtures/commands.yaml");
    let from_yaml: Value = serde_yaml::from_str(yaml).expect("the fixture parses as YAML");
    let json_text = serde_json::to_string(&from_yaml).expect("it converts to JSON");
    let from_json: Value = serde_json::from_str(&json_text).expect("and back");

    let built_from_yaml = build_all(&from_yaml);
    let built_from_json = build_all(&from_json);
    assert_eq!(built_from_yaml, built_from_json);
    assert_eq!(
        built_from_yaml.len(),
        2,
        "the fixture declares two commands"
    );
}

fn build_all(document: &Value) -> Vec<CommandMetadata> {
    document["commands"]
        .as_array()
        .expect("a `commands` array")
        .iter()
        .map(|declaration| {
            CommandDeclaration::from_document(declaration.clone())
                .finish()
                .expect("each declaration builds")
        })
        .collect()
}

/// The fixture is the plain-document case end to end: no introspection, so every field is
/// declared, defaults fill the rest, and a registration hint survives on the declaration while
/// staying out of the metadata.
#[test]
fn int04b_the_document_fixture_builds_what_it_says() {
    let yaml = include_str!("fixtures/commands.yaml");
    let document: Value = serde_yaml::from_str(yaml).expect("parses");
    let commands = build_all(&document);

    let to_upper = &commands[0];
    assert_eq!(to_upper.label, "To upper", "derived from the name");
    assert_eq!(to_upper.filename, "upper.txt");
    assert!(to_upper.state_argument.is_some());
    assert!(to_upper.cache, "the default");

    let repeat = &commands[1];
    assert_eq!(repeat.label, "Repeat text", "declared, so not derived");
    assert_eq!(repeat.arguments.len(), 1);
    assert_eq!(repeat.arguments[0].name, "count");
    assert_eq!(
        repeat.arguments[0].label, "Count",
        "derived from the argument name"
    );
    assert_eq!(
        repeat.arguments[0].argument_type,
        liquers_core::command_metadata::ArgumentType::Integer,
        "`type` reached `argument_type`"
    );
    assert_eq!(
        repeat.arguments[0].default,
        liquers_core::command_metadata::CommandParameterValue::Value(json!(2)),
        "the bare `2` shorthand"
    );
    assert_eq!(
        serde_json::to_value(repeat)
            .expect("serializes")
            .get("registration"),
        None,
        "a registration hint never reaches the metadata"
    );
}

/// A declaration and `register_command!` must agree on the metadata they produce, including
/// `metadata_version`.
///
/// The version is computed from stored content by `CommandMetadataRegistry`, so equal content
/// gives an equal version automatically — this asserts that rather than assuming it, which is the
/// only way to notice if the declaration path ever diverges in some field nobody looked at.
///
/// Phase 4 left the placement of this test open, on the chance that `register_command!` could not
/// be reached from a `liquers-core` test. It can: `liquers-macro` is a dev-dependency
/// (`liquers-core/Cargo.toml:78`), so the test lives here rather than in `liquers-lib`.
#[tokio::test]
async fn int02_declaration_and_macro_agree_including_metadata_version() {
    use liquers_core::command_metadata::CommandKey;
    use liquers_core::context::{Context, Environment, SimpleEnvironment};
    use liquers_core::error::Error;
    use liquers_core::state::State;
    use liquers_core::value::Value;
    use liquers_macro::register_command;

    // The macro's generated wrapper names both of these.
    type CommandEnvironment = SimpleEnvironment<Value>;

    fn repeat(state: &State<Value>, count: i64) -> Result<Value, Error> {
        let text = state.try_into_string()?;
        Ok(Value::from(text.repeat(count.max(0) as usize)))
    }

    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn repeat(state, count: i64) -> result)
        .expect("the macro registers the command");
    let envref = env.to_ref();
    let macro_metadata = envref
        .get_command_metadata_registry()
        .get(CommandKey::new("", "root", "repeat"))
        .cloned()
        .expect("the macro-registered command is in the registry");

    // The same command, declared. The declaration mirrors what the macro emits: the macro's own
    // label rule (underscores to spaces, no capitalisation) rather than the declaration path's,
    // since this compares the two registrations of *one* command.
    let declared = CommandDeclaration::from_document(json!({
        "name": "repeat",
        "label": "repeat",
        "state_argument": { "name": "state", "label": "state", "default": "None",
                            "gui_info": { "TextField": 40 } },
        // `gui_info` is declared on both sides rather than defaulted: the two paths disagree
        // about the default — the macro says TextField(20), `ArgumentInfo::any_argument` says
        // TextField(40) — which is ARGUMENT-GUI-INFO-HAS-THREE-DEFAULTS, not something this
        // design can settle. Declaring it keeps the test measuring what it is for.
        "arguments": [ { "name": "count", "label": "count", "argument_type": "int",
                         "gui_info": { "TextField": 20 } } ],
    }))
    .finish()
    .expect("the declaration builds");

    let mut from_declaration = CommandMetadataRegistry::new();
    from_declaration.add_command(&declared);
    let declaration_metadata = from_declaration
        .get(CommandKey::new("", "root", "repeat"))
        .cloned()
        .expect("the declared command is in the registry");

    // `impl_version` comes from registration rather than from the declaration, so it is compared
    // separately — see REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED.
    let mut macro_comparable = macro_metadata.clone();
    macro_comparable.impl_version = declaration_metadata.impl_version.clone();

    assert_eq!(
        macro_comparable.metadata_version, declaration_metadata.metadata_version,
        "equal content must give an equal metadata_version"
    );
    assert_eq!(macro_comparable, declaration_metadata);
}
