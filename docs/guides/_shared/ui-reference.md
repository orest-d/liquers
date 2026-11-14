EDITOR: shared fragment (the fields of a UI spec and the commands of `ns-lui`, the same in every guide), included by chapter 11 of the Rust and the Web guide. Keep it language-neutral: queries and YAML only, no host-language code. Written from reference/UI_INTERFACE_FSD.md and the `lui` commands of the command registry. The full example was run in the browser module.

This section lists what a UI spec may contain and what the commands of `ns-lui` do. It is the
same on the desktop and in the browser.

### The UI spec

A UI spec has three fields, and all three are optional:

| Field | Holds | When left out |
|---|---|---|
| `menu` | the menu bar: a list of `items` | no menu bar |
| `layout` | how the child elements are arranged | `horizontal` |
| `init` | queries that run once, when the element first appears | nothing runs |

A complete example:

```yaml
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: Add a greeting
      shortcut: Ctrl+G
      action:
        query: "hello/q/ns-lui/add-child"
    - !separator
    - !submenu
      label: More
      items:
      - !button
        label: Notes
        action: "notes/ns-lui/markdown/add-child"
    - !button
      label: Quit
      action: quit
  - !button
    label: Console
    action: "ns-lui/query_console/add-child"
layout: !grid
  columns: 2
init:
  - "hello/q/ns-lui/add-child"
```

**Menu items.** The kind of an item is written as a YAML tag before it.

| Item | Where | Fields |
|---|---|---|
| `!menu` | in the menu bar | `label`, `items`, optionally `shortcut` |
| `!button` | in the menu bar, in a menu, in a submenu | `label`, `action`, optionally `icon` and `shortcut` |
| `!submenu` | in a menu or a submenu | `label`, `items`, optionally `shortcut` |
| `!separator` | in a menu or a submenu | none; a line between items |

**Actions.** An `action` says what a button does:

| Written as | Does |
|---|---|
| `action: {query: "…"}`, or just `action: "…"` | submits the query for the element that owns the menu |
| `action: quit` | asks the application to close; this is for desktop programs |
| no `action` | nothing; a button that is not wired yet |

**Shortcuts.** A shortcut is written as its keys joined by `+`: `Ctrl+G`, `Ctrl+Shift+S`,
`Alt+N`. `Ctrl` means the command key of the platform, which is ⌘ on a Mac, so one spec serves
all of them. `Cmd` is another spelling of the same key.

**Layouts.**

| `layout` | Arranges the children |
|---|---|
| `horizontal` | side by side |
| `vertical` | one below another |
| `!grid` with `rows` or `columns` | in a grid with that many rows or columns |
| `!tabs`, optionally with `selected` | one at a time, chosen by a row of tabs; `selected` is the tab shown first, counted from 0 |
| `windows` | each in a window of its own, inside the element |

**Init queries.** Each entry of `init` is a query, submitted for the new element as a button of
it would be. An entry may also be a whole recipe (section 7.4), when the query needs a title or
arguments.

A UI spec becomes an element through the command `ns-lui/ui_spec`, which takes the YAML text as
its input. A spec stored as `ui/dashboard.yaml` is therefore shown by the query
`-R/ui/dashboard.yaml/-/ns-lui/ui_spec`, and a spec can open another spec.

### The commands of `ns-lui`

The commands that make elements work anywhere; their result is an element, which is a value:

| Command | Its input | Makes |
|---|---|---|
| `ui_spec` | the YAML text of a UI spec | a layout with a menu |
| `markdown` | Markdown text | a Markdown view |
| `query_console` | a query, or nothing | a console to type queries in, starting with that query |

The other commands change the tree of elements. They only work in a query that was submitted
from a user interface, by a button, an init query or your code, because that is where the tree
and the *current element* come from. Evaluated on their own they fail with an error that says
a payload is required.

| Command | Does |
|---|---|
| `add-<where>-<reference>` | adds its input as a new element. `where` is `child` (or `last`), `first`, `before`, `after` or `instead`; `reference` is a navigation word, `current` when left out |
| `remove-<target>` | removes an element and its children |
| `activate-<target>` | makes an element the active one, which is where shortcuts and menu actions without an owner go |
| `parent-<target>`, `first-<target>`, `last-<target>`, `next-<target>`, `prev-<target>` | return the handle of a neighbour of the target |
| `children-<target>` | returns the handles of the children |
| `roots` | returns the handles of the elements that have no parent |
| `submit` | takes its input as the text of a query and submits it for the current element |

A *navigation word* names an element relative to the current one:

| Word | The element |
|---|---|
| `current` | the element the query was submitted for |
| `parent` | its parent |
| `first`, `last` | its first and its last child |
| `next`, `prev` | the sibling after it and before it |
| `root` | the first element without a parent |
| a number | the element with that handle |

**What `add` adds.** `add` looks at its input. An element is added as it is. A query, which is
what the instruction `q` leaves, becomes an element that shows the value of that query and
follows it: it draws the status while the query runs and the result when it is ready. Any other
value becomes an element that shows the value. So these three differ:

| Query | Adds |
|---|---|
| `hello/q/ns-lui/add-child` | an element whose source is the query `hello` |
| `notes/ns-lui/markdown/add-child` | the Markdown view that `markdown` made of the value of `notes` |
| `-R/ui/dashboard.yaml/-/ns-lui/ui_spec/add-instead` | the layout of another spec, in place of the current element |

The first form is the usual one for data: the element appears at once, shows the progress of
the query, and shows an error in its own place when the query fails.

`add` returns the handle of the new element. When there is no current element at all, `add-child`
makes the new element a root.
