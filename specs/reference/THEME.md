---
title: UI Themes
kind: reference
audience: internal
area: [lib/ui, lib/egui]
reviewed: 2026-11-22
---
# UI Themes

A *theme* gives the Liquers UI its colours, spacing, fonts and icons. One renderer-neutral token
model (`liquers-lib/src/ui/theme/`) is applied by egui (`liquers-lib/src/egui/theme.rs`) and by
the browser (CSS custom properties). A theme is written in YAML as a nested dictionary and
composed in *layers* over the built-in theme. How to write one:
[`guides/THEME_GUIDE.md`](../guides/THEME_GUIDE.md). Design:
`design/query-editor-ergonomics/`.

## 1. The model

`Theme` (`ui/theme/model.rs`) is the typed view of a composed theme:

| Field (YAML key) | Type | Holds |
|---|---|---|
| `name` | string | a name for people; nothing reads it |
| `dark`, `light` | `Palette` | the colours of each mode |
| `comfortable`, `compact` | `Spacing` | the sizes of each density |
| `typography` | `Typography` | font families and sizes |
| `hues` | map name → `Hue` | named colours with a dark and a light value, used to tint icon tiles |
| `icons` | `IconTheme` | the icon set, a library of named icons, and icons by command and namespace |

YAML keys are kebab-case (`card-head`, `ink-muted`); the Rust fields are their snake-case form.

### 1.1 Colours

A colour (`Rgba`) is a string `#rrggbb` or `#rrggbbaa` (sRGB, alpha last). Any other form is
refused with a configuration error naming the value. A colour is written back as `#rrggbb` when
opaque.

A `Palette` has four groups of *roles*. Each role is a meaning, not a widget: a renderer maps it
onto its own controls (§4).

| Group | Role | Meaning |
|---|---|---|
| `surface` | `bg` | the window or page behind everything |
| | `panel` | a panel's background (egui `panel_fill`) |
| | `card` | a card's body; also non-interactive widget fill |
| | `card-head` | a card's header strip |
| | `control` | an input, button or chip at rest |
| | `control-hover` | the same hovered or pressed |
| | `line` | a hairline border (usually translucent) |
| | `line-strong` | a border that must be seen: a hovered control, a dashed guide |
| | `shadow` | a card's shadow (usually translucent black) |
| `text` | `ink` | body text |
| | `ink-muted` | secondary text: `namespace/name`, labels |
| | `ink-faint` | the least important text: help, positions |
| | `on-accent` | text on an `accent` fill |
| `state` | `accent` | selection, the active preset chip, links, focus |
| | `accent-wash` | a translucent accent: the selection background |
| | `focus-ring` | the outline of a focused control |
| | `danger`, `warning`, `success` | status colours; `danger` also marks a refused edit |
| | `modified` | the mark of a card with unsaved edits |
| `syntax` | one per `SyntaxRole` | the colour of a token of a highlighted query (§1.2) |

### 1.2 Query syntax roles

`SyntaxRole` (`ui/theme/model.rs`) lists the token kinds of a highlighted query. Each has a key in
`syntax` and a CSS class `lq-tok-<name>`:

| Role | Colours | Produced today |
|---|---|---|
| `header` | a segment header (`-R`, `-q`) | yes |
| `resource` | a resource key's names | yes |
| `separator` | `/` and `-` | yes |
| `action` | an action name | yes |
| `parameter` | an action parameter | yes |
| `entity` | an entity (`~X`, `~.`) | yes |
| `highlight` | the token at the cursor or position | yes |
| `namespace`, `link`, `filename`, `error`, `plain` | an `ns-…` step, a link parameter, the filename, an error, other text | not yet: they wait for a tokenizer that tells them apart (`query-editor-live-validation`) |

