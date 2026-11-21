# Phase 2: Solution & Architecture — query-editor-ergonomics

## Overview

The backend-free model in `ui/editor/` gains a **layout** step: `editor_layout(tree, mode)` turns
the existing `EditorTree` into what both renderers draw (step chips, card groups with headers, the
outline), so Simple and Advanced are a projection of one tree. The tree gains **text spans**,
derived from parse positions in `liquers-lib` (no core change), which the jump (AC-10) uses in both
directions. A new renderer-neutral `ui/theme/` module holds one serde token model (YAML authored,
merged over a built-in theme built in Rust), resolves icons and query-syntax colours, and writes
CSS; egui applies it to visuals and fonts, and each editor scopes its own density. Elements read it
with `UIContext::theme()` from a `ThemeCell` (an `Arc` behind a `std::sync::RwLock`) that the runner
owns and swaps when a theme asset changes.

Rejected:
- Two trees built by `EditorState`: the mode is view state, so AC-3 keeps edits for free.
- Spans from `query-editor-live-validation`'s tokens: not implemented yet (its Step 1 is unticked).
- Theme in `AppState` (option B): per-frame mutex. A saved UI document naming its own theme is
  deferred (`UI-DOCUMENT-CANNOT-NAME-ITS-THEME`): `DirectAppState` has a hand-written snapshot
  serializer, and AC-16 only needs the host's `ui` section.
- CSS as the theme source for egui: a selector engine for one consumer (`NO-RENDERER-NEUTRAL-STYLESHEET`).
- `arc-swap`: a new dependency for a value read a few times per frame.

**Amendment to AC-15, for the gate.** Today's tokenizer, `StyledQueryToken`
(`liquers-core/src/query.rs:240`), has seven kinds. The theme defines all twelve syntax roles, but
`namespace`, `link`, `filename` and `error` are only reached once `query-editor-live-validation`
lands `SyntaxKind`. To reach them now, the core tokenizer would have to change, and that tokenizer
is the very thing that design rewrites. AC-15 therefore reads "each token kind the highlighter
produces".

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `QUERY-EDITOR-LIVE-VALIDATION` (design) | in_review, impl. unticked | — | Both change `styled_query_to_layout_job`; its `SyntaxKind` keys our roles | no | no | Theme owns colours; the second to land maps `SyntaxKind` → `SyntaxRole` |
| `TEMPLATE-SYNTAX-HIGHLIGHTER` (design) | in_review, impl. | — | Plans a shared `syntax_kind_color` in `egui/widgets.rs` | no | no | That function becomes a theme lookup |
| `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND` | draft | P3 | Absorbed (AC-12) | — | no | Close in Phase 5 |
| `CONSOLE-MOUNTS-NESTED-ELEMENT` (design) | in_review, impl. | — | Same file as AC-12, other panel | no | no | Rebase on whichever merges first |
| `UI-ASSET-SNAPSHOT-BOUNDARY` (design) | in_review, impl. | — | Runner owns asset subscriptions | no | no | Theme watch lives in the runner |
| `EDITOR-CANNOT-EDIT-INSIDE-A-NESTED-PIPELINE` | draft | P3 | Nested nodes stay read-only cards | no | no | Non-goal |
| `WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT` | draft | P3 | Browser shortcuts come later | no | no | None |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | `from_hex(..).unwrap()` in `widgets.rs:352` | no | no | Removed by AC-15 |
| `NAMESPACES-HAVE-NO-METADATA` | draft | P3 | Slots into icon precedence | no | no | `icon_for` takes a namespace icon argument now |
| `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE` | draft | P1 | `ui/theme/` may move | no | no | No egui or web imports in `ui/theme/` |
| `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS` | draft | P3 | Chapter 11 | no | no | Absorbed in Phase 5 |

Namespaces involved: `pl`, `img`, `rec` (metadata only), `lui` (unchanged).

## Interfaces

All new code is sync: it runs inside a frame. The one async step, loading a theme from a store,
happens in the runner. There are no new commands and no new `ExtValue` variants.

### Theme — `liquers-lib/src/ui/theme/` (new)

Every struct derives `Debug, Clone, PartialEq, Serialize, Deserialize` and carries
`#[serde(rename_all = "kebab-case")]`, so `card-head` and `ink-muted` match reference-design §4.3.

