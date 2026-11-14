EDITOR: shared fragment (when to use a pipeline and when a query, the same in every guide), included by section 3.10 of all three guides. Keep it language-neutral: queries, YAML and JSON only, no host-language code. Written from reference/PIPELINES.md §3, reference/ASSETS.md.

### Query or pipeline?

| | Query | Pipeline |
|---|---|---|
| Form | one line of text | a list, in code or as JSON or YAML |
| Arguments | text, by position | any value, by position or by name |
| Escaping | needed for `-`, `/`, spaces and more | none |
| Works as a link, in a URL, in a template | yes | no |
| The result is cached as an asset | yes | no |

The last row matters most. Liquers identifies a result by its query: the query is the name under
which the result is cached, shared, watched and kept up to date (chapter 7). A pipeline has no
such name. For now, every evaluation of a pipeline therefore runs its steps again, and two
programs that evaluate the same pipeline do the work twice. For the same reason you cannot ask
for the asset of a pipeline, only for its value.

So prefer a query, and build it with the query editor of section 3.9 when its parts come from
values. Reach for a pipeline only when a query is not good enough:

- an argument is structured data, such as a map of options or a list of rows;
- a long text or a complicated value would make the query unreadable;
- another program sends you the computation as JSON.

When the result of a pipeline should be kept after all, there are two ways to give it a name.
A recipe can hold a pipeline, and then the result has a key in the store (section 7.4). And a
pipeline can be registered as a command (section 5.9): a call of that command is an ordinary
query, with an ordinary cached result.
