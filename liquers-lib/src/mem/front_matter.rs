//! A line-preserving front-matter editor: every byte outside the lines it changes is kept, so a
//! field set by the service is a one-line diff in a hand-maintained file.

use indexmap::IndexMap;
use liquers_core::error::Error;
use liquers_core::metadata::Version;

/// A Markdown document split into its YAML front-matter lines and its body.
#[derive(Clone, Debug)]
pub struct FrontMatterDoc {
    /// The opening `---` line with its line ending; empty when the document has no front-matter.
    open: String,
    /// The front-matter lines, each with its own line ending.
    lines: Vec<String>,
    /// The closing `---` line with its line ending.
    close: String,
    body: String,
}

/// The line ending a document uses, from its first line.
fn newline_of(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        Some(_) | None => "\n",
    }
}

fn split_lines_keep(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn is_delimiter(line: &str, delimiter: &str) -> bool {
    line.trim_end_matches(['\n', '\r']).trim_end() == delimiter
}

impl FrontMatterDoc {
    /// Splits `text`. A document without front-matter has none and keeps its whole text as the
    /// body; TOML front-matter (`+++`) is refused.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let lines = split_lines_keep(text);
        let first = lines.first().copied().unwrap_or("");
        if is_delimiter(first, "+++") {
            return Err(Error::not_supported(
                "TOML front-matter (+++) cannot be edited; use YAML (---)".to_string(),
            ));
        }
        if !is_delimiter(first, "---") {
            return Ok(FrontMatterDoc {
                open: String::new(),
                lines: Vec::new(),
                close: String::new(),
                body: text.to_string(),
            });
        }
        let close = lines[1..].iter().position(|line| is_delimiter(line, "---") || is_delimiter(line, "..."));
        match close {
            Some(i) => {
                let end = i + 1;
                Ok(FrontMatterDoc {
                    open: first.to_string(),
                    lines: lines[1..end].iter().map(|l| l.to_string()).collect(),
                    close: lines[end].to_string(),
                    body: lines[end + 1..].concat(),
                })
            }
            None => Err(Error::parse_error(
                "the front-matter opened with '---' is never closed".to_string(),
            )),
        }
    }

    /// Whether the document has front-matter.
    pub fn has_front_matter(&self) -> bool {
        !self.open.is_empty()
    }

    /// The front-matter as a YAML mapping (empty when there is none).
    pub fn fields(&self) -> Result<IndexMap<String, serde_json::Value>, Error> {
        let yaml = self.lines.concat();
        if yaml.trim().is_empty() {
            return Ok(IndexMap::new());
        }
        let value: serde_json::Value = serde_yaml::from_str(&yaml)
            .map_err(|e| Error::parse_error(format!("front-matter is not valid YAML: {e}")))?;
        match value {
            serde_json::Value::Object(map) => Ok(map.into_iter().collect()),
            serde_json::Value::Null => Ok(IndexMap::new()),
            serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
            | serde_json::Value::Array(_) => {
                Err(Error::parse_error("front-matter is not a YAML mapping".to_string()))
            }
        }
    }

    /// One field's value; `None` when absent or when the front-matter does not parse.
    pub fn get(&self, field: &str) -> Option<serde_json::Value> {
        self.fields().ok()?.get(field).cloned()
    }

    /// One field as text: a string as is, a number or boolean in its YAML form, a list joined
    /// with `, `; empty when absent or null.
    pub fn get_text(&self, field: &str) -> String {
        value_text(self.get(field).as_ref())
    }

    /// Sets a top-level field to a scalar or a list (written as a flow list), replacing its line
    /// and any continuation lines, or appending it at the end of the front-matter. Creates the
    /// front-matter when the document has none.
    pub fn set(&mut self, field: &str, value: &serde_json::Value) -> Result<(), Error> {
        if field.is_empty() || field.contains(':') || field.starts_with(['#', '-', ' ']) {
            return Err(Error::general_error(format!("'{field}' is not a front-matter field name")));
        }
        let newline = if self.has_front_matter() { newline_of(&self.open) } else { newline_of(&self.body) };
        let line = format!("{field}: {}{newline}", render_value(value)?);
        if !self.has_front_matter() {
            self.open = format!("---{newline}");
            self.close = format!("---{newline}");
            self.lines = vec![line];
            if !self.body.starts_with(['\n', '\r']) && !self.body.is_empty() {
                self.body = format!("{newline}{}", self.body);
            }
            return Ok(());
        }
        let start = self.lines.iter().position(|l| {
            l.strip_prefix(field).is_some_and(|rest| rest.starts_with(':'))
        });
        match start {
            Some(start) => {
                let mut end = start + 1;
                while end < self.lines.len() && is_continuation(&self.lines[end]) {
                    end += 1;
                }
                self.lines.splice(start..end, [line]);
            }
            None => {
                if let Some(last) = self.lines.last_mut() {
                    if !last.ends_with('\n') {
                        last.push_str(newline);
                    }
                }
                self.lines.push(line);
            }
        }
        Ok(())
    }

    /// The text after the front-matter.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Replaces the body, keeping the front-matter lines.
    pub fn set_body(&mut self, body: &str) {
        self.body = body.to_string();
    }

    /// The document as text.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(self.body.len() + 64 * self.lines.len());
        out.push_str(&self.open);
        for line in &self.lines {
            out.push_str(line);
        }
        out.push_str(&self.close);
        out.push_str(&self.body);
        out
    }
}

/// An indented line, a block-list item or a blank line continue the field above it.
fn is_continuation(line: &str) -> bool {
    line.starts_with([' ', '\t']) || line.starts_with("- ") || line.trim_end_matches(['\n', '\r']) == "-"
}