**Composition.** A theme is a nested dictionary first and a typed model second. A `ThemeLayer` is a
partial dictionary. Layers compose by deep merge: a later layer changes the values of fields that
exist and adds fields that don't. The typed `Theme` is a view of the composed dictionary. Keys the
model does not know are kept, so a later element can read its own tokens by path; they are not an
error. The built-in theme is the bottom layer. A host theme, and later a UI spec element, add
layers on top and give only what they change.

```rust
pub enum ThemeMode { Dark, Light }                 // Copy, Default = Dark, lowercase
pub enum Density { Comfortable, Compact }          // Copy, Default = Comfortable, lowercase
#[serde(try_from = "String", into = "String")]
pub struct Rgba(pub [u8; 4]);                      // "#rrggbb" | "#rrggbbaa"
pub struct Palette { pub surface: SurfaceRoles, pub text: TextRoles, pub state: StateRoles, pub syntax: SyntaxRoles }
// SurfaceRoles { bg, panel, card, card_head, control, control_hover, line, line_strong, shadow }
// TextRoles { ink, ink_muted, ink_faint, on_accent }
// StateRoles { accent, accent_wash, focus_ring, danger, warning, success, modified }
// SyntaxRoles { header, resource, separator, namespace, action, parameter, entity, link,
//               filename, plain, highlight, error }            (all Rgba)
pub enum SyntaxRole { Header, Resource, Separator, Namespace, Action, Parameter, Entity, Link, Filename, Plain, Highlight, Error }
impl From<&StyledQueryToken> for SyntaxRole;       // explicit arms, no `_`
pub struct Hue { pub dark: Rgba, pub light: Rgba }
pub struct Spacing { pub control_height: f32, pub row_gap: f32, pub card_gap: f32, pub card_radius: f32,
                     pub card_padding: f32, pub label_width: f32, pub icon_size: f32 }
pub struct Typography { pub font_ui: String, pub font_mono: String, pub size_body: f32, pub size_small: f32, pub size_title: f32 }
pub struct IconOverride { pub icon: Option<Icon>, pub hue: Option<String> }
pub struct IconTheme { pub icon_set: String,                                // default set: "phosphor"
                       pub library: BTreeMap<String, Icon>,              // named icons: "brush"
                       pub namespaces: BTreeMap<String, IconOverride>,   // "pl"
                       pub commands: BTreeMap<String, IconOverride> }    // "pl/gt", "realm:pl/gt"
pub struct Theme { pub name: String, pub dark: Palette, pub light: Palette, pub comfortable: Spacing,
                   pub compact: Spacing, pub typography: Typography, pub hues: BTreeMap<String, Hue>, pub icons: IconTheme }

/// The icon to draw (AC-8, AC-14, AC-17): every candidate's variants in precedence order, and a
/// Unicode text that is never empty (the first candidate's, else the label's monogram).
pub struct ResolvedIcon<'a> { pub variants: Vec<&'a IconVariant>, pub unicode: &'a str, pub hue: Option<&'a str> }

impl Theme {
    pub fn builtin() -> Arc<Theme>;                        // OnceLock over a Rust constructor; cannot fail
    pub fn compose(layers: &[ThemeLayer]) -> Result<Theme, Error>;   // builtin, then each layer in order
    pub fn with_layer(&self, layer: &ThemeLayer) -> Result<Theme, Error>;  // self's dictionary, merged with layer
    pub fn lookup(&self, path: &str) -> Option<&serde_json::Value>;  // "editor.card.radius": known or extra keys
    pub fn unknown_keys(&self) -> Vec<String>;   // paths outside the model: a lint, not an error
    pub fn palette(&self, mode: ThemeMode) -> &Palette;
    pub fn spacing(&self, density: Density) -> &Spacing;
    pub fn syntax(&self, mode: ThemeMode, role: SyntaxRole) -> Rgba;
    pub fn hue(&self, name: &str, mode: ThemeMode) -> Option<Rgba>;
    /// Theme command → theme namespace → `hint icon` → namespace icon (registry, later) → built-in table.
    pub fn icon_for<'a>(&'a self, key: &'a CommandKey, label: &'a str, hint: Option<&'a Icon>, ns_icon: Option<&'a Icon>) -> ResolvedIcon<'a>;
    pub fn to_css(&self) -> String;   // :root vars per [data-theme] and [data-density], .lq-tok-<role>
}
/// A partial theme: a nested dictionary (YAML or a structured store value).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThemeLayer(pub serde_json::Map<String, serde_json::Value>);
impl ThemeLayer {
    pub fn from_yaml(text: &str) -> Result<ThemeLayer, Error>;
    pub fn from_value(value: serde_json::Value) -> Result<ThemeLayer, Error>;  // must be a mapping
    pub fn merge(&mut self, over: &ThemeLayer);   // deep merge: maps by key, anything else replaced
}
pub fn is_icon_name(name: &str) -> bool;   // embedded Phosphor name list (icons.txt), both renderers
```

