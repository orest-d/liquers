# Query editor ergonomics: reference design

Supporting document of the `query-editor-ergonomics` design: the detailed reference design behind
[Phase 1](./phase1-high-level-design.md), written with Orest on 2026-11-21. Phase 1 holds the
acceptance criteria and wins where the two differ; Phase 2 decides the code structure.

Decided with Orest on 2026-11-21: headers read as human steps (label first); presets as chips plus
an overflow menu (to try); icon set Phosphor; one design including a minimal theme layer as a
proof of concept for a later theme design. Open: see Phase 1.

Mockups are in [`mockups/`](./mockups/) (PNG; [`mockups/source/`](./mockups/source/README.md) regenerates them). They are HTML
drawings of the intended egui result, not screenshots.

| # | File | Shows |
|---|---|---|
| 1 | [`01-current-state.png`](./mockups/01-current-state.png) | Today's editor, reconstructed from `liquers-lib/src/egui/editor.rs`, with six pain points |
| 2 | [`02-simple-mode.png`](./mockups/02-simple-mode.png) | Reference design, Simple mode |
| 3 | [`03-advanced-mode.png`](./mockups/03-advanced-mode.png) | Reference design, Advanced mode (key part, query part, outline, nested link) |
| 4 | [`04-card-anatomy.png`](./mockups/04-card-anatomy.png) | Collapsed and expanded action cards, states, error at a field |
| 4a | [`04a-card-headers.png`](./mockups/04a-card-headers.png) | Header variants: ids first, label only, label then namespace/name; realm |
| 5 | [`05-presets.png`](./mockups/05-presets.png) | Presets as a menu, as buttons, or as chips with an overflow menu |
| 6 | [`06-query-jump.png`](./mockups/06-query-jump.png) | Jumping from a cursor position in the query text to the matching field |
| 7 | [`07-light-theme.png`](./mockups/07-light-theme.png) | Simple mode with the light token set |
| 8 | [`08-icons.png`](./mockups/08-icons.png) | The icon fallback chain |
| 9 | [`09-icons-phosphor-vs-lucide.png`](./mockups/09-icons-phosphor-vs-lucide.png) | The same concepts in both icon sets, weights, facts |
| 10 | [`10-compact.png`](./mockups/10-compact.png) | Compact variant of Simple and Advanced |

Example query throughout (checked with `liquers-validate`):
`-R/data/sales.csv/-/ns-pl/gt-amount-1000/sort-amount-f/head-20`, and in Advanced mode the
variant `…/gt-amount-~X~-R/config/limit.txt~E/…` with a link argument.

## 1. Where it stands (PR #275, merged 2026-11-17)

- `liquers-lib/src/ui/editor/` holds the backend-free core: `EditorState` applies `EditorCommand`s
  and builds an `EditorTree` of `EditorNode`s (`Resource`, `Namespaces`, `Action`, `Directive`,
  `Value`, `Nested`, `Opaque`). Each action node has its `EditorField`s with the widget
  `effective_gui` picks, plus `presets`, `next` and `filename` from the command metadata.
- `liquers-lib/src/egui/editor.rs` (`show_editor`) draws the tree in one flat column: a toolbar
  ("Update immediately", Update, Revert), the highlighted query text, then every node. An action
  is a `CollapsingHeader` with a three-column `egui::Grid` (label, editor, `link`/`↺` buttons),
  followed by a detached row with `presets`, `next`, `filename` and `⋯`.
- `liquers-lib/src/ui/web/editor.rs` renders the same tree as HTML.
- The query console's Parameters panel already edits the last action only, which is Simple mode
  in embryo.
- No theme code exists: egui's default visuals are used.

Pain points (mockup 1): no frames between actions; the action menu sits below the fields, so it
reads as belonging to the next action; every segment is editable at once; the grid is cramped
and uses raw argument names; `link` and `↺` repeat at full strength on every row; no modes, icons
or shortcuts.

## 2. Proposed reference design

### 2.1 Modes

