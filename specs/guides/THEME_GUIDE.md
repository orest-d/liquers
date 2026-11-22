---
title: Theme Guide
kind: guide
audience: internal
area: [lib/ui, lib/egui]
reviewed: 2026-11-22
---
# Theme Guide

How to write a Liquers UI theme, how to use the theme from UI code, and how to extend the model.
The format and the data structures are [`reference/THEME.md`](../reference/THEME.md); this
guide does not repeat them.

## 1. Write a new theme

### 1.1 Start from what changes

A theme file is a *layer* over the built-in theme, so it gives only what it changes. The smallest
useful theme recolours the accent of both modes:

```yaml
name: ocean
dark:
  state: { accent: "#2bb3c0", accent-wash: "#2bb3c024", focus-ring: "#2bb3c080" }
light:
  state: { accent: "#11818c", accent-wash: "#11818c1a", focus-ring: "#11818c80" }
```

Everything not named keeps its built-in value. To see every key you can set, open
`liquers-lib/src/ui/theme/builtin.yaml`: it is the complete built-in theme in the same format.
Copy only the parts you change; copying the whole file freezes today's defaults into your theme,
and it will not follow later improvements.

### 1.2 Check it

Compose it in a test or a scratch program before you use it:

```rust
use liquers_lib::ui::theme::{Theme, ThemeLayer};

let layer = ThemeLayer::from_yaml(&std::fs::read_to_string("ocean.yaml")?)?;
let theme = Theme::compose(&[layer])?;       // fails on a bad colour, size or icon
assert!(theme.unknown_keys().is_empty(), "typos: {:?}", theme.unknown_keys());
```

`compose` refuses wrong types, but a misspelt key (`acent`) is silently an *unknown* key, kept and
ignored by the renderers. `unknown_keys()` is the check for that, and nothing runs it for you.

### 1.3 Use it

Put the file in a store and name it in the configuration's `ui` section, as a resource query:

```yaml
ui:
  theme: -R/config/ocean.yaml
  mode: system
```

Without `-R/` the text is parsed as actions, not as a key. The runner loads the theme on its next
run and reloads it whenever the file is rewritten, so you can edit a theme with the window open.
A theme that fails to compose leaves the previous one in place and reports the error (an error
status, and a line on stderr).

### 1.4 Design rules

- **Change both modes.** A theme that sets only `dark` looks right until a user's desktop is
  light. Give every colour you change in `dark` a counterpart in `light`, and check both: the
  `ui_query_editor` example takes `light` and `compact` arguments
  (`cargo run -p liquers-lib --example ui_query_editor -- light compact`).
- **Keep the roles' meanings.** `ink`, `ink-muted`, `ink-faint` go from strongest to weakest;
  `bg`, `panel`, `card`, `card-head`, `control` should be distinguishable steps. A role is used in
  several places (§1.1 of the reference lists them), so pick a colour for the meaning, not for one
  widget.
- **Contrast.** Aim for at least 4.5:1 between `ink` and `card`, and 3:1 for `ink-muted`, in both
  modes. The query syntax colours are read on `control`; keep `separator` quiet and `action`
  strong, since actions are what a reader scans for.
- **Translucent lines and shadows.** `line`, `line-strong`, `shadow` and `accent-wash` work best
  with an alpha (`#ffffff12`), so they sit on any surface beneath them.
- **Spacing in both densities.** If you change `comfortable`, check that `compact` is still
  smaller in every key: the console uses Compact by default.
- **Fonts** (`typography.font-ui`, `font-mono`) apply in the browser only, as CSS font-family
  lists; end them with a generic family (`sans-serif`, `monospace`).

### 1.5 Icons and hues

Give a namespace or a command an icon and a tint:

```yaml
hues:
  coral: { dark: "#ff7b6b", light: "#c24a3b" }      # a new hue: define both modes
icons:
  namespaces:
    sales: { icon: [chart-line, u:$], hue: coral }  # every command of ns-sales
  commands:
    "pl/head": { icon: rows }                       # one command
    "sales/forecast": { icon: lib:crystal }         # an icon of the library below
  library:
    crystal:
      unicode: "◇"
      variants:
        - svg: { inline: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 2l8 10-8 10-8-10z" fill="currentColor"/></svg>' }
```

- Prefer a Phosphor name (<https://phosphoricons.com>, the kebab-case names of
  `ui/theme/icons.txt`); every renderer that knows Phosphor draws it without loading anything.
- Add a Unicode text (`u:…` in a list, `unicode:` in the full form) to any icon that matters on a
  text-only surface; otherwise the monogram of the label is used.
