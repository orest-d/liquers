//! Assembling the command registry a validation runs against.
//!
//! The base is always empty; everything else is an override — a serialized registry merged in,
//! or a permissive command declared on the command line.

use serde::de::DeserializeOwned;

use crate::command_metadata::{
    ArgumentInfo, CommandKey, CommandMetadata, CommandMetadataRegistry,
};
use crate::error::Error;

use super::report::RegistryProvenance;

/// Deserialize JSON, falling back to YAML.
///
/// On failure *both* diagnostics are reported: which parser "should" have succeeded is not
/// knowable from the text alone, and showing only one misleads on a malformed file of the other
/// kind. YAML is a superset of JSON in practice, but a JSON file with a subtle syntax error
/// produces a far clearer message from `serde_json` than from `serde_yaml`.
pub fn from_json_or_yaml<T: DeserializeOwned>(source_name: &str, text: &str) -> Result<T, Error> {
    let json_error = match serde_json::from_str::<T>(text) {
        Ok(value) => return Ok(value),
        Err(e) => e,
    };
    let yaml_error = match serde_yaml::from_str::<T>(text) {
        Ok(value) => return Ok(value),
        Err(e) => e,
    };
    Err(Error::general_error(format!(
        "Could not parse '{source_name}' as JSON or YAML.\n  as JSON: {json_error}\n  as YAML: {yaml_error}"
    )))
}

/// Assembles a [`CommandMetadataRegistry`] from an empty base plus overrides, recording
/// provenance as it goes.
#[derive(Debug, Clone)]
pub struct ValidationRegistryBuilder {
    registry: CommandMetadataRegistry,
    provenance: RegistryProvenance,
    allow_overwrite: bool,
}

impl Default for ValidationRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationRegistryBuilder {
    pub fn new() -> Self {
        ValidationRegistryBuilder {
            registry: CommandMetadataRegistry::new(),
            provenance: RegistryProvenance::default(),
            allow_overwrite: false,
        }
    }

    /// Permit a merged command to replace one already present.
    ///
    /// Needed when a design document changes an existing command's *signature*: the key is
    /// already in the base registry, so redefining it is a duplicate. Adding a genuinely new
    /// command never needs this.
    pub fn with_overwrite_allowed(mut self, allow: bool) -> Self {
        self.allow_overwrite = allow;
        self
    }

    /// Merge a serialized [`CommandMetadataRegistry`] (JSON or YAML).
    ///
    /// Errors on a duplicate [`CommandKey`] unless overwrite is allowed. The check must happen
    /// *here*, before `add_command`, because `add_command` replaces an existing entry silently
    /// and so cannot report the collision itself.
    pub fn merge_str(&mut self, source_name: &str, text: &str) -> Result<&mut Self, Error> {
        let merged: CommandMetadataRegistry = from_json_or_yaml(source_name, text)?;

        for command in &merged.commands {
            let key = command.key();
            if !self.allow_overwrite && self.registry.get(key.clone()).is_some() {
                return Err(Error::general_error(format!(
                    "Duplicate command '{}' from '{}': already defined. \
                     Pass --allow-overwrite to replace it (needed when a design changes an \
                     existing command's signature).",
                    format_key(&key),
                    source_name
                )));
            }
            self.registry.add_command(command);
        }

        for namespace in merged.default_namespaces {
            if !self.registry.default_namespaces.contains(&namespace) {
                self.registry.default_namespaces.push(namespace);
            }
        }
        for (name, global_enum) in merged.global_enums {
            self.registry.global_enums.insert(name, global_enum);
        }

        self.provenance.merged_files.push(source_name.to_string());
        Ok(self)
    }

