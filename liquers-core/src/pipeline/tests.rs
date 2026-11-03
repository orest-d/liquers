//! Tests for the pipeline types and their serialized form.

use super::*;
use serde_json::json;

/// One pipeline holding every element kind and every source kind.
fn every_kind() -> Pipeline {
    let mut action = ActionElement::new("is_in");
    action.ns = Some(Namespaces::from(["pl", "polars"]));
    action.parameters = vec![
        Located::new(Source::Value(json!("region"))),
        Located::new(Source::Value(json!(1.5))),
    ];
    action.arguments.insert("values".into(), Located::new(Source::Value(json!(["EU", "US"]))));
    action.arguments.insert("config".into(), Located::new(Source::Value(json!({"a": 1}))));
    action.arguments.insert("n".into(), Located::new(Source::Placeholder("n".into())));
    action.arguments.insert("link".into(), Located::new(Source::Link("-R/x.txt".into())));
    action.arguments.insert(
        "nested".into(),
        Located::new(Source::Pipeline(Pipeline::new(vec![Located::new(ElementKind::Action(
            ActionElement::new("inner"),
        ))]))),
    );
    action.arguments.insert("input".into(), Located::new(Source::State(true)));
    action.arguments.insert(
        "items".into(),
        Located::new(Source::Multiple(vec![
            Source::Value(json!("a")),
            Source::Placeholder("b".into()),
        ])),
    );
    action.arguments.insert(
        "tricky".into(),
        Located::new(Source::Value(json!({"placeholder": "not one"}))),
    );
    Pipeline {
        realm: "web".into(),
        header_parameters: vec!["p".into()],
        rooted: true,
        elements: vec![
            Located::new(ElementKind::Text("-R/data/sales.csv/-/from_csv".into())),
            Located::new(ElementKind::Action(action)),
            Located::new(ElementKind::Value(Operand::Literal(json!({"k": [1, 2, null]})))),
            Located::new(ElementKind::Value(Operand::Placeholder("v".into()))),
            Located::new(ElementKind::Resource {
                resource: Operand::Literal("data/x.csv".into()),
                read: Read::Metadata,
            }),
            Located::new(ElementKind::Resource {
                resource: Operand::Placeholder("key".into()),
                read: Read::Asset,
            }),
            Located::new(ElementKind::KeyValue(Operand::Literal("a/b".into()))),
            Located::new(ElementKind::Cwd(Operand::Literal("a".into()))),
            Located::new(ElementKind::Evaluate(Operand::Literal(QueryOrPipeline::Query(
                "-R/y".into(),
            )))),
            Located::new(ElementKind::QueryValue(Operand::Literal(QueryOrPipeline::Pipeline(
                Pipeline::new(vec![Located::new(ElementKind::Quote)]),
            )))),
            Located::new(ElementKind::Quote),
            Located::new(ElementKind::Volatile),
            Located::new(ElementKind::Pipeline(Pipeline {
                realm: "other".into(),
                ..Pipeline::new(vec![Located::new(ElementKind::Action(ActionElement::new("x")))])
            })),
        ],
        filename: Some("out.csv".into()),
    }
}

/// AC-21 — every element and source kind survives JSON, YAML and TOML.
#[test]
fn pipeline_serde_round_trip_json_yaml_toml() -> Result<(), Box<dyn std::error::Error>> {
    let p = every_kind();

    let text = serde_json::to_string(&p)?;
    let back: Pipeline = serde_json::from_str(&text)?;
    assert_eq!(back, p, "JSON: {text}");

    let text = serde_yaml::to_string(&p)?;
    let back: Pipeline = serde_yaml::from_str(&text)?;
    assert_eq!(back, p, "YAML: {text}");

    // TOML has no null, so the one null in the fixture is dropped for it.
    let mut no_null = p.clone();
    no_null.elements[2] =
        Located::new(ElementKind::Value(Operand::Literal(json!({"k": [1, 2]}))));
    let text = toml::to_string(&no_null)?;
    let back: Pipeline = toml::from_str(&text)?;
    assert_eq!(back, no_null, "TOML: {text}");
    Ok(())
}

