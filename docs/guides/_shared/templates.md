EDITOR: shared fragment (templates, the same in every guide), included by chapter 10 of all three guides. Keep it language-neutral. The chapters are templates themselves, so this fragment switches the tag delimiters to `<% %>` at its top and back at its end; otherwise the examples would be expanded.

{{=<% %>=}}
## 10.1 Text with queries in it

A *template* is text with tags in it. Rendering the template replaces each tag with a value.
Liquers templates are [Mustache](https://mustache.github.io/mustache.5.html) templates, with one
addition: a tag can hold a query.

```text
Dear {{name}},

we recorded {{$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows}} sales in the EU.
```

`{{name}}` is a plain Mustache tag. It is replaced by the field `name` of the data you render the
template with. `{{$ …}}` is a *query tag*. It is replaced by the value of its query, which is
evaluated like any other query: it is cached, and it becomes a dependency of the rendered text.
When `data/sales.csv` changes, the rendered text is out of date and is rendered again on the next
request.

The tags:

| Tag | Replaced by |
|---|---|
| `{{name}}` | the field `name` of the data; `{{a.b}}` reaches into nested data |
| `{{.}}` | the data itself |
| `{{#items}} … {{/items}}` | the text between the tags, once for each element of `items`, or once when `items` is true |
| `{{^items}} … {{/items}}` | the text between the tags when `items` is empty, false or missing |
| `{{> templates/header.md}}` | another template, read from the store by its key |
| `{{$ query}}` | the value of the query |
| `{{#$ query}} … {{/}}` | the text between the tags, once for each row or element of the query's value |
| `{{^$ query}} … {{/}}` | the text between the tags when the query's value is empty |
| `{{! note }}` | nothing: a comment |

`{{/}}` closes any section.

A query tag may use the data in its query:

```text
{{#$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-{{region}}}}
- {{month}}: {{amount}}
{{/}}
```

The value of `region` is put into the query as *one argument*, escaped as chapter 3 describes.
Whatever the value is, it cannot add an action or a segment to the query, so it is safe to render
a template with data you did not write. Inside the section, each row of the table is the data, so
`{{month}}` and `{{amount}}` are the columns of that row.

Rendering can also escape the substituted text for HTML. When it does, `{{name}}` is escaped, and
`{{{name}}}` or `{{&name}}` inserts the value as it is.

## 10.2 Rendering and template commands

There are two commands that render, both in the namespace `template`:

| Command | The template is | The data is |
|---|---|---|
| `render` | an argument: a link to the template | the input of the command, plus an optional `data` argument |
| `expand` | the input of the command | the `data` argument |

So a stored template is rendered by a query like any other value is made:

```text
-R/data/customer.json/-/ns-template/render-~X~-R/templates/letter.md~E/letter.md
```

This reads `data/customer.json`, renders `templates/letter.md` with it, and names the result
`letter.md`. The template is a dependency of the result, so editing the template is enough to
have every letter rendered again.

When a tag fails, because a query has an error or a field is missing where one is required, the
`errors` argument decides what happens:

| `errors` | A failing tag |
|---|---|
| `strict` | fails the whole render, with the position of the tag. The default, and the right choice while writing a template |
| `expand`, `expand_md`, `expand_html` | is replaced by the error message, so all failures show up in the output at once |
| `silent` | is replaced by nothing |

Every error that did not fail the render is a warning in the log of the result.

A template that is rendered often deserves a name. A *template command* is a command that
consists of a template and nothing else: no function is written for it. Called in a query, it
renders its template with the input as data, so `-R/data/customer.json/-/letter` does what the
long query above does. Each guide shows how to register one. Because a template command is a
command, it appears in the command list, it can be used in recipes, and it is served by the web
API like every other command.

Before a template is used, `-R/templates/letter.md/-/ns-template/validate` lists every malformed
tag and every unknown command in it, with positions, and runs nothing.
<%={{ }}=%>