`SyntaxRole::from(&StyledQueryToken)` maps the highlighter's tokens to roles; `Theme::syntax(mode,
role)` gives the colour.

### 1.3 Spacing and typography

`Spacing` holds sizes in logical points (CSS pixels in the browser), one set per `Density`:

| Key | Used for |
|---|---|
| `control-height` | the height of inputs and buttons (egui `interact_size.y`) |
| `row-gap` | the vertical gap between form rows (egui `item_spacing.y`) |
| `card-gap` | the gap between cards |
| `card-radius` | a card's corner radius |
| `card-padding` | a card's inner margin |
| `label-width` | the width of the label column of a form grid |
| `icon-size` | the side of a card's icon tile |

`Typography` holds `font-ui` and `font-mono` (CSS font-family lists) and `size-body`,
`size-small`, `size-title` (points). Only the browser reads the font families today; egui keeps
its own fonts and adds the Phosphor icon font as a fallback.

### 1.4 Hues

`hues` is an open map: a theme may add hues. A hue is `{dark: <colour>, light: <colour>}`. The
built-in theme has `amber`, `blue`, `grey`, `indigo`, `olive`, `orange`, `pink`, `sky`, `teal` and
`violet`. An icon entry names its tint by hue (§2.3). In egui a hue name the theme does not define
falls back to `accent`; in the browser it leaves the tile untinted, so define every hue you name.

## 2. Icons

### 2.1 An icon

`Icon` (`ui/theme/icon.rs`) is a list of alternative *variants*, best first, and an optional
Unicode text. A renderer draws the first variant it can; a text-only surface draws the Unicode.

| Variant (`IconVariant`) | Meaning |
|---|---|
| `Set { set, name }` | an icon of an icon set; only `phosphor` is drawn today |
| `Library(name)` | an icon of the theme's `icons.library`, expanded when resolved |
| `Svg(IconSource)` | an SVG image |
| `Raster(IconSource)` | a PNG, JPEG or WebP image |

An `IconSource` is `Inline(IconBytes)` (bytes in the theme or embedded in code with
`Icon::embedded_svg` / `Icon::embedded_raster`) or `Key(String)` (a store key or query, loaded by
the runner).

### 2.2 Written forms

Everywhere an icon is written (a theme, a command's `hint icon`), these forms are accepted:

| Form | Meaning |
|---|---|
| `funnel` | the icon `funnel` of the theme's default set, `icons.icon-set`: Phosphor unless a theme changes it (kebab-case names of `ui/theme/icons.txt`) |
| `ph:funnel` | the Phosphor icon `funnel`, whatever the default set |
| `<set>:<name>` | an icon of another set (not drawn yet; the next variant is used) |
| `lib:brush` | the theme library's icon `brush` |
| `u:⏷` | only a Unicode text |
| `-R/icons/brush.svg`, `icons/brush.png` | an image by key: a string with `/` or a leading `-` that ends in `.svg`, `.png`, `.jpg`, `.jpeg` or `.webp` |
| `[ph:funnel, -R/icons/f.svg, u:⏷]` | a list of the forms above, best first; a `u:` entry sets the Unicode text |
| `{unicode: "⏷", variants: [ph:funnel, {svg: {inline: "<svg …>"}}]}` | the full form |

In the full form a variant is a string form or one of `{svg: {inline: …}}`,
`{svg: {key: …}}`, `{raster: {inline: <base64>}}` (also `png:`), `{raster: {key: …}}`,
`{library: <name>}` and `{set: <set>, name: <name>}`. A set and icon name is ASCII letters,
digits, `-` and `_`.

### 2.3 Icons in a theme

`icons` (`IconTheme`):

| Key | Type | Meaning |
|---|---|---|
| `icon-set` | string | the set a bare name refers to; `phosphor` in the built-in theme, so a theme never has to give it. A set name (ASCII letters, digits, `-`, `_`), else composition fails. The built-in namespace table names its icons `ph:` explicitly, so it keeps drawing when a theme changes the set |
| `library` | map name → icon | named icons, referred to as `lib:<name>`. A library icon may not refer to another library icon |
| `commands` | map key → entry | by command: `pl/gt`, or `realm:pl/gt` for one realm |
| `namespaces` | map namespace → entry | every command of a namespace (`pl`; `""` is the root namespace) |
| `defaults` | map namespace → entry | the bottom of the precedence: the built-in table of the bundled namespaces lives here |

An *entry* (`IconOverride`) is `{icon: <icon>, hue: <hue name>}`, both optional. A `lib:`
reference to a name missing from `library` fails composition (§3).

### 2.4 Which icon a card draws

`Theme::icon_for(key, label, hint, namespace_icon) -> ResolvedIcon` collects candidates in this
order:

1. `icons.commands["realm:ns/name"]`, then `icons.commands["ns/name"]`;
2. `icons.namespaces["ns"]`;
3. the command's `hint icon`;
4. the namespace's own icon from the registry (none yet: `NAMESPACES-HAVE-NO-METADATA`);
5. `icons.defaults["ns"]`.

The variants of every candidate are concatenated in that order, with `lib:` references expanded,
so a renderer that cannot draw the first falls through to the next. A bare name's set is filled
in from `icons.icon-set` here (`IconTheme::resolve_set`), so a renderer only sees named sets;
`ResolvedIcon::variants` holds `Cow`s for that reason. The Unicode text is the first
candidate's that has one, else the *monogram* (the first letter or digit of the label, upper
case). The hue is the first one a theme entry gives. A `ResolvedIcon` therefore always has a
Unicode text.

### 2.5 What each renderer draws

| Variant | egui (`egui::theme::draw_icon`) | browser (`ui::web::editor::icon_html`) |
|---|---|---|
| Phosphor name it knows | a glyph of the Phosphor font | `<i class="ph ph-<name>" data-fallback="<unicode>">` (the page loads the Phosphor CSS; without it CSS shows the Unicode) |
| another set, unknown name | skipped | skipped |
| inline SVG | rasterized with `resvg`, cached as a texture | a data URI, after removing `<script>` and `on…` attributes |
| inline raster | decoded with `image`, cached | a data URI |
| image by key | requested through `UIContext::icon_bytes` (the runner loads it, `AppMessage::LoadIcon`); the next variant meanwhile | skipped (`WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`) |
| nothing drawable | the Unicode text | the Unicode text |

## 3. Layers and composition

`ThemeLayer` (`ui/theme/layer.rs`) is a partial theme: a JSON-compatible mapping.
`ThemeLayer::from_yaml` reads YAML (JSON is YAML too); `from_value` takes a structured value; an
empty document is an empty layer; anything but a mapping is refused.

**Merge.** `ThemeLayer::merge(&mut base, &over)` is a deep merge: where both sides hold a mapping
the two merge by key; anywhere else the value of `over` replaces the base's. A list is replaced
whole, never appended to. A key that is only in `over` is added. Merging is associative.

**Composition.** `Theme::compose(&[layers])` serializes `Theme::builtin()` to a dictionary,
merges the layers over it in order, and deserializes the result once. `theme.with_layer(&layer)`
does the same over an existing theme, so `compose(&[a, b])` equals
`compose(&[a])?.with_layer(&b)`. Because the base is complete, a layer gives only what it
changes. Composition fails (a configuration error) when a value has the wrong type (a colour that
is not `#rrggbb[aa]`, a size that is not a number, an icon that does not parse) or a `lib:`
reference is missing. A failed composition changes nothing.