/// Positions survive serialization, and equality ignores them.
#[test]
fn positions_are_serialized_and_ignored_by_equality() -> Result<(), Box<dyn std::error::Error>> {
    let pos = Position {
        offset: 3,
        line: 1,
        column: 4,
    };
    let mut action = ActionElement::new("a");
    action.parameters.push(Located::at(Source::Value(json!("x")), &pos));
    let p = Pipeline::new(vec![Located::at(ElementKind::Action(action), &pos)]);
    let v = p.to_json();
    assert_eq!(v["elements"][0]["pos"]["column"], json!(4));
    assert_eq!(v["elements"][0]["parameters"][0], json!({"value": "x", "pos": {"offset": 3, "line": 1, "column": 4}}));
    let back = Pipeline::from_json(&v)?;
    assert_eq!(back.elements[0].pos, Some(pos));
    let mut unpositioned = back.clone();
    unpositioned.elements[0].pos = None;
    assert_eq!(unpositioned, back);
    Ok(())
}

/// AC-21 — a bare list is a pipeline with only `elements`.
#[test]
fn bare_list_is_a_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let p: Pipeline = serde_yaml::from_str(
        "- \"-R/data/sales.csv/-/ns-pl/from_csv\"\n- {action: head, arguments: {n: 5}}\n",
    )?;
    assert_eq!(p.realm, "");
    assert_eq!(p.elements.len(), 2);
    assert_eq!(
        p.elements[0].item,
        ElementKind::Text("-R/data/sales.csv/-/ns-pl/from_csv".into())
    );
    match &p.elements[1].item {
        ElementKind::Action(a) => {
            assert_eq!(a.action, "head");
            assert_eq!(a.arguments["n"].item, Source::Value(json!(5)));
        }
        other => panic!("expected an action, got {other:?}"),
    }
    Ok(())
}

/// AC-21 — an unknown key is an error naming the key and the element index.
#[test]
fn unknown_element_key_names_the_index() {
    let err = Pipeline::from_json(&json!(["a", {"action": "b", "colour": "red"}]))
        .expect_err("unknown key");
    assert!(err.message.contains("element 1"), "{}", err.message);
    assert!(err.message.contains("'colour'"), "{}", err.message);

    let err = Pipeline::from_json(&json!([{"frobnicate": 1}])).expect_err("no element key");
    assert!(err.message.contains("element 0"), "{}", err.message);

    let err = Pipeline::from_json(&json!([{"action": "a", "value": 1}])).expect_err("two kinds");
    assert!(err.message.contains("more than one element key"), "{}", err.message);

    let err = Pipeline::from_json(&json!({"elements": [], "title": "x"})).expect_err("pipeline key");
    assert!(err.message.contains("'title'"), "{}", err.message);

    let err = Pipeline::from_json(&json!([{"resource": "a", "read": "sideways"}]))
        .expect_err("unknown read");
    assert!(err.message.contains("sideways"), "{}", err.message);
}

/// `read` accepts the header instructions' short spellings and writes the long ones.
#[test]
fn read_accepts_short_spellings() -> Result<(), Box<dyn std::error::Error>> {
    for (short, read) in [
        ("b", Read::Binary),
        ("meta", Read::Metadata),
        ("r", Read::Recipe),
        ("dir", Read::Directory),
        ("sbin", Read::Stored),
        ("stored_meta", Read::StoredMetadata),
        ("sdir", Read::StoredDirectory),
        ("data", Read::Asset),
    ] {
        let p = Pipeline::from_json(&json!([{"resource": "a/b", "read": short}]))?;
        assert_eq!(
            p.elements[0].item,
            ElementKind::Resource {
                resource: Operand::Literal("a/b".into()),
                read
            }
        );
        let written = p.to_json();
        match read.instruction() {
            Some(long) => assert_eq!(written["elements"][0]["read"], json!(long)),
            None => assert!(written["elements"][0].get("read").is_none()),
        }
    }
    Ok(())
}

