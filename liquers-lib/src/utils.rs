use crate::value::Value;
use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::parse::parse_query;
use liquers_core::query::{ActionParameter, ActionRequest, QuerySegment};
use liquers_core::state::State;

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
/// The last command is resolved as the planner does: planner instructions (`ns`, `q`, `v`) are
/// skipped, it is looked up in the namespaces of the last `ns` before it, and in the segment's
/// realm.
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

    // The last action that is a command, not a planner instruction
    let Some(last_index) = tqs.query.iter().rposition(|a| !is_planner_instruction(a)) else {
        return vec![];
    };
    let last_action = &tqs.query[last_index];

    // The command sees only an `ns` before it; an appended preset sees the whole segment's.
    let mut command_prefix = tqs.clone();
    command_prefix.query.truncate(last_index + 1);
    let Some(command_namespaces) = namespaces_from(command_prefix.last_ns(), registry) else {
        return vec![];
    };
    let Some(namespaces) = namespaces_from(tqs.last_ns(), registry) else {
        return vec![];
    };
    let realm = tqs.name();

    // Look up command metadata for the last action
    let cmd_meta =
        match registry.find_command_in_namespaces(&realm, &command_namespaces, &last_action.name) {
            Some(m) => m,
            None => return vec![],
        };

    // Build presets from metadata.next
    let base_query = query.trim_end_matches('/');
    let mut result = Vec::with_capacity(cmd_meta.next.len());
    for preset in &cmd_meta.next {
        let ns_hint = if !preset.ns.is_empty() {
            preset.ns.clone()
        } else {
            let preset_meta =
                registry.find_command_in_namespaces(&realm, &namespaces, &preset.action.name);
            if preset_meta.is_some() {
                "".to_string()
            } else {
                registry
                    .commands
                    .iter()
                    .find(|c| c.name == preset.action.name)
                    .map(|m| m.namespace.clone())
                    .unwrap_or_default()
            }
        };
        let full_query = match liquers_core::plan::append_action(
            &parsed,
            &ns_hint,
            preset.action.clone(),
            registry,
        ) {
            Ok(q) => q.encode(),
            Err(_) => format!("{}/{}", base_query, preset.action.encode()),
        };

        result.push(NextPreset {
            query: full_query,
            label: preset.label.clone(),
            description: preset.description.clone(),
        });
    }

    result
}

/// Whether the action is a planner instruction rather than a command (`RECIPES_AND_PLANS.md`
/// §Planner instructions). `v` has no public predicate in `liquers-core` yet
/// (`planner-instruction-names-public`).
fn is_planner_instruction(action: &ActionRequest) -> bool {
    action.is_ns() || action.is_q() || action.name == "v"
}

/// The namespaces a command is looked up in, as the planner builds them: those of the `ns`
/// instruction, then the registry's defaults. `None` when an `ns` parameter is not a plain
/// string, which the planner refuses.
fn namespaces_from(
    ns: Option<Vec<ActionParameter>>,
    registry: &CommandMetadataRegistry,
) -> Option<Vec<String>> {
    let mut explicit = Vec::new();
    for parameter in ns.unwrap_or_default() {
        match parameter {
            ActionParameter::String(s, _) => explicit.push(s),
            ActionParameter::Link(_, _) => return None,
        }
    }
    Some(registry.lookup_namespaces(&explicit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::command_metadata::{CommandMetadata, CommandPreset};
    use liquers_core::metadata::Metadata;
    use std::sync::Arc;

    fn make_state() -> State<Value> {
        State::from_parts(Arc::new(Value::from("test")), Arc::new(Metadata::new()))
    }

    fn make_registry_with_preset() -> CommandMetadataRegistry {
        let mut registry = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("uppercase");
        cmd.namespace = "".to_string();
        cmd.next.push(CommandPreset {
            action: ActionRequest::new("lowercase".to_string()),
            ns: "".to_string(),
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
            ns: "special".to_string(),
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
        assert_eq!(
            result[0].query,
            "text-Hello/uppercase/ns-special/special_cmd"
        );
    }

    fn preset(action: &str, ns: &str) -> CommandPreset {
        CommandPreset {
            action: ActionRequest::new(action.to_string()),
            ns: ns.to_string(),
            label: action.to_string(),
            description: String::new(),
        }
    }

    /// `head` in namespace `pl` with a `tail` preset; `tail` lives in `pl` too.
    fn registry_with_pl_head(next: CommandPreset) -> CommandMetadataRegistry {
        let mut registry = CommandMetadataRegistry::new();
        let mut head = CommandMetadata::new("head");
        head.namespace = "pl".to_string();
        head.next.push(next);
        registry.add_command(&head);
        let mut tail = CommandMetadata::new("tail");
        tail.namespace = "pl".to_string();
        registry.add_command(&tail);
        registry
    }

    /// AC-1: a trailing `v` is a planner instruction, not the last command.
    #[test]
    fn test_trailing_v_is_skipped() {
        let state = make_state();
        let registry = make_registry_with_preset();
        let result = find_next_presets("text-Hello/uppercase/v", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].query, "text-Hello/uppercase/v/lowercase");
    }

    /// AC-2: every namespace of the last `ns` applies, not only its last parameter.
    #[test]
    fn test_all_namespaces_of_ns_apply() {
        let state = make_state();
        let registry = registry_with_pl_head(preset("tail", ""));
        let result = find_next_presets("ns-pl-img/head", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "tail");
    }

    /// AC-3: an `ns` after the command does not apply to it.
    #[test]
    fn test_only_ns_before_the_command_applies() {
        let state = make_state();
        let registry = registry_with_pl_head(preset("tail", ""));
        let result = find_next_presets("ns-pl/head/ns-img", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "tail");
    }

    /// AC-4: the command is looked up in the segment's realm.
    #[test]
    fn test_segment_realm_applies() {
        let state = make_state();
        let mut registry = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("cmd");
        cmd.realm = "r".to_string();
        cmd.next.push(preset("next", ""));
        registry.add_command(&cmd);
        let result = find_next_presets("-r/cmd", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "next");
    }

    /// AC-5: a preset appended after a trailing `ns-img` needs no injection when it lives there.
    #[test]
    fn test_preset_after_trailing_ns_is_not_injected() {
        let state = make_state();
        let mut registry = registry_with_pl_head(preset("show", "img"));
        let mut show = CommandMetadata::new("show");
        show.namespace = "img".to_string();
        registry.add_command(&show);
        let result = find_next_presets("ns-pl/head/ns-img", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].query, "ns-pl/head/ns-img/show");
    }

    /// plan-default-namespaces AC-9: `next` presets resolve the last command through the
    /// configured chain, and a preset in a configured namespace still carries its `ns`.
    #[test]
    fn next_presets_use_configured_chain() {
        let state = make_state();
        let mut registry = registry_with_pl_head(preset("tail", "pl"));
        assert!(find_next_presets("head", &state, &registry).is_empty());
        registry.default_namespaces =
            CommandMetadataRegistry::default_namespace_chain(&["pl".to_string()]);
        let result = find_next_presets("head", &state, &registry);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].query, "head/ns-pl/tail");
    }
}
