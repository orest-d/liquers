//! The `server` section of a configuration document: [`ServerConfig`], [`Api`] and
//! [`WebSetting`]. Documented with the rest of the server in [`crate::server`].

use liquers_core::environment_config::ConfigDocument;
use liquers_core::error::Error;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use serde::{Deserialize, Serialize};

/// One of the standard APIs a server can mount under its prefix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Api {
    /// `{prefix}/q/{*query}`: evaluate a query.
    Query,
    /// `{prefix}/api/store/…`: read and write the store (left out when `read_only`).
    Store,
    /// `{prefix}/api/assets/…`: assets, their state and the WebSocket notifications.
    Assets,
    /// `{prefix}/api/recipes/…`: the recipes the providers know.
    Recipes,
}

impl Api {
    /// Every standard API, in mounting order: the default of `apis`.
    pub fn all() -> Vec<Api> {
        vec![Api::Query, Api::Store, Api::Assets, Api::Recipes]
    }
}

/// Whether, and from which store folder, `{prefix}/web/…` serves stored files.
///
/// `web: true` (or absent) serves store key `web/…`; `web: false` turns the web folder and the
/// index redirect off; `web: site` serves store key `site/…`. Serializes back to the same form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum WebSetting {
    /// On (root `web`) or off.
    Enabled(bool),
    /// On, with this store folder as its root.
    Root(String),
}

impl Default for WebSetting {
    fn default() -> Self {
        WebSetting::Enabled(true)
    }
}

impl WebSetting {
    /// The store folder served, or `None` when the web folder is off. A root that is not a key
    /// (or is empty) is a `ConfigurationError`.
    pub fn root(&self) -> Result<Option<Key>, Error> {
        match self {
            WebSetting::Enabled(false) => Ok(None),
            WebSetting::Enabled(true) => Ok(Some(Key::new().join("web"))),
            WebSetting::Root(root) => {
                let trimmed = root.trim_matches('/');
                let key = parse_key(trimmed).map_err(|e| {
                    Error::configuration_error(format!(
                        "server.web: '{root}' is not a store key: {}",
                        e.message
                    ))
                })?;
                if key.is_empty() {
                    return Err(Error::configuration_error(format!(
                        "server.web: '{root}' names no store folder; use `web: false` to turn the web folder off"
                    )));
                }
                Ok(Some(key))
            }
        }
    }
}

/// The settings of a Liquers server, as the `server` section of a configuration document.
///
/// Every key is optional; the defaults are a local server on `127.0.0.1:8080` with every API under
/// `/liquer`, writes enabled and the web folder on. An unknown key is refused.
///
/// ```
/// use liquers_axum::server::{Api, ServerConfig, WebSetting};
/// let config: ServerConfig = serde_yaml::from_str("port: 0\napis: [query, assets]\nweb: site\n").unwrap();
/// assert_eq!(config.host, "127.0.0.1");
/// assert_eq!(config.apis, vec![Api::Query, Api::Assets]);
/// assert_eq!(config.web, WebSetting::Root("site".into()));
/// assert!(serde_yaml::from_str::<ServerConfig>("prot: 1\n").is_err());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// The address to listen on (default `127.0.0.1`, this machine only). `0.0.0.0` makes the
    /// server reachable from the network: security-sensitive.
    #[serde(default = "default_host")]
    pub host: String,
    /// The port (default 8080); 0 picks a free one, which the running server reports.
    #[serde(default = "default_port")]
    pub port: u16,
    /// The path every route is under (default `/liquer`); a trailing `/` is ignored and `/` means
    /// the root.
    #[serde(default = "default_prefix")]
    pub prefix: String,
    /// The APIs to mount (default: all four).
    #[serde(default = "Api::all")]
    pub apis: Vec<Api>,
    /// Leave out every route that changes data: the store API and the assets mutation routes.
    /// Off by default (writes enabled): security-sensitive.
    #[serde(default)]
    pub read_only: bool,
    /// The web folder (default on, store key `web`).
    #[serde(default)]
    pub web: WebSetting,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_prefix() -> String {
    "/liquer".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: default_host(),
            port: default_port(),
            prefix: default_prefix(),
            apis: Api::all(),
            read_only: false,
            web: WebSetting::default(),
        }
    }
}

impl ServerConfig {
    /// Takes the `server` section of a configuration document; an absent section gives the
    /// defaults. The prefix and the web root are checked here.
    pub fn from_document(doc: &mut ConfigDocument) -> Result<Self, Error> {
        let config: ServerConfig = doc.take_section("server")?.unwrap_or_default();
        config.validate()?;
        Ok(config)
    }

