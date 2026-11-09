---
title: Templates
status: draft
sources: [guides/TEMPLATE_GUIDE.md, reference/TEMPLATE_LANGUAGE.md, guides/PYTHON_GUIDE.md]
reviewed: 2026-11-09
---
# 10. Templates

PURPOSE: The reader can write a Mustache template with query tags, render it from Python, and make a command of it.

Reports, letters, web pages and prompts for language models have the same shape: fixed text with
results in it. A Liquers template is such a text, with the results written as queries. The first
two sections describe templates in the words shared by all three guides. The rest of the chapter
renders them from Python.

{{> ../_shared/templates.md}}

{{=<% %>=}}
## 10.3 Rendering from Python

An environment has two methods that render. `expand` takes a template and the data for it:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def number(n: int) -> int:
    return n

text = env.expand("{{name}} has {{$ number-42}} points", data={"name": "Ann"})
assert text == "Ann has 42 points"
env.shutdown()
```

`render` is for a template applied to *one value*, which the template sees as `{{.}}`. Extra
names can be added with `data`:

```python
import liquers as lq

env = lq.Environment()
env.store_set("templates/greet.md", "Hello, {{.}}!")

template = lq.parse("-R/templates/greet.md")                 # a link to the stored template
assert env.render(template, state="Ann") == "Hello, Ann!"
assert env.render("{{a}} and {{b}}", state={"a": 1}, data={"b": 2}) == "1 and 2"
env.shutdown()
```

The template is given either as its text (a `str`) or as a link to where it is stored: an
`lq.Query`, made with `lq.parse`, or an `lq.Key`. A plain string is always the text of a
template, never a key, so that a template cannot be mistaken for a path.

In `render`, a field of `data` wins over a field of `state` with the same name, and the other
fields of `state` stay visible.

The data may contain things Liquers has no type for. A dictionary that holds a `datetime` still
renders its other fields, because a template reads its data field by field (chapter 4).

## 10.4 A report from a table

A section with a query tag repeats its content for every row of a table:

```python
import liquers as lq

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")
env.store_set("templates/report.md", """# Sales in {{region}}

{{#$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
- {{month}}: {{amount}}
{{/}}
{{^$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
No sales.
{{/}}
""")

report = env.render(lq.parse("-R/templates/report.md"), state={"region": "EU"})
print(report)
# # Sales in EU
#
# - jan: 120
# - feb: 95
env.shutdown()
```

The value of `region` is placed into the query as one escaped argument. A region called
`EU/-/delete_everything` would be looked up as a region of that name, and would find no rows.

## 10.5 When a tag fails

By default a failing tag fails the render, and the exception says where in the template the tag
is. The `errors` argument chooses another behaviour, and `full=True` returns the warnings along
with the text:

```python
import liquers as lq

env = lq.Environment()

try:
    env.expand("line one\n{{$ no_such_command}}")
except lq.ActionNotRegisteredError as error:
    print(error.position)                                    # line 2, position 1

assert env.expand("a {{$ no_such_command}} b", errors="silent") == "a  b"

result = env.expand("ok\n{{$ no_such_command}}", errors="silent", full=True)
assert result.text == "ok\n"
assert result.warnings[0].position.line == 2

problems = env.validate_template("{{$ no_such_command}}")    # checks; runs nothing
assert problems[0].severity == "error"
env.shutdown()
```

Use `"strict"`, the default, while you write a template. Use `"expand_md"` or `"expand_html"` to
see all failures at once in the output, and `"silent"` when an incomplete text is better than
none.

For a web page, pass `html=True`: substituted text is then escaped, so a value containing `<`
cannot break the page.

## 10.6 Template commands

A template that you render often can become a command. No function is needed:

```python
import liquers as lq

env = lq.Environment()
env.store_set("templates/greet.md", "Hello, {{.}}!")

@env.first_command
def number(n: int) -> int:
    return n

env.template_command("greeting", template="-R/templates/greet.md", label="Greeting")

assert env.evaluate("number-7/greeting") == "Hello, 7!"
assert env.command_metadata("greeting").definition.kind == "alias"
env.shutdown()
```

`greeting` renders the stored template with its input as `{{.}}`. It is a command like any
other: it can be used in queries and recipes, and it appears in the command list. The template
is a dependency of every result, so editing `templates/greet.md` through Liquers is enough to
have the results rendered again.

`template_command` takes the same descriptive options as the decorators: `namespace`, `label`,
`doc` and `filename`.

## 10.7 Rendering inside a command

Inside a command, render with the context: `context.render` and `context.expand` take the same
arguments as the methods of the environment. The queries in the template then become
dependencies of the command's result. In an async command use `render_async` and
`expand_async`.

A template can include another one by a relative key, `{{> ./header.md}}`. Relative keys are
resolved against a folder that you pass as `cwd`:

```python
import liquers as lq

env = lq.Environment()
env.store_set("templates/header.md", "Header")
env.store_set("templates/page.md", "{{> ./header.md}} body")

page = env.render(lq.parse("-R/templates/page.md"), cwd=lq.parse_key("templates"))
assert page == "Header body"
env.shutdown()
```
<%={{ }}=%>