**Simple** (mockup 2). The query text, a read-only row of step chips (`data/sales.csv › gt › sort
› head`), one card for the **last action**, and "Continue with" chips from that command's `next`
presets followed by "Add action…". No tree elements: no outline, no nesting, no link subtrees (a
link argument shows its query text; editing inside it needs Advanced). Clicking an earlier step
chip switches to Advanced with that card focused.

**Advanced** (mockup 3). Every segment is editable, grouped as **Key part** (the resource: header
and key, with the store browser) and **Query part** (namespaces, actions, directives). Tree
elements appear: a collapsible outline pane on the left mirroring the structure, segment group
labels, and link arguments nested under their field with a dashed guide line. Cards gain Move up
and Move down. "Collapse all" and "Expand all" sit below the stack.

The mode is a per-editor UI setting (`EditorView`, not the tree), remembered per editor id, with
Simple as the default for the query console and Advanced for recipe editors. Open question:
should `EditorTree` gain a `focus: Option<NodePath>` so the core, not each renderer, decides what
Simple shows?

### 2.2 The action card (mockup 4)

- **Frame.** `egui::Frame` with the card fill, a 1 px line, 8–9 px radius and a soft shadow. The
  focused card gets an accent outline; a card with a refused edit gets a danger outline.
- **Header.** The pipeline should read as a sequence of human steps, not ids (mockup 4a).
  Collapsed: chevron, icon tile, the command **label** ("Greater than"), then the dimmed
  `namespace/name` (`pl/gt`); at the right a modified dot, an error mark, the position and a `⋯`
  menu. Expanded: two lines, `[realm] namespace/name` small and dimmed above, the label below.
  The realm chip appears only when the registry has more than one realm. The variant with icon
  and label only is the most readable but hides which command runs. Step chips in Simple mode and
  the Advanced outline use labels too. Many argument labels are still ids (`n`, `sigma`), so
  better labels are part of the work.
- **Action toolbar** (expanded only). Reset · Presets · Next · (Advanced: Move up, Move down) ·
  Delete at the far right, away from Reset.
- **Form grid.** Three columns: labels right-aligned (the argument `label` from metadata, required
  ones marked), editors at full width, and a narrow column for per-field actions (link ↔ value,
  reset). Per-field actions are faint until the row is hovered or focused, solid when active.
  The argument description is a help line under its editor; the default is placeholder text plus
  "default 5" beside short fields. A refused edit shows its message under the field.

In egui: `egui::collapsing_header::CollapsingState` with a custom header row inside a `Frame`, and
`egui::Grid::new(..).num_columns(3)` with fixed label and action column widths. The browser
renderer uses CSS grid with the same tokens.

### 2.3 Presets (mockup 5)

| Option | For | Against |
|---|---|---|
| A. Menu (today) | Compact; any number; room for preview | Two clicks; can't show the active preset |
| B. Buttons | One click; active one highlighted | Wraps with many or long labels; Delete moves |
| **C. Chips + overflow** | Common ones one click away; fixed toolbar height; active chip lit | Needs toolbar fitting |

**Decision: try C.** Show the presets that fit (at most 3) as chips and the rest under "+N";
light the chip whose values equal the current parameters; number them so `Alt+1`…`Alt+9` applies
one. Argument presets stay a small `▾` inside their field. **Next** stays a menu in the toolbar
(it adds a card rather than changing this one) and appears as "Continue with" chips under the
last card.

Note: no registered command declares `preset:` or `next:` today (`specs/command_registry.yaml`
has none), so the feature is invisible until some commands get them. The mockup's `head` presets
are invented.

### 2.4 Icons (mockup 8)

**Do actions have icons? Not yet.** `CommandMetadata.hints` is a free map and `register_command!`
accepts `hint icon: "…"` (tested in `liquers-lib/tests/register_command_metadata.rs`), but none of
the 192 registered commands sets it. (`unicode_icon` in asset metadata is a different thing.)

Proposal, a fallback chain so every card has an icon from day one:

1. The command's own icon (`hint icon: "<ref>"`).
2. Otherwise its namespace's icon and colour.
3. Otherwise a monogram: the command's first letter on the namespace colour. An icon reference
   that cannot be resolved also falls back here.

**Icon set: Phosphor** (decided 2026-11-21; comparison in mockup 9). MIT, 1,512 icons in 6
weights; the `egui-phosphor` crate gives egui the font and name constants (check its release for
egui 0.36), and the web font serves the browser renderer, so one name works in both. Then add
`hint icon` to the most used commands of `pl`, `img` and `rec`.

**Namespaces are open.** Users define their own namespaces and commands, so the namespace icons
and colours cannot be a fixed table. The built-in namespaces get defaults in the theme module, and
a namespace must be able to declare its own icon and colour. Nothing in the registry describes a
namespace today (there is no namespace metadata, only `Namespaces` lists in pipelines), so this
needs a place: a namespace-level metadata entry in the command registry, or the theme or
environment configuration. Section 7 records the wider model; this design needs only the
fallback chain and a way to override the built-in table.

### 2.5 Jump from the query text to a field (mockup 6)

The parser already keeps source positions: `ActionRequest.position` (the command name) and
`ActionParameter::String(_, Position)` / link parameters (`liquers-core/src/query.rs`).
`Position.offset` is a byte offset of the **start** only.

- The tree records, per action node and per field, the byte span in `EditorTree.text`: a token
  ends where the next begins, because escapes (`~I`, `~_`) make the decoded length differ from the
  source length. Spans come from re-parsing the encoded text the tree shows.
- Cursor offset → the action whose span contains it → the parameter whose span contains it → the
  argument, through the same positional binding the editor already uses for fields (`excess`
  parameters map to no field). On the name or a separator → the card header. Inside a link →
  the nested node (Advanced only). On the resource → the resource card (Advanced only).
- While the cursor moves, the matching action and parameter are highlighted and a hint names the
  target (`gt › Value · Ctrl J`). `Ctrl+J` (or Ctrl-click) expands the card and focuses the
  editor; `Ctrl+J` from a field goes back to the text with that parameter selected.
- In Simple mode, a target outside the last action shows "Open in Advanced (Ctrl B)" instead of
  jumping.

### 2.6 Compact variant (mockup 10)

For side panels and the query console: the same structure at about half the height. Collapsed
cards become 28 px rows separated by hairlines and only the open card gets a frame; the toolbar
moves into the open card's header as icon buttons (Reset, Presets ▾, Next ▾, Delete); controls are
23 px with a 4 px row gap and help text goes to tooltips; the window chrome shrinks to an
icon-only mode switch. Only density tokens differ, so it is a setting (Comfortable / Compact) in
the theme rather than a second design.

## 3. Keyboard shortcuts

`Mod` is Ctrl on Windows and Linux, Cmd on macOS. Focus has three levels: the query text, the
card headers, the fields. Reserved browser keys (`Mod+T/W/N`, `Ctrl+Tab`, `Ctrl+PgUp/PgDn`) are
avoided so the browser renderer can use the same table; the others are taken with
`preventDefault` only while the editor has focus.