    /// Checks the prefix (empty, or starting with `/`) and the web root (a store key).
    pub fn validate(&self) -> Result<(), Error> {
        normalize_prefix(&self.prefix)?;
        self.web.root()?;
        Ok(())
    }

    /// The prefix as routes use it: no trailing `/`, and `""` for the root.
    pub fn normalized_prefix(&self) -> Result<String, Error> {
        normalize_prefix(&self.prefix)
    }
}

pub(crate) fn normalize_prefix(prefix: &str) -> Result<String, Error> {
    let trimmed = prefix.trim_end_matches('/');
    if !trimmed.is_empty() && !trimmed.starts_with('/') {
        return Err(Error::configuration_error(format!(
            "server.prefix: '{prefix}' must start with '/'"
        )));
    }
    if trimmed.contains("//") || trimmed.contains('{') || trimmed.contains('}') {
        return Err(Error::configuration_error(format!(
            "server.prefix: '{prefix}' is not a plain path"
        )));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::error::ErrorType;

    #[test]
    fn web_setting_forms() {
        let parse = |text: &str| serde_yaml::from_str::<ServerConfig>(text).expect("parse");
        assert_eq!(parse("{}").web, WebSetting::Enabled(true));
        assert_eq!(parse("web: true").web, WebSetting::Enabled(true));
        assert_eq!(parse("web: false").web, WebSetting::Enabled(false));
        assert_eq!(parse("web: site").web, WebSetting::Root("site".into()));
        assert_eq!(parse("web: site").web.root().expect("root").map(|k| k.encode()), Some("site".into()));
        assert_eq!(parse("{}").web.root().expect("root").map(|k| k.encode()), Some("web".into()));
        assert_eq!(parse("web: false").web.root().expect("root"), None);
        for web in [WebSetting::Enabled(true), WebSetting::Enabled(false), WebSetting::Root("site".into())] {
            let text = serde_yaml::to_string(&ServerConfig { web: web.clone(), ..Default::default() }).expect("yaml");
            assert_eq!(serde_yaml::from_str::<ServerConfig>(&text).expect("back").web, web, "{text}");
        }
        assert!(serde_yaml::to_string(&WebSetting::Root("site".into())).expect("yaml").contains("site"));
    }

    #[test]
    fn unknown_key_is_refused() {
        let mut doc = ConfigDocument::from_yaml("server:\n  prot: 8000\n").expect("document");
        let error = ServerConfig::from_document(&mut doc).expect_err("unknown key");
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        assert!(error.to_string().contains("prot"), "{error}");
        let mut doc = ConfigDocument::from_yaml("server:\n  port: high\n").expect("document");
        assert_eq!(
            ServerConfig::from_document(&mut doc).expect_err("wrong type").error_type,
            ErrorType::ConfigurationError
        );
    }

    /// Pins the defaults that `configuration-document` Example 1 writes out.
    #[test]
    fn defaults_match_the_documented_ones() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.prefix, "/liquer");
        assert_eq!(config.apis, vec![Api::Query, Api::Store, Api::Assets, Api::Recipes]);
        assert!(!config.read_only);
        assert_eq!(config.web, WebSetting::Enabled(true));
        let mut empty = ConfigDocument::from_yaml("").expect("document");
        assert_eq!(ServerConfig::from_document(&mut empty).expect("defaults"), config);
        let mut bare = ConfigDocument::from_yaml("server:\n").expect("document");
        assert_eq!(ServerConfig::from_document(&mut bare).expect("defaults"), config);
        let written = "server:\n  host: 127.0.0.1\n  port: 8080\n  prefix: /liquer\n  apis: [query, store, assets, recipes]\n  read_only: false\n  web: true\n";
        let mut doc = ConfigDocument::from_yaml(written).expect("document");
        assert_eq!(ServerConfig::from_document(&mut doc).expect("written"), config);
    }

    #[test]
    fn bad_prefix_or_web_key_is_refused() {
        for text in ["server: {prefix: liquer}", "server: {web: ''}", "server: {web: '/'}", "server: {web: [a]}", "server: {prefix: '/a//b'}"] {
            let mut doc = ConfigDocument::from_yaml(text).expect("document");
            let error = ServerConfig::from_document(&mut doc).expect_err(text);
            assert_eq!(error.error_type, ErrorType::ConfigurationError, "{text}: {error}");
        }
        assert_eq!(normalize_prefix("/").expect("root"), "");
        assert_eq!(normalize_prefix("/api/").expect("trailing"), "/api");
        assert_eq!(normalize_prefix("").expect("empty"), "");
    }
}
