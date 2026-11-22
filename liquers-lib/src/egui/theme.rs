//! The theme in egui: visuals and fonts from a [`ThemeState`], colours, Phosphor glyphs and icon
//! drawing.
//!
//! # Main components
//!
//! - [`apply_theme`]: sets the context's visuals from the palette of the state's mode and installs
//!   the Phosphor font as a fallback of proportional text, once per theme generation. It also
//!   keeps the state in the context, so a widget without a `UIContext` reads it with [`theme_of`].
//! - [`color`]: a theme colour as an `egui::Color32`.
//! - [`phosphor_glyph`]: the glyph of a Phosphor icon by its kebab-case name.
//! - [`draw_icon`]: draws a [`ResolvedIcon`] with its first drawable variant: a set icon through
//!   the font, an SVG through `resvg`, a raster through `image`, an image by key once the runner
//!   has loaded it (`UIContext::icon_bytes`), and the Unicode text otherwise.
//! - [`spacing_style`]: applies a density's spacing to a `Ui`'s style.
//!
//! # Where it is used
//!
//! - `egui::app` and the hosts call [`apply_theme`] each frame (it returns at once when nothing
//!   changed).
//! - `egui::editor` draws card icons with [`draw_icon`] and scopes the editor's density with
//!   [`spacing_style`]; `egui::widgets` colours queries from [`theme_of`].
//!
//! # Related concepts
//!
//! - [`crate::ui::theme`]: the token model, layers, icons and the theme cell.
//!
//! Design: `specs/design/query-editor-ergonomics/` (AC-11, AC-14, AC-17).
//!
//! # Examples
//!
//! ```
//! use liquers_lib::egui::theme::{apply_theme, color, phosphor_glyph, theme_of};
//! use liquers_lib::ui::theme::{ThemeMode, ThemeState};
//!
//! let ctx = egui::Context::default();
//! let state = ThemeState::default();
//! apply_theme(&ctx, &state);
//! let panel = color(state.theme.palette(ThemeMode::Dark).surface.panel);
//! assert_eq!(ctx.global_style().visuals.panel_fill, panel);
//! assert_eq!(theme_of(&ctx).generation, state.generation);
//! assert!(phosphor_glyph("funnel-simple").is_some());
//! ```

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use egui::{Color32, Id, RichText, Stroke, TextureHandle};

use crate::ui::theme::{
    Density, IconSource, IconVariant, Palette, ResolvedIcon, Rgba, ThemeMode, ThemeState, PHOSPHOR,
};
use crate::ui::ui_context::UIContext;

const STATE_ID: &str = "liquers-theme-state";
const APPLIED_ID: &str = "liquers-theme-applied";
const FONTS_ID: &str = "liquers-theme-fonts";
const TEXTURES_ID: &str = "liquers-icon-textures";

/// A theme colour in egui.
pub fn color(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The glyph of the Phosphor icon `name` (kebab-case, as in `icons.txt`).
pub fn phosphor_glyph(name: &str) -> Option<&'static str> {
    static MAP: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        egui_phosphor::regular::ICONS
            .iter()
            .map(|(n, glyph)| (n.to_lowercase().replace('_', "-"), *glyph))
            .collect()
    })
    .get(name)
    .copied()
}

/// What identifies an applied theme: its generation and the theme itself.
fn stamp(state: &ThemeState) -> (u64, usize) {
    (state.generation, Arc::as_ptr(&state.theme) as usize)
}

/// Applies `state` to `ctx`: visuals from the palette of its mode, the Phosphor font (once), and
/// the state kept for [`theme_of`]. Does nothing when this state is already applied.
pub fn apply_theme(ctx: &egui::Context, state: &ThemeState) {
    let applied: Option<(u64, usize)> = ctx.data(|d| d.get_temp(Id::new(APPLIED_ID)));
    if applied == Some(stamp(state)) {
        return;
    }
    ensure_icon_font(ctx);
    let palette = state.theme.palette(state.mode);
    ctx.set_visuals(visuals(palette, state.mode));
    ctx.request_repaint(); // the background of this frame was painted with the old visuals
    ctx.data_mut(|d| {
        d.insert_temp(Id::new(APPLIED_ID), stamp(state));
        d.insert_temp(Id::new(STATE_ID), state.clone());
    });
}

/// Installs the Phosphor font as a fallback of proportional text, once per context.
pub fn ensure_icon_font(ctx: &egui::Context) {
    let fonts: Option<bool> = ctx.data(|d| d.get_temp(Id::new(FONTS_ID)));
    if fonts == Some(true) {
        return;
    }
    ctx.add_font(egui::epaint::text::FontInsert::new(
        "phosphor",
        egui_phosphor::Variant::Regular.font_data(),
        vec![egui::epaint::text::InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority: egui::epaint::text::FontPriority::Lowest,
        }],
    ));
    ctx.data_mut(|d| d.insert_temp(Id::new(FONTS_ID), true));
}