| Keys | Where | Does |
|---|---|---|
| `Mod+B` | anywhere | Toggle Simple / Advanced (shows or hides the tree, like a side bar) |
| `Mod+L` | anywhere | Focus the query text |
| `Mod+J` | query text | Jump to the action and field at the cursor |
| `Mod+J` | field | Back to the query text, with that parameter selected |
| `Mod+click` | query text | Same as `Mod+J` at the clicked position |
| `F6` / `Shift+F6` | anywhere | Next / previous region: query text → outline (Advanced) → cards → footer |
| `Tab` / `Shift+Tab` | fields | Next / previous field; past the last field of a card, into the next card |
| `PgDn` / `PgUp` | header or single-line field | Next / previous action card (focus its header, scroll into view) |
| `Alt+PgDn` / `Alt+PgUp` | anywhere, also in text areas | Same, where PgUp/PgDn scroll text |
| `Home` / `End` | header | First / last card |
| `Enter` | header | Into the card's first field (expands it) |
| `Esc` | field | Discard the uncommitted typing; a second `Esc` goes to the card header |
| `Esc` | header | To the query text |
| `Space`, `←` / `→` | header | Toggle, collapse / expand the card |
| `Mod+Shift+[` / `Mod+Shift+]` | anywhere | Collapse all / expand all |
| `Alt+R` | card | Reset the action's parameters |
| `Alt+P` | card | Open the presets menu |
| `Alt+1` … `Alt+9` | card | Apply preset 1–9 |
| `Alt+N` | card | Open the next menu |
| `Mod+K` | anywhere | Add an action after the focused one (or append): opens the command picker |
| `Alt+↑` / `Alt+↓` | card (Advanced) | Move the action up / down |
| `Mod+Delete` | header | Delete the action |
| `Alt+L` | field | Switch between value and link |
| `Mod+Space` | field | Argument presets or enum alternatives |
| `Mod+Enter` | anywhere | Update (when not updating immediately) |

Risks to check during the design: `Alt`+letter on macOS types characters (Option+A = å), so
these must match the key code, not the character; the survey notes the Craft egui fork fixed
exactly this for non-Latin layouts. `Mod+J` and `Mod+L` are browser shortcuts that a page can
override, but only while the editor has focus.

## 4. Theme (proof of concept, in the same design)

The theme is part of this design, as a PoC for a later proper theme design. The look follows the survey's recommendation (`specs/archive/2026-11-19-egui-craft-suite-survey.md`
§4): a `theme` module in `liquers-lib` with one token struct, `dark()` and `light()`,
`apply(ctx)` for egui and `to_css()` for the browser renderer. The mockups use ArtCraft-derived
dark tokens (background `#121316`, controls `#1e1f22`–`#26272d`, ink `#f2f1ee`, accent `#2d81ff`,
danger) and a new light set, Inter for text and a mono face for command names and query text.
Namespace colours (pl teal, img violet, resource blue), namespace icons and the density tokens
(Comfortable / Compact) live in the same module. The PoC covers only what the editor needs.

### 4.1 Overrides, and whether CSS can style egui

**Precedence** for an icon or colour: theme override for the command (`[realm/]ns/name`) → theme
override for the namespace → the command's `hint icon` → the namespace metadata in the registry
(`NAMESPACES-HAVE-NO-METADATA`) → the theme's built-in table → monogram and neutral colour. A
colour in the registry is a named hue (`teal`), which the theme maps to a dark and a light value.

This is a small, keyed override table, not a selector engine, and it fits this design:

```yaml
theme: dark            # or light, or a file
density: comfortable   # or compact
tokens: { accent: "#2d81ff" }
hues: { teal: { dark: "#2fb3a3", light: "#1d8a7d" } }
namespaces: { pl: { icon: table, hue: teal }, my_ns: { icon: flask, hue: violet } }
commands: { "pl/gt": { icon: funnel } }
```

**CSS for egui: not recommended.** Checked on 2026-11-21:

- egui is immediate mode: there is no DOM and no element tree to match selectors against, so the
  cascade, `:hover`/`:focus`, specificity and layout properties have nothing to apply to. What
  egui can be given is a `Style`/`Visuals` (global or per `Ui` scope) and our own token values.
