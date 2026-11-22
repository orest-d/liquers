//! The `ui` section of a configuration document (AC-16).
//!
//! ```yaml
//! ui:
//!   theme: -R/config/liquers-theme.yaml   # a key or query, or a built-in name (`default`)
//!   mode: dark                            # dark | light | system
//!   density: compact                      # comfortable | compact
//! ```

use std::fmt;

use liquers_core::error::Error;
use serde::{Deserialize, Serialize};

use super::cell::{ThemeCell, ThemeState};
use super::model::{Density, Theme, ThemeMode};

/// The colour mode a host is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeSetting {
    /// Follow the platform (egui's system theme, the browser's `prefers-color-scheme`).
    #[default]
    System,
    Dark,
    Light,
}

/// A theme that ships with Liquers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTheme {
    Default,
}

impl BuiltinTheme {
    const ALL: [(&'static str, BuiltinTheme); 1] = [("default", BuiltinTheme::Default)];

    /// The theme.
    pub fn theme(&self) -> std::sync::Arc<Theme> {
        match self {
            BuiltinTheme::Default => Theme::builtin(),
        }
    }
}

/// Where a host's theme comes from: a built-in name, else a key or query evaluated in the
/// environment (a file is a key on a `file` store mount).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ThemeSource {
    Builtin(BuiltinTheme),
    Query(String),
}

impl TryFrom<String> for ThemeSource {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let text = value.trim();
        if text.is_empty() {
            return Err(Error::configuration_error("ui.theme is empty".to_string()));
        }
        if let Some((_, b)) = BuiltinTheme::ALL.iter().find(|(n, _)| *n == text) {
            return Ok(ThemeSource::Builtin(*b));
        }
        if text.contains('/') || text.contains('.') {
            return Ok(ThemeSource::Query(text.to_string()));
        }
        Err(Error::configuration_error(format!(
            "ui.theme '{text}' is neither a built-in theme ({}) nor a key or query",
            BuiltinTheme::ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
        )))
    }
}

impl fmt::Display for ThemeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeSource::Builtin(b) => {
                let name = BuiltinTheme::ALL.iter().find(|(_, x)| x == b).map(|(n, _)| *n).unwrap_or("default");
                f.write_str(name)
            }
            ThemeSource::Query(q) => f.write_str(q),
        }
    }
}

impl From<ThemeSource> for String {
    fn from(value: ThemeSource) -> Self {
        value.to_string()
    }
}

/// The `ui` host section, taken with `ConfigDocument::take_section::<UiConfig>("ui")`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UiConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ModeSetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
}

impl UiConfig {
    /// The mode to start in: `system_dark` answers `System` (and an unset mode).
    pub fn initial_mode(&self, system_dark: bool) -> ThemeMode {
        match self.mode.unwrap_or_default() {
            ModeSetting::Dark => ThemeMode::Dark,
            ModeSetting::Light => ThemeMode::Light,
            ModeSetting::System => {
                if system_dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                }
            }
        }
    }

    /// Whether the mode follows the platform.
    pub fn follows_system(&self) -> bool {
        matches!(self.mode.unwrap_or_default(), ModeSetting::System)
    }

    /// A cell with the built-in theme named by the section (a key or query is loaded later by
    /// the runner), the initial mode and the density.
    pub fn cell(&self, system_dark: bool) -> ThemeCell {
        let theme = match &self.theme {
            Some(ThemeSource::Builtin(b)) => b.theme(),
            Some(ThemeSource::Query(_)) | None => Theme::builtin(),
        };
        ThemeCell::new(ThemeState {
            theme,
            mode: self.initial_mode(system_dark),
            density: self.density.unwrap_or_default(),
            generation: 0,
        })
    }
}