### Icons — `ui/theme/icon.rs` (new; AC-17)

```rust
/// An icon: alternative variants, best first, and a Unicode text for any surface that draws none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "IconSpec", into = "IconSpec")]
pub struct Icon { pub variants: Vec<IconVariant>, pub unicode: Option<String> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IconVariant {
    Set { set: String, name: String },   // "ph:funnel"; set defaults to `icons.icon-set`
    Library(String),                     // "lib:brush": an entry of `icons.library`
    Svg(IconSource),
    Raster(IconSource),                  // png, jpeg, webp: what `image` decodes
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IconSource {
    Inline(IconBytes),                   // SVG text, or base64 for a raster, in YAML
    Key(String),                         // a store key or query: "-R/icons/brush.svg"
}
/// Bytes that code can embed without copying (`include_bytes!`) and YAML can carry
/// (hand-written serde: a string for SVG, base64 for a raster).
#[derive(Debug, Clone, PartialEq)]
pub struct IconBytes(pub Cow<'static, [u8]>);
impl Icon {
    pub fn unicode(text: impl Into<String>) -> Icon;
    pub fn set(name: &str) -> Icon;
    pub const fn embedded_svg(bytes: &'static [u8]) -> IconVariant;    // code-embedded
    pub const fn embedded_raster(bytes: &'static [u8]) -> IconVariant;
    pub fn with(self, variant: IconVariant) -> Icon;
    pub fn with_unicode(self, text: impl Into<String>) -> Icon;
}
/// What an element asks for a `Key` variant; the runner loads it (async) and fills the cache.
#[derive(Debug, Clone, Default)]
pub struct IconCache(Arc<std::sync::RwLock<HashMap<String, IconLoad>>>);
#[derive(Debug, Clone)]
pub enum IconLoad { Pending, Ready(Arc<[u8]>), Failed }
impl IconCache { pub fn get_or_request(&self, key: &str, tx: &AppMessageSender) -> Option<Arc<[u8]>>; }
```

In YAML, an icon is written either as one string or as a structure. The string forms are:
- `ph:funnel` (an icon-set reference)
- `lib:brush` (a library icon)
- `u:⏷` (Unicode only)
- a key ending in `.svg`, `.png`, `.jpg` or `.webp`

The structure is `{unicode: "⏷", variants: [ph:funnel, {svg: {inline: "<svg…>"}}]}`. A
`hint icon` takes the same string, and the macro's `hint icon:` stays a string.

Code embeds an icon either as a variant (`Icon::set("funnel").with(Icon::embedded_svg(include_bytes!(…)))`)
or as a library entry in a code-built `ThemeLayer`, so a command can declare `hint icon: "lib:brush"`.

`Theme::icon_for` follows the precedence of AC-8 and AC-14 and concatenates each candidate's
variants, so a theme override that has only a raster still falls back to the command's Phosphor
name. A `Library` variant expands in place, one level deep (a cycle is refused when the layer is
composed). The Unicode text is the first candidate's, or the monogram, so it is never empty.

Each renderer takes the first variant it can draw:
- **egui:** `Set` through the Phosphor font when `is_icon_name`. `Svg` is rasterized with `resvg`
  (already a dependency) at `icon-size × pixels_per_point`. `Raster` is decoded with `image`.
  Textures are cached by bytes hash and size.
- **Browser:** `Set` as `<i class="ph ph-…">`. `Svg` is sanitized and inlined (no scripts). An inline
  `Raster` becomes a `data:` URI.
- **Text (later TUI, terminal output):** the Unicode text.

A `Key` variant is drawn from `IconCache`. Until the runner has loaded the key (`AppMessage::LoadIcon`,
evaluated like any query and kept while the theme lives), the renderer moves on to the next variant.
A failed key is not retried until the theme changes.