/// A literal object shaped like a tagged source is written inside `{value: …}`.
#[test]
fn literal_objects_are_not_mistaken_for_sources() -> Result<(), Box<dyn std::error::Error>> {
    let s = Source::Value(json!({"link": "not a link"}));
    let v = s.to_json();
    assert_eq!(v, json!({"value": {"link": "not a link"}}));
    assert_eq!(Source::from_json(&v)?, s);
    // A bare object without a tag key is a literal.
    assert_eq!(
        Source::from_json(&json!({"region": ["EU"]}))?,
        Source::Value(json!({"region": ["EU"]}))
    );
    Ok(())
}

/// AC-38 — binding replaces every placeholder, nested ones included.
#[test]
fn bind_replaces_every_placeholder() -> Result<(), Box<dyn std::error::Error>> {
    let p = every_kind();
    assert_eq!(
        p.placeholders().into_iter().collect::<Vec<_>>(),
        vec!["b", "key", "n", "v"]
    );
    let values: BTreeMap<String, Json> = [
        ("b", json!("bee")),
        ("key", json!("data/k.csv")),
        ("n", json!(5)),
        ("v", json!({"x": 1})),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let bound = p.bind(&values)?;
    assert!(bound.placeholders().is_empty());
    assert!(bound.uses_input_state(), "the input state is not a placeholder");
    assert_eq!(
        bound.elements[5].item,
        ElementKind::Resource {
            resource: Operand::Literal("data/k.csv".into()),
            read: Read::Asset
        }
    );
    match &bound.elements[1].item {
        ElementKind::Action(a) => {
            assert_eq!(a.arguments["n"].item, Source::Value(json!(5)));
            assert_eq!(
                a.arguments["items"].item,
                Source::Multiple(vec![Source::Value(json!("a")), Source::Value(json!("bee"))])
            );
        }
        other => panic!("expected an action, got {other:?}"),
    }
    Ok(())
}

/// AC-38 — an unbound placeholder is named; a text operand refuses a non-string.
#[test]
fn bind_reports_unbound_placeholder() {
    let p = every_kind();
    let err = p.bind(&BTreeMap::new()).expect_err("unbound");
    assert!(err.message.contains("is not bound"), "{}", err.message);

    let p = Pipeline::from_json(&json!([{"resource": {"placeholder": "k"}}])).expect("parses");
    let values: BTreeMap<String, Json> = [("k".to_owned(), json!(3))].into_iter().collect();
    let err = p.bind(&values).expect_err("not text");
    assert!(err.message.contains("must be text"), "{}", err.message);
}

/// AC-21 — `0.1 + 0.2` and the non-finite tokens survive exactly.
#[test]
fn non_finite_and_exact_floats_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let exact = 0.1_f64 + 0.2_f64;
    let p = Pipeline::new(vec![Located::new(ElementKind::Value(Operand::Literal(json!([
        exact,
        f64::MAX,
        f64::MIN_POSITIVE,
        crate::value::json_f64::NAN,
        crate::value::json_f64::INFINITY,
        crate::value::json_f64::NEG_INFINITY,
    ]))))]);
    for text in [serde_json::to_string(&p)?, serde_yaml::to_string(&p)?] {
        let back: Pipeline = if text.starts_with('{') {
            serde_json::from_str(&text)?
        } else {
            serde_yaml::from_str(&text)?
        };
        assert_eq!(back, p, "{text}");
        match &back.elements[0].item {
            ElementKind::Value(Operand::Literal(v)) => {
                assert_eq!(v[0].as_f64(), Some(exact));
                assert_eq!(v[1].as_f64(), Some(f64::MAX));
            }
            other => panic!("expected a value, got {other:?}"),
        }
    }
    Ok(())
}