    /// Add a permissive command from a CLI spec: `name`, `ns/name` or `realm/ns/name`.
    ///
    /// The command carries a single `Any` + `multiple` argument, so it accepts any number of
    /// arguments of any type — which is the point: it stands in for a command that does not
    /// exist yet.
    pub fn add_permissive_command(&mut self, spec: &str) -> Result<&mut Self, Error> {
        let key = parse_command_spec(spec)?;
        if !self.allow_overwrite && self.registry.get(key.clone()).is_some() {
            return Err(Error::general_error(format!(
                "Duplicate command '{}' declared with --command: already defined. \
                 Pass --allow-overwrite to replace it.",
                format_key(&key)
            )));
        }

        let mut command = CommandMetadata::from_key(key.clone());
        command.arguments = vec![ArgumentInfo::any_argument("arguments").set_multiple()];
        command.doc =
            "Permissive command declared on the command line; accepts any arguments.".to_string();
        self.registry.add_command(&command);
        self.provenance.cli_commands.push(key);
        Ok(self)
    }

    /// Note a registry source that was resolved but is not a file (used for provenance only).
    pub fn note_source(&mut self, name: &str) -> &mut Self {
        self.provenance.merged_files.push(name.to_string());
        self
    }

    pub fn build(mut self) -> (CommandMetadataRegistry, RegistryProvenance) {
        self.provenance.command_count = self.registry.commands.len();
        self.provenance
            .default_namespaces
            .clone_from(&self.registry.default_namespaces);
        (self.registry, self.provenance)
    }
}

/// `name` | `ns/name` | `realm/ns/name`
fn parse_command_spec(spec: &str) -> Result<CommandKey, Error> {
    let parts: Vec<&str> = spec.split('/').collect();
    let key = match parts.as_slice() {
        [name] => CommandKey::new("", "root", name),
        [namespace, name] => CommandKey::new("", namespace, name),
        [realm, namespace, name] => CommandKey::new(realm, namespace, name),
        _ => {
            return Err(Error::general_error(format!(
                "Malformed command spec '{spec}'; expected 'name', 'ns/name' or 'realm/ns/name'"
            )))
        }
    };
    if key.name.is_empty() {
        return Err(Error::general_error(format!(
            "Malformed command spec '{spec}': the command name is empty"
        )));
    }
    Ok(key)
}