**Unknown keys** are kept in the composed dictionary. `theme.lookup("editor.card.elevation")`
reads any value by dotted path, known to the model or not, so a component can carry its own
settings in a theme before the model has a field for them. `theme.unknown_keys()` lists the
topmost unknown paths (the open maps `hues`, `icons.library`, `icons.namespaces`,
`icons.commands` and `icons.defaults` accept any key): a lint for typos, which nothing reports
automatically yet. `theme.layer()` returns the composed dictionary.

**The built-in theme.** `Theme::builtin()` is built in Rust (`ui/theme/builtin.rs`) so it cannot
fail, and is shared as one `Arc`. `ui/theme/builtin.yaml` is the same theme written out, the
starting point for a new theme; a test keeps the two equal (`LIQUERS_BLESS=1 cargo test -p
liquers-lib --lib ui::theme` rewrites the file after a change to `builtin.rs`).

## 4. In the renderers

**egui** (`egui/theme.rs`, feature `egui`):

- `apply_theme(ctx, &ThemeState)` sets the context's `Visuals` from the palette of the state's
  mode, installs the Phosphor font, and stores the state in the context. It does the work once
  per (generation, theme) and returns at once otherwise, so a host calls it every frame. It
  requests a repaint after a change. The host still paints its own background:
  `ui.painter().rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill)`.
- The palette maps onto egui as: `panel` → `panel_fill`; `card` → `window_fill` and
  non-interactive widgets; `bg` → `extreme_bg_color`; `card-head` → `faint_bg_color`; `control`
  → `code_bg_color` and inactive / open widgets; `control-hover` → hovered and active widgets;
  `line` → `window_stroke` and non-interactive borders; `line-strong` → hovered borders;
  `accent` → hyperlinks and the selection stroke; `accent-wash` → the selection fill;
  `focus-ring` → active borders; `warning`, `danger` → `warn_fg_color`, `error_fg_color`; `ink`,
  `ink-muted` → widget text.
- `theme_of(ctx)` is the state last applied (the built-in default before any), for a widget with
  no `UIContext`. `color(Rgba) -> Color32`. `spacing_style(style, state, density)` applies a
  density's `control-height` and `row-gap` to a scoped style. `phosphor_glyph(name)` and
  `draw_icon` draw icons (§2.5).
- `egui::widgets::styled_query_to_layout_job(&StyledQuery, &ThemeState)` colours a query; the
  older `query_to_layout_job*` functions use `theme_of`.

**Browser** (`ui/theme/css.rs`, feature-free; mounted by `ui/web/app.rs`):

