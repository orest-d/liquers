//! Parsing the structure of a document: JSON, YAML, or Markdown front-matter (YAML between `---`
//! lines, TOML between `+++` lines). Used by `field_extract`, which takes the format from the
//! file extension, so nothing is guessed. Everything converts to `serde_json::Value`, so one
//! navigation path serves all three.

use serde::{Deserialize, Serialize};

/// How a document's structure is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructFormat {
    /// The whole text; the result must be a map.
    Json,
    /// The whole text; the result must be a map.
    Yaml,
    /// The leading front-matter block of a Markdown document; it must parse to a map.
    FrontMatter,
}

/// The parsed structure, or `None` when the text has none in that format.
pub fn parse_structure(text: &str, format: StructFormat) -> Option<serde_json::Value> {
    parse_structure_with_report(text, format).0
}

/// As [`parse_structure`], also returning how many YAML map keys were skipped because they are
/// not scalars (a sequence or a map used as a key has no field name).
pub fn parse_structure_with_report(
    text: &str,
    format: StructFormat,
) -> (Option<serde_json::Value>, usize) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut skipped = 0;
    let parsed = match format {
        StructFormat::Json => serde_json::from_str::<serde_json::Value>(text.trim()).ok(),
        StructFormat::Yaml => serde_yaml::from_str::<serde_yaml::Value>(text)
            .ok()
            .map(|value| yaml_to_json(value, &mut skipped)),
        StructFormat::FrontMatter => match front_matter(text) {
            Some((FrontMatterKind::Yaml, block)) => serde_yaml::from_str::<serde_yaml::Value>(block)
                .ok()
                .map(|value| yaml_to_json(value, &mut skipped)),
            Some((FrontMatterKind::Toml, block)) => {
                toml::from_str::<toml::Value>(block).ok().map(toml_to_json)
            }
            None => None,
        },
    };
    (parsed.filter(serde_json::Value::is_object), skipped)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrontMatterKind {
    Yaml,
    Toml,
}

/// The front-matter block (without its delimiter lines) and its kind.
fn front_matter(text: &str) -> Option<(FrontMatterKind, &str)> {
    let (kind, block_start, block_end, _) = front_matter_bounds(text)?;
    Some((kind, &text[block_start..block_end]))
}

/// The byte offset just after a leading front-matter block, or 0 when there is none — where the
/// body of a Markdown document starts.
pub fn front_matter_end(text: &str) -> usize {
    front_matter_bounds(text).map(|(_, _, _, end)| end).unwrap_or(0)
}

/// `(kind, block start, block end, end of the closing delimiter line)`.
fn front_matter_bounds(text: &str) -> Option<(FrontMatterKind, usize, usize, usize)> {
    let bom = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let mut lines = LineIter { text, pos: bom };
    let (first, _, after_first) = lines.next()?;
    let (kind, closers): (FrontMatterKind, &[&str]) = match first.trim_end() {
        "---" => (FrontMatterKind::Yaml, &["---", "..."]),
        "+++" => (FrontMatterKind::Toml, &["+++"]),
        _ => return None,
    };
    for (line, start, end) in lines {
        if closers.contains(&line.trim_end()) {
            return Some((kind, after_first, start, end));
        }
    }
    None
}

/// Lines with their byte range: `(line without the newline, start, end after the newline)`.
struct LineIter<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Iterator for LineIter<'a> {
    type Item = (&'a str, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.text.len() {
            return None;
        }
        let start = self.pos;
        let rest = &self.text[start..];
        let (line, advance) = match rest.find('\n') {
            Some(i) => (&rest[..i], i + 1),
            None => (rest, rest.len()),
        };
        self.pos = start + advance;
        Some((line, start, self.pos))
    }
}

fn yaml_to_json(value: serde_yaml::Value, skipped: &mut usize) -> serde_json::Value {
    match value {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::from(i)
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s),
        serde_yaml::Value::Sequence(items) => serde_json::Value::Array(
            items.into_iter().map(|item| yaml_to_json(item, skipped)).collect(),
        ),
        serde_yaml::Value::Mapping(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                match yaml_key(&key) {
                    Some(name) => {
                        out.insert(name, yaml_to_json(value, skipped));
                    }
                    None => *skipped += 1,
                }
            }
            serde_json::Value::Object(out)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(tagged.value, skipped),
    }
}

/// A scalar map key as a field name; `None` for a key that is a sequence, map or tagged value.
fn yaml_key(key: &serde_yaml::Value) -> Option<String> {
    match key {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Null => Some("null".to_string()),
        serde_yaml::Value::Sequence(_)
        | serde_yaml::Value::Mapping(_)
        | serde_yaml::Value::Tagged(_) => None,
    }
}

/// TOML to JSON with an explicit match: `serde_json::to_value` would turn a datetime into a
/// private wrapper object rather than its ISO-8601 text.
fn toml_to_json(value: toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::Value::from(i),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(toml_to_json).collect())
        }
        toml::Value::Table(table) => serde_json::Value::Object(
            table.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_yaml_and_toml() {
        let yaml = "---\nid: X\nstatus: draft\narea: [core/assets]\n---\n# Body\n";
        let value = parse_structure(yaml, StructFormat::FrontMatter).expect("yaml front-matter");
        assert_eq!(value["status"], "draft");
        assert_eq!(value["area"][0], "core/assets");
        assert_eq!(&yaml[front_matter_end(yaml)..], "# Body\n");

        let dotted = "---\nid: Y\n...\nbody";
        assert_eq!(parse_structure(dotted, StructFormat::FrontMatter).expect("...")["id"], "Y");

        let toml = "+++\nstatus = \"accepted\"\n[links]\ngithub = 12\n+++\nbody";
        let value = parse_structure(toml, StructFormat::FrontMatter).expect("toml front-matter");
        assert_eq!(value["status"], "accepted");
        assert_eq!(value["links"]["github"], 12);
    }

    #[test]
    fn toml_datetime_becomes_iso_string() {
        let toml = "+++\ncreated = 2026-10-01T00:00:00Z\n+++\n";
        let value = parse_structure(toml, StructFormat::FrontMatter).expect("toml");
        assert_eq!(value["created"], "2026-10-01T00:00:00Z");
    }

    #[test]
    fn yaml_non_string_keys() {
        let (value, skipped) =
            parse_structure_with_report("1: one\ntrue: yes\n[a, b]: list\n", StructFormat::Yaml);
        let value = value.expect("yaml map");
        assert_eq!(value["1"], "one");
        assert_eq!(value["true"], "yes");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn no_front_matter_is_none() {
        assert!(parse_structure("# Title\n\nNote: a colon", StructFormat::FrontMatter).is_none());
        assert!(parse_structure("---\nunclosed: true\n", StructFormat::FrontMatter).is_none());
        assert_eq!(front_matter_end("# Title"), 0);
        // A whole-text YAML scalar or list is not a structure with fields.
        assert!(parse_structure("just text", StructFormat::Yaml).is_none());
        assert!(parse_structure("[1, 2]", StructFormat::Json).is_none());
        assert_eq!(parse_structure("{\"a\": 1}", StructFormat::Json).expect("json")["a"], 1);
    }
}