/// A value as text, as [`FrontMatterDoc::get_text`] gives it.
pub fn value_text(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Array(items)) => {
            items.iter().map(|v| value_text(Some(v))).collect::<Vec<_>>().join(", ")
        }
        Some(serde_json::Value::Object(_)) => value.map(|v| v.to_string()).unwrap_or_default(),
    }
}

/// A value as list items: a list's items, or a non-empty scalar as one item.
pub fn value_items(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Array(items)) => {
            items.iter().map(|v| value_text(Some(v))).filter(|s| !s.is_empty()).collect()
        }
        Some(_) | None => {
            let text = value_text(value);
            if text.is_empty() { Vec::new() } else { vec![text] }
        }
    }
}

/// A scalar in YAML: plain when that reads back as the same string, else double-quoted (a JSON
/// string is a valid YAML double-quoted scalar).
fn render_scalar(text: &str, in_flow: bool) -> String {
    let plain_ok = !text.is_empty()
        && !(in_flow && text.contains([',', '[', ']', '{', '}']))
        && serde_yaml::from_str::<serde_json::Value>(&format!("k: {text}"))
            .ok()
            .and_then(|v| v.get("k").cloned())
            == Some(serde_json::Value::String(text.to_string()));
    if plain_ok {
        text.to_string()
    } else {
        serde_json::Value::String(text.to_string()).to_string()
    }
}

fn render_value(value: &serde_json::Value) -> Result<String, Error> {
    match value {
        serde_json::Value::Null => Ok(String::new()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::String(s) => Ok(render_scalar(s, false)),
        serde_json::Value::Array(items) => {
            let items = items
                .iter()
                .map(|item| match item {
                    serde_json::Value::String(s) => Ok(render_scalar(s, true)),
                    serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {
                        render_value(item)
                    }
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => Err(Error::general_error(
                        "a front-matter list holds scalars only".to_string(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", items.join(", ")))
        }
        serde_json::Value::Object(_) => Err(Error::general_error(
            "a front-matter field set by the memory service is a scalar or a list".to_string(),
        )),
    }
}

/// A new document: the fields in order, then the body after one blank line.
pub fn compose(fields: &IndexMap<String, serde_json::Value>, body: &str) -> Result<String, Error> {
    let mut out = String::from("---\n");
    for (name, value) in fields {
        out.push_str(&format!("{name}: {}\n", render_value(value)?));
    }
    out.push_str("---\n");
    if !body.is_empty() {
        if !body.starts_with('\n') {
            out.push('\n');
        }
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

/// A version as agents carry it: 32 hexadecimal digits.
pub fn parse_version(text: &str) -> Result<Version, Error> {
    let text = text.trim();
    if text.len() != 32 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::parse_error(format!(
            "'{text}' is not a version: a version is 32 hexadecimal digits, as read returns it"
        )));
    }
    u128::from_str_radix(text, 16)
        .map(Version::new)
        .map_err(|e| Error::parse_error(format!("'{text}' is not a version: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\n# kept comment\nid: X\ntitle: \"A: title\"\nstatus: draft\narea:\n  - lib/commands\n  - core\nreviewed: 2026-01-01\n---\n\n# X\n\nBody.\n";

    #[test]
    fn set_field_changes_one_line() -> Result<(), Error> {
        let mut doc = FrontMatterDoc::parse(DOC)?;
        doc.set("status", &serde_json::json!("in_progress"))?;
        let out = doc.render();
        assert_eq!(out, DOC.replace("status: draft", "status: in_progress"));
        assert_eq!(doc.get_text("title"), "A: title");
        Ok(())
    }

    #[test]
    fn set_list_field_writes_flow_list() -> Result<(), Error> {
        let mut doc = FrontMatterDoc::parse(DOC)?;
        doc.set("area", &serde_json::json!(["mcp", "a, b"]))?;
        let out = doc.render();
        assert!(out.contains("area: [mcp, \"a, b\"]\nreviewed: 2026-01-01\n"), "{out}");
        assert!(!out.contains("lib/commands"));
        assert_eq!(doc.get("area"), Some(serde_json::json!(["mcp", "a, b"])));
        Ok(())
    }

    #[test]
    fn set_appends_a_new_field_and_creates_front_matter() -> Result<(), Error> {
        let mut doc = FrontMatterDoc::parse(DOC)?;
        doc.set("updated", &serde_json::json!("2026-10-19"))?;
        assert!(doc.render().contains("reviewed: 2026-01-01\nupdated: 2026-10-19\n---\n"));
        let mut bare = FrontMatterDoc::parse("# Title\n\ntext\n")?;
        bare.set("title", &serde_json::json!("yes"))?;
        assert_eq!(bare.render(), "---\ntitle: yes\n---\n\n# Title\n\ntext\n");
        assert_eq!(bare.get_text("title"), "yes");
        Ok(())
    }

    #[test]
    fn toml_front_matter_is_not_supported() {
        assert!(FrontMatterDoc::parse("+++\na = 1\n+++\n").is_err());
    }

    #[test]
    fn compose_and_versions_round_trip() -> Result<(), Error> {
        let mut fields = IndexMap::new();
        fields.insert("id".to_string(), serde_json::json!("X"));
        fields.insert("area".to_string(), serde_json::json!(["core"]));
        let text = compose(&fields, "# X\n")?;
        assert_eq!(text, "---\nid: X\narea: [core]\n---\n\n# X\n");
        let version = Version::from_content(text.as_bytes());
        assert_eq!(parse_version(&version.to_string())?, version);
        assert!(parse_version("abc").is_err());
        Ok(())
    }
}
