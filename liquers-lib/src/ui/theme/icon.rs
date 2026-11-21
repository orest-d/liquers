//! Icons as lists of alternative variants with a Unicode fallback (AC-17).
//!
//! An [`Icon`] lists variants best first: an icon-set reference (Phosphor by default), a library
//! icon of the theme, an SVG or a raster image, each image inline, embedded in code or named by
//! a store key. A renderer draws the first variant it can; a text-only surface draws the Unicode
//! text, which a resolved icon always has.
//!
//! In YAML an icon is a string or a structure:
//!
//! | Form | Meaning |
//! |---|---|
//! | `funnel` or `ph:funnel` | Phosphor icon `funnel` |
//! | `<set>:<name>` | an icon of another set |
//! | `lib:brush` | the theme library's icon `brush` |
//! | `u:⏷` | Unicode text only |
//! | `-R/icons/brush.svg` | an image by store key (`.svg`, `.png`, `.jpg`, `.jpeg`, `.webp`) |
//! | `{unicode: "⏷", variants: [ph:funnel, {svg: {inline: "<svg…>"}}]}` | the full form |
//! | `[ph:funnel, u:⏷]` | a list of string forms |
//!
//! # Main components
//!
//! - [`Icon`]: the variants and the Unicode text; [`Icon::parse`] reads a string form,
//!   [`Icon::from_hint`] a command's `hint icon` value; [`Icon::embedded_svg`] and
//!   [`Icon::embedded_raster`] build variants from bytes compiled in.
//! - [`IconVariant`] and [`IconSource`] (inline [`IconBytes`] or a store key).
//! - [`IconTheme`]: the theme's `icons` section: the default set, a library of named icons,
//!   per-command [`IconOverride`]s and `defaults` for commands without a hint.
//!
//! # Where it is used
//!
//! - `Theme::icon_for` resolves a card's icon from these; the egui and browser editors draw the
//!   result (`crate::egui::theme::draw_icon`, `crate::ui::web::editor::icon_html`).
//!
//! # Related concepts
//!
//! - `super::icon_resolve`: resolution to a [`super::ResolvedIcon`] and the monogram fallback.
//!
//! # Examples
//!
//! ```
//! use liquers_lib::ui::theme::{Icon, IconVariant};
//!
//! let icon = Icon::parse("ph:funnel").unwrap();
//! assert!(matches!(&icon.variants[0], IconVariant::Set { name, .. } if name == "funnel"));
//! let hinted = Icon::from_hint(&serde_json::json!(["funnel", "u:F"])).unwrap();
//! assert_eq!(hinted.unicode.as_deref(), Some("F"));
//! ```

use std::borrow::Cow;
use std::collections::BTreeMap;

use base64::Engine;
use liquers_core::error::Error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as Json};

/// The default icon set.
pub const PHOSPHOR: &str = "phosphor";

/// Bytes that code embeds without copying (`include_bytes!`) and a theme file carries inline.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IconBytes(pub Cow<'static, [u8]>);

impl IconBytes {
    /// Bytes embedded in the binary.
    pub const fn embedded(bytes: &'static [u8]) -> Self {
        IconBytes(Cow::Borrowed(bytes))
    }
    /// The bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Where an image variant's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconSource {
    /// In the theme or in code.
    Inline(IconBytes),
    /// A store key or query, loaded by the runner.
    Key(String),
}

/// One way of drawing an icon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconVariant {
    /// An icon of an icon set (`set` is [`PHOSPHOR`] unless given).
    Set { set: String, name: String },
    /// An icon of the theme's library (`icons.library`).
    Library(String),
    /// An SVG image.
    Svg(IconSource),
    /// A raster image (PNG, JPEG or WebP).
    Raster(IconSource),
}

/// An icon: variants best first and an optional Unicode text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Json", into = "Json")]
pub struct Icon {
    pub variants: Vec<IconVariant>,
    pub unicode: Option<String>,
}

impl Icon {
    /// An icon drawn only as `text`.
    pub fn unicode(text: impl Into<String>) -> Icon {
        Icon { variants: vec![], unicode: Some(text.into()) }
    }
    /// A Phosphor icon.
    pub fn set(name: &str) -> Icon {
        Icon { variants: vec![IconVariant::Set { set: PHOSPHOR.to_string(), name: name.to_string() }], unicode: None }
    }
    /// An SVG embedded in code.
    pub const fn embedded_svg(bytes: &'static [u8]) -> IconVariant {
        IconVariant::Svg(IconSource::Inline(IconBytes::embedded(bytes)))
    }
    /// A raster image embedded in code.
    pub const fn embedded_raster(bytes: &'static [u8]) -> IconVariant {
        IconVariant::Raster(IconSource::Inline(IconBytes::embedded(bytes)))
    }
    /// The icon with `variant` appended.
    pub fn with(mut self, variant: IconVariant) -> Icon {
        self.variants.push(variant);
        self
    }
    /// The icon with its Unicode text set.
    pub fn with_unicode(mut self, text: impl Into<String>) -> Icon {
        self.unicode = Some(text.into());
        self
    }

