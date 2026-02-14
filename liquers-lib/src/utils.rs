use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::parse::parse_query;
use liquers_core::query::{ActionParameter, QuerySegment};
use liquers_core::state::State;
use crate::value::Value;

/// A resolved next-command preset with the full query already constructed.
/// The query string has the preset action already appended (with namespace
/// injection if needed), ready to be submitted directly.
#[derive(Clone, Debug)]
pub struct NextPreset {
    /// The complete query string with the preset applied.
    pub query: String,
    /// Human-readable label for UI display.
    pub label: String,
    /// Description of what the preset does.
    pub description: String,
}

/// Find next-command presets for a given query and state.
///
/// Sources of presets:
/// 1. Explicit presets from `CommandMetadata.next` of the last action in the query.
///
/// For each preset, constructs the full output query by appending the preset's
/// encoded action to the original query string, with `ns-<namespace>/` injection
/// when the preset command's namespace differs from the query's active namespace.
///
/// Returns an empty Vec if the query cannot be parsed or has no transform segment.
pub fn find_next_presets(
    query: &str,
    _state: &State<Value>,
    registry: &CommandMetadataRegistry,
) -> Vec<NextPreset> {
    let parsed = match parse_query(query) {
        Ok(q) => q,
        Err(_) => return vec![],
    };

    // Get the last transform query segment
    let tqs = match parsed.segments.last() {
        Some(QuerySegment::Transform(tqs)) => tqs,
        Some(QuerySegment::Resource(_)) => return vec![],
        None => return vec![],
    };

    // Find the last non-ns, non-q action
    let last_action = match tqs.query.iter().rev().find(|a| !a.is_ns() && !a.is_q()) {
        Some(a) => a,
        None => return vec![],
    };

    // Determine the active namespace context from the query
    let active_ns = active_namespace(&tqs.query.iter().flat_map(|a| a.ns()).flatten().collect(), registry);

    // Look up command metadata for the last action
    let namespaces = build_namespace_list(&active_ns, registry);
    let cmd_meta = match registry.find_command_in_namespaces("", &namespaces, &last_action.name) {
        Some(m) => m,
        None => return vec![],
    };

    // Build presets from metadata.next
    let base_query = query.trim_end_matches('/');
    let mut result = Vec::with_capacity(cmd_meta.next.len());
    for preset in &cmd_meta.next {
        let encoded_action = preset.action.encode();

        // Check if the preset action's command lives in a different namespace.
        // First try finding it in the current namespace context.
        let preset_meta = registry.find_command_in_namespaces("", &namespaces, &preset.action.name);
        let full_query = if preset_meta.is_some() {
            // Found in active namespace context — no prefix needed
            format!("{}/{}", base_query, encoded_action)
        } else {
            // Not in active namespaces — search all commands to find its namespace
            let all_ns_meta = registry.commands.iter().find(|c| c.name == preset.action.name);
            match all_ns_meta {
                Some(m) if !m.namespace.is_empty() => {
                    format!("{}/ns-{}/{}", base_query, m.namespace, encoded_action)
                }
                _ => {
                    // Command not found at all — just append without prefix
                    format!("{}/{}", base_query, encoded_action)
                }
            }
        };

        result.push(NextPreset {
            query: full_query,
            label: preset.label.clone(),
            description: preset.description.clone(),
        });
    }

    result
}

/// Determine the active namespace from ns action parameters.
/// Returns the last namespace set by `ns-<name>` actions, or empty string if none.
fn active_namespace(ns_params: &Vec<ActionParameter>, _registry: &CommandMetadataRegistry) -> String {
    // The ns action parameters encode the namespace(s)
    // e.g. ns-lui sets namespace to "lui"
    ns_params
        .last()
        .map(|p| p.encode())
        .unwrap_or_default()
}

/// Build the list of namespaces to search, starting with the active namespace
/// and including the registry's default namespaces.
fn build_namespace_list(active_ns: &str, registry: &CommandMetadataRegistry) -> Vec<String> {
    let mut namespaces = Vec::new();
    if !active_ns.is_empty() {
        namespaces.push(active_ns.to_string());
    }
    for ns in &registry.default_namespaces {
        if !namespaces.contains(ns) {
            namespaces.push(ns.clone());
        }
    }
    namespaces
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_core::command_metadata::{CommandMetadata, CommandPreset};
    use liquers_core::metadata::Metadata;
    use liquers_core::query::ActionRequest;

    fn make_state() -> State<Value> {
        State {
            data: Arc::new(Value::from("test")),
            metadata: Arc::new(Metadata::new()),
        }
    }

    fn make_registry_with_preset() -> CommandMetadataRegistry {
        let mut registry = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("uppercase");
        cmd.namespace = "".to_string();
        cmd.next.push(CommandPreset {
            action: ActionRequest::new("lowercase".to_string()),
            label: "Lowercase".to_string(),
            description: "Convert to lowercase".to_string(),
        });
        registry.add_command(&cmd);

        let mut lower = CommandMetadata::new("lowercase");
        lower.namespace = "".to_string();
        registry.add_command(&lower);

        registry
    }

    #[test]
    fn test_empty_query() {
        let state = make_state();
        let registry = CommandMetadataRegistry::new();
        let result = find_next_presets("", &state, &registry);
        assert!(result.is_empty());
    }

    #[test]
    fn test_invalid_query() {
        let state = make_state();
        let registry = CommandMetadataRegistry::new();
        // An unparseable query should return empty
        let result = find_next_presets("///", &state, &registry);
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_command_metadata() {
        let state = make_state();
        let registry = CommandMetadataRegistry::new();
        let result = find_next_presets("text-Hello/unknown_cmd", &state, &registry);
        assert!(result.is_empty());
    }

    #[test]
    fn test_preset_found() {
        let state = make_state();
        let registry = make_registry_with_preset();
        let result = find_next_presets("text-Hello/uppercase", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].query, "text-Hello/uppercase/lowercase");
        assert_eq!(result[0].label, "Lowercase");
        assert_eq!(result[0].description, "Convert to lowercase");
    }

    #[test]
    fn test_no_presets_defined() {
        let state = make_state();
        let mut registry = CommandMetadataRegistry::new();
        let cmd = CommandMetadata::new("uppercase");
        registry.add_command(&cmd);
        let result = find_next_presets("text-Hello/uppercase", &state, &registry);
        assert!(result.is_empty());
    }

    #[test]
    fn test_preset_with_namespace_injection() {
        let state = make_state();
        let mut registry = CommandMetadataRegistry::new();

        let mut cmd = CommandMetadata::new("uppercase");
        cmd.namespace = "".to_string();
        cmd.next.push(CommandPreset {
            action: ActionRequest::new("special_cmd".to_string()),
            label: "Special".to_string(),
            description: "A special command".to_string(),
        });
        registry.add_command(&cmd);

        // The special_cmd lives in "special" namespace
        let mut special = CommandMetadata::new("special_cmd");
        special.namespace = "special".to_string();
        registry.add_command(&special);

        let result = find_next_presets("text-Hello/uppercase", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].query, "text-Hello/uppercase/ns-special/special_cmd");
    }
}
