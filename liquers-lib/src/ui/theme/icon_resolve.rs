//! Which icon a card draws: the precedence of AC-8 and AC-14, `lib:` expansion and the Unicode
//! fallback of AC-17.

use std::collections::HashSet;
use std::sync::OnceLock;

use liquers_core::command_metadata::CommandKey;
use liquers_core::error::Error;

use super::icon::{Icon, IconTheme, IconVariant, PHOSPHOR};
use super::model::Theme;

static ICON_NAMES: &str = include_str!("icons.txt");

/// Whether `name` is a Phosphor icon (from the embedded name list, so it works without egui).
pub fn is_icon_name(name: &str) -> bool {
    static NAMES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| ICON_NAMES.lines().map(str::trim).filter(|l| !l.is_empty()).collect())
        .contains(name)
}

/// Whether a renderer that knows only Phosphor can draw `variant` without loading anything.
pub fn is_known_set_icon(variant: &IconVariant) -> bool {
    match variant {
        IconVariant::Set { set, name } => set == PHOSPHOR && is_icon_name(name),
        IconVariant::Library(_) | IconVariant::Svg(_) | IconVariant::Raster(_) => false,
    }
}

/// The icon to draw: every candidate's variants in precedence order, and a Unicode text that is
/// never empty.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedIcon<'a> {
    pub variants: Vec<&'a IconVariant>,
    pub unicode: String,
    pub hue: Option<&'a str>,
}

/// The first letter of `label` (or `?`), upper-cased: the monogram fallback.
pub fn monogram(label: &str) -> String {
    label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_string())
}

/// Refuses a `lib:` reference to a missing library icon, or to another library icon (a library
/// icon is expanded one level deep, so a chain could not be drawn and a cycle could not end).
pub(crate) fn check_library(icons: &IconTheme) -> Result<(), Error> {
    for (name, icon) in &icons.library {
        for variant in &icon.variants {
            match variant {
                IconVariant::Library(other) => {
                    return Err(Error::configuration_error(format!(
                        "theme: library icon '{name}' refers to library icon '{other}'; \
                         a library icon cannot refer to another"
                    )));
                }
                IconVariant::Set { .. } | IconVariant::Svg(_) | IconVariant::Raster(_) => {}
            }
        }
    }
    let all = icons
        .namespaces
        .values()
        .chain(icons.commands.values())
        .chain(icons.defaults.values())
        .filter_map(|o| o.icon.as_ref());
    for icon in all {
        for variant in &icon.variants {
            if let IconVariant::Library(name) = variant {
                if !icons.library.contains_key(name) {
                    return Err(Error::configuration_error(format!(
                        "theme: icon 'lib:{name}' is not in icons.library"
                    )));
                }
            }
        }
    }
    Ok(())
}

impl Theme {
    /// The icon of a card (AC-8, AC-14, AC-17). Precedence: the theme's entry for the command
    /// (with realm, then without), the theme's entry for the namespace, the command's `hint
    /// icon`, the namespace's registry icon (`NAMESPACES-HAVE-NO-METADATA`, later), and the
    /// theme's namespace defaults (`icons.defaults`, the built-in table). Variants of every candidate
    /// are concatenated, `lib:` references expanded; the Unicode text is the first candidate's,
    /// else the monogram of `label`. The hue is the first a theme entry gives.
    pub fn icon_for<'a>(
        &'a self,
        key: &CommandKey,
        label: &str,
        hint: Option<&'a Icon>,
        namespace_icon: Option<&'a Icon>,
    ) -> ResolvedIcon<'a> {
        let plain = format!("{}/{}", key.namespace, key.name);
        let realmed = format!("{}:{}", key.realm, plain);
        let mut candidates: Vec<&'a Icon> = Vec::new();
        let mut hue: Option<&'a str> = None;
        let mut take = |o: Option<&'a super::icon::IconOverride>, candidates: &mut Vec<&'a Icon>| {
            if let Some(o) = o {
                if let Some(icon) = &o.icon {
                    candidates.push(icon);
                }
                if hue.is_none() {
                    hue = o.hue.as_deref();
                }
            }
        };
        if !key.realm.is_empty() {
            take(self.icons.commands.get(&realmed), &mut candidates);
        }
        take(self.icons.commands.get(&plain), &mut candidates);
        take(self.icons.namespaces.get(&key.namespace), &mut candidates);
        candidates.extend(hint);
        candidates.extend(namespace_icon);
        take(self.icons.defaults.get(&key.namespace), &mut candidates);

        let mut variants: Vec<&'a IconVariant> = Vec::new();
        let mut unicode: Option<String> = None;
        for icon in candidates {
            for variant in &icon.variants {
                match variant {
                    IconVariant::Library(name) => {
                        if let Some(lib) = self.icons.library.get(name) {
                            variants.extend(lib.variants.iter());
                            if unicode.is_none() {
                                unicode = lib.unicode.clone();
                            }
                        }
                    }
                    IconVariant::Set { .. } | IconVariant::Svg(_) | IconVariant::Raster(_) => {
                        variants.push(variant)
                    }
                }
            }
            if unicode.is_none() {
                unicode = icon.unicode.clone();
            }
        }
        ResolvedIcon {
            variants,
            unicode: unicode.filter(|u| !u.is_empty()).unwrap_or_else(|| monogram(label)),
            hue,
        }
    }
}
