EDITOR: shared fragment (the query language, the same in every guide), included by chapter 3 of all three guides. Keep it language-neutral: queries only, no host-language code.

## 3.1 A query is a path of actions

A *query* is a short piece of text that says how to make a value. It reads from left to right,
like a path:

```text
hello/greet-Hi
```

This query has two *actions*, separated by `/`. The first, `hello`, makes a value. The second,
`greet-Hi`, takes that value and makes a new one from it. Every action receives the result of the
action before it, so a query is a small pipeline, and the value of the query is the result of its
last action.

Each action names a *command*: a function that somebody registered under that name. The query
does not say where the function lives or in which language it was written. It only says which
commands to apply, in which order and with which arguments.

A query contains only characters that are safe in a URL. That is deliberate: the same text works
as a link in a browser, as a file name, as a key in a cache and as an argument on a command line.

## 3.2 Arguments

The text after the command's name, separated by `-`, is the list of *arguments*:

```text
hello/greet-Hi
hello/repeat-3/greet-Hi
```

Arguments are positional: the first one fills the command's first parameter, the second one the
second. A command declares a type for each parameter, and Liquers converts the text of the query
to that type, so `repeat-3` receives the number 3 and not the text `"3"`.

An argument that has a default can be left out. `hello/greet` uses the default greeting, and
`hello/greet-Hi` replaces it. To set a later argument and keep the default of an earlier one,
leave the earlier one empty: in `resize-200--f`, the second argument is empty. For a number or a
truth value, an empty argument means "use the default". For text it is the empty text.

Every argument you write must be used. A query that gives a command more arguments than it
declares is an error, reported before anything runs, with the position of the first surplus
argument.

## 3.3 Namespaces

Commands are grouped into *namespaces*, so that two libraries can both have a command called
`head`. The instruction `ns-<name>` selects the namespace for the actions that follow it:

```text
-R/data/sales.csv/-/ns-pl/from_csv/head-2
```

Here `from_csv` and `head` are looked up in the namespace `pl`. Commands registered without a
namespace are always found, so your own commands and the ones of a library mix freely in one
query. `ns` is an instruction to the planner and not a command; it takes no part in the
computation.

## 3.4 Reading stored data

A query does not have to start from nothing. It can start from a file in the *store*, the place
where Liquers keeps data (chapter 6). A file is addressed by its *key*, a path such as
`data/sales.csv`, and the prefix `-R/` reads it:

```text
-R/data/sales.csv
-R/data/sales.csv/-/ns-pl/from_csv/head-2
```

The first query is the content of the file. The second one reads the file and then applies two
actions to it. Between the key and the actions stands `/-/`. It is not decoration: `-R/` takes
everything up to the next `/-/` as the key.

```text
-R/data/report.txt/-/to_text      read data/report.txt, then apply to_text
-R/data/report.txt/to_text        read the file named data/report.txt/to_text
```

Both queries are valid, and they mean different things. When a query fails with "key not found"
and the key ends with the name of a command, a missing `/-/` is the usual cause.

## 3.5 A file name at the end

The last element of a query may be a file name:

```text
-R/data/sales.csv/-/ns-pl/from_csv/head-2/top.csv
```

A file name runs no command. It gives the result a name and, through its extension, a format:
`top.csv` asks for CSV, `top.json` for JSON. The format matters whenever the result leaves the
program as bytes: when it is written to the store, served over HTTP or saved by a user. Inside
the program you still receive the value itself.

A name is recognized as a file name because it contains a dot. Command names never do.

## 3.6 Escaping

Because `-` and `/` have a meaning in a query, an argument that contains one of them is written
with an *escape*. Every escape starts with `~`:

| To write | Type | Example |
|---|---|---|
| `-` | `~_` | `greet-well~_known` passes `well-known` |
| `/` | `~/` | `open-a~/b` passes `a/b` |
| a space | `~.` | `greet-Good~.morning` passes `Good morning` |
| `~` | `~~` | |
| `https://` | `~H` | `fetch-~Hexample.com` |
| `http://` | `~h` | |
| any other character | `~n…~` with its name | `~ncolon~` is `:`, `~nast~` is `*` |

Letters, digits, `_`, `+` and `.` never need an escape. When a program builds a query from values
it does not control, it should not paste them into the text by hand. Every guide shows, in the
sections that follow, how to build a query from values so that the escaping is done for you.

An argument can also be a whole query. Such a *link* is written between `~X~` and `~E`:

```text
ns-template/render-~X~-R/templates/page.md~E
```

The command receives the value that the inner query produces, here the text of a template.
Appendix A lists the complete syntax.
