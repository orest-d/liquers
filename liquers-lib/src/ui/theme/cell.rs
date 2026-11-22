//! The theme an element reads, shared between the runner (which swaps it) and every
//! `UIContext` (which reads it), and the cache of icon images loaded by key.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::model::{Density, Theme, ThemeMode};

/// A snapshot of the current theme: cheap to clone (one `Arc`).
#[derive(Debug, Clone)]
pub struct ThemeState {
    pub theme: Arc<Theme>,
    pub mode: ThemeMode,
    /// The density of an editor that does not set its own.
    pub density: Density,
    /// Bumped by every change, so a renderer re-applies the theme only when it changed.
    pub generation: u64,
}

impl Default for ThemeState {
    fn default() -> Self {
        ThemeState {
            theme: Theme::builtin(),
            mode: ThemeMode::Dark,
            density: Density::Comfortable,
            generation: 0,
        }
    }
}

/// The shared, swappable theme (AC-16). Clones share the cell.
#[derive(Debug, Clone, Default)]
pub struct ThemeCell(Arc<RwLock<ThemeState>>);

impl ThemeCell {
    /// A cell holding `state`.
    pub fn new(state: ThemeState) -> Self {
        ThemeCell(Arc::new(RwLock::new(state)))
    }

    /// The current theme. A poisoned lock still yields the last value.
    pub fn get(&self) -> ThemeState {
        match self.0.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn update(&self, f: impl FnOnce(&mut ThemeState)) {
        let mut guard = match self.0.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        f(&mut guard);
        guard.generation = guard.generation.wrapping_add(1);
    }

    /// Replaces the theme.
    pub fn set_theme(&self, theme: Arc<Theme>) {
        self.update(|s| s.theme = theme);
    }

    /// Sets the colour mode; a change only (setting the same mode does not bump the generation).
    pub fn set_mode(&self, mode: ThemeMode) {
        if self.get().mode != mode {
            self.update(|s| s.mode = mode);
        }
    }

    /// Sets the default density.
    pub fn set_density(&self, density: Density) {
        if self.get().density != density {
            self.update(|s| s.density = density);
        }
    }
}

/// The state of an icon image loaded by key.
#[derive(Debug, Clone)]
pub enum IconLoad {
    /// Requested; the renderer draws the next variant meanwhile.
    Pending,
    /// The image's bytes.
    Ready(Arc<[u8]>),
    /// Loading failed; not retried until the cache is cleared.
    Failed,
}

/// Icon images loaded by key (AC-17). Renderers ask with [`Self::get_or_request`]; the runner
/// fills the cache when it has evaluated the key.
#[derive(Debug, Clone, Default)]
pub struct IconCache(Arc<RwLock<HashMap<String, IconLoad>>>);

impl IconCache {
    /// The bytes of `key` when loaded. The first ask marks it pending and returns `true` in the
    /// second element, which tells the caller to request the load.
    pub fn get_or_mark(&self, key: &str) -> (Option<Arc<[u8]>>, bool) {
        if let Some(state) = self.peek(key) {
            return match state {
                IconLoad::Ready(bytes) => (Some(bytes), false),
                IconLoad::Pending | IconLoad::Failed => (None, false),
            };
        }
        let mut guard = match self.0.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let first = !guard.contains_key(key);
        if first {
            guard.insert(key.to_string(), IconLoad::Pending);
        }
        (None, first)
    }

    /// The state of `key`, if it was ever asked for.
    pub fn peek(&self, key: &str) -> Option<IconLoad> {
        match self.0.read() {
            Ok(g) => g.get(key).cloned(),
            Err(p) => p.into_inner().get(key).cloned(),
        }
    }

    /// Records the result of a load.
    pub fn set(&self, key: &str, load: IconLoad) {
        let mut guard = match self.0.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.insert(key.to_string(), load);
    }

    /// Forgets every entry (after a theme change).
    pub fn clear(&self) {
        let mut guard = match self.0.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.clear();
    }
}
