---
title: Templates
status: ready
sources: [reference/TEMPLATE_LANGUAGE.md, guides/TEMPLATE_GUIDE.md]
reviewed: 2026-11-10
---
# 10. Templates

PURPOSE: The reader can write a Mustache template with query tags, render it through a query and from Rust code, and make a value readable by templates.

{{=<% %>=}}
<%> ../_shared/templates.md%>

## 10.3 Rendering through a query

Rendering is done by two commands of the library, so in Rust the first way to render is to
evaluate a query:

```rust
    fn number(n: i64) -> Result<Value, Error> {
        Ok(Value::from(n))
    }
    fn place(region: String) -> Result<Value, Error> {
        Value::try_from_json_value(&serde_json::json!({ "region": region }))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let report = b"# Sales in {{region}}
{{#$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
- {{month}}: {{amount}}
{{/}}
{{^$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
No sales.
{{/}}
";
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    store.set(&parse_key("templates/greet.md")?, b"Hello, {{.}}!", &Metadata::new()).await?;
    store.set(&parse_key("templates/report.md")?, report, &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn number(n: i64) -> result)?;
    register_command!(cr, fn place(region: String) -> result)?;
    let env = builder.build()?;

    let text = |query: &'static str| {
        let env = env.clone();
        async move { env.evaluate(query).await?.get().await?.try_into_string() }
    };

    // render: the template is an argument, the input is what `{{.}}` and plain names read.
    assert_eq!(
        text("number-7/ns-template/render-~X~-R/templates/greet.md~E").await?,
        "Hello, 7!"
    );
    assert_eq!(
        text("place-EU/ns-template/render-~X~-R/templates/report.md~E").await?,
        "# Sales in EU\n- jan: 120\n- feb: 95\n"
    );

    // expand: the template is the input, the data is an argument.
    assert_eq!(
        text("-R/templates/report.md/-/ns-template/expand-~X~place-ASIA~E").await?,
        "# Sales in ASIA\nNo sales.\n"
    );
```

The first query renders a template with a number, which the template reads as `{{.}}`. The
second renders a report with an object, whose field `region` the template reads by name and also
places into its own queries. The third turns the roles around with `expand`.

The value of `region` is placed into the inner query as one escaped argument. A region called
`EU/-/delete_everything` would be looked up as a region of that name, and would find no rows. A
template cannot be made to run a query that its author did not write.

A render is the value of a query like any other, so it is cached. It depends on the template, on
every query in the template and on the data, and it is rendered again when one of them changes.
If one of the template's queries is volatile, the render is volatile too.

## 10.4 When a tag fails

The `errors` argument of `render` and `expand` chooses what a failing tag does:

```rust
let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
let draft = b"line one\n{{$ no_such_command}} end";
store.set(&parse_key("templates/draft.md")?, draft, &Metadata::new()).await?;
let env = library_environment(store)?;

// strict, the default: the render fails with the error of the tag.
let strict = env.evaluate("-R/templates/draft.md/-/ns-template/expand").await?.get().await?;
let error = strict.value().err().expect("a failing tag");
assert_eq!(error.error_type, ErrorType::ActionNotRegistered);
assert!(error.message.contains("no_such_command"));

// silent: the failing tag renders as nothing, and the log has a warning.
let silent = env.evaluate("-R/templates/draft.md/-/ns-template/expand--silent").await?.get().await?;
assert_eq!(silent.try_into_string()?, "line one\n end");

// validate: checks every tag and runs nothing.
let problems = env.evaluate("-R/templates/draft.md/-/ns-template/validate").await?.get().await?;
let problems = problems.value()?.try_into_json_value()?; // a list, one entry for each problem
assert!(problems[0]["message"].to_string().contains("no_such_command"));
```

Use `strict`, the default, while you write a template. Use `expand_md` or `expand_html` to see
all failures at once in the output, and `silent` when an incomplete text is better than none.
`validate` is the check to run before a template is committed or deployed: it lists every
malformed tag and every unknown command with its position in the template, and it evaluates
nothing.

## 10.5 Template commands

A template that is rendered often can become a command, with no function written for it. It is
an alias of `template/render` with the template fixed, as section 5.9 showed:

```rust
// Sketch: the registration of a template command, from section 5.9.
cr.register_alias(
    CommandKey::new("", "sales", "count_text"),
    CommandKey::new("", "template", "render"),
    vec![CommandParameterValue::Query(parse_query("-R/templates/count.txt")?)],
    vec![
        ArgumentInfo::any_argument("data").with_default(""),
        ArgumentInfo::string_argument("errors").with_default("strict"),
        ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?),
    ],
)?;
```

The first list fixes the leading argument of `render`, the template, to a link. The second list
declares what is left for the caller, in the order of `render`'s remaining arguments. Because
the template is a link, it is a dependency of every result of the command.

Write a template command when the result is text assembled from other queries. Write a function
when the command computes.

## 10.6 Rendering from Rust code

To render from a program without going through a query, describe the render in a
`RenderRequest`:

```rust
use liquers_lib::template::{
    render_top_level, ErrorMode, FrameData, FrameKind, RenderRequest, TemplateSource,
};

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("templates/header.md")?, b"Report for {{name}}", &Metadata::new()).await?;
let page = b"{{> ./header.md}}: {{$ -R/data/sales.csv/-/ns-rec/to_record/nrows}} rows";
store.set(&parse_key("templates/page.md")?, page, &Metadata::new()).await?;
store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
let env = library_environment(store)?;

let data = State::new().with_data(Value::try_from_json_value(&serde_json::json!({"name": "Ann"}))?);
let request = RenderRequest {
    template: TemplateSource::Link(parse_query("-R/templates/page.md")?),
    frames: vec![(FrameData::State(data), FrameKind::Data)],
    errors: ErrorMode::Strict,
    cwd: parse_key("templates")?, // where `./header.md` is looked up
    html_escape: false,
};
let render = render_top_level(&env, request)?; // a parse error fails here
let rendered = render.run().await?.expect("not cancelled");
assert_eq!(rendered.text, "Report for Ann: 3 rows");
assert!(rendered.warnings.is_empty());
```

`render_top_level` creates an asset for the render, so everything in chapter 7 applies to it:
the queries of the template run as its dependencies, and `render.asset()` gives the `AssetRef`
with which you can watch or cancel it. `run()` returns the text and the warnings of the error
mode, or `None` when the render was cancelled.

The template is `TemplateSource::Link(query)` for a stored one or `TemplateSource::Text(text)`.
The `frames` are the data: a list, because a template can look names up in several layers. `cwd`
is the folder against which a relative partial such as `{{> ./header.md}}` is resolved.

Inside a command, use `render_in_context(context, request)` instead. The queries of the template
then become dependencies of the command's own result.

The template language itself has no need of an environment. The crate `liquers-template` parses
and renders with any *host* that can evaluate a query and find a partial. Its `MapHost` holds
partials in a map and refuses queries, which is enough for plain Mustache:

```rust
use liquers_core::data_node::AnyNode;
use liquers_template::{
    parse_template, render, ContextStack, FrameKind, MapHost, RenderOptions, TemplateSyntax,
};

let template = parse_template("Hello {{name}}{{#items}}, {{.}}{{/items}}", TemplateSyntax::Mustache)?;
let mut stack = ContextStack::new();
stack.push(
    AnyNode::from_json(serde_json::json!({"name": "world", "items": [1, 2]})),
    FrameKind::Data,
);
let text = render(&template, stack, &MapHost::new(), &RenderOptions::default()).await?;
assert_eq!(text, "Hello world, 1, 2");
```

## 10.7 Making a value readable by templates

A template reads its data field by field: `{{customer.name}}`, a section over a list, a row of a
table. It does so through a *data node*, a read-only view of a value that answers three
questions: what kind of thing are you, what are your fields or items, what is your scalar.

The values of the library have their nodes. JSON-like values are read as what they are. A record
view is read as a list of rows, each row a map from column name to cell, without converting the
table. A folder of the store is read as a map of its files.

A value type of your own (section 4.5) becomes readable by implementing `data_node` for it,
returning an `AnyNode::Sync` with a `DataNode` when the data is in memory, or an
`AnyNode::Async` with an `AsyncDataNode` when it is read on demand. Without it, the value is
opaque to templates: it can be passed along, and not looked into.
<%={{ }}=%>