/// The theme last applied to `ctx` (the built-in one before any).
pub fn theme_of(ctx: &egui::Context) -> ThemeState {
    ctx.data(|d| d.get_temp(Id::new(STATE_ID))).unwrap_or_default()
}

fn visuals(p: &Palette, mode: ThemeMode) -> egui::Visuals {
    let mut v = match mode {
        ThemeMode::Dark => egui::Visuals::dark(),
        ThemeMode::Light => egui::Visuals::light(),
    };
    let s = &p.surface;
    v.panel_fill = color(s.panel);
    v.window_fill = color(s.card);
    v.extreme_bg_color = color(s.bg);
    v.faint_bg_color = color(s.card_head);
    v.code_bg_color = color(s.control);
    v.window_stroke = Stroke::new(1.0, color(s.line));
    v.hyperlink_color = color(p.state.accent);
    v.warn_fg_color = color(p.state.warning);
    v.error_fg_color = color(p.state.danger);
    v.selection.bg_fill = color(p.state.accent_wash);
    v.selection.stroke = Stroke::new(1.0, color(p.state.accent));
    let ink = color(p.text.ink);
    let muted = color(p.text.ink_muted);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = color(s.card);
    w.noninteractive.weak_bg_fill = color(s.card);
    w.noninteractive.bg_stroke = Stroke::new(1.0, color(s.line));
    w.noninteractive.fg_stroke = Stroke::new(1.0, muted);
    w.inactive.bg_fill = color(s.control);
    w.inactive.weak_bg_fill = color(s.control);
    w.inactive.fg_stroke = Stroke::new(1.0, ink);
    w.hovered.bg_fill = color(s.control_hover);
    w.hovered.weak_bg_fill = color(s.control_hover);
    w.hovered.bg_stroke = Stroke::new(1.0, color(s.line_strong));
    w.hovered.fg_stroke = Stroke::new(1.5, ink);
    w.active.bg_fill = color(s.control_hover);
    w.active.weak_bg_fill = color(s.control_hover);
    w.active.bg_stroke = Stroke::new(1.0, color(p.state.focus_ring));
    w.active.fg_stroke = Stroke::new(2.0, ink);
    w.open.bg_fill = color(s.control);
    w.open.weak_bg_fill = color(s.control);
    w.open.fg_stroke = Stroke::new(1.0, ink);
    v
}

/// Applies the spacing of `density` in `state` to `style` (an editor scopes its own density).
pub fn spacing_style(style: &mut egui::Style, state: &ThemeState, density: Density) {
    let sp = state.theme.spacing(density);
    style.spacing.interact_size.y = sp.control_height;
    style.spacing.item_spacing.y = sp.row_gap;
    style.spacing.button_padding.y = ((sp.control_height - 14.0) / 2.0).max(1.0);
}

/// What [`draw_icon`] drew.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawnIcon {
    /// A glyph of an icon font.
    Glyph(&'static str),
    /// An image, decoded into a texture.
    Image,
    /// The Unicode text.
    Text(String),
}

#[derive(Clone, Default)]
struct IconTextures(HashMap<u64, Option<TextureHandle>>);

fn texture(ctx: &egui::Context, bytes: &[u8], svg: bool, px: u32) -> Option<TextureHandle> {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    svg.hash(&mut h);
    px.hash(&mut h);
    let key = h.finish();
    let cached = ctx.data_mut(|d| d.get_temp_mut_or_default::<IconTextures>(Id::new(TEXTURES_ID)).0.get(&key).cloned());
    if let Some(t) = cached {
        return t;
    }
    let image = if svg { rasterize_svg(bytes, px) } else { decode_raster(bytes) };
    let handle = image.map(|img| ctx.load_texture(format!("liquers-icon-{key:x}"), img, egui::TextureOptions::LINEAR));
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<IconTextures>(Id::new(TEXTURES_ID)).0.insert(key, handle.clone());
    });
    handle
}

fn rasterize_svg(bytes: &[u8], px: u32) -> Option<egui::ColorImage> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let px = px.max(1);
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let scale = (px as f32 / size.width()).min(px as f32 / size.height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied RGBA.
    Some(egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data()))
}