/// Render a key for a diagnostic. `CommandKey::new` normalizes the default namespace `root` to
/// the empty string, so print something a reader recognizes.
fn format_key(key: &CommandKey) -> String {
    match (key.realm.as_str(), key.namespace.as_str()) {
        ("", "") => key.name.clone(),
        ("", namespace) => format!("{}/{}", namespace, key.name),
        (realm, namespace) => format!("{}/{}/{}", realm, namespace, key.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::CommandMetadata;

    fn registry_yaml(name: &str, namespace: &str) -> String {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cm = CommandMetadata::new(name);
        cm.namespace = namespace.to_string();
        cmr.add_command(&cm);
        serde_yaml::to_string(&cmr).expect("serialize registry")
    }

    /// U11
    #[test]
    fn merge_adds_commands() -> Result<(), Error> {
        let mut builder = ValidationRegistryBuilder::new();
        builder.merge_str("a.yaml", &registry_yaml("to_text", "root"))?;
        let (registry, provenance) = builder.build();

        assert_eq!(registry.commands.len(), 1);
        assert_eq!(provenance.command_count, 1);
        assert_eq!(provenance.merged_files, vec!["a.yaml".to_string()]);
        assert!(registry.find_command("", "root", "to_text").is_some());
        Ok(())
    }

    /// U12
    #[test]
    fn merge_duplicate_key_errors() -> Result<(), Error> {
        let mut builder = ValidationRegistryBuilder::new();
        builder.merge_str("base.yaml", &registry_yaml("head", "pl"))?;
        let err = builder
            .merge_str("overlay.yaml", &registry_yaml("head", "pl"))
            .expect_err("duplicate key must be rejected");

        assert!(err.message.contains("head"), "message names the key: {err}");
        assert!(
            err.message.contains("overlay.yaml"),
            "message names the file: {err}"
        );
        assert!(
            err.message.contains("--allow-overwrite"),
            "message suggests the escape: {err}"
        );
        Ok(())
    }

    /// U13
    #[test]
    fn merge_duplicate_allowed_with_overwrite() -> Result<(), Error> {
        let mut builder = ValidationRegistryBuilder::new().with_overwrite_allowed(true);
        builder.merge_str("base.yaml", &registry_yaml("head", "pl"))?;
        builder.merge_str("overlay.yaml", &registry_yaml("head", "pl"))?;
        let (registry, _) = builder.build();

        assert_eq!(registry.commands.len(), 1, "overwrite replaces, not appends");
        Ok(())
    }

    /// U14
    #[test]
    fn permissive_spec_forms() -> Result<(), Error> {
        let mut builder = ValidationRegistryBuilder::new();
        builder.add_permissive_command("greet")?;
        builder.add_permissive_command("custom/transform")?;
        builder.add_permissive_command("other/ns/thing")?;
        let (_, provenance) = builder.build();

        // CommandKey::new normalizes the default namespace `root` to the empty string.
        assert_eq!(provenance.cli_commands[0], CommandKey::new("", "root", "greet"));
        assert_eq!(provenance.cli_commands[0].namespace, "");
        assert_eq!(
            provenance.cli_commands[1],
            CommandKey::new("", "custom", "transform")
        );
        assert_eq!(
            provenance.cli_commands[2],
            CommandKey::new("other", "ns", "thing")
        );

        let mut builder = ValidationRegistryBuilder::new();
        assert!(builder.add_permissive_command("a/b/c/d").is_err());
        assert!(builder.add_permissive_command("").is_err());
        Ok(())
    }

    /// U15 — the permissive command must accept any argument list.
    #[test]
    fn permissive_command_takes_multiple_any_arguments() -> Result<(), Error> {
        let mut builder = ValidationRegistryBuilder::new();
        builder.add_permissive_command("greet")?;
        let (registry, _) = builder.build();

        let command = registry
            .find_command("", "root", "greet")
            .expect("greet registered");
        assert_eq!(command.arguments.len(), 1);
        assert!(command.arguments[0].multiple, "must consume all parameters");
        assert!(
            command.arguments[0].argument_type.is_any(),
            "must accept any type"
        );
        Ok(())
    }

    /// U22 — JSON and YAML both work; garbage names both parsers.
    #[test]
    fn from_json_or_yaml_both_and_neither() -> Result<(), Error> {
        let json = serde_json::to_string(&CommandMetadataRegistry::new())
            .map_err(|e| Error::from_error(crate::error::ErrorType::General, e))?;
        let _: CommandMetadataRegistry = from_json_or_yaml("r.json", &json)?;

        let yaml = registry_yaml("to_text", "root");
        let _: CommandMetadataRegistry = from_json_or_yaml("r.yaml", &yaml)?;

        let err = from_json_or_yaml::<CommandMetadataRegistry>("bad.txt", "{{{ not either")
            .expect_err("garbage must fail");
        assert!(err.message.contains("as JSON"), "reports JSON: {err}");
        assert!(err.message.contains("as YAML"), "reports YAML: {err}");
        assert!(err.message.contains("bad.txt"), "names the source: {err}");
        Ok(())
    }

    #[test]
    fn merge_unions_default_namespaces() -> Result<(), Error> {
        let mut cmr = CommandMetadataRegistry::new();
        cmr.default_namespaces.push("pl".to_string());
        let text = serde_yaml::to_string(&cmr).expect("serialize");

        let mut builder = ValidationRegistryBuilder::new();
        builder.merge_str("ns.yaml", &text)?;
        let (registry, provenance) = builder.build();

        assert!(registry.default_namespaces.contains(&"pl".to_string()));
        assert!(provenance.default_namespaces.contains(&"pl".to_string()));
        Ok(())
    }
}
