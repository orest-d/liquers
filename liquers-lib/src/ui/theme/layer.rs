//! Themes as composable nested dictionaries.
//!
//! A [`ThemeLayer`] is a partial theme. Layers compose by deep merge: a later layer changes the
//! values of fields that exist and adds fields that do not. The typed [`Theme`] is read from the
//! composed dictionary; keys the model does not know are kept and can be read with
//! [`Theme::lookup`].

use std::sync::{Arc, OnceLock};

use liquers_core::error::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use super::builtin::builtin_theme;
use super::model::Theme;

/// A partial theme: a nested dictionary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThemeLayer(pub Map<String, Json>);

impl ThemeLayer {
    /// A layer from YAML text (JSON is YAML too).
    pub fn from_yaml(text: &str) -> Result<ThemeLayer, Error> {
        let value: Json = serde_yaml::from_str(text)
            .map_err(|e| Error::configuration_error(format!("theme: {e}")))?;
        ThemeLayer::from_value(value)
    }

    /// A layer from a structured value, which must be a mapping (an empty document is an empty
    /// layer).
    pub fn from_value(value: Json) -> Result<ThemeLayer, Error> {
        match value {
            Json::Object(map) => Ok(ThemeLayer(map)),
            Json::Null => Ok(ThemeLayer::default()),
            Json::Bool(_) | Json::Number(_) | Json::String(_) | Json::Array(_) => Err(
                Error::configuration_error("theme: a theme layer must be a mapping".to_string()),
            ),
        }
    }

    /// Deep merge of `over` into `self`: mappings merge by key, anything else is replaced.
    pub fn merge(&mut self, over: &ThemeLayer) {
        merge_maps(&mut self.0, &over.0);
    }
}

fn merge_maps(base: &mut Map<String, Json>, over: &Map<String, Json>) {
    for (key, value) in over {
        match (base.get_mut(key), value) {
            (Some(Json::Object(b)), Json::Object(o)) => merge_maps(b, o),
            (Some(slot), v) => *slot = v.clone(),
            (None, v) => {
                base.insert(key.clone(), v.clone());
            }
        }
    }
}

fn theme_dict(theme: &Theme) -> Map<String, Json> {
    match serde_json::to_value(theme) {
        Ok(Json::Object(map)) => map,
        Ok(_) | Err(_) => Map::new(),
    }
}

fn from_dict(dict: Map<String, Json>) -> Result<Theme, Error> {
    let mut theme: Theme = serde_json::from_value(Json::Object(dict.clone()))
        .map_err(|e| Error::configuration_error(format!("theme: {e}")))?;
    super::icon_resolve::check_library(&theme.icons)?;
    theme.dict = dict;
    Ok(theme)
}

impl Theme {
    /// The built-in theme (dark and light, Comfortable and Compact). It cannot fail.
    pub fn builtin() -> Arc<Theme> {
        static BUILTIN: OnceLock<Arc<Theme>> = OnceLock::new();
        BUILTIN
            .get_or_init(|| {
                let mut theme = builtin_theme();
                theme.dict = theme_dict(&theme);
                Arc::new(theme)
            })
            .clone()
    }

    /// The built-in theme with `layers` merged over it, in order.
    pub fn compose(layers: &[ThemeLayer]) -> Result<Theme, Error> {
        let mut dict = ThemeLayer(Theme::builtin().dict.clone());
        for layer in layers {
            dict.merge(layer);
        }
        from_dict(dict.0)
    }

    /// This theme with `layer` merged over it.
    pub fn with_layer(&self, layer: &ThemeLayer) -> Result<Theme, Error> {
        let mut dict = ThemeLayer(self.dict.clone());
        dict.merge(layer);
        from_dict(dict.0)
    }

    /// The composed dictionary, unknown keys included.
    pub fn layer(&self) -> ThemeLayer {
        ThemeLayer(self.dict.clone())
    }

    /// The value at a dotted path (`editor.card.elevation`), known to the model or not.
    pub fn lookup(&self, path: &str) -> Option<&Json> {
        let mut parts = path.split('.');
        let first = parts.next()?;
        let mut current = self.dict.get(first)?;
        for part in parts {
            current = current.as_object()?.get(part)?;
        }
        Some(current)
    }

    /// The topmost paths of the composed dictionary that the model does not know: a lint for
    /// typos, not an error. Open maps (`hues` and the maps of `icons`) accept any key.
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        let known = Theme::builtin();
        unknown_in(&self.dict, &known.dict, "", &mut out);
        out
    }
}

const OPEN_MAPS: [&str; 5] =
    ["hues", "icons.library", "icons.namespaces", "icons.commands", "icons.defaults"];

fn unknown_in(dict: &Map<String, Json>, known: &Map<String, Json>, prefix: &str, out: &mut Vec<String>) {
    for (key, value) in dict {
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match known.get(key) {
            None => out.push(path),
            Some(k) => {
                if OPEN_MAPS.contains(&path.as_str()) {
                    continue;
                }
                if let (Json::Object(d), Json::Object(k)) = (value, k) {
                    unknown_in(d, k, &path, out);
                }
            }
        }
    }
}