`Theme` keeps its composed dictionary in a private field (`#[serde(skip)]`) beside the typed
fields. `compose` serializes the built-in theme to a `ThemeLayer`, merges each layer in order, and
deserializes the result once. Merging is associative, so `with_layer` on a composed theme equals
composing the longer list; a Phase 3 test checks this. `builtin.yaml` is a test fixture asserted equal to `Theme::builtin()`. The
Phosphor name list `ui/theme/icons.txt` is generated from `egui_phosphor::regular::ICONS`, and a
test under the `egui` feature checks that the two agree. The browser build (no egui) therefore also
falls back on an unknown name, as AC-14 requires.

```rust
/// What elements read; the runner swaps it (AC-16). Clones share the cell.
#[derive(Debug, Clone)] pub struct ThemeCell(Arc<std::sync::RwLock<ThemeState>>);
#[derive(Debug, Clone)] pub struct ThemeState { pub theme: Arc<Theme>, pub mode: ThemeMode, pub density: Density, pub generation: u64 }
impl ThemeCell {
    pub fn new(state: ThemeState) -> Self;
    pub fn get(&self) -> ThemeState;            // one Arc clone; a poisoned lock yields into_inner
    pub fn set_theme(&self, theme: Arc<Theme>); // each setter bumps `generation`
    pub fn set_mode(&self, mode: ThemeMode);
}
/// The `ui` host section, taken with `ConfigDocument::take_section::<UiConfig>("ui")`.
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UiConfig { pub theme: Option<ThemeSource>, pub mode: Option<ModeSetting>, pub density: Option<Density> }
pub enum ModeSetting { System, Dark, Light }       // lowercase; None = System
#[serde(try_from = "String", into = "String")]
pub enum ThemeSource { Builtin(BuiltinTheme), Query(String) }   // a built-in name, else a key or query
pub enum BuiltinTheme { Default }
```

`System` is resolved by the host. The egui window reads `ctx.theme()` each frame and calls
`set_mode` when it changes. The browser uses a `prefers-color-scheme` media rule in `to_css()`.

### Context

```rust
impl UIContext {
    pub fn with_theme(self, cell: ThemeCell) -> Self;
    pub fn theme(&self) -> ThemeState;   // builtin when no cell; never locks AppState
}
impl<E> AppRunner<E> { pub fn with_ui_config(self, ui: &UiConfig, cell: ThemeCell) -> Self; }
```

### Editor model — `ui/editor/`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMode { #[default] Simple, Advanced }

#[derive(Debug, Clone, PartialEq)]
pub enum SpanTarget { Element, Namespaces, Argument { arg: String, item: Option<usize> } }
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan { pub path: NodePath, pub target: SpanTarget, pub range: Range<usize> }
// EditorTree gains: spans: Vec<TextSpan> (empty while `stale`), show_realm: bool (registry has > 1 realm)
// EditorNode::Action gains: realm: String, hint_icon: Option<Icon> (parsed from hints["icon"])
// PresetEntry gains: active: bool (the preset whose parameters equal the action's, AC-7)
impl EditorTree {
    pub fn locate(&self, offset: usize) -> Option<&TextSpan>;   // innermost span holding offset
    pub fn span_of(&self, path: &NodePath, target: &SpanTarget) -> Option<Range<usize>>;
}

