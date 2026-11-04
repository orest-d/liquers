//! Tests for the pipeline conversions.

use std::collections::BTreeSet;

use serde_json::json;

use super::super::*;
use crate::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
use crate::parse::parse_query;
use crate::error::Error;
use crate::query::Query;
use crate::recipes::Recipe;
use serde_json::Value as Json;

fn corpus() -> Vec<String> {
    include_str!("../../../tests/fixtures/query_corpus.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Every (realm, namespace, name) an action of `p` names, nested pipelines and links included.
fn collect_commands(p: &Pipeline, out: &mut BTreeSet<(String, String, String)>) {
    for e in p.elements.iter() {
        match &e.item {
            ElementKind::Action(a) => {
                let ns = a.ns.as_ref().and_then(|n| n.0.first().cloned()).unwrap_or_default();
                out.insert((p.realm.clone(), ns, a.action.clone()));
                for s in a.parameters.iter().map(|p| &p.item).chain(a.arguments.values().map(|a| &a.item)) {
                    if let Source::Link(text) = s {
                        if let Ok(inner) = Pipeline::from_query_text(text) {
                            collect_commands(&inner, out);
                        }
                    }
                }
            }
            ElementKind::Pipeline(np) => collect_commands(np, out),
            _ => {}
        }
    }
}

/// A registry with every command the pipelines name, each taking any number of parameters.
fn permissive_registry(pipelines: &[Pipeline]) -> CommandMetadataRegistry {
    let mut names = BTreeSet::new();
    for p in pipelines {
        collect_commands(p, &mut names);
    }
    let mut cmr = CommandMetadataRegistry::new();
    for (realm, ns, name) in names {
        let mut cm = CommandMetadata::new(&name);
        cm.with_realm(&realm).with_namespace(&ns);
        cm.with_argument(ArgumentInfo::any_argument("items").set_multiple());
        cmr.add_command(&cm);
    }
    cmr
}

/// Corpus queries a pipeline refuses, each for a reason the design states.
const REFUSED: [(&str, &str); 2] = [
    // A header level other than `-` is reserved for nesting queries (AC-40).
    ("/--R-meta-extra/data/input.csv", "header level"),
    // `q` takes no parameters; the planner refuses this query too.
    ("command1-arg/q-invalid", "Too many parameters"),
];

pub(crate) struct Corpus {
    pub(crate) cmr: CommandMetadataRegistry,
    /// (text, parsed query, pipeline)
    pub(crate) entries: Vec<(String, Query, Pipeline)>,
}

pub(crate) fn converted_corpus() -> Corpus {
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    for text in corpus() {
        let q = parse_query(&text).unwrap_or_else(|e| panic!("{text}: {}", e.message));
        match (Pipeline::try_from(&q), REFUSED.iter().find(|(t, _)| *t == text)) {
            (Ok(p), None) => entries.push((text, q, p)),
            (Err(e), Some((_, reason))) => {
                assert!(e.message.contains(reason), "{text}: refused for another reason: {}", e.message)
            }
            (Ok(_), Some(_)) => failures.push(format!("{text}: expected a refusal")),
            (Err(e), None) => failures.push(format!("{text}: {}", e.message)),
        }
    }
    assert!(failures.is_empty(), "failures:\n  {}", failures.join("\n  "));
    let pipelines: Vec<Pipeline> = entries.iter().map(|(_, _, p)| p.clone()).collect();
    Corpus {
        cmr: permissive_registry(&pipelines),
        entries,
    }
}

/// AC-25 — every corpus query converts to a pipeline, except the stated refusals.
#[test]
fn every_corpus_query_converts() {
    let corpus = converted_corpus();
    assert!(corpus.entries.len() > 100, "the corpus is the parser's 147 queries");
}

/// AC-28 — every pipeline that came from a query converts back.
#[test]
fn query_origin_pipeline_converts_back() {
    let corpus = converted_corpus();
    let mut failures = Vec::new();
    for (text, _, p) in corpus.entries.iter() {
        if let Err(e) = p.to_query_with(&corpus.cmr) {
            failures.push(format!("{text}: {}", e.message));
        }
    }
    assert!(failures.is_empty(), "failures:\n  {}", failures.join("\n  "));
}

/// AC-27 — the canonical form is a fixed point, as query and as pipeline.
#[test]
fn round_trip_is_idempotent() -> Result<(), Error> {
    let corpus = converted_corpus();
    for (text, _, p) in corpus.entries.iter() {
        let once = p.to_query_with(&corpus.cmr)?;
        let p2 = Pipeline::try_from(&once)?;
        let twice = p2.to_query_with(&corpus.cmr)?;
        assert_eq!(twice.encode(), once.encode(), "{text}");
        assert_eq!(Pipeline::try_from(&twice)?, p2, "{text}");
    }
    Ok(())
}

/// AC-26 — the round trip gives the canonical form; a canonical query comes back unchanged.
#[test]
fn round_trip_gives_canonical_form() -> Result<(), Error> {
    let corpus = converted_corpus();
    let canonical = |text: &str| -> Result<String, Error> {
        let p = Pipeline::from_query_text(text)?;
        Ok(p.to_query_with(&corpus.cmr)?.encode())
    };
    for (written, expected) in [
        // Long spellings of the resource header instructions.
        ("-R-bin/data.txt", "-R-binary/data.txt"),
        ("-R-meta/-/dr", "-R-metadata/-/dr"),
        ("-R-stored/./identity", "-R-stored_binary/./identity"),
        // Resource shorthand is written out.
        ("a/b/-/c/d", "-R/a/b/-/c/d"),
        // Consecutive segments of one realm merge.
        ("-R/abc/def/-q/xxx/-q/qqq", "-R/abc/def/-q/xxx/qqq"),
        // An empty header on a lone first segment is dropped; with more segments it stays, so
        // the text cannot read back as a resource shorthand.
        ("/-/applied", "/applied"),
        ("abc/def/-/xxx/-q/qqq", "-/abc/def/xxx/-q/qqq"),
        // An `ns` nothing follows has no effect.
        ("ns-abc", ""),
        // A `q` keeps the `ns` after it: the quoted prefix is unchanged.
        ("yyy/q/ns-lui/query_console", "yyy/q/ns-lui/query_console"),
    ] {
        assert_eq!(canonical(written)?, expected, "{written}");
    }
    // A query already in canonical form comes back as the same text.
    for (text, q, p) in corpus.entries.iter() {
        let once = p.to_query_with(&corpus.cmr)?;
        let canonical_text = Pipeline::try_from(&once)?.to_query_with(&corpus.cmr)?.encode();
        if q.encode() == canonical_text {
            assert_eq!(once.encode(), q.encode(), "{text}");
        }
    }
    Ok(())
}

/// A registry of small typed commands.
fn typed_registry() -> CommandMetadataRegistry {
    let mut cmr = CommandMetadataRegistry::new();
    let mut add = |realm: &str, ns: &str, name: &str, args: Vec<ArgumentInfo>| {
        let mut cm = CommandMetadata::new(name);
        cm.with_realm(realm).with_namespace(ns);
        for a in args {
            cm.with_argument(a);
        }
        cmr.add_command(&cm);
    };
    add("", "", "text", vec![]);
    add(
        "",
        "",
        "pick",
        vec![
            ArgumentInfo::string_argument("column"),
            ArgumentInfo::integer_argument("n", false).with_default(1),
            ArgumentInfo::float_argument("scale", false).with_default(1.0),
            ArgumentInfo::boolean_argument("flag").with_default(false),
        ],
    );
    add(
        "",
        "",
        "tag",
        vec![
            ArgumentInfo::string_argument("prefix"),
            ArgumentInfo::string_argument("items").set_multiple(),
        ],
    );
    add("", "", "join", vec![ArgumentInfo::any_argument("other")]);
    let mut env = ArgumentInfo::any_argument("env");
    env.injected = true;
    add(
        "",
        "",
        "inj",
        vec![
            ArgumentInfo::string_argument("a"),
            env,
            ArgumentInfo::string_argument("b").with_default("x"),
        ],
    );
    add("", "", "req", vec![ArgumentInfo::string_argument("a"), ArgumentInfo::string_argument("b")]);
    add("", "", "head", vec![ArgumentInfo::integer_argument("n", false).with_default(10)]);
    add("", "pl", "head", vec![ArgumentInfo::integer_argument("n", false).with_default(5)]);
    add("", "pl", "from_csv", vec![]);
    add("", "img", "head", vec![ArgumentInfo::integer_argument("n", false)]);
    add("", "img", "blur", vec![]);
    add("c", "", "show", vec![]);
    add("c", "", "join", vec![ArgumentInfo::any_argument("other")]);
    cmr
}

fn action(name: &str) -> ActionElement {
    ActionElement::new(name)
}
fn el(kind: ElementKind) -> PipelineElement {
    Located::new(kind)
}
fn act(a: ActionElement) -> PipelineElement {
    el(ElementKind::Action(a))
}
fn arg(v: Json) -> Argument {
    Located::new(Source::Value(v))
}

/// S1, AC-25, AC-27 — through the registry, a text parameter takes its argument's type and
/// keeps it; without the registry it stays text.
#[test]
fn typed_scalars_keep_type_through_registry() -> Result<(), Error> {
    let cmr = typed_registry();
    let syntactic = Pipeline::from_query_text("pick-col-3-0.1-t")?;
    match &syntactic.elements[0].item {
        ElementKind::Action(a) => assert_eq!(a.parameters[1].item, Source::Value(json!("3"))),
        other => panic!("{other:?}"),
    }
    let typed = syntactic.typed_with(&cmr);
    let ElementKind::Action(a) = &typed.elements[0].item else {
        panic!("not an action");
    };
    let values: Vec<_> = a.parameters.iter().map(|p| p.item.clone()).collect();
    assert_eq!(
        values,
        vec![
            Source::Value(json!("col")),
            Source::Value(json!(3)),
            Source::Value(json!(0.1)),
            Source::Value(json!(true)),
        ]
    );
    // The typed pipeline converts to the canonical query and back to the same typed pipeline.
    let q = typed.to_query_with(&cmr)?;
    assert_eq!(q.encode(), "pick-col-3-0.1-true");
    assert_eq!(Pipeline::try_from(&q)?.typed_with(&cmr), typed);
    // An exact float survives in shortest round-trip form.
    let mut p = Pipeline::new(vec![act(ActionElement {
        parameters: vec![arg(json!("c")), arg(json!(1)), arg(json!(0.1 + 0.2))],
        ..action("pick")
    })]);
    let q = p.to_query_with(&cmr)?;
    assert_eq!(q.encode(), "pick-c-1-0.30000000000000004");
    p = Pipeline::try_from(&q)?.typed_with(&cmr);
    let ElementKind::Action(a) = &p.elements[0].item else {
        panic!("not an action");
    };
    assert_eq!(a.parameters[2].item, Source::Value(json!(0.1 + 0.2)));
    // Empty text stays text: in a query it means the default.
    let typed = Pipeline::from_query_text("pick-c-")?.typed_with(&cmr);
    let ElementKind::Action(a) = &typed.elements[0].item else {
        panic!("not an action");
    };
    assert_eq!(a.parameters[1].item, Source::Value(json!("")));
    Ok(())
}

/// N2 — named arguments become positional; a gap takes the skipped argument's default.
#[test]
fn named_arguments_become_positional() -> Result<(), Error> {
    let cmr = typed_registry();
    let mut a = action("pick");
    a.parameters.push(arg(json!("col")));
    a.arguments.insert("flag".into(), arg(json!(true)));
    let q = Pipeline::new(vec![act(a)]).to_query_with(&cmr)?;
    assert_eq!(q.encode(), "pick-col-1-1.0-true");
    // An injected argument takes no query parameter.
    let mut a = action("inj");
    a.arguments.insert("b".into(), arg(json!("y")));
    a.arguments.insert("a".into(), arg(json!("z")));
    let q = Pipeline::new(vec![act(a)]).to_query_with(&cmr)?;
    assert_eq!(q.encode(), "inj-z-y");
    Ok(())
}

/// Q21 — namespaces: none when the defaults resolve every action; one `ns` listing the
/// namespaces in order of use; else an `ns` before each action that still resolves wrongly.
#[test]
fn namespace_placement_three_steps() -> Result<(), Error> {
    let cmr = typed_registry();
    let with_ns = |name: &str, ns: Option<&str>| {
        let mut a = action(name);
        a.ns = ns.map(Namespaces::from);
        act(a)
    };
    let encode = |elements: Vec<PipelineElement>| -> Result<String, Error> {
        Ok(Pipeline::new(elements).to_query_with(&cmr)?.encode())
    };
    // 1. No ns.
    assert_eq!(encode(vec![with_ns("text", None), with_ns("head", None)])?, "text/head");
    // 2. One ns at the start, in order of first use.
    assert_eq!(
        encode(vec![with_ns("from_csv", Some("pl")), with_ns("blur", Some("img")), with_ns("text", None)])?,
        "ns-pl-img/from_csv/blur/text"
    );
    // 3. `head` exists in root, pl and img: after `ns-pl`, a root `head` needs its own `ns`.
    assert_eq!(
        encode(vec![
            with_ns("head", Some("pl")),
            with_ns("head", Some("img")),
            with_ns("head", None),
        ])?,
        "ns-pl/head/ns-img/head/ns-root/head"
    );
    // The query form resolves to the same commands.
    let p = Pipeline::from_query_text("ns-pl/head/ns-img/head/ns-root/head")?;
    let back = p.to_query_with(&cmr)?;
    assert_eq!(back.encode(), "ns-pl/head/ns-img/head/ns-root/head");
    Ok(())
}

/// AC-40 — consecutive segments of one realm merge; another realm nests; levels are refused.
#[test]
fn realm_segments_merge_and_nest() -> Result<(), Error> {
    let cmr = typed_registry();
    // One pipeline in realm `c`.
    let p = Pipeline::from_query_text("-Rc/file.txt/-c/show")?;
    assert_eq!(p.realm, "c");
    assert_eq!(p.elements.len(), 2);
    assert_eq!(p.to_query_with(&cmr)?.encode(), "-Rc/file.txt/-c/show");
    // A realm change is a nested pipeline, and the default realm resumes after it.
    let p = Pipeline::from_query_text("-R/f.txt/-c/show/-/text")?;
    assert_eq!(p.realm, "");
    assert_eq!(p.elements.len(), 3);
    match &p.elements[1].item {
        ElementKind::Pipeline(np) => {
            assert_eq!(np.realm, "c");
            assert_eq!(np.elements.len(), 1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(p.to_query_with(&cmr)?.encode(), "-R/f.txt/-c/show/-/text");
    // A nested pipeline in its parent's realm is inlined.
    let p = Pipeline::new(vec![
        act(action("text")),
        el(ElementKind::Pipeline(Pipeline::new(vec![act(action("head"))]))),
    ]);
    assert_eq!(p.to_query_with(&cmr)?.encode(), "text/head");
    Ok(())
}

/// AC-40 — a header level other than `-` is refused, naming the segment.
#[test]
fn header_level_is_refused() {
    let err = Pipeline::from_query_text("-R/a/--/text").expect_err("level 1");
    assert!(err.message.contains("segment 1"), "{}", err.message);
    assert!(err.message.contains("header level 1"), "{}", err.message);
}

/// AC-29 — each representability rule, by its id.
#[test]
fn not_representable_each_rule() -> Result<(), Error> {
    let cmr = typed_registry();
    let refused = |p: Pipeline, rule: &str| {
        let err = p.to_query_with(&cmr).expect_err(rule);
        assert!(
            err.message.contains(&format!("({rule})")),
            "expected {rule}: {}",
            err.message
        );
    };
    // P1
    refused(Pipeline::new(vec![el(ElementKind::Value(Operand::Literal(json!(1))))]), "P1");
    refused(
        Pipeline::new(vec![el(ElementKind::Evaluate(Operand::Literal(QueryOrPipeline::Query("text".into()))))]),
        "P1",
    );
    // P2
    refused(
        Pipeline::new(vec![el(ElementKind::Pipeline(Pipeline {
            filename: Some("x.txt".into()),
            ..Pipeline::new(vec![act(action("text"))])
        }))]),
        "P2",
    );
    refused(
        Pipeline {
            filename: Some("noextension".into()),
            ..Pipeline::new(vec![act(action("text"))])
        },
        "P2",
    );
    // P3: carried as they are…
    let p = Pipeline {
        realm: "c".into(),
        header_parameters: vec!["p".into()],
        rooted: true,
        ..Pipeline::new(vec![act(action("show"))])
    };
    assert_eq!(p.to_query_with(&cmr)?.encode(), "/-c-p/show");
    // …but must be header values.
    refused(
        Pipeline {
            realm: "c".into(),
            header_parameters: vec!["a b".into()],
            ..Pipeline::new(vec![act(action("show"))])
        },
        "P3",
    );
    // P4
    refused(
        Pipeline {
            realm: "Upper".into(),
            ..Pipeline::new(vec![act(action("show"))])
        },
        "P4",
    );
    // E1
    refused(Pipeline::new(vec![act(action("ns"))]), "E1");
    refused(Pipeline::new(vec![act(action("two words"))]), "E1");
    // E2
    let mut a = action("text");
    a.ns = Some(Namespaces::from("not ok"));
    refused(Pipeline::new(vec![act(a)]), "E2");
    // E3: a nested pipeline obeys the rules itself.
    refused(
        Pipeline::new(vec![el(ElementKind::Pipeline(Pipeline {
            realm: "c".into(),
            ..Pipeline::new(vec![el(ElementKind::Value(Operand::Literal(json!(1))))])
        }))]),
        "P1",
    );
    // E4
    refused(
        Pipeline::new(vec![el(ElementKind::Resource {
            resource: Operand::Placeholder("k".into()),
            read: Read::Asset,
        })]),
        "E4",
    );
    refused(
        Pipeline::new(vec![el(ElementKind::Resource {
            resource: Operand::Literal("my file.txt".into()),
            read: Read::Asset,
        })]),
        "E4",
    );
    // E5
    refused(Pipeline::new(vec![el(ElementKind::Cwd(Operand::Placeholder("d".into())))]), "E5");
    // E6
    refused(Pipeline::new(vec![act(action("text")), el(ElementKind::Text("/head".into()))]), "E6");
    refused(Pipeline::new(vec![el(ElementKind::Text("a b".into()))]), "E6");
    // E7: always.
    let p = Pipeline::new(vec![act(action("text")), el(ElementKind::Quote), el(ElementKind::Volatile)]);
    assert_eq!(p.to_query_with(&cmr)?.encode(), "text/q/v");
    // E8
    refused(
        Pipeline::new(vec![
            act(action("text")),
            el(ElementKind::QueryValue(Operand::Literal(QueryOrPipeline::Query("head".into())))),
        ]),
        "E8",
    );
    let p = Pipeline::new(vec![el(ElementKind::QueryValue(Operand::Literal(QueryOrPipeline::Query(
        "text/head".into(),
    ))))]);
    assert_eq!(p.to_query_with(&cmr)?.encode(), "text/head/q");
    // S1: always.
    let mut a = action("pick");
    a.parameters = vec![arg(json!("a")), arg(json!(2)), arg(json!(1.5)), arg(json!(false))];
    assert_eq!(Pipeline::new(vec![act(a)]).to_query_with(&cmr)?.encode(), "pick-a-2-1.5-false");
    // S2: a list only for the variadic argument.
    let mut a = action("tag");
    a.parameters = vec![arg(json!("p")), arg(json!(["x", "y"]))];
    assert_eq!(Pipeline::new(vec![act(a)]).to_query_with(&cmr)?.encode(), "tag-p-x-y");
    let mut a = action("tag");
    a.parameters = vec![arg(json!("p")), arg(json!([["nested"]]))];
    refused(Pipeline::new(vec![act(a)]), "S2");
    // S3
    let mut a = action("tag");
    a.parameters.push(arg(json!("p")));
    a.arguments.insert(
        "items".into(),
        Located::new(Source::Multiple(vec![Source::Value(json!("a")), Source::Link("text".into())])),
    );
    assert_eq!(Pipeline::new(vec![act(a)]).to_query_with(&cmr)?.encode(), "tag-p-a-~X~text~E");
    let mut a = action("join");
    a.parameters.push(Located::new(Source::Multiple(vec![Source::Value(json!("a"))])));
    refused(Pipeline::new(vec![act(a)]), "S3");
    // S4
    for v in [json!(null), json!({"a": 1}), json!([1, 2])] {
        let mut a = action("join");
        a.parameters.push(arg(v));
        refused(Pipeline::new(vec![act(a)]), "S4");
    }
    // S5
    let mut a = action("join");
    a.parameters.push(Located::new(Source::Link("a b".into())));
    refused(Pipeline::new(vec![act(a)]), "S5");
    // S6
    let mut a = action("join");
    a.parameters.push(Located::new(Source::Pipeline(Pipeline::new(vec![el(ElementKind::Value(
        Operand::Literal(json!(1)),
    ))]))));
    refused(Pipeline::new(vec![act(a)]), "S6");
    let mut a = action("join");
    a.parameters.push(Located::new(Source::Pipeline(Pipeline::new(vec![act(action("text"))]))));
    assert_eq!(Pipeline::new(vec![act(a)]).to_query_with(&cmr)?.encode(), "join-~X~text~E");
    // S7
    for s in [Source::Placeholder("x".into()), Source::State(true)] {
        let mut a = action("join");
        a.parameters.push(Located::new(s));
        refused(Pipeline::new(vec![act(a)]), "S7");
    }
    // N1: an unknown command cannot be written.
    let err = Pipeline::new(vec![act(action("nowhere"))]).to_query_with(&cmr).expect_err("N1");
    assert!(err.message.contains("not registered"), "{}", err.message);
    // N3: a gap at a required argument.
    let mut a = action("req");
    a.arguments.insert("b".into(), arg(json!("y")));
    refused(Pipeline::new(vec![act(a)]), "N3");
    // N4: by position and by name.
    let mut a = action("req");
    a.parameters.push(arg(json!("x")));
    a.arguments.insert("a".into(), arg(json!("y")));
    let err = Pipeline::new(vec![act(a)]).to_query_with(&cmr).expect_err("N4");
    assert!(err.message.contains("both by position and by name"), "{}", err.message);
    // Unknown and injected arguments.
    let mut a = action("inj");
    a.arguments.insert("env".into(), arg(json!(1)));
    let err = Pipeline::new(vec![act(a)]).to_query_with(&cmr).expect_err("injected");
    assert!(err.message.contains("injected"), "{}", err.message);
    Ok(())
}

/// The error names the element path and carries the position.
#[test]
fn not_representable_names_the_path() {
    let cmr = typed_registry();
    let pos = crate::query::Position {
        offset: 7,
        line: 1,
        column: 8,
    };
    let mut inner = action("join");
    inner.arguments.insert("other".into(), Located::new(Source::State(true)));
    let p = Pipeline::new(vec![
        act(action("text")),
        el(ElementKind::Pipeline(Pipeline {
            realm: "c".into(),
            ..Pipeline::new(vec![act(action("show")), Located::at(ElementKind::Action(inner), &pos)])
        })),
    ]);
    let err = p.to_query_with(&cmr).expect_err("S7");
    assert!(
        err.message.contains("nested pipeline 1, element 1, argument 'other'"),
        "{}",
        err.message
    );
    assert_eq!(err.position, pos);
}

/// AC-30 — a recipe is its cwd, volatility and query, with overrides as arguments of the query's
/// last action.
#[test]
fn recipe_to_pipeline_concatenates() -> Result<(), Error> {
    let cmr = typed_registry();
    let mut recipe = Recipe::new("text/pick-a-2/out.txt".into(), String::new(), String::new())?;
    recipe.arguments.insert("n".into(), json!(7));
    recipe.arguments.insert("flag".into(), json!(true));
    recipe.cwd = Some("data".into());
    recipe.volatile = true;
    let p = recipe.to_pipeline(&cmr)?;
    assert_eq!(p.filename.as_deref(), Some("out.txt"));
    assert_eq!(p.elements[0].item, ElementKind::Cwd(Operand::Literal("data".into())));
    assert_eq!(p.elements[1].item, ElementKind::Volatile);
    let ElementKind::Action(pick) = &p.elements[3].item else {
        panic!("not an action");
    };
    // `n` was given by position, so the override replaces it; `flag` is named.
    assert_eq!(pick.parameters[1].item, Source::Value(json!(7)));
    assert_eq!(pick.arguments["flag"].item, Source::Value(json!(true)));
    assert_eq!(
        p.to_query_with(&cmr)?.encode(),
        "-R-cwd/data/-/v/text/pick-a-7-1.0-true/out.txt"
    );
    Ok(())
}

/// AC-18 — a link-like parameter has a pipeline; a value does not.
#[test]
fn parameter_value_to_pipeline() -> Result<(), Error> {
    use crate::plan::ParameterValue as P;
    use crate::query::Position;
    let q = parse_query("-R/data/x.csv/-/head-5")?;
    let from_query = Pipeline::try_from(&q)?;
    for p in [
        P::DefaultLink("a".into(), q.clone()),
        P::ParameterLink("a".into(), q.clone(), Position::unknown()),
        P::OverrideLink("a".into(), q.clone()),
        P::EnumLink("a".into(), q.clone(), Position::unknown()),
    ] {
        assert_eq!(Pipeline::try_from(&p)?, from_query, "{p}");
    }
    let nested = Pipeline::from_json(&json!([{"value": [1, 2]}, {"action": "sum"}]))?;
    for p in [
        P::ParameterPipeline("a".into(), nested.clone(), Position::unknown()),
        P::OverridePipeline("a".into(), nested.clone()),
        P::DefaultPipeline("a".into(), nested.clone()),
    ] {
        assert_eq!(Pipeline::try_from(&p)?, nested, "{p}");
    }
    for p in [
        P::DefaultValue("a".into(), json!(1)),
        P::ParameterValue("a".into(), json!("x"), Position::unknown()),
        P::OverrideValue("a".into(), json!(true)),
        P::Placeholder("a".into()),
        P::Injected("a".into()),
        P::InputState("a".into()),
        P::MultipleParameters("a".into(), vec![]),
        P::None,
    ] {
        let err = Pipeline::try_from(&p).expect_err("not link-like");
        assert!(err.message.contains("only a link or a pipeline"), "{}", err.message);
    }
    Ok(())
}

/// AC-21 — a pipeline default survives the registry's JSON and YAML forms, written tagged.
#[test]
fn registry_export_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    use crate::command_metadata::CommandParameterValue;
    use crate::plan::ParameterValue;
    let nested = Pipeline::from_json(&json!([{"value": {"region": ["EU"]}}, {"action": "keys"}]))?;
    let mut meta = CommandMetadata::new("report");
    meta.arguments.push(ArgumentInfo::any_argument("source").with_default_pipeline(nested.clone())?);
    let mut cmr = CommandMetadataRegistry::new();
    cmr.add_command(&meta);

    let text = serde_json::to_string(&cmr)?;
    assert!(text.contains(r#""default":{"pipeline":"#), "{text}");
    let back: CommandMetadataRegistry = serde_json::from_str(&text)?;
    assert_eq!(back.commands[0].arguments, cmr.commands[0].arguments);
    let text = serde_yaml::to_string(&cmr)?;
    let back: CommandMetadataRegistry = serde_yaml::from_str(&text)?;
    assert_eq!(back.commands[0].arguments, cmr.commands[0].arguments, "{text}");

    // The hand-written map form and the capitalised tag read the same.
    for written in [json!({"pipeline": nested.to_json()}), json!({"Pipeline": nested.to_json()})] {
        let cpv: CommandParameterValue = serde_json::from_value(written)?;
        assert_eq!(cpv, CommandParameterValue::Pipeline(nested.clone()));
    }
    // A bare list stays a value.
    let cpv: CommandParameterValue = serde_json::from_value(json!([1, 2]))?;
    assert_eq!(cpv, CommandParameterValue::Value(json!([1, 2])));

    // Planned as `DefaultPipeline`.
    match ParameterValue::from_arginfo(&meta.arguments[0]) {
        ParameterValue::DefaultPipeline(name, p) => {
            assert_eq!(name, "source");
            assert_eq!(p, nested);
        }
        other => panic!("expected a default pipeline, got {other}"),
    }

    // A default is not a definition: placeholders and the input state are refused.
    let with_placeholder = Pipeline::from_json(&json!([{"value": {"placeholder": "n"}}]))?;
    assert!(ArgumentInfo::any_argument("x").with_default_pipeline(with_placeholder).is_err());
    let with_state = Pipeline::from_json(&json!([{"action": "f", "arguments": {"s": {"state": true}}}]))?;
    assert!(ArgumentInfo::any_argument("x").with_default_pipeline(with_state).is_err());
    Ok(())
}
