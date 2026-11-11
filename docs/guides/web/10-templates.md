---
title: Templates
status: ready
sources: [guides/TEMPLATE_GUIDE.md, guides/JAVASCRIPT_GUIDE.md]
reviewed: 2026-11-10
---
# 10. Templates

PURPOSE: The reader can write a Mustache template with query tags and render it from JavaScript in the browser.

Templates are the same in the browser as in native Liquers: the whole template library is in
the module. Sections 10.1 and 10.2 describe the template language in the words shared by all
three guides. The rest of the chapter shows how a page renders a template, and what to watch
when the result goes into the page as HTML.

{{> ../_shared/templates.md}}

{{=<% %>=}}
## 10.3 Rendering from a page

Two functions render a template and resolve with the text:

| Call | The data of the template |
|---|---|
| `liquers.expand(template, data)` | the object `data`: `{{name}}` reads its fields |
| `liquers.render(template, value, data)` | `value`, which is `{{.}}`, with `data` as extra names |

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "count", run: () => 3 });
liquers.registerCommand({
  name: "people",
  run: () => [{ name: "Ann", city: "Oslo" }, { name: "Bob", city: "Rome" }],
});

assert.strictEqual(
  await liquers.expand("{{name}} has {{$ count}} items", { name: "Ann" }),
  "Ann has 3 items");
assert.strictEqual(await liquers.render("Hello, {{.}}!", "world"), "Hello, world!");

const list = await liquers.expand("{{#$ people}}- {{name}} ({{city}})\n{{/}}");
assert.strictEqual(list, "- Ann (Oslo)\n- Bob (Rome)\n");

const mixed = await liquers.expand("{{#rows}}{{.}};{{/rows}}", { rows: [1, 2, 3] });
assert.strictEqual(mixed, "1;2;3;");
```

Both run the queries of the template in the page's environment, so the commands you registered
are available in query tags. A section over a query, `{{#$ people}} … {{/}}`, repeats its text
for each element of an array or each row of a table.

A string is the template itself. To render a template that is in the store, pass a link to it,
`liquers.Query.parse("-R/templates/page.md")`, in place of the string (section 10.5).

The data you pass is converted like any value that enters Liquers (section 4.1), so it must be
plain data: no `Map`, no `Date`, no class instance.

## 10.4 When a tag fails

By default a tag that fails makes the whole rendering fail. The error says where in the template
the tag is:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "count", run: () => 3 });

await assert.rejects(liquers.expand("ok\n{{$ missing}}"), (e) =>
  e.errorType === "action_not_registered" && e.position.line === 2);

const result = await liquers.expand("ok\n{{$ missing}}", null, { errors: "silent", full: true });
assert.strictEqual(result.text, "ok\n");
assert.strictEqual(result.warnings.length, 1);

const problems = await liquers.validateTemplate("{{$ count}} {{$ missing}}");
assert.strictEqual(problems.length, 1);
assert.strictEqual(problems[0].position.column, 17);       // where `missing` starts
```

The fourth argument of `render`, the third of `expand`, holds the options:

| Option | Meaning |
|---|---|
| `errors` | `"strict"` (the default) fails at the first failing tag. `"silent"` leaves the tag out. `"expand"`, `"expand_html"` and `"expand_md"` put a description of the error in its place, as plain text, HTML or Markdown |
| `full` | `true` resolves with `{ text, warnings }` instead of the text, so that the errors a recovering mode skipped are not lost |
| `html` | `true` escapes what `{{name}}` inserts, for HTML (section 10.7) |
| `cwd` | the folder that a relative key in the template is resolved against; the root of the store otherwise |

`validateTemplate(template)` checks a template without rendering it. It parses the template,
checks every query in it against the registered commands, runs nothing, and resolves with the
list of problems, each with its position. Use it in an editor, or in a test over the templates
of your application.

## 10.5 Templates in the store, and template commands

Templates usually live in the store, where they can include one another. A *template command*
turns a stored template into a command: the declaration has `template`, a link to the template,
in place of `run`. A call renders the template with the command's input as `{{.}}`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommands([
  { name: "hello", run: () => "world" },
  { name: "count", run: () => 3 },
  { name: "greeting", template: "-R/templates/greet.md" },
]);

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "templates" }] });
const bytes = (s) => new TextEncoder().encode(s);
await env.store().set("templates/greet.md", bytes("{{> templates/header.md}}Hello, {{.}}!"), {});
await env.store().set("templates/header.md", bytes("[{{$ count}} items] "), {});

// The template command.
assert.strictEqual(await liquers.evaluate("hello/greeting"), "[3 items] Hello, world!");

// The same through the library's render command, with a link to the template as its argument.
assert.strictEqual(
  await liquers.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E"),
  "[3 items] Hello, world!");

// And from the page, with a link in place of the template text.
const link = liquers.Query.parse("-R/templates/greet.md");
assert.strictEqual(await liquers.render(link, "page"), "[3 items] Hello, page!");
```

`{{> templates/header.md}}` includes another template by its key. A template command is the
tidy form for an application: `hello/greeting` is a query like any other, so it has a result
that is kept, it can be the query of a recipe, and it can be shown in a user interface. The
query that renders through `ns-template/render` does the same without registering anything.

A template command has no function, so `run`, `state`, `async`, `pipeline` and `arguments` are
refused beside `template`. It takes the arguments `data`, `errors` and `cwd` of the render
command.

With an `http` store, the templates of a page can simply be files published next to it. List
them in the store's `keys` (section 6.3).

## 10.6 Rendering inside a command

A command of yours renders through its context: `context.render`, `context.expand` and
`context.validate` take the same arguments as the functions of the module. Every query and every
included template becomes a dependency of the command's result, as with `context.evaluate`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "count", run: () => 3 });
liquers.registerCommand({
  name: "summary",
  async: true,
  arguments: [{ name: "title", type: "string", default: "Summary" }, { name: "context" }],
  run: (title, context) => context.expand("# {{title}}\n\n{{$ count}} items.", { title }),
});

assert.strictEqual(await liquers.evaluate("summary-Stock"), "# Stock\n\n3 items.");

const state = await (await liquers.getAsset("summary-Stock")).get();
assert(state.metadata().dependencies.some((dependency) => dependency.key === "count"));
```

Inside a command, a tag that fails in a recovering mode becomes a warning in the log of the
result, so the option `full` is refused there.

## 10.7 Templates and HTML

A page often renders a template in order to put the text into the document. Then the text is
HTML, and whatever a tag inserts must not be able to break out of it. Ask for escaping with
`html: true`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const data = { name: "<img src=x onerror=alert(1)>" };

assert.strictEqual(await liquers.expand("<b>{{name}}</b>", data),
  "<b><img src=x onerror=alert(1)></b>");                              // not escaped: the default
assert.strictEqual(await liquers.expand("<b>{{name}}</b>", data, { html: true }),
  "<b>&lt;img src=x onerror=alert(1)&gt;</b>");                        // escaped
assert.strictEqual(await liquers.expand("<b>{{{name}}}</b>", data, { html: true }),
  "<b><img src=x onerror=alert(1)></b>");                              // three braces: inserted as it is
```

Escaping is off unless you ask for it, because a template is as often Markdown or plain text as
HTML. So the rule for a page is short: when the rendered text is assigned to `innerHTML`, render
with `html: true`, and use the three-brace form only for values that are HTML you made yourself.
When the text is plain, assign it to `textContent` and no escaping is needed.

To show Markdown that a template produced, the user-interface library has an element that
renders it (`ns-lui/markdown`, chapter 11).
<%={{ }}=%>