    /// Parses one of the string forms of the module docs.
    pub fn parse(text: &str) -> Result<Icon, Error> {
        let text = text.trim();
        if let Some(u) = text.strip_prefix("u:") {
            if u.is_empty() {
                return Err(Error::configuration_error("icon 'u:' has no text".to_string()));
            }
            return Ok(Icon::unicode(u));
        }
        Ok(Icon { variants: vec![IconVariant::parse(text)?], unicode: None })
    }

    /// A value from a command hint (`hints["icon"]`): a string form or the structured form.
    pub fn from_hint(value: &Json) -> Result<Icon, Error> {
        Icon::try_from(value.clone())
    }
}

const IMAGE_SUFFIXES: [(&str, bool); 5] =
    [(".svg", true), (".png", false), (".jpg", false), (".jpeg", false), (".webp", false)];

fn image_key(text: &str) -> Option<IconVariant> {
    let lower = text.to_ascii_lowercase();
    if !(text.starts_with('-') || text.contains('/')) {
        return None;
    }
    IMAGE_SUFFIXES.iter().find(|(s, _)| lower.ends_with(s)).map(|(_, svg)| {
        let source = IconSource::Key(text.to_string());
        if *svg {
            IconVariant::Svg(source)
        } else {
            IconVariant::Raster(source)
        }
    })
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl IconVariant {
    /// Parses a string form other than `u:`.
    pub fn parse(text: &str) -> Result<IconVariant, Error> {
        let text = text.trim();
        if let Some(v) = image_key(text) {
            return Ok(v);
        }
        let (set, name) = match text.split_once(':') {
            Some(("lib", name)) => {
                return if valid_name(name) {
                    Ok(IconVariant::Library(name.to_string()))
                } else {
                    Err(Error::configuration_error(format!("icon '{text}': bad library name")))
                };
            }
            Some(("ph", name)) => (PHOSPHOR, name),
            Some((set, name)) => (set, name),
            None => (PHOSPHOR, text),
        };
        if !valid_name(set) || !valid_name(name) {
            return Err(Error::configuration_error(format!(
                "icon '{text}' is not a set name, 'lib:', 'u:' or an image key"
            )));
        }
        Ok(IconVariant::Set { set: set.to_string(), name: name.to_string() })
    }

    /// The short string form, when the variant has one.
    pub fn short_form(&self) -> Option<String> {
        match self {
            IconVariant::Set { set, name } if set == PHOSPHOR => Some(format!("ph:{name}")),
            IconVariant::Set { set, name } => Some(format!("{set}:{name}")),
            IconVariant::Library(name) => Some(format!("lib:{name}")),
            IconVariant::Svg(IconSource::Key(k)) | IconVariant::Raster(IconSource::Key(k)) => {
                image_key(k).map(|_| k.clone())
            }
            IconVariant::Svg(IconSource::Inline(_)) | IconVariant::Raster(IconSource::Inline(_)) => None,
        }
    }
}

fn source_from_json(value: &Json, svg: bool) -> Result<IconSource, Error> {
    let obj = value
        .as_object()
        .ok_or_else(|| Error::configuration_error("an image icon needs {inline: …} or {key: …}".to_string()))?;
    if let Some(k) = obj.get("key").and_then(Json::as_str) {
        return Ok(IconSource::Key(k.to_string()));
    }
    let inline = obj
        .get("inline")
        .and_then(Json::as_str)
        .ok_or_else(|| Error::configuration_error("an image icon needs {inline: …} or {key: …}".to_string()))?;
    let bytes = if svg {
        inline.as_bytes().to_vec()
    } else {
        base64::engine::general_purpose::STANDARD
            .decode(inline.trim())
            .map_err(|e| Error::configuration_error(format!("raster icon is not base64: {e}")))?
    };
    Ok(IconSource::Inline(IconBytes(Cow::Owned(bytes))))
}

fn source_to_json(source: &IconSource, svg: bool) -> Json {
    match source {
        IconSource::Key(k) => json!({ "key": k }),
        IconSource::Inline(b) if svg => json!({ "inline": String::from_utf8_lossy(b.as_slice()) }),
        IconSource::Inline(b) => {
            json!({ "inline": base64::engine::general_purpose::STANDARD.encode(b.as_slice()) })
        }
    }
}

fn variant_from_json(value: &Json) -> Result<Vec<IconVariant>, Error> {
    match value {
        Json::String(s) => Ok(vec![IconVariant::parse(s)?]),
        Json::Object(obj) => {
            if let Some(svg) = obj.get("svg") {
                return Ok(vec![IconVariant::Svg(source_from_json(svg, true)?)]);
            }
            if let Some(raster) = obj.get("raster").or_else(|| obj.get("png")) {
                return Ok(vec![IconVariant::Raster(source_from_json(raster, false)?)]);
            }
            if let Some(name) = obj.get("library").and_then(Json::as_str) {
                return Ok(vec![IconVariant::Library(name.to_string())]);
            }
            if let Some(name) = obj.get("name").and_then(Json::as_str) {
                let set = obj.get("set").and_then(Json::as_str).unwrap_or(PHOSPHOR);
                return Ok(vec![IconVariant::Set { set: set.to_string(), name: name.to_string() }]);
            }
            Err(Error::configuration_error(format!("unknown icon variant {value}")))
        }
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::Array(_) => {
            Err(Error::configuration_error(format!("unknown icon variant {value}")))
        }
    }
}

fn variant_to_json(variant: &IconVariant) -> Json {
    if let Some(s) = variant.short_form() {
        return Json::String(s);
    }
    match variant {
        IconVariant::Set { set, name } => json!({ "set": set, "name": name }),
        IconVariant::Library(name) => json!({ "library": name }),
        IconVariant::Svg(s) => json!({ "svg": source_to_json(s, true) }),
        IconVariant::Raster(s) => json!({ "raster": source_to_json(s, false) }),
    }
}

impl TryFrom<Json> for Icon {
    type Error = Error;
    fn try_from(value: Json) -> Result<Self, Self::Error> {
        match &value {
            Json::String(s) => Icon::parse(s),
            Json::Array(items) => {
                let mut icon = Icon::default();
                for item in items {
                    match item.as_str().and_then(|s| s.strip_prefix("u:")) {
                        Some(u) => icon.unicode = Some(u.to_string()),
                        None => icon.variants.extend(variant_from_json(item)?),
                    }
                }
                Ok(icon)
            }
            Json::Object(obj) => {
                let mut icon = Icon {
                    variants: vec![],
                    unicode: obj.get("unicode").and_then(Json::as_str).map(str::to_string),
                };
                match obj.get("variants") {
                    Some(Json::Array(items)) => {
                        for item in items {
                            icon.variants.extend(variant_from_json(item)?);
                        }
                    }
                    Some(other) => icon.variants.extend(variant_from_json(other)?),
                    None => {
                        let rest: Map<String, Json> =
                            obj.iter().filter(|(k, _)| k.as_str() != "unicode").map(|(k, v)| (k.clone(), v.clone())).collect();
                        if !rest.is_empty() {
                            icon.variants.extend(variant_from_json(&Json::Object(rest))?);
                        }
                    }
                }
                Ok(icon)
            }
            Json::Null | Json::Bool(_) | Json::Number(_) => {
                Err(Error::configuration_error(format!("an icon cannot be {value}")))
            }
        }
    }
}

impl From<Icon> for Json {
    fn from(icon: Icon) -> Self {
        match (icon.variants.as_slice(), &icon.unicode) {
            ([], Some(u)) => Json::String(format!("u:{u}")),
            ([v], None) if v.short_form().is_some() => variant_to_json(v),
            (variants, unicode) => {
                let mut obj = Map::new();
                if let Some(u) = unicode {
                    obj.insert("unicode".to_string(), Json::String(u.clone()));
                }
                obj.insert("variants".to_string(), Json::Array(variants.iter().map(variant_to_json).collect()));
                Json::Object(obj)
            }
        }
    }
}

/// An icon and a hue for a namespace or a command (AC-14).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IconOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue: Option<String>,
}

/// The icon part of a theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IconTheme {
    /// The set a bare name refers to.
    pub icon_set: String,
    /// Named icons, referred to as `lib:<name>`.
    #[serde(default)]
    pub library: BTreeMap<String, Icon>,
    /// By namespace (`pl`).
    #[serde(default)]
    pub namespaces: BTreeMap<String, IconOverride>,
    /// By command (`pl/gt`, or `realm:pl/gt` for one realm).
    #[serde(default)]
    pub commands: BTreeMap<String, IconOverride>,
    /// Namespace icons used after a command's own hint: the built-in table of the bundled
    /// namespaces lives here, below everything a theme or command declares.
    #[serde(default)]
    pub defaults: BTreeMap<String, IconOverride>,
}
