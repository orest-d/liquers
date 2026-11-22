//! The configuration document of a Python host: what `lq.Environment(config=…)` reads.
//!
//! One file (or YAML text, or a dict) configures the whole host: the environment's keys (`store`,
//! `recipes`, `assets`, `default_namespaces`), a command metadata registry file
//! (`command_registry`), the server `env.serve()` starts (`server`) and this host's own settings
//! (`python`) and the window `lq.UI.run()` opens (`ui`: theme, mode, density). An unknown section
//! or key fails at `lq.Environment` or `configure`, naming what this host knows. A keyword argument
//! of `lq.Environment` or `serve` overrides the file, which overrides the built-in default.
//!
//! # Main components
//!
//! - [`PyConfig`]: the assembled configuration; [`PyConfig::from_python`] reads the three forms of
//!   `config=` (a `pathlib.Path` or other `os.PathLike` is a file, a `str` is YAML text, a `dict`
//!   is the document itself) and merges `overrides=` over it (`ConfigDocument::overlay`, so a
//!   file's folder still applies to its own `command_registry`); [`PyConfig::from_document`]
//!   takes the sections.
//! - [`PythonSettings`]: the `python` section (`conversion`, `pickle`, `async_loops`), each the
//!   default of the matching keyword.
//!
//! # Where it is used
//!
//! - `Environment.__new__` and `Environment.configure` (`environment.rs`, `Settings::resolve`)
//!   build the environment's settings from it; `liquers/_cli.py` passes command-line options as
//!   `overrides`.
//! - `Environment.serve` (`server.rs`) starts from its `server` section.
//! - `tests/test_CONFIG_document.py` checks it end to end.
//!
//! # Related concepts
//!
//! - [`ConfigDocument`] (liquers-core) is the host-neutral document; this module is the Python
//!   host's choice of sections.
//! - [`ServerConfig`] (liquers-axum) is the `server` section alone, shared with Rust hosts.
//! - `EnvironmentConfig` is the environment's part; the `python` and `server` sections never reach
//!   it.
//!
//! # Examples
//!
//! ```
//! use liquers_core::environment_config::ConfigDocument;
//! use liquers_py::config::PyConfig;
//!
//! let doc = ConfigDocument::from_yaml("python: {pickle: true}\nserver: {port: 0}\n").unwrap();
//! let config = PyConfig::from_document(doc).unwrap();
//! assert_eq!(config.python.pickle, Some(true));
//! assert_eq!(config.server.port, 0);
//! assert!(PyConfig::from_document(ConfigDocument::from_yaml("sever: {}\n").unwrap()).is_err());
//! ```
//!
//! More: `liquers-py/tests/test_CONFIG_document.py` and `specs/reference/PYTHON_INTEGRATION.md`.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use liquers_axum::ServerConfig;
use liquers_core::environment_config::{ConfigDocument, EnvironmentConfig};
use liquers_core::error::Error;
use liquers_lib::ui::theme::UiConfig;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

/// The `python` section: defaults of the `lq.Environment` keywords of the same names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PythonSettings {
    /// `"permissive"` (the default) or `"strict"`.
    #[serde(default)]
    pub conversion: Option<String>,
    /// Whether Python objects without another byte form are pickled (default false).
    #[serde(default)]
    pub pickle: Option<bool>,
    /// The asyncio loops `async def` commands run on (default 1).
    #[serde(default)]
    pub async_loops: Option<NonZeroUsize>,
}

/// Everything a Python host reads from its configuration document.
#[derive(Debug, Clone, Default)]
pub struct PyConfig {
    /// The environment's keys.
    pub environment: EnvironmentConfig,
    /// The command metadata registry file, resolved against the document's folder.
    pub command_registry: Option<PathBuf>,
    /// The `server` section, used by `serve` only.
    pub server: ServerConfig,
    /// The `python` section.
    pub python: PythonSettings,
    /// The `ui` section: the theme, mode and density of the window `lq.UI.run()` opens.
    pub ui: Option<UiConfig>,
}

impl PyConfig {
    /// Takes the sections this host knows and refuses any other.
    pub fn from_document(mut doc: ConfigDocument) -> Result<Self, Error> {
        let server = ServerConfig::from_document(&mut doc)?;
        let python = doc.take_section::<PythonSettings>("python")?.unwrap_or_default();
        let ui = doc.take_section::<UiConfig>("ui")?;
        let command_registry = doc.command_registry_path();
        let environment = doc.finish()?;
        Ok(PyConfig { environment, command_registry, server, python, ui })
    }

    /// Reads `config=`: an `os.PathLike` names a file (`.yaml`, `.yml` or `.json`), a `str` is
    /// YAML text, a `dict` is the document. `None` is the defaults.
    pub fn from_python(
        py: Python<'_>,
        config: Option<&Bound<'_, PyAny>>,
        overrides: Option<&Bound<'_, PyAny>>,
    ) -> Result<Self, Error> {
        let overrides = match overrides {
            Some(overrides) if !overrides.is_none() => Some(overrides_to_json(py, overrides)?),
            Some(_) | None => None,
        };
        let mut doc = match config {
            Some(config) if !config.is_none() => Self::document(py, config)?,
            Some(_) | None => match overrides {
                None => return Ok(PyConfig::default()),
                Some(_) => ConfigDocument::default(),
            },
        };
        if let Some(overrides) = overrides {
            doc.overlay(overrides)?;
        }
        PyConfig::from_document(doc)
    }

