EDITOR: shared fragment (what a pipeline is and how to read one, the same in every guide), included by section 3.10 of all three guides. Keep it language-neutral: queries, YAML and JSON only, no host-language code. Written from reference/PIPELINES.md §1 and §2, guides/PIPELINE_GUIDE.md.

A query is text. A *pipeline* is the same chain of steps written as data: a list with one entry
for each step. Because it is data, a pipeline can be written as JSON or YAML, kept in a file,
sent to another program, and built or changed by code without parsing and without escaping.

```yaml
- -R/data/sales.csv/-/ns-rec/to_record
- action: eq
  ns: rec
  parameters: [region, EU]
- action: head
  ns: rec
  arguments: {n: 10}
```

This pipeline says the same as the query
`-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/head-10`. The first entry is a piece of query
text, taken as it is. The other two each call one command.

### The steps of a pipeline

Each entry of the list is one *step*, and what the step does is decided by its form:

| Step | Written as | What it does |
|---|---|---|
| Query text | `"hello/greet-Hi"` | The steps of that piece of query, in place |
| Action | `{action: head, ns: rec, parameters: [10]}` | Calls a command. `ns` is its namespace; leave it out for a command of the default namespaces |
| Resource | `{resource: data/sales.csv}` | Reads the key, as `-R/data/sales.csv` does. `read: metadata`, `binary` or `directory` reads something other than the value |
| Value | `{value: {a: [1, 2]}}` | The value becomes the input of the next step |
| Evaluate | `{evaluate: "hello/greet"}` | Evaluates a query, or a nested pipeline, and passes its result on |
| Nested pipeline | `{pipeline: […]}` | A pipeline inside the pipeline |

An action takes its arguments in two ways. `parameters` is a list of values in the order the
command declares them, as in a query. `arguments` is a map from the name of an argument to its
value. You can use both in one step: the positional ones are placed first.

In a query every argument is text. In a pipeline an argument is a value of any shape:

| Argument | Written as |
|---|---|
| A number, a boolean, a text | `10`, `true`, `"North-West / 2"`, with no escaping |
| A list or a map | `[EU, US]`, `{descending: true}` |
| The result of another query (a link) | `{link: "-R/data/limits.json"}` |
| The result of a nested pipeline | `{pipeline: […]}` |

### Reading a pipeline

Read a pipeline from top to bottom, as you read a query from left to right: the result of each
step is the input of the next. A piece of query text may hold several steps, and it may be mixed
freely with the other forms.

Every query can be written as a pipeline. The other direction works only sometimes: a pipeline
has a query form when everything in it can be written as query text. Three things cannot be: a
`value` step, a map as an argument, and a list given to an argument that takes a single value.
Converting such a pipeline to a query fails with an error that names the step and the argument.
A pipeline that does have a query form means exactly what that query means.
