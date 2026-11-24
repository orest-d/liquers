EDITOR: shared fragment (the query editor: modes, cards, keys, themes and icons, the same in every guide), included by chapter 11 of all three guides. Keep it language-neutral: queries and YAML only, no host-language code. Written from reference/UI_INTERFACE_FSD.md §Editors (with the console update modes and §Recipe editors) and §Hosts and guides/COMMAND_REGISTRATION_GUIDE.md §5. Queries checked with liquers-validate.

The query console and the editor elements (`ns-lui/query_editor`, `action_editor`,
`resource_editor`, `key_editor`) show a query as a form next to its text. Each step of the query
is a *card*: an icon, the command's label, its `namespace/name` in grey, and a grid of its
arguments, each drawn with the widget the command declares. Editing a field rewrites the text,
and editing the text rebuilds the form. An edit the command refuses changes nothing and shows its
message under the field.

### Simple and Advanced

The editor has two modes, and starts in Simple.

- **Simple** shows the steps of the pipeline as a row of chips and one card for the last step.
  Below it, *Continue with* chips offer the commands that usually follow, so a pipeline grows one
  click at a time.
- **Advanced** shows every step as a card. It opens with one card expanded (the last step, or the
  step you came from) and the others collapsed to their headers; a link argument (an argument
  whose value is another query) opens as a nested set of cards under its field.

An argument's description and its default are in the hover of its label and field; the reset
button next to the field sets the default.

An expanded card has a toolbar: *Reset* puts every argument back to its default, the command's
presets appear as chips (the first three, the rest under "+N"), *Next* adds a following step,
and in Advanced mode the card moves up and down. Picking a preset such as `head-10` sets the
arguments at once:

```text
-R/data/sales.csv/-/ns-pl/head-10
```

### When edits take effect

Each edit made in a form rewrites the query text at once. When the value is then recalculated
depends on the *update mode*, chosen with the mode control in the toolbar:

- **Apply immediately** (a lightning icon) recalculates the value after each edit. The query
  console waits until you have paused for a moment (300 ms), so a quick run of edits is evaluated
  once.
- **On demand** (a hand icon) keeps the edits until you press ✓, the button beside the query
  text, or Enter in it. ✓ is highlighted while there is something to apply.

Text you type into the query field itself is taken only when you press Enter or ✓, and only when
the query parses and every command in it exists; otherwise the error is shown and the value stays
as it was. Undo and Redo (the arrow buttons, Ctrl+Z and Ctrl+Shift+Z) step back and forth through
the edits, in either mode.

While a new value is evaluated, the console keeps showing the old one. A value that arrives
quickly simply replaces it. When the evaluation takes longer than a quarter of a second, the old
value is dimmed and a status line under the toolbar shows the progress and the latest message;
after three seconds it also offers to switch to On demand, which suits a query that is expensive
to evaluate. When a value the console shows becomes out of date because something it depends on
changed, Apply immediately recalculates it; On demand dims it and says "changed upstream" until
you press ✓.

### Keyboard

On the desktop every action of the editor has a key. The ones used most:

| Key | Does |
|---|---|
| Ctrl+B | switch between Simple and Advanced |
| Ctrl+L | go to the query text |
| Ctrl+J | jump between the query text and the field at the cursor |
| Alt+PageUp, Alt+PageDown | previous or next card |
| Space, Left, Right (on a card's header) | open, collapse or expand the card |
| Alt+R | reset the card |
| Alt+1 … Alt+9 | apply the card's first to ninth preset |
| Alt+Shift+N | add a following step |
| Ctrl+K | add an action |
| Ctrl+Enter | update the query |
| Ctrl+Z, Ctrl+Shift+Z | undo, redo an edit |

**Ctrl+J** is the quickest way between the two views. With the cursor in the text, on an
argument of a step, Ctrl+J opens that card and puts you in that field; Ctrl+J again goes back to
the text, with the argument selected. On a step other than the last one, Ctrl+J opens Advanced
mode with only that step expanded. In the query console the editor opens from the pencil button
beside the query, as a narrow column next to the value, and Ctrl+J in the console's query field
opens it at the cursor. In the browser the editor follows the page's own focus
order and has no keys of its own yet.

### Recipe editors

Two more editors work on recipes. `ns-lui/recipes_editor`, given the accessor of a folder's
`recipes.yaml`, is a narrow side panel listing the folder's recipes; `ns-lui/recipe_editor`
edits one recipe as a form: its title, description, filename and options, with the query editor
inside. `ns-acc/recipe_file` finds a recipe by the file it produces, so the form follows its
recipe when the list is reordered:

```text
-R-key/reports/recipes.yaml/-/ns-acc/accessor/ns-lui/recipes_editor
-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe_file-result.txt/ns-lui/recipe_editor
```

Each row of the panel shows the file's type icon, its filename (or, in a recipe tree, its node
name), and a coloured dot for the status of its value, which becomes a small percentage while the
value is being made. The settings button chooses whether rows show the filename, the title or
both; hovering over the name gives the title and description, and over the dot the status or the
error. Click a row to open its recipe in a window. Right-click it (in the browser, its `⋯`
button) for View, Metadata, Evaluate, Rename, Duplicate, Delete, Save as and Export as a format
the value's type can be written in; a tree adds Add child and Set as result. A rename rewrites
every reference to the node, and moves its stored data with it. Delete is refused while another
recipe uses the node. The `⑂` button shows the dependencies instead: the recipes nothing depends
on at the top, each one's direct dependencies under it, so a shared recipe appears more than once.
The `{}` button opens the file as highlighted YAML, which is applied only when it parses.

### Icons

A card's icon comes from the command's metadata: its `icon` hint names an icon, usually by its
[Phosphor](https://phosphoricons.com) name (`funnel`, `sort-ascending`). The commands of `ns-pl`,
`ns-img` and `ns-rec` all have one. A theme can replace the icon of a command or of a whole
namespace; a command with no icon from either gets its namespace's default icon, then a letter
from its label. The browser draws Phosphor icons when the page loads the
Phosphor stylesheet, and a Unicode stand-in otherwise.

### Themes and the `ui` section

Colours, spacing and icons come from a *theme*. The built-in one has a dark and a light palette,
the colours of query highlighting, and two densities. A configuration document picks the theme,
the mode and the density in its `ui` section:

```yaml
ui:
  theme: default            # or a key or query of a theme file, reloaded when it changes
  mode: system              # dark, light, or system: follow the platform
  density: compact          # comfortable or compact
```

A theme file is YAML that gives only what it changes, as a nested dictionary over the built-in
theme. A later layer overrides the keys it repeats and adds the ones it is the first to give:

```yaml
dark:
  surface: { card: "#1b1f27" }
  syntax: { action: "#7cc4ff" }
icons:
  commands:
    "pl/head": { icon: rows, hue: teal }    # one command
  namespaces:
    pl: { icon: table }                     # every command of a namespace
```

An icon in a theme or a hint is a name (`funnel`, from the theme's icon set, which is Phosphor
unless the theme says otherwise; `ph:funnel` is always Phosphor's), `u:` and a Unicode text (`u:⏷`), an image by key (`-R/icons/brush.svg`, also `.png`,
`.jpg`, `.webp`), or a list of these, best first: a surface draws the first one it can, and a
Unicode text is always there to fall back on.