- An inline SVG should draw with `fill="currentColor"` or `stroke="currentColor"` so it takes the
  tile's tint; a fixed colour stays fixed. Scripts and `on…` attributes are removed in the
  browser.
- An image by key (`-R/icons/x.svg`) costs a store read and is not drawn in the browser yet; put a
  set name or an inline image before it in the list.
- A theme entry wins over the command's own `hint icon` (reference §2.4). To *suggest* an icon
  for namespaces you do not own without overriding their commands' hints, use `icons.defaults`.
- Name only hues the theme defines: the browser leaves a tile with an undefined hue untinted.

### 1.6 Extra settings

A key the model does not know is kept, so a component can be configured from the theme before
the model has a field for it:

```yaml
editor:
  card: { elevation: 2 }
```

Read it with `theme.lookup("editor.card.elevation")`. Such keys show in `unknown_keys()`; list
them in the component's documentation so a reader knows they are deliberate.

## 2. Use the theme in UI code

- **Read it from the context, every frame.** An element gets the theme from
  `UIContext::theme()`, a cheap clone of one `Arc`. Do not keep a `Theme` across frames: a theme
  file can be reloaded at any time. When something is expensive to derive from the theme (a
  texture, a layout), key the cache by `ThemeState::generation`.
- **Use roles, not colours.** Never hard-code a `Color32` or a CSS colour in an element. In egui
  take `crate::egui::theme::color(theme.theme.palette(theme.mode).text.ink_muted)`; in HTML write
  `var(--lq-ink-muted)`. Widgets that need only egui's defaults already follow the theme, because
  `apply_theme` sets the `Visuals`.
- **Spacing by density.** Use `theme.theme.spacing(density)` for sizes; in egui scope a density
  with `ui.scope(|ui| spacing_style(ui.style_mut(), &theme, density))`. An element that has its
  own density setting falls back to `ThemeState::density`.
- **Highlighted queries.** Use `egui::widgets::styled_query_to_layout_job(&styled, &theme)` in
  egui and `ui::web::widgets::query_to_html` in HTML; both colour tokens by `SyntaxRole`.
- **Icons.** Resolve with `theme.icon_for(&key, label, hint.as_ref(), None)` and draw with
  `egui::theme::draw_icon(ui, &resolved, size, tint, Some(ui_ctx))` or
  `ui::web::editor::icon_html`. Parse a hint with `Icon::from_hint(&hints["icon"])`; a parse
  error should fall back to the namespace's icon, not fail the card.
- **Without a context.** A widget with no `UIContext` reads `egui::theme::theme_of(ctx)`. A
  `render_web` keeps the `ThemeState` of its last `init`/`update` in a `#[serde(skip)]` field.
- **In a host.** Call `apply_theme(ctx, &ui_context.theme())` each frame and paint the
  background with `ui.visuals().panel_fill`; give the runner the `ui` section
  (`AppRunner::with_ui_config`, `NativeDriver::with_ui_config`, `WindowOptions::with_ui`).
- **Tests.** `ThemeState::default()` is the built-in dark theme; build a light one with
  `ThemeState { mode: ThemeMode::Light, ..Default::default() }`.

## 3. Extend the model

To add a token (a new role, a spacing key, a typography field):

1. Add the field to its struct in `ui/theme/model.rs` (kebab-case YAML comes from
   `rename_all`).
2. Give it a value in both palettes or both densities in `ui/theme/builtin.rs`.
3. Regenerate the fixture: `LIQUERS_BLESS=1 cargo test -p liquers-lib --lib ui::theme`, then run
   the same tests without the variable.
4. Emit it in `ui/theme/css.rs` if the browser needs it (`--lq-<name>`), and map it in
   `egui/theme.rs` if egui widgets should use it.
5. Update [`reference/THEME.md`](../reference/THEME.md) §1 (and its History).

A new field is required in the typed model, but a theme file never has to give it: composition
starts from the built-in theme. Existing theme files therefore keep working. Renaming or removing
a key does break them (the old key becomes unknown); prefer adding.

A new syntax role also needs a `SyntaxRole` variant, its entry in `SyntaxRole::ALL`, `name()` and
`SyntaxRoles::get`, and a token that produces it.

To add a built-in theme, add a `BuiltinTheme` variant and its name to `BuiltinTheme::ALL` in
`ui/theme/config.rs`, built as a layer over `Theme::builtin()` so it inherits every key.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-22 | Created: writing a theme, using it from UI code, extending the model. | `design/query-editor-ergonomics/` |