- [`egui_css`](https://github.com/SergioRibera/egui_css) (0.1.1, January 2025, egui 0.29) is the
  only crate that tries; its README calls it "very very very WIP", lists layout, media queries and
  pseudo-classes as missing or broken, and it is two egui releases behind (Liquers is on 0.36).
- The rest of the egui theming crates are token or utility systems, not CSS:
  [`egui_styled`](https://docs.rs/egui_styled) (Tailwind-like utilities, active),
  [`egui-theme`](https://docs.rs/egui-theme) (a serializable theme container, last release 2022),
  Rerun's `re_ui` design tokens, `catppuccin-egui` palettes. They confirm the token approach;
  none is worth a dependency for this PoC.
- In the browser CSS is native, so the theme is emitted as custom properties
  (`--lq-accent`, `[data-theme=dark]`, `[data-lq-ns="pl"]` for namespace hues) by `to_css()`.

So the theme is data (serde), applied to egui by code and to the browser as generated CSS. A
middle way stays open: write the theme as CSS custom properties only (`:root { --lq-accent: … }`,
`[data-lq-ns="pl"] { --lq-ns-hue: teal }`), which the browser uses as is and egui reads as plain
key-value tokens with a small parser (`cssparser`), never as general CSS. It is tempting because
one file serves both renderers, but it invites users to expect real CSS in egui. Recommended:
serde configuration now; reconsider in the later theme design.

### 4.2 A universal CSS-like resolver for egui, the browser and a TUI?

Asked by Orest on 2026-11-21; surveyed on crates.io the same day.

| Candidate | What it is | Fit |
|---|---|---|
| [`selectors`](https://crates.io/crates/selectors) (servo) | Selector parsing and matching over **any** tree that implements its `Element` trait; specificity included. MPL-2.0, ~40M recent downloads, part of servo/stylo | **Best building block.** Our element paths implement `Element`; the cascade is ours |
| [`cssparser`](https://crates.io/crates/cssparser) (servo) | Tokenizer and rule/declaration parser. MPL-2.0 | Building block for reading a stylesheet |
| [`lightningcss`](https://crates.io/crates/lightningcss) | Parser, transformer, minifier for browser CSS | Parsing only; heavier than `cssparser`, no matching over our tree |
| [`stylo`](https://crates.io/crates/stylo) | Servo/Firefox's complete style engine | Computes browser properties for a DOM; far too heavy for tokens |
| [`egui_css`](https://github.com/SergioRibera/egui_css) | CSS for egui | "Very very very WIP", egui 0.29, pseudo-classes broken |
| [`ratatui-style`](https://crates.io/crates/ratatui-style) | Cascade with selectors, specificity, pseudo-states for ratatui | Proves the idea for a TUI, but outputs ratatui types only |
| [`style-cascade`](https://crates.io/crates/style-cascade) | Cascade with `var()` and `@media` | Tied to its own accessibility DOM; first release 2026-11-06 |
| `style-engine`, `csscascade`, `muskitty-cascade` | Generic cascade engines | One release or pre-release each; too young to depend on |

No universal library fits as is. What would work, later, is a **style query**: a widget describes
itself as a short path (`editor > card[ns=pl][command=gt]:focus > field[type=int]`), the theme
resolves it with `selectors` matching and our own cascade, and returns **tokens** that each
renderer maps (egui `Visuals`/`Frame`, terminal styles; the browser can use real CSS). In egui the
path is built while drawing, so no DOM is needed, and results are cached by path. Python's
Textual does this for terminals with its own CSS dialect, a useful reference.

For this design: the editor asks the theme through a small resolver interface (element kind,
namespace, command, realm, state → tokens) implemented with the keyed overrides of §4.1, so a
selector-backed resolver can replace it without touching the widgets. The resolver itself is
filed as `NO-RENDERER-NEUTRAL-STYLESHEET` for the later theme design.

### 4.3 The theme subset of this design

Decided 2026-11-21: YAML authoring, one token model in Rust, CSS generated for the browser
([`theme-formats.md`](./theme-formats.md)). This design defines the part of the model the editors
need, named so the rest of the UI can use it too (it is not limited to the editors).

| Group | Tokens | Used by |
|---|---|---|
| Surface roles (per mode) | `bg`, `panel`, `card`, `card-head`, `control`, `control-hover`, `line`, `line-strong`, `shadow` | Window, cards, inputs |
| Text roles (per mode) | `ink`, `ink-muted`, `ink-faint`, `on-accent` | Labels, help lines, ids |
| State roles (per mode) | `accent`, `accent-wash`, `focus-ring`, `danger`, `warning`, `success`, `modified` | Focus, errors, the modified dot, preset chip |
| Query syntax (per mode) | `header`, `resource`, `separator`, `namespace`, `action`, `parameter`, `entity`, `link`, `filename`, `plain`, `highlight`, `error` | The highlighted query text in egui (`egui/widgets.rs` `styled_query_to_layout_job`, colours hard-coded today), the browser (`lq-tok-*` classes) and the terminal (`DarkAnsiQueryRenderStyle` in `liquers-core/src/query.rs`) |
| Hues | named hues with `dark`, `light`, `ansi` | Namespace tiles (registry metadata names a hue) |
| Density (Comfortable, Compact) | `control-height`, `row-gap`, `card-gap`, `card-radius`, `card-padding`, `label-width`, `icon-size`; `cells` variants for a terminal | Cards and the form grid |
| Typography | `font-ui`, `font-mono`, `size-body`, `size-small`, `size-title` | All text; mono for queries and ids |
| Icons | `icon-set` (Phosphor), `namespaces.<ns>`, `commands.<ns>/<name>` (icon, hue) | AC-8, AC-14 |
| Terminal fallbacks | `terminal.<role>` as ANSI names | A later TUI; the query syntax already has an ANSI renderer |

The query syntax group is keyed by token kind, matching the `SyntaxKind` of
`query-editor-live-validation` (`Separator`, `Header`, `ResourceName`, `ActionName`, `Parameter`,
`Entity`, `Filename`, `Plain`) plus what the editors add (`namespace`, `link`, `highlight`,
`error`). Today's `StyledQueryToken` maps onto the same keys.

### 4.4 How UI elements reach the theme

Today an element gets everything through `UIContext` (`liquers-lib/src/ui/ui_context.rs`): the
`AppState` behind a `tokio::sync::Mutex`, the message sender, its handle, and since #275 the
command metadata registry as an `Arc` snapshot (`with_command_registry`). Options:

| | Where the theme lives | Read per frame | Change at run time | Persisted | Assessment |
|---|---|---|---|---|---|
| **A. In `UIContext`** | `Arc<Theme>` beside the registry (`with_theme`, `theme()`), set by the runner or host | Lock-free `Arc` read | The runner swaps the `Arc` and repaints | No (the host's job) | Simplest; mirrors the registry; renderer-neutral |
| **B. In `AppState`** | A `theme` field or trait method on `AppState` (`DirectAppState`) | Needs the mutex (`try_sync_lock`) every frame | Any command can set it (`lui/set_theme`) | Yes, saved with the app state | Persists per UI document, but puts styling behind the state lock and into every `AppState` implementation |
| **C. As an asset** | A key or query (`-R/config/theme.yaml`), loaded and watched like any asset; the runner keeps the resolved `Arc<Theme>` | Via A | Editing the file in the store reloads it (asset notifications) | In the store | The most Liquers-like: a theme is data in a store, editable with the existing editors |
| D. In egui's context | `ctx.data` / `Style` only | Fast | egui only | No | Rejected: nothing for the browser or a TUI |

**Recommended: A for reading, C for the source.** Elements call `ui_context.theme()` and never
lock anything; the runner owns where the theme comes from: the built-in default, a file, or a
key/query loaded as an asset and reloaded when it changes. A theme *name or reference* (not the
whole theme) may also be kept in `AppState` (B, small), so a saved UI reopens with its theme;
the runner resolves it. Mode and density are separate small settings, also in the UI state, so a
user can flip dark/light without touching the theme file.

### 4.5 The theme in the environment configuration

**Possible today, with one gap.** `ConfigDocument` (`liquers-core/src/environment_config.rs`)
already splits a configuration document into the environment's keys and *host sections*: a host
takes its own with `take_section::<T>("name")`, and `finish()` refuses any section nobody took,
so a typo fails loudly. `liquers-axum` takes `server` this way, `liquers-py` takes `python`, and
the user guide shows a section of one's own (`liquers-lib/tests/user_guide_GUIDE.rs`, `report`).
`EnvironmentConfig` on its own ignores unknown keys. So a `ui` section needs no change to the
configuration mechanism:

```yaml
store: { stores: [] }
ui:
  theme: -R/config/liquers-theme.yaml   # a key or query, a file path, or a built-in name
  mode: dark                            # dark | light | system
  density: comfortable                  # comfortable | compact
```

The gap: the egui native host (`liquers-lib/src/ui/native.rs`, `egui-native-app`) does not read a
`ConfigDocument` today, so nothing would take the `ui` section. This design would add a typed
`UiConfig` taken by the UI hosts (native egui window, the browser host, the Python window), with
`ui.theme` resolved as in §4.4 C. Recommended: offer both, the `ui` section as the default
source and `AppState`'s theme reference as a per-UI override.

## 5. Open questions

Kept in [Phase 1](./phase1-high-level-design.md#open-questions).

## 6. Gaps found

- No registered command declares `preset:`, `next:` or `hint icon`: in scope (Phase 1 AC-13).
- `Position` records start offsets only: Phase 2 decides between spans from
  `query-editor-live-validation`'s tokens and spans derived here.
- The registry has no namespace metadata: filed as `NAMESPACES-HAVE-NO-METADATA`.
- Icons cannot be assets or Unicode, and there are no tool palettes: filed as
  `ICONS-CANNOT-BE-ASSETS-OR-UNICODE` (basis in §7).

## 7. Basis for later design: icon definitions, tool palettes and the TUI

Not requirements of this design; recorded from Orest's notes (2026-11-21) so the later designs
start from them.

**Why commands and namespaces need icons beyond the editor.** Some commands behave like tools a
user picks from a **palette of buttons** (drawing, image operations, inserting a step), so the
command's icon becomes its primary face, not a decoration. Users also define their own namespaces
and commands, so every icon must be declarable, not chosen by the UI code.

**An icon reference, three kinds.** One value type, used for a command (`hint icon`), a namespace,
and later a palette entry:

| Kind | Example | For | Renders in |
|---|---|---|---|
| Asset | an SVG or PNG by key or query (`-R/icons/brush.svg`) | Non-standard icons, palettes, branding | egui (image loader), browser; TUI falls back |
| Icon set | `ph:funnel` (Phosphor by default, so `funnel` alone works) | Most commands and namespaces | egui font, browser web font; TUI falls back |
| Unicode | `⏷`, `∑`, `🖌` | Terminal UI; quick user definitions | Everywhere, including the TUI |

A definition may carry several kinds at once (set icon plus a Unicode fallback, or an asset plus
a set icon), and each renderer takes the first it can draw: the TUI prefers Unicode, the GUIs
prefer an asset or set icon. Without any, the monogram applies. Open points for that design: the
string syntax for the reference (prefixes like `ph:`, `asset:`, `u:` versus a structured hint),
whether an asset icon is a query (cached, versioned like any asset) or a plain key, tinting rules
for SVG (recolour to the text colour or keep its colours), PNG sizes for high-DPI, and a check at
registration that a set name exists.

**Decided 2026-11-21:** the icon definition is now in scope (Phase 1 AC-17; types in Phase 2,
`ui/theme/icon.rs`): an icon is a list of variants (set reference, SVG, raster; inline, embedded
in code or by key) with an always-available Unicode text. Palettes stay out
(`COMMANDS-HAVE-NO-TOOL-PALETTE`).

**Unicode and the TUI.** A terminal UI is planned; it can show neither SVGs nor icon fonts reliably,
so Unicode icons (and the monogram) are what it draws. That argues for letting every built-in
command and namespace carry a Unicode fallback from the start. Asset metadata already has a
`unicode_icon` field (`liquers-core/src/metadata.rs`), a precedent for the name and the idea.

**Palette.** A grid of icon buttons, each applying a command (possibly with a preset) as the next
action or as the tool for an interactive element. It needs: which commands appear (a hint such as
`toolbar` or `palette`, which `CommandMetadata.hints` already mentions as an example), grouping
(by namespace or declared group), icon size, and tooltips from the label and doc. The icon
reference above is its foundation.
