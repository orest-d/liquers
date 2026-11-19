# egui in the Craft app suite, and whether Liquers should depend on craft-ui

A survey taken 2026-11-17 to 2026-11-19, true on those dates. It asks three questions: which GUI
toolkits the Craft applications listed on [craft.wiki](https://craft.wiki/apps/craft-anything/overview/)
use, which reusable egui components they have produced, and whether Liquers should build its
styling on their `craft-ui` crate or on a theme layer of its own. The aim behind the last question
is a look similar to ArtCraft's, with dark and light themes, and a web version styled to match.

Sources: the craft.wiki overview pages, the root `Cargo.toml` of each repository listed below
(read from `HEAD` through `raw.githubusercontent.com`), the `craft-libs` README and its
`crates/ui` and `crates/window` READMEs, `artcraft-native/src/theme.rs`, and the GitHub API
(repository metadata, commit history of `crates/ui`, and a comparison of the egui fork with
upstream). Nothing here was built or run.

## 1. GUI toolkits across the suite

The craft.wiki pages name no GUI stack; they list platforms (Windows, macOS, Linux, FreeBSD and,
for most apps, a web version), licence (MIT or Apache-2.0) and the `github.com/storytold/<app>`
repository. The repositories answer the question.

| App | GUI | Renderer | Notes |
|---|---|---|---|
| PhotoCraft | egui 0.37 through `eframe` | wgpu | `photocraft-ui-egui`, a separate `photocraft-gpu`; a web build |
| VectorCraft | egui | not checked | `craft-window` with `features = ["egui"]` |
| FilmCraft | egui 0.37 (`filmcraft-ui-egui`) | not checked | same egui fork pins as PhotoCraft |
| WordCraft | egui | not checked | `vello_cpu` for page rendering |
| CADCraft | egui 0.37, `egui-wgpu` | wgpu | `tiny-skia` rasterising, `krilla` for PDF plots |
| Craft Anything | egui 0.36 (`craft-anything-ui-egui`) | not checked | |
| ArtCraft Studio | two desktop apps | glow (native app) | `artcraft-native`, "pure Rust: an egui UI … (no Tauri, no webview)"; the older `crates/desktop/artcraft` is Tauri 2 |

Not checked: LightCraft, DesignCraft, FigCraft, TextureCraft, MaterialCraft, EffectCraft, AniCraft,
SoundCraft, GridCraft, DeckCraft, PdfCraft, and the third-party SolidCraft and MailCraft.
`craft-ui`'s README says it serves DesignCraft, EffectCraft, PDFCraft, LightCraft and SoundCraft as
well, so they are egui too.

Common pattern:

- **One toolkit, forked.** The apps patch the whole egui family (`egui`, `epaint`, `eframe`,
  `egui-winit`, `egui-wgpu`, `egui_glow`, `egui_extras`, `egui_kittest`, …) to
  [storytold/egui](https://github.com/storytold/egui) and `winit` to `storytold/winit`, at pinned
  revisions.
- **A shared framework.** [storytold/craft-libs](https://github.com/storytold/craft-libs) — a Cargo
  workspace of `craft-<name>` crates, consumed by git and `rev`, behind a `craft::ui::…` facade.
- **Immediate-mode chrome, separate rendering.** egui draws panels and tools; documents are drawn
  by dedicated crates (wgpu, `vello_cpu`, `tiny-skia`).
- **Desktop and web from one codebase** — egui on wasm, with a `wasm-release` profile tuned for
  size.
- **UI tests** with `egui_kittest`, snapshot tests in PhotoCraft.

## 2. Reusable egui components

All in `craft-libs`, MIT OR Apache-2.0. Its README says libraries "may be promoted out" into the
wider ecosystem; none is yet.

**`craft-ui`** — "Reusable egui controls and icon catalogs for Craft frontends". Exported modules
(`lib.rs`): `buttons`, `capture`, `combo`, `direction`, `docking`, `fonts`, `icons`, `ime`,
`layout`, `localized`, `panes`, `printing`, `tabs`, `testing` (feature `screen-testing`), `text`,
`theme`. The notable ones:

- `docking` — transactional panel moves, a docking surface, tab groups, floating frames, iconic
  flyouts, and host ownership hooks for drawing the chrome. The most valuable piece.
- `tabs` (fitting and interaction), `layout` (toolbar fitting, split geometry, resize handles,
  `Section` frames for dialogs and inspectors), `buttons` and `icons` (accessible icon buttons over
  embedded, recoloured SVG catalogs), `localized` (region-aware number fields), `fonts` (one
  fallback font stack), `printing` (printer and job controls), `theme` (widget-state styling only).
- It bundles no icons, fonts or themes: "complete themes … remain in each application".

**App-shell crates:** `craft-window` (who draws the window frame on each desktop — the app only on
Wayland without `zxdg_decoration_manager_v1`, i.e. GNOME — plus window lists for one window per
document), `craft-menu` (a macOS menu bar feeding key equivalents back into egui),
`craft-dialogs` (native dialogs that never block the event loop), `craft-keymap` (chords, scopes,
conflicts, user overrides, imported shortcut sets), `craft-quit`, `craft-font-manager` (with
feature `egui`, a complete `FontsPanel`), and below the UI `craft-i18n`, `craft-settings`,
`craft-update`, `craft-web-host`.

**The egui fork** was 90 commits ahead of and 29 behind `emilk/egui` `main` at revision
`6d518fc7`. Most of the lead is upstream fixes taken early; the suite's own work is right-to-left
text and layout (bidi shaping, `Style::right_to_left`, based on the draft emilk/egui#8577), letter
keys from key codes on non-Latin layouts (Windows and web), IME focus and glyph-clipping fixes,
and web screen-reader language.

## 3. craft-ui as a dependency of Liquers

### Maintenance

| Signal | Value |
|---|---|
| `craft-libs` created | 2026-10-09 |
| Churn | 100 commits to `crates/ui` between 2026-10-24 and 2026-11-17 |
| Versioning | `0.1.0`, unchanged; consumers pin a git `rev` |
| Open issues on `craft-libs` | 867 |
| Contributors | many; activity daily (last push 2026-11-19) |
| egui | 0.37 |
| Required dependencies | `craft-i18n`, `craft-fonts-build` (build-time inputs from `CRAFT_FONTS_DIR` / `CRAFT_I18N_DIR`), `skrifa`; macOS `objc2` |
| crates.io | no `storytold` publication. A `craft-ui` 1.4.7 exists there, pointing at `aphrody-labs/craft-libs` — not established to be the same code |

Well maintained, not stable: it serves the Craft suite, and a downstream user should expect API
breaks at every revision bump. Not established: whether `craft-ui` needs the egui fork, or only
the apps do.

### Where ArtCraft's look comes from

`artcraft-native` does **not** depend on `craft-ui` or any `craft-libs` crate; it uses stock
egui 0.36 with glow. Its whole look is `src/theme.rs`, 193 lines: about 25 colour constants
(`BG 0x121316`, `PANEL 0x101014`, `CONTROLS 0x1e1f22`, `INK 0xf2f1ee`, translucent `LINE` / `WASH`
layers, `ACCENT 0x2d81ff`, `DANGER 0xd33242`), `RADIUS` 3, `CONTROL_H` 34, the fonts Inter,
Archivo and Geist Mono (OFL), applied over `Visuals::dark()`. It is dark only — "The app is dark
only, like the webapp" — and its comments name the web app's CSS tokens (`ui-panel`): the egui
theme mirrors the web design by hand.

### Liquers at the time

- `liquers-lib` uses egui / eframe / egui_extras 0.36.1 (locked 0.36.2), `egui_plot` 0.37.0 and
  `egui_commonmark` 0.25.0. Moving to egui 0.37 means moving each add-on with it.
- No theme code: no `Visuals` or `Style` customisation in `liquers-lib/src`.
- The web UI (`liquers-lib/src/ui/web/`) is not egui on wasm. It renders HTML strings
  (`UIElement::render_web`) with `lq-*` CSS classes. `craft-ui` cannot reach it.

### Trade-off

| Gained with craft-ui | Given up |
|---|---|
| Transactional docking (expensive to write) | Choosing the egui version and when to upgrade |
| Tab fitting, toolbar overflow, resize handles | A stable API |
| Accessible icon buttons, SVG catalogs, localized number fields | A small dependency tree |
| IME and international-keyboard fixes (mostly in the fork) | — and dark/light/web consistency still has to be built: craft-ui has none |
| | Component design shaped by the Craft apps' needs rather than Liquers' UI model |

## 4. Recommendation

Build a theme layer of our own; do not take `craft-ui` as the foundation.

1. A `theme` module in `liquers-lib`: a token struct with `dark()` and `light()`, `apply(ctx, …)`
   for egui, and `to_css()` emitting the custom-property block (`:root`, `[data-theme=…]`,
   `prefers-color-scheme`) that the `lq-*` classes consume. One source of truth for both
   renderers. ArtCraft's dark palette and fonts are a reasonable starting point (MIT/Apache, OFL);
   its branding is not. The light theme is new design work either way.
2. A few widgets that read the tokens: section frame, icon button, toolbar.
3. Docking only when a Liquers UI needs dockable panes: `craft-ui` alone behind a feature, a
   vendored copy of its docking module with attribution, or `egui_dock` from crates.io — compare
   them then.

This is a new feature across the egui and web UIs, so it goes through a design
(`liquers-project`), which also settles whether Python and JavaScript need theme selection.
