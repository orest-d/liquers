//! Themes for the Liquers UI: one renderer-neutral token model that egui and the browser apply.
//!
//! # Purpose
//!
//! Colours, spacing, fonts and icons of the UI come from one place instead of constants in each
//! renderer. A theme is authored in YAML as a nested dictionary and composed in layers: the
//! built-in theme at the bottom, then a host's theme, then (later) a UI spec element's own
//! changes. Each layer gives only what it changes, and may add keys the model does not know,
//! which stay readable by path. Design: `specs/design/query-editor-ergonomics/`.
//!
//! # Main components
//!
//! - [`Theme`]: the typed view of a composed theme: a dark and a light [`Palette`] (surface,
//!   text, state and query-syntax roles), [`Spacing`] for [`Density::Comfortable`] and
//!   [`Density::Compact`], [`Typography`], named [`Hue`]s and the [`IconTheme`].
//!   [`Theme::builtin`] cannot fail; [`Theme::compose`] and [`Theme::with_layer`] merge
//!   [`ThemeLayer`]s; [`Theme::to_css`] writes CSS custom properties for the browser.
//! - [`ThemeLayer`]: a partial theme, merged by key (`merge`).
//! - [`Icon`]: alternative [`IconVariant`]s (an icon-set name, a library icon, an SVG or a raster,
//!   inline, embedded in code or by store key) and a Unicode text; [`Theme::icon_for`] resolves
//!   the icon of a command card into a [`ResolvedIcon`], whose Unicode text is never empty.
//! - [`ThemeCell`]: the shared, swappable [`ThemeState`] (theme, mode, default density) a
//!   runner owns and every `UIContext` reads; [`IconCache`] holds icon images loaded by key.
//! - [`UiConfig`]: the `ui` section of a configuration document ([`ThemeSource`], mode, density).
//! - [`SyntaxRole`]: the token kinds of a highlighted query, with their colours in
//!   [`SyntaxRoles`].
//!
//! # Where it is used
//!
//! The query and pipeline editors (`crate::ui::editor`), the egui renderer and the browser
//! renderer read it; the highlighted query of the query console and query fields takes its
//! colours from [`Theme::syntax`].
//!
//! # Related concepts
//!
//! `crate::ui::ui_context::UIContext` carries the theme to elements; the configuration
//! document's `ui` section names it; command hints (`hint icon`) give commands their icons.
//!
//! # Example
//!
//! ```
//! use liquers_lib::ui::theme::{Theme, ThemeLayer, ThemeMode, Rgba};
//!
//! let layer = ThemeLayer::from_yaml(r##"
//! dark: { surface: { card: "#1b1f27" } }
//! editor: { card: { elevation: 2 } }
//! "##).unwrap();
//! let theme = Theme::compose(&[layer]).unwrap();
//! assert_eq!(theme.palette(ThemeMode::Dark).surface.card, Rgba::rgb(0x1b, 0x1f, 0x27));
//! assert_eq!(theme.palette(ThemeMode::Dark).surface.bg, Theme::builtin().dark.surface.bg);
//! assert_eq!(theme.lookup("editor.card.elevation"), Some(&serde_json::json!(2)));
//! assert_eq!(theme.unknown_keys(), vec!["editor".to_string()]);
//! assert!(theme.to_css().contains("--lq-card:#1b1f27;"));
//! ```

mod builtin;
mod cell;
mod config;
mod css;
pub mod icon;
mod icon_resolve;
mod layer;
mod model;

pub use cell::{IconCache, IconLoad, ThemeCell, ThemeState};
pub use config::{BuiltinTheme, ModeSetting, ThemeSource, UiConfig};
pub use icon::{Icon, IconBytes, IconOverride, IconSource, IconTheme, IconVariant, PHOSPHOR};
pub use icon_resolve::{is_icon_name, is_known_set_icon, monogram, ResolvedIcon};
pub use layer::ThemeLayer;
pub use model::{
    Density, Hue, Palette, Rgba, Spacing, StateRoles, SurfaceRoles, SyntaxRole, SyntaxRoles,
    TextRoles, Theme, ThemeMode, Typography,
};

#[cfg(test)]
mod tests;
