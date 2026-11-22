//! The typed token model of a theme: colours, roles, spacing, typography and hues.

use std::collections::BTreeMap;
use std::fmt;

use liquers_core::error::Error;
use liquers_core::query::StyledQueryToken;
use serde::{Deserialize, Serialize};

use super::icon::IconTheme;

/// The colour mode a palette is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// How tightly the editors are laid out (AC-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// An sRGB colour with alpha, written `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    /// An opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Rgba([r, g, b, 0xff])
    }
    /// A colour with alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba([r, g, b, a])
    }
    /// Parses `#rrggbb` or `#rrggbbaa`.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let hex = text.trim().strip_prefix('#').ok_or_else(|| {
            Error::configuration_error(format!("colour '{text}' does not start with '#'"))
        })?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::configuration_error(format!(
                "colour '{text}' is not #rrggbb or #rrggbbaa"
            )));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        let alpha = if hex.len() == 8 { byte(6) } else { 0xff };
        Ok(Rgba([byte(0), byte(2), byte(4), alpha]))
    }
    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = self.0;
        if a == 0xff {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl TryFrom<String> for Rgba {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Rgba::parse(&value)
    }
}

impl From<Rgba> for String {
    fn from(value: Rgba) -> Self {
        value.to_hex()
    }
}

/// Backgrounds, borders and shadow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SurfaceRoles {
    pub bg: Rgba,
    pub panel: Rgba,
    pub card: Rgba,
    pub card_head: Rgba,
    pub control: Rgba,
    pub control_hover: Rgba,
    pub line: Rgba,
    pub line_strong: Rgba,
    pub shadow: Rgba,
}

/// Text colours, strongest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TextRoles {
    pub ink: Rgba,
    pub ink_muted: Rgba,
    pub ink_faint: Rgba,
    pub on_accent: Rgba,
}

/// Focus, selection and status colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StateRoles {
    pub accent: Rgba,
    pub accent_wash: Rgba,
    pub focus_ring: Rgba,
    pub danger: Rgba,
    pub warning: Rgba,
    pub success: Rgba,
    pub modified: Rgba,
}

/// The colours of a highlighted query, one per [`SyntaxRole`] (AC-15).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SyntaxRoles {
    pub header: Rgba,
    pub resource: Rgba,
    pub separator: Rgba,
    pub namespace: Rgba,
    pub action: Rgba,
    pub parameter: Rgba,
    pub entity: Rgba,
    pub link: Rgba,
    pub filename: Rgba,
    pub plain: Rgba,
    pub highlight: Rgba,
    pub error: Rgba,
}

/// The colours of one mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Palette {
    pub surface: SurfaceRoles,
    pub text: TextRoles,
    pub state: StateRoles,
    pub syntax: SyntaxRoles,
}

/// A kind of token in a highlighted query. `Namespace`, `Link`, `Filename` and `Error` are
/// produced once the tokenizer distinguishes them (`query-editor-live-validation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxRole {
    Header,
    Resource,
    Separator,
    Namespace,
    Action,
    Parameter,
    Entity,
    Link,
    Filename,
    Plain,
    Highlight,
    Error,
}

impl SyntaxRole {
    /// Every role, in the order of [`SyntaxRoles`].
    pub const ALL: [SyntaxRole; 12] = [
        SyntaxRole::Header,
        SyntaxRole::Resource,
        SyntaxRole::Separator,
        SyntaxRole::Namespace,
        SyntaxRole::Action,
        SyntaxRole::Parameter,
        SyntaxRole::Entity,
        SyntaxRole::Link,
        SyntaxRole::Filename,
        SyntaxRole::Plain,
        SyntaxRole::Highlight,
        SyntaxRole::Error,
    ];

    /// The role's token name (`lq-tok-<name>` in the browser).
    pub fn name(&self) -> &'static str {
        match self {
            SyntaxRole::Header => "header",
            SyntaxRole::Resource => "resource",
            SyntaxRole::Separator => "separator",
            SyntaxRole::Namespace => "namespace",
            SyntaxRole::Action => "action",
            SyntaxRole::Parameter => "parameter",
            SyntaxRole::Entity => "entity",
            SyntaxRole::Link => "link",
            SyntaxRole::Filename => "filename",
            SyntaxRole::Plain => "plain",
            SyntaxRole::Highlight => "highlight",
            SyntaxRole::Error => "error",
        }
    }
}

