//! Includes from `specs/`: a section of a Markdown document by its heading (`ns-docs/include`),
//! and the command library generated from the committed registry (`ns-docs/command_table`).

use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::value::Value;

use crate::markdown::{code_mask, heading, is_absolute_url, link_targets, strip_number};

/// The body of the section whose heading is `wanted`, with its subsections, without its own
/// heading. The heading matches case-insensitively, with or without its section number
/// (`Argument defaults` matches `### 4.3 Argument defaults`).
///
/// Fails when no heading matches, or when the body holds a relative link, which would break
/// once the text is moved into a chapter. `source` names the document in errors.
pub fn include_section(source: &str, text: &str, wanted: &str) -> Result<String, Error> {
    let lines: Vec<&str> = text.lines().collect();
    let mask = code_mask(&lines);
    let wanted_full = wanted.trim().to_lowercase();
    let wanted_bare = strip_number(wanted).to_lowercase();
    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        if mask[i] {
            continue;
        }
        if let Some((level, title)) = heading(line) {
            let full = title.to_lowercase();
            let bare = strip_number(&title).to_lowercase();
            if full == wanted_full || bare == wanted_bare {
                start = Some((i, level));
                break;
            }
        }
    }
    let (start, level) = start.ok_or_else(|| {
        Error::general_error(format!("{source}: no section with the heading '{wanted}'"))
    })?;
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if mask[i] {
            continue;
        }
        if let Some((l, _)) = heading(line) {
            if l <= level {
                end = i;
                break;
            }
        }
    }
    let body = &lines[start + 1..end];
    let body_mask = &mask[start + 1..end];
    for (offset, line) in body.iter().enumerate() {
        if body_mask[offset] {
            continue;
        }
        for target in link_targets(line) {
            if !is_absolute_url(&target) {
                return Err(Error::general_error(format!(
                    "{source}: section '{wanted}' has a relative link '{target}' (line {}); an \
                     included section must be self-contained",
                    start + offset + 2
                )));
            }
        }
    }
    let first = body.iter().position(|l| !l.trim().is_empty()).unwrap_or(body.len());
    let last = body.iter().rposition(|l| !l.trim().is_empty()).map(|p| p + 1).unwrap_or(first);
    Ok(body[first..last.max(first)].join("\n"))
}

/// The source key of a state, for error messages.
fn source_of(state: &State<Value>) -> String {
    match state.metadata.key() {
        Ok(Some(key)) => key.encode(),
        Ok(None) | Err(_) => "the included document".to_string(),
    }
}

/// `ns-docs/include-<heading>`: the section of the state (a Markdown document) with that heading.
pub fn include(state: &State<Value>, heading: String) -> Result<Value, Error> {
    let source = source_of(state);
    let text = state.try_into_string()?;
    Ok(Value::from(include_section(&source, &text, &heading)?))
}

/// The command library as Markdown: one table per namespace, from a registry export
/// (`specs/command_registry.yaml`).
pub fn command_table_markdown(source: &str, text: &str) -> Result<String, Error> {
    let registry: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|e| Error::general_error(format!("{source}: cannot parse the registry: {e}")))?;
    let commands = registry
        .get("commands")
        .and_then(|c| c.as_sequence())
        .ok_or_else(|| Error::general_error(format!("{source}: no `commands` list")))?;
    let field = |c: &serde_yaml::Value, name: &str| -> String {
        c.get(name)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut rows: Vec<(String, String, String, String)> = Vec::new();
    for command in commands {
        let realm = field(command, "realm");
        let namespace = match (realm.is_empty(), field(command, "namespace")) {
            (true, ns) if ns.is_empty() => "root".to_string(),
            (true, ns) => format!("ns-{ns}"),
            (false, ns) if ns.is_empty() => format!("realm {realm}"),
            (false, ns) => format!("realm {realm}, ns-{ns}"),
        };
        let name = field(command, "name");
        let arguments: Vec<String> = command
            .get("arguments")
            .and_then(|a| a.as_sequence())
            .map(|args| {
                args.iter()
                    .filter(|a| !a.get("injected").and_then(|i| i.as_bool()).unwrap_or(false))
                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let doc = field(command, "doc");
        let first = doc.lines().next().unwrap_or("");
        let mut description = match first.find(". ") {
            Some(end) => first[..=end].to_string(),
            None => first.to_string(),
        };
        if description.is_empty() {
            description = field(command, "label");
        }
        rows.push((namespace, name, arguments.join(", "), description));
    }
    rows.sort();
    let mut out = String::new();
    let mut current: Option<String> = None;
    for (namespace, name, arguments, description) in rows {
        if current.as_deref() != Some(namespace.as_str()) {
            let title = &namespace;
            if current.is_some() {
                out.push('\n');
            }
            out.push_str(&format!(
                "### {title}\n\n| Command | Arguments | Description |\n|---|---|---|\n"
            ));
            current = Some(namespace.clone());
        }
        let cell = |s: &str| s.replace('|', "\\|").replace('<', "&lt;").replace('>', "&gt;");
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            name,
            cell(&arguments),
            cell(&description)
        ));
    }
    Ok(out)
}

/// `ns-docs/command_table`: the state (a registry export) as Markdown tables.
pub fn command_table(state: &State<Value>) -> Result<Value, Error> {
    let source = source_of(state);
    let text = state.try_into_string()?;
    Ok(Value::from(command_table_markdown(&source, &text)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Commands\n\nIntro.\n\n## 4. Arguments\n\n### 4.3 Argument defaults\n\nA default is used\nwhen omitted.\n\n#### Links\n\nA link default.\n\n```\n# not a heading\n```\n\n### 4.4 Next\n\nOther.\n\n## Linked\n\nSee [x](../x.md).\n";

    #[test]
    fn section_body_with_subsections() {
        let body = include_section("doc.md", DOC, "Argument defaults").expect("found");
        assert!(body.starts_with("A default is used"));
        assert!(body.contains("#### Links"));
        assert!(body.contains("# not a heading"));
        assert!(!body.contains("4.4 Next"));
        assert!(!body.contains("4.3"));
        assert_eq!(include_section("doc.md", DOC, "4.3 argument DEFAULTS").expect("found"), body);
        let arguments = include_section("doc.md", DOC, "Arguments").expect("found");
        assert!(arguments.contains("### 4.4 Next"));
    }

    #[test]
    fn missing_section_and_relative_link_fail() {
        let missing = include_section("specs/doc.md", DOC, "Nowhere").expect_err("missing");
        assert!(missing.to_string().contains("specs/doc.md"));
        assert!(missing.to_string().contains("Nowhere"));
        let linked = include_section("specs/doc.md", DOC, "Linked").expect_err("relative");
        assert!(linked.to_string().contains("../x.md"));
        assert!(linked.to_string().contains("specs/doc.md"));
    }

    #[test]
    fn command_table_groups_by_namespace() {
        let yaml = "commands:\n- namespace: pl\n  name: head\n  label: First rows\n  arguments:\n  - name: n\n  - name: context\n    injected: true\n- namespace: ''\n  name: to_text\n  doc: Convert to text. More.\n- realm: store_config\n  name: set\n  label: Set\n";
        let table = command_table_markdown("registry.yaml", yaml).expect("table");
        assert!(table.contains("### root"));
        assert!(table.contains("| `to_text` |  | Convert to text. |"));
        assert!(table.contains("### ns-pl"));
        assert!(table.contains("| `head` | n | First rows |"));
        assert!(table.contains("### realm store_config"));
        assert!(table.contains("| `set` |  | Set |"));
    }
}
