//! The per-document rules a collection's schema expresses, shared by the write path (a write may
//! not add a violation) and by `check`.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::query::Key;

use crate::search::md_split::{blank_front_matter, sections};

use super::front_matter::{value_items, value_text, FrontMatterDoc};
use super::layout::{load_yaml, CollectionSchema, FieldEquals, FieldType};

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One finding about one document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
}

impl Violation {
    pub fn error(rule: &str, message: String) -> Self {
        Violation { rule: rule.to_string(), severity: Severity::Error, message }
    }
    pub fn warning(rule: &str, message: String) -> Self {
        Violation { rule: rule.to_string(), severity: Severity::Warning, message }
    }
}

/// The vocabularies and transition tables a collection's fields name, loaded once per call.
#[derive(Clone, Debug, Default)]
pub struct Vocabularies {
    lists: BTreeMap<String, Vec<String>>,
    transitions: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

impl Vocabularies {
    pub async fn load<E: Environment>(
        envref: &EnvRef<E>,
        root: &Key,
        collection: &CollectionSchema,
    ) -> Result<Self, Error> {
        let mut out = Vocabularies::default();
        for rule in collection.fields.values() {
            if let Some(rel) = &rule.enum_from {
                if !out.lists.contains_key(rel) {
                    let list: Vec<String> = load_yaml(envref, root, rel).await?;
                    out.lists.insert(rel.clone(), list);
                }
            }
            if let Some(rel) = &rule.transitions {
                if !out.transitions.contains_key(rel) {
                    let map: BTreeMap<String, Vec<String>> = load_yaml(envref, root, rel).await?;
                    out.transitions.insert(rel.clone(), map);
                }
            }
        }
        Ok(out)
    }

    /// The values a field allows; empty means any.
    pub fn allowed(&self, collection: &CollectionSchema, field: &str) -> Vec<String> {
        let Some(rule) = collection.fields.get(field) else { return Vec::new() };
        let mut values = rule.values.clone();
        if let Some(list) = rule.enum_from.as_ref().and_then(|rel| self.lists.get(rel)) {
            values.extend(list.iter().cloned());
        }
        values
    }