#[derive(Debug, Clone, PartialEq)] pub struct StepChip { pub path: NodePath, pub label: String, pub command: Option<CommandKey> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum CardGroupKind { KeyPart, QueryPart }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)] pub struct CardMarks { pub modified: bool, pub link: bool, pub error: bool }
#[derive(Debug, Clone)] pub struct Card<'t> { pub node: &'t EditorNode, pub marks: CardMarks }
#[derive(Debug, Clone)] pub struct CardGroup<'t> { pub kind: CardGroupKind, pub cards: Vec<Card<'t>> }
#[derive(Debug, Clone)] pub struct EditorLayout<'t> { pub before: Vec<StepChip>, pub groups: Vec<CardGroup<'t>>, pub after: Vec<StepChip>, pub outline: bool }
pub fn editor_layout(tree: &EditorTree, mode: EditorMode) -> EditorLayout<'_>;
```

In Simple mode, for the query kind, the last action is the only card. Elements before it become
`before` chips and elements after it (`q`/`v` directives, values) become `after` chips. A link
stays as its field's text and `outline` is false (AC-1). With no action in the query, the leading
resource becomes the card. For the Action, Resource and Key kinds, the single node is the card in
both modes. In Advanced mode, the leading `Resource` nodes form the `KeyPart` group, the rest form
the `QueryPart`, and `outline` is true (AC-2).

The `EditorView` (not serialized; `Debug, Clone, Default`) gains these fields:
- `focus: Option<FocusTarget>`
- `collapsed: HashSet<String>`
- `jump_back: Option<Range<usize>>`
- `pending_jump: Option<usize>`
- `simple_offer: Option<NodePath>`

`EditorElement` and `QueryConsoleElement` gain `#[serde(default)] mode: EditorMode`, so the mode is
saved and a new editor starts in Simple (AC-3). They also gain `density: Option<Density>`, which
falls back to `ThemeState.density`; the console falls back to `Compact`. `EditorCommand` is
unchanged.

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FocusTarget { Text, Node(NodePath), Field { path: NodePath, arg: String, item: Option<usize> } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKey { ToggleMode, FocusQuery, Jump, NextRegion, PrevRegion, NextCard, PrevCard, FirstCard,
    LastCard, Expand, Collapse, CollapseAll, ExpandAll, Reset, Presets, Next, ApplyPreset(u8), AddAction,
    MoveUp, MoveDown, DeleteCard, ToggleLink, Complete, Update }
pub fn editor_keymap() -> &'static [(KeyboardShortcut, EditorKey)];  // OnceLock; "Ctrl+B" etc. (Ctrl = Cmd on macOS)
```

**Spans** are computed in `EditorState::rebuild` (`state.rs:891`). The steps:
1. Re-parse `tree.text` with `parse_query`. Each `ActionRequest.position`, `ns` included, starts an
   element. An element ends where the next one starts, or where the text ends.
2. Each action parameter's position starts an argument span. The parameter maps to an argument
   through the root editor's `PipelineEditor::argument_info(at, ArgRef::Position(i))`, and the
   excess positions of a variadic argument give `item`.
3. A link parameter recurses with an offset.
4. A header-parameter (realm) pipeline has no position, so it gets no span and no jump.

The `Namespaces` node shares the path of the following action (`state.rs:1050`). `SpanTarget`
tells the two apart.

**Jump.** It works only when the typed buffer equals `tree.text`. Otherwise `Ctrl+J` commits the
text (`SetText`), stores `pending_jump`, and the jump runs after the rebuild.

### egui and browser

```rust
// egui/theme.rs (new, feature egui)
pub fn apply_theme(ctx: &egui::Context, state: &ThemeState);   // visuals, fonts, Phosphor; on generation change
pub fn color(c: Rgba) -> egui::Color32;
pub fn phosphor_glyph(name: &str) -> Option<&'static str>;    // OnceLock<HashMap> keyed by kebab name
// egui/editor.rs — density applied in ui.scope(|ui| ui.style_mut().spacing …)
pub fn show_editor(ui: &mut egui::Ui, tree: &EditorTree, view: &mut EditorView,
                   mode: EditorMode, density: Density, theme: &ThemeState) -> EditorOutput;
pub struct EditorOutput { pub commands: Vec<EditorCommand>, pub mode: Option<EditorMode> }
// egui/widgets.rs: styled_query_to_layout_job(&StyledQuery, &ThemeState)
// ui/web/editor.rs
pub fn editor_html(tree: &EditorTree, title: &str, mode: EditorMode, density: Density, theme: &Theme) -> String;
// ui/web/widgets.rs: query_to_html emits <span class="lq-tok-<role>"> per token
```

`EditorElement::render_web` gets no `UIContext`. The element therefore caches the
`ThemeState` from its last `init`/`update` in a field marked `#[serde(skip)]`.

## Integration Points

**`liquers-lib`**
- `ui/theme/{mod.rs, model.rs, layer.rs, icon.rs, icons.txt, cell.rs, css.rs, config.rs, builtin.yaml}` (new).
- `ui/ui_context.rs`: `theme: Option<ThemeCell>`, `with_theme`, `theme()`.
- `ui/ui_context.rs` also gains `icon_cache: IconCache` (shared per runner) and `icon_cache()`;
  `ui/message.rs` `AppMessage` gains `LoadIcon(String)`, which the runner evaluates and stores.
- `ui/runner.rs`:
  - `AppRunner` gains `theme: ThemeCell` and `theme_watch: Option<MonitoredAsset<E>>`.
  - `context()` (l.193) adds `.with_theme`.
  - `deliver_update` (l.987, an associated fn) takes the cell as a parameter, like the registry.
  - For `ThemeSource::Query`, the runner evaluates the query, subscribes, and on each value calls
    `ThemeLayer::from_yaml` (on text) or `from_value` (on a structured value), then `Theme::compose`
and `set_theme`.
  - A refused value keeps the previous theme.
  - `has_monitoring()` (l.1042) leaves `theme_watch` out, so the runner does not repaint forever.
- `ui/native.rs` `ui_context` (l.158) passes the runner's cell.
- `ui/web/app.rs`: `MountOptions` gains `ui: Option<UiConfig>`. The mount creates the cell before
  the `UIContext` (l.663) and before `AppRunner::new` (l.735), and gives the cell to both. It writes
  `<style id="lq-theme">` from `to_css()` and rewrites it when `generation` changes.
- `egui/app.rs`:
  - `WindowOptions` gains `ui: Option<UiConfig>`.
  - `run_window` calls `apply_theme` when `generation` changes.
  - `run_window` resolves `System`.
- `egui/theme.rs` is new.
- `ui/editor/`:
  - `tree.rs`, `view.rs`, `element.rs` and `state.rs` change: spans, `hint_icon`, `realm`, `active`
    presets and `show_realm`, all built in `build_nodes` and `rebuild`.
  - `layout.rs` and `keys.rs` are new.
- `egui/editor.rs`:
  - Cards are `Frame` plus `CollapsingState`, with a toolbar, a form in a `Grid`, preset chips and
    an outline.
  - The keymap is applied with `consume_shortcut` while the editor has focus.
  - The jump reads the `TextEdit` `CCursor` and converts it to a byte offset.
- `ui/web/editor.rs`: same structure, as `<section class="lq-card" data-ns data-cmd>` with
  `<i class="ph ph-…">`.
- `ui/widgets/query_console_element.rs`: the Parameters panel calls `show_editor` with the
  console's `mode` and density. The switch never submits (AC-12).
- `egui/widgets.rs` (l.352): theme colours replace the `unwrap`s.
- `polars/{selection,sorting,filtering,aggregation}.rs`, `image/commands.rs` and
  `records/commands.rs`: `hint icon`, plus a few `preset:` and `next:` entries.
  `specs/command_registry.yaml` is regenerated.
- `Cargo.toml`: `egui-phosphor = { version = "0.14", optional = true }` (egui 0.36), and
  `"dep:egui-phosphor"` in the `egui` feature. The build matrix is rerun.

**`liquers-py`**: `src/config.rs` takes the `ui` section beside `python`, and the window entry point
(`src/ui.rs`) passes it to `WindowOptions`. There is no Python API change.

**`liquers-core`**: none.

## Error Handling

| Failure | Constructor | Effect |
|---|---|---|
| Theme YAML or JSON is malformed, a known key has the wrong type, or a colour is bad | `Error::configuration_error(format!("theme: {e}"))` (every serde error) | Runner keeps the previous theme; stderr and a runner event |
| `ui` section invalid, or an unknown built-in name | `Error::configuration_error` via `take_section` | Host start fails, as for `server` |
| Theme query fails to evaluate | the evaluation's `Error` | Built-in theme stays; reported once |
| Keymap entry does not parse | none (`eprintln!`, entry skipped) | A test asserts every entry parses and none repeats |
| Icon name unknown, image undecodable, key not loaded yet or failed | none | Next variant, finally the Unicode text (AC-14, AC-17) |
| `hint icon` or theme icon string malformed | `Error::configuration_error` (theme compose; a hint is reported by the registry check and drawn as its monogram) | Theme refused / monogram |
| `lib:` cycle | `Error::configuration_error` | Theme refused |
| Jump target outside the last action in Simple | none | `simple_offer`: "Open in Advanced" |

## Relevant Commands

No new commands. Existing registrations gain metadata. The preset and next queries below were
checked with `liquers-validate` (`-R/data/sales.csv/-/ns-pl/head-100`,
`…/head-20/describe`: ok):

```rust
register_command!(cr, fn head(state, n: i32 = 5) -> result
    namespace: "pl" label: "Get first rows" hint icon: "rows"
    preset: "head-5" (label: "Top 5") preset: "head-20" (label: "Top 20") preset: "head-100" (label: "Top 100")
    next: "describe" (label: "Describe"))?;
```

The same pattern applies to `pl/sort` (`sort-ascending`), the `pl` comparisons (`funnel`),
`pl/select_columns` (`columns`), and common `img` and `rec` commands.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/UI_INTERFACE_FSD.md` | reference | §Editors: modes, cards, keymap, jump, `UIContext::theme`, `UiConfig`, tokens |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | `hint icon` (Phosphor, kebab-case); presets as chips |
| `specs/command_registry.yaml` | generated | Regenerated with a changelog line |
| `docs/guides/{rust,python,web}/11-ui.md` | user guide | The editors and the `ui` section |

API docs: `ui/theme/mod.rs` (new: all five parts); `ui/editor/mod.rs`, `egui/editor.rs` (main
components, example); `ui/web/editor.rs`, `ui/ui_context.rs` (main components).
`affects_docs` adds the three `11-ui.md` chapters. `specs/README.md`: no new link.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | ~22 in `liquers-lib`, `liquers-py/src/config.rs`, the registry |
| Crates and workflows affected | `liquers-lib` (`egui`, `webui`), the Python window, build matrix, Playwright demo |
| Existing tests likely to change | `ui/editor/tests.rs` (new tree fields), `widgets.rs` `query_layout_underlines_the_positioned_token_only` (signature), `registry_export`, web editor HTML assertions |
| New validation | Layout per mode and kind; spans on the problem example (ns, link, variadic); composition (override, addition, associativity, unknown keys kept), CSS snapshot, fixture equality; icon precedence and name-list agreement; keymap |
| Compatibility / data / concurrency / performance / security | Saved elements gain defaulted fields. The cell read lock covers one `Arc` clone. Spans re-parse on `rebuild`, not per frame. A theme is data, not code |
| Recovery | Theme, layout, spans and commands land as separate steps |
| Certainty and open questions | High for the model and theme. Medium for egui focus (a field inside a card expanded the same frame) and for the order of menu shortcuts versus the editor's `consume_shortcut`. AC-15 amendment above needs approval |

## Implementation Changes

Departures found during Phase 4 (2026-11-21). None changes an acceptance criterion; the reference
documents describe the code as built.

| Planned | Built | Why |
|---|---|---|
| `show_editor(ui, tree, view, mode, density, theme)` | adds `ui_ctx: Option<&UIContext>` | icons loaded by key need `UIContext::icon_bytes` |
| `editor_keymap() -> &[(KeyboardShortcut, EditorKey)]` | each entry also has a `KeyScope` (`Anywhere`, `Header`, `Field`), matched most specific first | egui's `consume_shortcut` ignores extra Shift/Alt, so `Alt+Shift+N` must be tried before `Alt+N`-like entries, and header keys must not fire in a text field |
| Next: `Alt+N` | `Alt+Shift+N` | the bundled browser console uses `Alt+N` for New Console (test 12) |
| namespace icons in `icons.namespaces` only | the built-in table moved to `icons.defaults`, below a command's `hint icon`; `icons.namespaces` stays above it | a theme's namespace entry overrides hints, the built-in table must not |
| egui reads `ctx.theme()` for `system` | `run_window` reads `ctx.system_theme()` while `WindowOptions.ui` has a `system` mode; `NativeDriver::with_ui_config` and `AppRunner::set_ui_config` added | the host owns the mode; `ctx.theme()` is the applied one |
| the browser renderer has the keyboard model | the browser has the layout, modes and events but no editor keys | the keydown dispatcher of the mount is shared with menus; `WEB-EDITOR-HAS-NO-KEYBOARD-MODEL` |
| the console panel shows the last action | the panel shows the whole query (Simple: the last action's card; Advanced: every step) | `QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND` was cheaper to close here |
| icon images by key in the browser | egui loads them through the runner; the browser draws inline and embedded images, and a keyed image falls back to Unicode | the browser mount has no asset loader for icons yet (`WEB-EDITOR-HAS-NO-KEYBOARD-MODEL`) |
| `mountUI` unchanged | unchanged, so a page cannot pick a theme | `WEB-MOUNTUI-HAS-NO-THEME-OPTION` |