impl From<&StyledQueryToken> for SyntaxRole {
    fn from(token: &StyledQueryToken) -> Self {
        match token {
            StyledQueryToken::StringParameter(_) => SyntaxRole::Parameter,
            StyledQueryToken::Entity(_) => SyntaxRole::Entity,
            StyledQueryToken::Separator(_) => SyntaxRole::Separator,
            StyledQueryToken::ResourceName(_) => SyntaxRole::Resource,
            StyledQueryToken::ActionName(_) => SyntaxRole::Action,
            StyledQueryToken::Header(_) => SyntaxRole::Header,
            StyledQueryToken::Highlight(_) => SyntaxRole::Highlight,
        }
    }
}

impl SyntaxRoles {
    /// The colour of `role`.
    pub fn get(&self, role: SyntaxRole) -> Rgba {
        match role {
            SyntaxRole::Header => self.header,
            SyntaxRole::Resource => self.resource,
            SyntaxRole::Separator => self.separator,
            SyntaxRole::Namespace => self.namespace,
            SyntaxRole::Action => self.action,
            SyntaxRole::Parameter => self.parameter,
            SyntaxRole::Entity => self.entity,
            SyntaxRole::Link => self.link,
            SyntaxRole::Filename => self.filename,
            SyntaxRole::Plain => self.plain,
            SyntaxRole::Highlight => self.highlight,
            SyntaxRole::Error => self.error,
        }
    }
}

/// A named colour with one value per mode, used to tint namespace tiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Hue {
    pub dark: Rgba,
    pub light: Rgba,
}

impl Hue {
    /// The hue's colour in `mode`.
    pub fn get(&self, mode: ThemeMode) -> Rgba {
        match mode {
            ThemeMode::Dark => self.dark,
            ThemeMode::Light => self.light,
        }
    }
}

/// Sizes of one density, in logical points (CSS pixels in the browser).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Spacing {
    pub control_height: f32,
    pub row_gap: f32,
    pub card_gap: f32,
    pub card_radius: f32,
    pub card_padding: f32,
    pub label_width: f32,
    pub icon_size: f32,
}

/// Font families and sizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Typography {
    pub font_ui: String,
    pub font_mono: String,
    pub size_body: f32,
    pub size_small: f32,
    pub size_title: f32,
}

/// A whole theme: the typed view of a composed nested dictionary (see [`super::ThemeLayer`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Theme {
    pub name: String,
    pub dark: Palette,
    pub light: Palette,
    pub comfortable: Spacing,
    pub compact: Spacing,
    pub typography: Typography,
    pub hues: BTreeMap<String, Hue>,
    pub icons: IconTheme,
    /// The composed dictionary the typed fields were read from, unknown keys included.
    #[serde(skip)]
    pub(crate) dict: serde_json::Map<String, serde_json::Value>,
}

impl Theme {
    /// The palette of `mode`.
    pub fn palette(&self, mode: ThemeMode) -> &Palette {
        match mode {
            ThemeMode::Dark => &self.dark,
            ThemeMode::Light => &self.light,
        }
    }

    /// The spacing of `density`.
    pub fn spacing(&self, density: Density) -> &Spacing {
        match density {
            Density::Comfortable => &self.comfortable,
            Density::Compact => &self.compact,
        }
    }

    /// The colour of a query token of `role` in `mode` (AC-15).
    pub fn syntax(&self, mode: ThemeMode, role: SyntaxRole) -> Rgba {
        self.palette(mode).syntax.get(role)
    }

    /// The named hue in `mode`, when the theme defines it.
    pub fn hue(&self, name: &str, mode: ThemeMode) -> Option<Rgba> {
        self.hues.get(name).map(|h| h.get(mode))
    }
}
