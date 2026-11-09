---
title: Pipeline Guide
kind: guide
audience: internal
area: [core/commands, core/plan, core/query, core/validate]
reviewed: 2026-11-04
---
# Pipeline Guide

How to build, convert, edit and validate pipelines, how to define a pipeline command, and how to
give a recipe a pipeline. What a pipeline *is*, its serialized form and the exact rules are in
[`reference/PIPELINES.md`](../reference/PIPELINES.md); this guide is the how-to.

## 1. Which tool to reach for

| You need | Use |
|---|---|
| A query built in code, values escaped for you | `QueryEditor` (§3) |
| An argument a query cannot hold (a JSON object, a named argument, a nested pipeline) | a pipeline, evaluated with `evaluate_pipeline` (§2) |
| A command made of other commands, with arguments | a pipeline command (§4) |
| A command that is one other command with some arguments fixed | an alias (`reference/COMMAND_ALIASES.md`) |
| A recipe whose first steps need structured values | a recipe `pipeline` (§5) |
| A query typed on a shell without escaping | `liquers-validate --build` (§6) |

## 2. Writing a pipeline and evaluating it

A pipeline is usually written as JSON or YAML and read with `Pipeline::from_json`:

```rust
use liquers_core::pipeline::Pipeline;
use serde_json::json;

let pipeline = Pipeline::from_json(&json!([
    "-R/data/sales.csv/-/ns-pl/from_csv",                  // query text, inlined
    {"action": "is_in", "ns": "pl",
     "arguments": {"column": "region", "values": ["EU", "US"]}},
    {"action": "head", "ns": "pl", "parameters": [3]}
]))?;
let state = liquers_core::interpreter::evaluate_pipeline(envref.clone(), &pipeline, None).await?;
```

- Mix query text and structured elements freely: a bare string is query text in place.
- Give an action no `ns` unless it needs one: an element without `ns` resolves through the default
  namespaces only.
- Prefer `arguments` (by name) over `parameters` when the meaning is not obvious from position.
- A value that the query language cannot write goes in as JSON: `{"value": {"a": [1, 2]}}` as an
  element sets the state, and an object in `arguments` reaches an `any` argument whole.

To see the plan without running it: `pipeline.to_plan(&cmr)`. Planning errors name the element's
index and, for a pipeline converted from a query, its position.

## 3. Converting and editing

```rust
let pipeline = Pipeline::try_from(&parse_query("-R/data/x.txt/-/upper")?)?;   // always works
let query = pipeline.to_query_with(&cmr)?;                                     // may refuse
```

`to_query_with` fails with `Error::not_representable`, a `ParameterError` whose message reads
"Pipeline is not representable as a query (<rule>) at <path>: …", naming the rule (P1–N4 in the
reference) and the element path. When it does:

1. Read the rule. The usual ones are P1 (a `value` element), S4 (an object or a list for a single
   argument) and S7 (a placeholder).
2. Either keep the pipeline (evaluate it with `evaluate_pipeline`, store it as `Value::Pipeline`),
   or move the structured value out: store it under a key and pass a link to it.

`to_query_text` / `from_query_text` do the same without a registry; use them only where no
registry exists, since named arguments cannot then be placed.

To build a query, use the editor instead of formatting text. It escapes values, checks every name
against a registry snapshot and refuses a bad edit without changing anything:

```rust
let mut q = QueryEditor::new(envref.command_metadata_snapshot());
q.resource("data/sales.csv", Default::default())?;
q.action("is_in")?.ns("pl")?.set("column", "region")?.push("EU")?.push("US")?;
assert_eq!(q.query().encode(), "-R/data/sales.csv/-/ns-pl/is_in-region-EU-US");
```

Use `PipelineEditor` for the same with pipeline-only sources (`link`, `pipeline`, objects);
`QueryEditor` refuses an edit after which the result has no query form, so its `query()` never
fails. `editor.command_metadata(i)`, `argument_info(i, name)` and `unset_arguments(i)` give a UI
what it needs to show an argument form.

## 4. Defining a pipeline command

1. **Write the body** as a pipeline. Put a `{placeholder: name}` wherever a caller's argument goes,
   and `{state: true}` where an argument should be the command's own input.
2. **Declare every placeholder** as an `ArgumentInfo`, with its default. An `any` argument takes
   the type of the slot it fills; a placeholder that fills a variadic slot whole makes the last
   argument variadic. Do not declare injected arguments: the body's commands get them from the
   caller's context.
