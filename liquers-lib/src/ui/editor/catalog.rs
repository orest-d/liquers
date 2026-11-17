//! The searchable command tree of the command picker (AC-17).

use liquers_core::command_metadata::CommandMetadataRegistry;

use super::tree::{CatalogCommand, CatalogNamespace};

/// The commands of `realm`, grouped by namespace (the root namespace first, then by name) and
/// filtered by `filter`: a command is kept when its name, label or doc contains the filter's text,
/// ignoring case, or when its namespace name does.
pub fn command_catalog(registry: &CommandMetadataRegistry, realm: &str, filter: &str) -> Vec<CatalogNamespace> {
    let needle = filter.trim().to_lowercase();
    let mut out: Vec<CatalogNamespace> = Vec::new();
    let mut commands: Vec<_> = registry.commands.iter().filter(|c| c.realm == realm).collect();
    commands.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    for md in commands {
        let matches = needle.is_empty()
            || md.name.to_lowercase().contains(&needle)
            || md.label.to_lowercase().contains(&needle)
            || md.doc.to_lowercase().contains(&needle)
            || md.namespace.to_lowercase().contains(&needle);
        if !matches {
            continue;
        }
        let command = CatalogCommand {
            key: md.key(),
            label: if md.label.is_empty() { md.name.clone() } else { md.label.clone() },
            doc: md.doc.clone(),
        };
        match out.last_mut() {
            Some(ns) if ns.namespace == md.namespace => ns.commands.push(command),
            Some(_) | None => out.push(CatalogNamespace {
                namespace: md.namespace.clone(),
                commands: vec![command],
            }),
        }
    }
    out
}
