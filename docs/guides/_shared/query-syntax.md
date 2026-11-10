EDITOR: shared fragment (the query syntax reference, the same in every guide), included by appendix A of all three guides. Keep it language-neutral and in step with chapter 3 (`queries.md`).

## A.1 The shape of a query

A query is a sequence of *segments*. There are two kinds:

```text
-R/data/sales.csv/-/ns-pl/from_csv/head-2/top.csv
└───────┬───────┘ └────────────┬────────────────┘
 resource segment      transform segment
```

| Part | Written | Meaning |
|---|---|---|
| Resource segment | `-R/<key>` | reads the asset at the key |
| Transform segment | `-/<action>/<action>/…` | applies the actions, in order |
| Action | `<name>-<argument>-<argument>…` | calls a command with arguments |
| File name | a last element with a dot: `top.csv` | names the result and chooses its format |

A query that consists of actions only needs no `-/` at its start: `hello/greet-Hi` is one
transform segment. After a resource segment, the transform segment must start with `/-/`.

A key is a path of names separated by `/`. In a query, a key may be relative: `.` is the current
folder and `..` its parent, resolved against the folder of the recipe the query is in.

## A.2 Ways to read a key

A word after `-R-` changes what the resource segment gives you:

| Written | Gives |
|---|---|
| `-R/<key>` | the value at the key: stored, or made by a recipe |
| `-R-meta/<key>` | the metadata of that asset |
| `-R-dir/<key>` | the listing of the folder at the key |
| `-R-recipe/<key>` | the recipe for the key |
| `-R-stored/<key>` | the bytes exactly as stored; no recipe is used |
| `-R-key/<key>` | the key itself, as a value; nothing is read |
| `-R-cwd/<key>` | nothing; makes the key the current folder for the rest of the query |

## A.3 Instructions

Three names look like actions but are instructions to the planner. They run no command.

| Written | Effect |
|---|---|
| `ns-<a>-<b>…` | the actions that follow in this segment are looked up in these namespaces first; a later `ns` replaces an earlier one |
| `v` | everything from here on is volatile: it is computed on every request, while the part before it stays cached |
| `q` | the query before it is not evaluated; it becomes a value, the query itself, for the next action |

## A.4 Arguments

Arguments are separated by `-` and are positional. An empty argument (`--`) is allowed.

| Declared type | Written | Notes |
|---|---|---|
| text | the text, escaped | an empty argument is the empty text |
| integer, float | `42`, `3.14`, `~5` for -5 | an empty argument means the default |
| truth value | `t`, `true`, `yes`; `f`, `false`, `no` | an empty argument means the default |
| a link: the value of another query | `~X~<query>~E` | the inner query uses `/-/` explicitly |

Giving more arguments than the command declares is an error. A command may declare that its
last argument takes all remaining ones.

## A.5 Escapes

In an argument, only letters, digits, `_`, `+` and `.` stand for themselves. Everything else is
written with an escape that starts with `~`:

| Escape | Stands for |
|---|---|
| `~~` | `~` |
| `~_` | `-` |
| `~/` or `~I` | `/` |
| `~.` | a space |
| `~H`, `~h`, `~f` | `https://`, `http://`, `file://` |
| `~P` | `://` |
| `~<digits>` | `-` followed by the digits, a negative number |
| `~n<name>~` | the character with that HTML name: `~ncolon~` is `:`, `~nast~` is `*`, `~ncomma~` is `,`, `~nquot~` is `"` |
| `~U<hex>~` | the character with that code point: `~UE9~` is `é` |
| `~X~…~E` | a link (section A.4) |

A program should not write escapes by hand. Each guide shows, in chapter 3, how to build a query
from values so that they are escaped correctly.
