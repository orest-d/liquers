# Query placeholders — positions and expansion

Supporting note for [Phase 1](./phase1-high-level-design.md), open questions 1 and 2. Written
2026-10-17 from the maintainer's review of the same day. It maps every position in the query
grammar (`liquers-core/src/query.rs`, `liquers-core/src/parse.rs`) and says where a placeholder
`{{name}}` may appear in a hygienic query tag.

## The two expansion modes

| Tag | Mode | What `{{x}}` inside the query does |
|---|---|---|
| `{{$ query}}` | **hygienic** | The query is parsed once, with `{{x}}` as a placeholder node, into a `QueryTemplate`. Expansion fills the placeholder in the **AST**, never in the text. The value cannot change the query's structure. |
| `{{$$ query}}` | **classic** (string) | The text between `{{$$` and `}}` is expanded first, with values inserted verbatim. The result is then parsed and evaluated as an ordinary query. A value may add actions, parameters or segments. |

The maintainer's example, with `x = "a-b"`:

```
{{$ cmd-{{x}}/q}}    ->  Query: cmd("a-b") / q       encodes as  cmd-a~_b/q    (1 parameter)
{{$$ cmd-{{x}}/q}}   ->  text "cmd-a-b/q", parsed    encodes as  cmd-a-b/q     (2 parameters)
```

`{{$$ …}}` is unsafe with untrusted data and is documented as such. Inside a query tag, `{{{x}}}`
and `{{&x}}` are an error, because `$$` is the raw form. Both modes balance nested braces, so the
tag ends at the first unbalanced `}}` (open question 9, agreed).

## Where a hygienic placeholder may appear

Only action parameters have an escape mechanism (`liquers-core/src/escape.rs` `encode_token`).
Every other name in a query has a fixed character class and no escape form. In those positions,
hygienic means **checked**: the expanded value must be valid there, otherwise it is a positioned
error. A value is never re-lexed, so it can never split a token or start a new one.

"Whole" means the placeholder is the entire token (`cmd-{{x}}`). "Fragment" means it is part of
a token with literal text around it (`search-area~ncolon~{{area}}`, `to_{{fmt}}`).

| # | Position | AST field | Example | Whole | Fragment | How the value is filled |
|---|---|---|---|---|---|---|
| P1 | Action parameter | `ActionParameter::String` | `cmd-{{x}}`, `search-area~ncolon~{{area}}` | yes | yes | Any value. The decoded parameter becomes literal + value + literal, and `encode_token` escapes it on output |
| P2 | Namespace in `ns-…` | `ActionParameter::String` of `ns` | `ns-{{lib}}/to_text` | yes | yes | Same as P1, since `ns` is an action. Its value must also be an identifier, because a namespace is one |
| P3 | Action name | `ActionRequest::name` | `{{op}}-1`, `to_{{fmt}}` | yes | yes | Must match `identifier` (`[A-Za-z_][A-Za-z0-9_]*`), otherwise an error |
| P4 | Resource key segment | `ResourceName` in `Key` | `-R/data/{{name}}`, `-R/data/{{stem}}.csv` | yes | yes | Must match `resource_name` (`[A-Za-z0-9_.]` with an inner `-`). A `/` is an error. `.` and `..` as a whole segment are an error, so a value cannot climb the key tree |
| P5 | Output filename | `TransformQuerySegment::filename` | `…/to_html/{{name}}.html`, `…/report.{{ext}}` | yes | yes | Must keep the filename shape (`filename`: a `.` and an extension). See the classification rule below |
| P6 | Inside a link parameter | `ActionParameter::Link` | `cmd-~X~-R/{{key}}~E` | — | — | The link is a nested query, and P1 to P5 apply inside it recursively |

Positions with **no** hygienic placeholder. Use `{{$$ …}}` for these:

| Position | Why not |
|---|---|
| Segment header name, which is the realm (`-{{r}}/…`) | The realm selects the command set at plan time. A data-dependent realm is a structural change, and the `store_config` realm rules assume a realm fixed in the text |
| Header parameters and level (`-R-{{p}}`, `--R`) | They have no escaping and are rarely used. They can be added later if a use appears |
| A whole action, several actions or a whole segment (`{{step}}` standing for `a-1/b`) | This is structure by definition |
| The leading `/` (`Query::rooted`) and the `/-/` separators | Structure |

**Classification rule (P3 versus P5).** In a transform segment, the parser decides whether a token
is an action or a filename from its **literal** text: a token containing a literal `.` outside placeholders is a
filename (`{{name}}.html`), and any other token is an action (`{{name}}`). The value must keep that
classification. A value containing `.` in an action-name position is an error, not a filename.

**Values that are not text.** A number or a boolean is written as its text (`42`, `true`). Null or
a missing name is an error in a query: Mustache's empty string would change the query silently. An
object is an error. A list or a `Key` in a *whole* P1 or P4 position is **open question 2a**.

## Proposed details for Phase 2

- `QueryTemplate` mirrors `Query`, with a `Placeholder { path, position }` element allowed in the
  positions above. `QueryTemplate::instantiate(&data) -> Result<Query, Error>` fills it, and
  `QueryTemplate::try_into_query()` succeeds exactly when there are no placeholders.
- An expansion error is reported at the placeholder's position in the template, naming the position
  kind (P1 to P5) and the rejected value.

## Open question 2a

**Can one whole placeholder fill several parameters or segments?** For example, `cmd-{{cols}}`
with `cols = ["a", "b"]` could become `cmd-a-b` with two encoded parameters. That would fit
`multiple` arguments. Likewise `-R/{{dir}}/x.csv` with a `Key` value `data/sub` could become two
segments, each checked as in P4. The structure still comes from the template, which marks the slot
as a list, and every element is encoded.

*Recommended: yes for lists in a whole P1 position and for a `Key` value in a whole P4 position. A
list anywhere else, and a string containing `/` in P4, are errors.*