- `Theme::to_css()` writes custom properties: `--lq-<role>` for every surface, text and state
  role, `--lq-syntax-<role>` for the syntax roles, `--lq-hue-<name>` for every hue (each for the
  dark palette on `:root` and `[data-lq-theme="dark"]`, the light one on
  `[data-lq-theme="light"]` and under `@media (prefers-color-scheme: light)` when no mode is
  forced); `--lq-<spacing key>` in `px` on `[data-lq-density="comfortable"|"compact"]`;
  `--lq-font-ui`, `--lq-font-mono`, `--lq-size-*`; and one rule `.lq-tok-<role>{color:
  var(--lq-syntax-<role>)}` per syntax role.
- The mount writes that CSS and the editor's own `EDITOR_CSS` into `<style id="lq-theme">` and
  sets `data-lq-theme` (unless the mode follows the system) and `data-lq-density` on its root,
  again whenever the theme generation changes. A page can override any `--lq-*` property in its
  own stylesheet.
- A card's icon tile takes its tint from `--lq-card-hue`, set to `var(--lq-hue-<name>)` when the
  icon has a hue.

**Text-only surfaces** (a future TUI) draw `ResolvedIcon::unicode` and can read the palette as
they like; nothing in the model depends on a renderer.

## 5. At run time

`ThemeState { theme: Arc<Theme>, mode: ThemeMode, density: Density, generation: u64 }`
(`ui/theme/cell.rs`) is what a renderer reads. `ThemeMode` is `Dark` or `Light`; `Density` is
`Comfortable` or `Compact`. `ThemeState::default()` is the built-in theme, dark, comfortable,
generation 0.

`ThemeCell` is a shared, swappable `ThemeState` (an `Arc<RwLock<…>>`; clones share it).
`get()` clones one `Arc` under a read lock and never blocks on the app state; a poisoned lock still
yields the last value. `set_theme`, `set_mode` and `set_density` bump `generation`; `set_mode`
and `set_density` do nothing when the value does not change.

The **runner** owns the cell (`AppRunner::theme_cell`) and gives it to every `UIContext` it
makes; a host that builds its own context passes it with `UIContext::with_theme`. Elements read
`UIContext::theme()` (the built-in default when the context has no cell) and must not cache it
across frames: compare `generation` to notice a change. An element that renders HTML without a
context (`render_web`) keeps the `ThemeState` of its last `init`/`update` in a `#[serde(skip)]`
field.

`IconCache` holds icon images loaded by key, shared per runner; `UIContext::icon_bytes(key)`
returns them, requesting the load on first ask. The cache is cleared when the theme changes.

## 6. Configuration

A configuration document's `ui` section is `UiConfig` (`ui/theme/config.rs`; unknown keys are
refused):

```yaml
ui:
  theme: -R/config/liquers-theme.yaml   # `default`, or a key or query of a theme layer
  mode: system                          # dark | light | system (the default)
  density: compact                      # comfortable (the default) | compact
```

`theme` is a `ThemeSource`: a built-in name (`default` is the only one) or, when the text holds a
`/` or a `.`, a query. **Write a key as a resource query** (`-R/config/theme.yaml`): without
`-R` the text parses as actions. `UiConfig::cell(system_dark)` makes the initial cell from the
built-in theme; `AppRunner::with_ui_config` / `set_ui_config` gives the runner the section. On
its next run the runner evaluates the query, reads the value as a layer (YAML text, or a
structured value when the store already parsed the file), composes it over the built-in theme,
swaps it into the cell and clears the icon cache. It watches the asset and composes it again when
the key is rewritten or the value expires. A value that fails to compose is reported as an error
status (and on stderr) and the previous theme stays.

`system` is resolved by the host: `run_window` with `WindowOptions::with_ui` follows egui's
`system_theme()` each frame; the browser leaves `data-lq-theme` unset so the
`prefers-color-scheme` rule applies. Hosts that take the section: `run_window`
(`WindowOptions.ui`), `NativeDriver::with_ui_config`, the browser mount (`MountOptions.ui`), and
the Python configuration's `ui` section for `UI.run()`. `liquers-web`'s `mountUI` passes none yet
(`WEB-MOUNTUI-HAS-NO-THEME-OPTION`).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-22 | §2.2, §2.3, §2.4: a bare icon name means `icons.icon-set` (Phosphor by default), resolved by `icon_for`; `ph:` is always Phosphor. | `design/query-editor-ergonomics/`, `issues/THEME-ICON-SET-SETTING-IS-IGNORED.md` |
| 2026-11-22 | Created: the model, icons, layers, renderers, run time and configuration, reviewed against the implementation. | `design/query-editor-ergonomics/` |