fn decode_raster(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// The first drawable variant of `icon`: a known glyph of a set with a font, an image whose bytes
/// are inline or loaded, else the Unicode text. A key not loaded yet is skipped (and its load
/// requested once through `ui_ctx`).
fn pick(
    egui_ctx: &egui::Context,
    icon: &ResolvedIcon<'_>,
    px: u32,
    ui_ctx: Option<&UIContext>,
) -> (DrawnIcon, Option<TextureHandle>) {
    for variant in &icon.variants {
        let variant: &IconVariant = variant;
        match variant {
            IconVariant::Set { set, name } => {
                if set.as_deref() == Some(PHOSPHOR) {
                    if let Some(glyph) = phosphor_glyph(name) {
                        return (DrawnIcon::Glyph(glyph), None);
                    }
                }
            }
            IconVariant::Library(_) => {} // expanded by `Theme::icon_for`
            IconVariant::Svg(source) | IconVariant::Raster(source) => {
                let svg = matches!(variant, IconVariant::Svg(_));
                let bytes: Option<Arc<[u8]>> = match source {
                    IconSource::Inline(b) => Some(Arc::from(b.as_slice())),
                    IconSource::Key(key) => ui_ctx.and_then(|c| c.icon_bytes(key)),
                };
                if let Some(bytes) = bytes {
                    if let Some(t) = texture(egui_ctx, &bytes, svg, px) {
                        return (DrawnIcon::Image, Some(t));
                    }
                }
            }
        }
    }
    (DrawnIcon::Text(icon.unicode.clone()), None)
}

/// Draws `icon` at `size` points in `tint` (glyphs and text; images keep their colours).
pub fn draw_icon(
    ui: &mut egui::Ui,
    icon: &ResolvedIcon<'_>,
    size: f32,
    tint: Color32,
    ui_ctx: Option<&UIContext>,
) -> (egui::Response, DrawnIcon) {
    let px = (size * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
    let (drawn, tex) = pick(ui.ctx(), icon, px, ui_ctx);
    let response = match (&drawn, tex) {
        (DrawnIcon::Image, Some(t)) => ui.add(egui::Image::new((t.id(), egui::vec2(size, size)))),
        (DrawnIcon::Glyph(g), _) => ui.label(RichText::new(*g).size(size).color(tint)),
        (DrawnIcon::Text(t), _) => ui.label(RichText::new(t).size(size * 0.8).color(tint)),
        (DrawnIcon::Image, None) => ui.label(RichText::new(&icon.unicode).size(size * 0.8).color(tint)),
    };
    (response, drawn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{IconBytes, Theme, ThemeCell};

    fn run(ctx: &egui::Context, f: impl FnMut(&mut egui::Ui)) {
        let mut f = f;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| f(ui));
        out.textures_delta.clear(); // no painter here
    }

    // Test 17a (AC-11).
    #[test]
    fn apply_theme_sets_visuals_and_density() {
        let ctx = egui::Context::default();
        let cell = ThemeCell::new(ThemeState::default());
        apply_theme(&ctx, &cell.get());
        let dark = color(Theme::builtin().palette(ThemeMode::Dark).surface.panel);
        assert_eq!(ctx.global_style().visuals.panel_fill, dark);
        cell.set_mode(ThemeMode::Light);
        apply_theme(&ctx, &cell.get());
        let light = color(Theme::builtin().palette(ThemeMode::Light).surface.panel);
        assert_eq!(ctx.global_style().visuals.panel_fill, light);
        assert_eq!(theme_of(&ctx).mode, ThemeMode::Light);

        let state = cell.get();
        let mut comfortable = egui::Style::default();
        let mut compact = egui::Style::default();
        spacing_style(&mut comfortable, &state, Density::Comfortable);
        spacing_style(&mut compact, &state, Density::Compact);
        assert!(compact.spacing.interact_size.y < comfortable.spacing.interact_size.y);
        assert!(compact.spacing.item_spacing.y < comfortable.spacing.item_spacing.y);
    }

    // Test 26 (AC-17).
    #[test]
    fn egui_draws_first_drawable_variant() {
        let ctx = egui::Context::default();
        apply_theme(&ctx, &ThemeState::default());
        let set = IconVariant::Set { set: Some(PHOSPHOR.into()), name: "funnel".into() };
        let unknown = IconVariant::Set { set: Some(PHOSPHOR.into()), name: "no-such-icon".into() };
        const SVG: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><path d='M2 14L14 2' stroke='black'/></svg>";
        let svg = IconVariant::Svg(IconSource::Inline(IconBytes::embedded(SVG)));
        let key = IconVariant::Svg(IconSource::Key("-R/icons/brush.svg".into()));
        let (tx, _rx) = crate::ui::app_message_channel();
        let app_state: Arc<tokio::sync::Mutex<dyn crate::ui::AppState>> =
            Arc::new(tokio::sync::Mutex::new(crate::ui::DirectAppState::new()));
        let ui_ctx = UIContext::new(app_state, tx);

        let mut drawn = Vec::new();
        run(&ctx, |ui| {
            let cases: [Vec<&IconVariant>; 4] =
                [vec![&unknown, &set], vec![&svg], vec![&key, &unknown], vec![&key, &svg]];
            for variants in cases {
                let icon = ResolvedIcon { variants: variants.into_iter().map(std::borrow::Cow::Borrowed).collect(), unicode: "⚙".into(), hue: None };
                drawn.push(draw_icon(ui, &icon, 18.0, Color32::WHITE, Some(&ui_ctx)).1);
            }
        });
        assert_eq!(drawn[0], DrawnIcon::Glyph(phosphor_glyph("funnel").expect("funnel")));
        assert_eq!(drawn[1], DrawnIcon::Image);
        assert_eq!(drawn[2], DrawnIcon::Text("⚙".into()), "a key not loaded yet is skipped");
        assert_eq!(drawn[3], DrawnIcon::Image);
    }
}