3. **Register** it:

   ```rust
   let mut n = ArgumentInfo::any_argument("n").with_type(ArgumentType::Integer);
   n.default = CommandParameterValue::Value(json!(10));
   env.command_registry.register_pipeline(
       CommandKey::new("", "sales", "top"),
       Pipeline::from_json(&json!([
           {"action": "from_csv", "ns": "pl"},
           {"action": "head", "ns": "pl", "arguments": {"n": {"placeholder": "n"}}}
       ]))?,
       vec![n],
   )?;
   ```

   Registration refuses an undeclared placeholder, a step that does not resolve, an unknown named
   argument, a body filename and a cycle; an unused argument is a warning in `CommandMetadata::check`.
   In a registry file the same command is `definition: !Pipeline {pipeline: […]}`.
4. **Call** it like any command: `-R/data/sales.csv/-/ns-sales/top-3`. Check it plans as the long
   query you meant by comparing results, as `liquers-lib/tests/pipeline_polars.rs` does.
5. **Regenerate** `specs/command_registry.yaml` if the command is registered in `liquers-lib`.

An error inside the body ends with `(in pipeline command '<key>')`. An argument error names the
pipeline command's argument, not the inner one. Editing the body invalidates cached results.

## 5. Giving a recipe a pipeline

```yaml
recipes:
  - pipeline:
      - "-R/data/sales.csv/-/ns-pl/from_csv"
      - {action: is_in, ns: pl, arguments: {column: region, values: [EU, US]}}
    query: ns-pl/sort-amount-f/head-10/eu.csv
```

The query continues from the pipeline's state, so it must not start with a resource. Put the
filename in one part only. A recipe pipeline takes literal values: no placeholders, no
`{state: true}`. `arguments` and `links` overrides need a query; they do not reach a pipeline
command that ends the query yet (`RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND`).
`liquers-validate --recipes recipes.yaml --cwd <folder>` checks all of this without running it.

## 6. Validating

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- --pipeline p.yaml
echo '[{"resource": "data/x.txt"}, {"action": "upper"}]' | \
  cargo run -p liquers-core --features cli --bin liquers-validate -- --pipeline -
cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --build R:data/x.txt A:suffix 'P:a b/c'
```

- A pipeline with a query form reports it as `encoded`; one without is a warning naming the rule.
  Add `--emit query` to make that an error (exit 1), for a check that something *is* a query.
- `--build` takes `R:<key>`, `A:<action>`, `P:<parameter>` and prints the escaped query; no
  value needs shell or Liquers escaping.
- `--emit pipeline` adds the pipeline form of each input, which is a quick way to see how a query
  reads as a pipeline.

## 7. Testing

Planning tests compare plans, and should compare against the long query the pipeline stands for:
`liquers-core/tests/pipeline_planning.rs`. Pipeline command tests (`pipeline_commands.rs`)
register a command in a `SimpleEnvironment`, evaluate a call and compare with the expanded query.
Strip `pos` before comparing serialized pipelines converted from parsed queries
(`validate_pipelines.rs` `without_positions`). A new representability rule gets a row in
`not_representable_each_rule` in `liquers-core/src/pipeline/convert/tests.rs`.

## 8. From Python

`liquers-py` exposes the same model (`reference/PYTHON_INTEGRATION.md` §16): a pipeline is a Python
list of steps (query text or dicts in the JSON form) or an `lq.Pipeline`, and `env.evaluate` takes
one wherever it takes a query. The Rust names map one to one:

| Rust | Python |
|---|---|
| `Pipeline::from_json` / `to_json`, `TryFrom<&Query>`, `to_query_with(&cmr)` | `lq.Pipeline(list)`, `Pipeline.from_json` / `to_json`, `Pipeline.from_query`, `p.to_query(env)` |
| `evaluate_pipeline` | `env.evaluate(pipeline)`; inside a command `context.evaluate(pipeline)` |
| `PipelineEditor`, `QueryEditor`, `ActionEditor` | `env.pipeline_editor()`, `env.query_editor()`, `lq.ActionEditor` |
| `CommandRegistry::register_pipeline` | `env.pipeline_command(name, body, arguments=[…])` |

```python
env.pipeline_command("best", [{"action": "top", "parameters": [{"placeholder": "n"}]}],
                     arguments=["n"])
assert env.evaluate("numbers/best-1") == [3]
```

Two differences: a refused edit leaves a Python editor unchanged even when the call made two
steps, and inside a command a pipeline with a relative key is refused before it runs. Tests:
`liquers-py/tests/test_PIPELINE_*.py`; usage: `guides/PYTHON_GUIDE.md` §6 Pipelines.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-04 | §8 From Python: the Python names of the pipeline API. | `design/liquers-py-pipelines/` |
| 2026-11-04 | Created: choosing the tool, writing and evaluating pipelines, converting and editing, pipeline commands, recipe pipelines, validation, testing. | phase-5, `design/composite-command-definition/` |