    /// The document `config=` names: a file, YAML text or a dict.
    fn document(py: Python<'_>, config: &Bound<'_, PyAny>) -> Result<ConfigDocument, Error> {
        let doc = if let Ok(text) = config.cast::<PyString>() {
            let text = text.to_string();
            ConfigDocument::from_yaml(&text).map_err(|e| explain_text(&text, e))?
        } else if config.hasattr("__fspath__").unwrap_or(false) {
            let path: PathBuf = py
                .import("os")
                .and_then(|os| os.call_method1("fspath", (config,)))
                .and_then(|path| path.extract::<PathBuf>())
                .map_err(|e| Error::configuration_error(format!("config: not a usable path: {e}")))?;
            ConfigDocument::load(&path)?
        } else if config.cast::<PyDict>().is_ok() {
            let json = py
                .import("json")
                .and_then(|json| json.call_method1("dumps", (config,)))
                .and_then(|dumped| dumped.extract::<String>())
                .map_err(|e| {
                    Error::configuration_error(format!("config: the dict is not plain data: {e}"))
                })?;
            ConfigDocument::from_json(&json)?
        } else {
            let type_name = config
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "?".to_string());
            return Err(Error::configuration_error(format!(
                "config must be a pathlib.Path (a file), a str (YAML text) or a dict, not {type_name}"
            )));
        };
        Ok(doc)
    }
}

/// `overrides=`: a dict in the document's form, as JSON.
fn overrides_to_json(py: Python<'_>, overrides: &Bound<'_, PyAny>) -> Result<serde_json::Value, Error> {
    if overrides.cast::<PyDict>().is_err() {
        return Err(Error::configuration_error(
            "overrides must be a dict in the configuration document's form".to_string(),
        ));
    }
    let json = py
        .import("json")
        .and_then(|json| json.call_method1("dumps", (overrides,)))
        .and_then(|dumped| dumped.extract::<String>())
        .map_err(|e| Error::configuration_error(format!("overrides: the dict is not plain data: {e}")))?;
    serde_json::from_str(&json).map_err(|e| Error::parse_error(format!("overrides: {e}")))
}

/// A `str` that is not a document: if it is a single scalar it was most likely meant as a file
/// name, so the error says how to pass one.
fn explain_text(text: &str, error: Error) -> Error {
    match serde_yaml::from_str::<serde_yaml::Value>(text) {
        Ok(serde_yaml::Value::String(_))
        | Ok(serde_yaml::Value::Number(_))
        | Ok(serde_yaml::Value::Bool(_)) => Error::configuration_error(format!(
            "config={text:?} is a str, which is read as YAML text, and it is not a document; to read a file, pass a path: config=pathlib.Path({text:?})"
        )),
        Ok(serde_yaml::Value::Null)
        | Ok(serde_yaml::Value::Sequence(_))
        | Ok(serde_yaml::Value::Mapping(_))
        | Ok(serde_yaml::Value::Tagged(_))
        | Err(_) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::error::ErrorType;

    #[test]
    fn sections_are_taken_and_others_refused() {
        let doc = ConfigDocument::from_yaml(
            "assets: {job_capacity: 3}\ncommand_registry: c.yaml\npython: {pickle: true, async_loops: 2}\nserver: {port: 0}\n",
        )
        .unwrap();
        let config = PyConfig::from_document(doc).unwrap();
        assert_eq!(config.environment.assets.job_capacity, Some(3));
        assert_eq!(config.command_registry, Some(PathBuf::from("c.yaml")));
        assert_eq!(config.python.async_loops.map(NonZeroUsize::get), Some(2));
        assert_eq!(config.server.port, 0);
        let error = PyConfig::from_document(ConfigDocument::from_yaml("sever: {port: 1}\n").unwrap())
            .unwrap_err();
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        assert!(error.to_string().contains("python") && error.to_string().contains("server"), "{error}");
        let error = PyConfig::from_document(ConfigDocument::from_yaml("python: {async_loops: 0}\n").unwrap())
            .unwrap_err();
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
    }

    #[test]
    fn the_ui_section_is_taken() {
        use liquers_lib::ui::theme::{Density, ModeSetting};
        let doc = ConfigDocument::from_yaml("ui: {theme: default, mode: light, density: compact}\n").unwrap();
        let config = PyConfig::from_document(doc).unwrap();
        let ui = config.ui.unwrap();
        assert_eq!(ui.mode, Some(ModeSetting::Light));
        assert_eq!(ui.density, Some(Density::Compact));
        let none = PyConfig::from_document(ConfigDocument::from_yaml("python: {}\n").unwrap()).unwrap();
        assert!(none.ui.is_none());
    }

    #[test]
    fn a_scalar_str_is_explained() {
        let error = explain_text("liquers.yaml", Error::parse_error("x".into()));
        assert!(error.to_string().contains("pathlib.Path(\"liquers.yaml\")"), "{error}");
        let kept = explain_text("[1, 2]", Error::parse_error("kept".into()));
        assert!(kept.to_string().contains("kept"));
    }
}