    /// Refuses a change of `field` from `from` to `to` that its transition table does not list.
    /// A field without a table, or a first value, may change freely.
    pub fn check_transition(
        &self,
        collection: &CollectionSchema,
        field: &str,
        from: &str,
        to: &str,
    ) -> Result<(), Error> {
        let Some(rule) = collection.fields.get(field) else { return Ok(()) };
        let Some(table) = rule.transitions.as_ref().and_then(|rel| self.transitions.get(rel)) else {
            return Ok(());
        };
        if from.is_empty() || from == to {
            return Ok(());
        }
        let allowed = table.get(from).cloned().unwrap_or_default();
        if allowed.iter().any(|v| v == to) {
            Ok(())
        } else {
            Err(Error::general_error(format!(
                "{field}: the transition {from} → {to} is not allowed; from {from} the allowed \
                 values are: {}",
                if allowed.is_empty() { "none".to_string() } else { allowed.join(", ") }
            )))
        }
    }
}

/// Whether `text` is a `YYYY-MM-DD` date.
pub fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The longest title the rules accept without a warning.
pub const MAX_TITLE: usize = 120;

/// The schema rules one document breaks: front-matter, required and typed fields, vocabularies,
/// `id = filename stem`, required sections, and a long title.
pub fn document_violations(
    collection: &CollectionSchema,
    vocabularies: &Vocabularies,
    key: &Key,
    text: &str,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let doc = match FrontMatterDoc::parse(text) {
        Ok(doc) => doc,
        Err(e) => {
            out.push(Violation::error("front_matter", e.message.clone()));
            return out;
        }
    };
    let fields: IndexMap<String, serde_json::Value> = match doc.fields() {
        Ok(fields) => fields,
        Err(e) => {
            out.push(Violation::warning(
                "front_matter",
                format!("{}; read as flat `key: value` lines", e.message),
            ));
            doc.fields_lenient().0
        }
    };
    if !doc.has_front_matter() && !collection.fields.is_empty() {
        out.push(Violation::error("front_matter", "no front-matter".to_string()));
        return out;
    }
    for (name, rule) in &collection.fields {
        let value = fields.get(name);
        let items = value_items(value);
        if rule.required && items.is_empty() {
            out.push(Violation::error("required", format!("required field '{name}' is missing")));
            continue;
        }
        let allowed = vocabularies.allowed(collection, name);
        if !allowed.is_empty() {
            for item in &items {
                if !allowed.iter().any(|a| a == item) {
                    out.push(Violation::error(
                        "enum",
                        format!("{name}: '{item}' is not one of: {}", allowed.join(", ")),
                    ));
                }
            }
        }
        if !rule.list && matches!(value, Some(serde_json::Value::Array(_))) {
            out.push(Violation::error("list", format!("{name} takes one value, not a list")));
        }
        match rule.field_type {
            FieldType::Text | FieldType::Key => {}
            FieldType::Date => {
                for item in &items {
                    if !is_date(item) {
                        out.push(Violation::error(
                            "date",
                            format!("{name}: '{item}' is not a YYYY-MM-DD date"),
                        ));
                    }
                }
            }
        }
        match rule.equals {
            Some(FieldEquals::FilenameStem) => {
                let stem = collection.id_of(key);
                let text = value_text(value);
                if !text.is_empty() && text != stem {
                    out.push(Violation::error(
                        "id",
                        format!("{name} '{text}' does not match filename '{stem}'"),
                    ));
                }
            }
            None => {}
        }
    }
    let title = fields.get("title").map(|v| value_text(Some(v))).unwrap_or_default();
    if title.chars().count() > MAX_TITLE {
        out.push(Violation::warning(
            "title_length",
            format!("title is {} characters, over {MAX_TITLE}", title.chars().count()),
        ));
    }
    if !collection.sections.is_empty() {
        let present: Vec<String> = sections(&blank_front_matter(text))
            .into_iter()
            .filter_map(|(heading, _, _, _)| heading)
            .map(|h| h.rsplit(" > ").next().unwrap_or(&h).to_lowercase())
            .collect();
        for section in &collection.sections {
            if !present.iter().any(|p| *p == section.to_lowercase()) {
                out.push(Violation::error(
                    "sections",
                    format!("required section '{section}' is missing"),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::layout::MemoryLayout;
    use liquers_core::parse::parse_key;

    fn issues() -> Result<CollectionSchema, Error> {
        let layout = MemoryLayout::parse(
            "collections:\n  - name: issues\n    folder: issues\n    sections: [Problem]\n    fields:\n      id: { required: true, equals: filename_stem }\n      title: { required: true }\n      status: { enum: [draft, closed] }\n      date: { type: date }\n",
        )?;
        Ok(layout.collection("issues")?.clone())
    }

    #[test]
    fn rules_name_each_violation() -> Result<(), Error> {
        let collection = issues()?;
        let key = parse_key("issues/X.md")?;
        let good = "---\nid: X\ntitle: T\nstatus: draft\n---\n## Problem\n";
        assert!(document_violations(&collection, &Vocabularies::default(), &key, good).is_empty());
        let bad = "---\nid: Y\nstatus: open\ndate: soon\n---\nno sections\n";
        let mut rules: Vec<String> = document_violations(&collection, &Vocabularies::default(), &key, bad)
            .into_iter()
            .map(|v| v.rule)
            .collect();
        rules.sort();
        assert_eq!(rules, vec!["date", "enum", "id", "required", "sections"]);
        Ok(())
    }

    #[test]
    fn transitions_refuse_unlisted_changes() -> Result<(), Error> {
        let mut collection = issues()?;
        if let Some(rule) = collection.fields.get_mut("status") {
            rule.transitions = Some("t.yaml".to_string());
        }
        let mut vocabularies = Vocabularies::default();
        let mut table = BTreeMap::new();
        table.insert("draft".to_string(), vec!["closed".to_string()]);
        vocabularies.transitions.insert("t.yaml".to_string(), table);
        vocabularies.check_transition(&collection, "status", "draft", "closed")?;
        vocabularies.check_transition(&collection, "status", "", "closed")?;
        let error = vocabularies.check_transition(&collection, "status", "closed", "draft").err();
        assert!(error.is_some_and(|e| e.message.contains("closed → draft")));
        Ok(())
    }
}
