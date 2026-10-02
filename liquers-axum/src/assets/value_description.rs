//! The client-settable part of a value's metadata.
//!
//! A value written through `POST key/data` or `POST key/entry` carries at most five descriptive
//! fields. Everything else in a [`MetadataRecord`] — status, version, log, dependencies, timings —
//! is owned by the asset manager, so the record handed to `set_binary` is built **fresh** from a
//! `ValueDescription`, never cleaned from a client document: a field added to `MetadataRecord`
//! later is not client-settable by accident.
//!
//! Design: `specs/design/axum-assets-endpoints/phase2-architecture.md`, "Data Structures".

use std::collections::HashMap;

use liquers_core::{
    error::{Error, ErrorType},
    metadata::{AssetInfo, MetadataRecord},
    type_system::TypeRegistry,
};
use serde::{Deserialize, Serialize};

/// The client-settable part of a value's metadata. Everything else in `MetadataRecord` is owned
/// by the asset manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueDescription {
    pub type_identifier: Option<String>,
    pub data_format: Option<String>,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

/// The names of the client-settable fields, in declaration order.
pub const VALUE_DESCRIPTION_FIELDS: [&str; 5] =
    ["type_identifier", "data_format", "media_type", "title", "description"];

/// The identifier used when the client names no type: raw bytes.
const DEFAULT_TYPE_IDENTIFIER: &str = "Bytes";

impl ValueDescription {
    fn set_field(&mut self, name: &str, value: String) -> bool {
        let slot = match name {
            "type_identifier" => &mut self.type_identifier,
            "data_format" => &mut self.data_format,
            "media_type" => &mut self.media_type,
            "title" => &mut self.title,
            "description" => &mut self.description,
            _ => return false,
        };
        *slot = Some(value);
        true
    }

    /// From a `DataEntry.metadata` object. Returns the description and the names of the
    /// top-level fields that were not taken, sorted, so the caller can report them.
    ///
    /// A non-object value, or an allow-listed field whose value is not a string, is a
    /// `ParameterError`. A `null` field counts as absent.
    pub fn from_json(value: &serde_json::Value) -> Result<(Self, Vec<String>), Error> {
        let object = value.as_object().ok_or_else(|| {
            Error::from_error(
                ErrorType::ParameterError,
                "Entry metadata must be a JSON object".to_string(),
            )
        })?;
        let mut description = ValueDescription::default();
        let mut dropped = Vec::new();
        for (name, field) in object {
            if !VALUE_DESCRIPTION_FIELDS.contains(&name.as_str()) {
                dropped.push(name.clone());
                continue;
            }
            match field {
                serde_json::Value::Null => {}
                serde_json::Value::String(text) => {
                    description.set_field(name, text.clone());
                }
                serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => {
                    return Err(Error::from_error(
                        ErrorType::ParameterError,
                        format!("Metadata field '{name}' must be a string"),
                    ));
                }
            }
        }
        dropped.sort();
        Ok((description, dropped))
    }

    /// From `POST data` query parameters. Unknown parameter names are returned, sorted, as
    /// ignored.
    pub fn from_params(params: &HashMap<String, String>) -> (Self, Vec<String>) {
        let mut description = ValueDescription::default();
        let mut dropped = Vec::new();
        for (name, value) in params {
            if !description.set_field(name, value.clone()) {
                dropped.push(name.clone());
            }
        }
        dropped.sort();
        (description, dropped)
    }

    /// Fill absent fields from the key's current value.
    ///
    /// - `type_identifier` and `data_format` are filled **as a pair**, only when the client gave
    ///   neither, and only from a previous value that holds data (`status.has_data()`) with a
    ///   non-empty `type_identifier`. A never-evaluated recipe key reports an empty type and the
    ///   recipe's format, and a client that names only a type must not inherit a format that
    ///   type may not support.
    /// - `title` and `description` are filled when absent and the previous one is non-empty.
    /// - `media_type` is never filled: `AssetInfo.media_type` is the *effective* type, and
    ///   copying it would turn a derived type into an override.
    pub fn or_previous(mut self, previous: Option<&AssetInfo>) -> Self {
        let Some(previous) = previous else {
            return self;
        };
        if self.type_identifier.is_none()
            && self.data_format.is_none()
            && previous.status.has_data()
            && !previous.type_identifier.is_empty()
        {
            self.type_identifier = Some(previous.type_identifier.clone());
            self.data_format = previous.data_format.clone();
        }
        if self.title.is_none() && !previous.title.is_empty() {
            self.title = Some(previous.title.clone());
        }
        if self.description.is_none() && !previous.description.is_empty() {
            self.description = Some(previous.description.clone());
        }
        self
    }

    /// Build a fresh record. `type_identifier` defaults to `Bytes`; `type_name` comes from the
    /// registry. An identifier the registry does not contain is a `ParameterError` (a 400 at the
    /// boundary, rather than the 500 the write path's validation would give). The status is not
    /// decided here: `set_binary` decides it from the recipe.
    pub fn into_metadata_record(self, registry: &TypeRegistry) -> Result<MetadataRecord, Error> {
        let type_identifier = self
            .type_identifier
            .unwrap_or_else(|| DEFAULT_TYPE_IDENTIFIER.to_string());
        let type_info = registry.get(&type_identifier).ok_or_else(|| {
            Error::from_error(
                ErrorType::ParameterError,
                format!("Unknown type identifier '{type_identifier}'"),
            )
        })?;
        Ok(MetadataRecord {
            type_name: type_info.type_name.to_string(),
            type_identifier,
            data_format: self.data_format,
            media_type: self.media_type,
            title: self.title.unwrap_or_default(),
            description: self.description.unwrap_or_default(),
            ..MetadataRecord::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::{
        error::ErrorType,
        metadata::{AssetInfo, Status},
        type_system::TypeRegistry,
        value::Value,
    };
    use serde_json::json;
    use std::collections::HashMap;

    /// `TypeRegistry::from_value_type` is the verified public constructor
    /// (`liquers-core/src/type_system.rs`).
    fn test_registry() -> TypeRegistry {
        TypeRegistry::from_value_type::<Value>()
    }

    #[test]
    fn vd01_from_json_keeps_five_fields() {
        let value = json!({
            "type_identifier": "Text", "data_format": "txt", "media_type": "text/plain",
            "title": "My Note", "description": "A description"
        });
        let (desc, dropped) = ValueDescription::from_json(&value).expect("from_json");
        assert_eq!(desc.type_identifier, Some("Text".to_string()));
        assert_eq!(desc.data_format, Some("txt".to_string()));
        assert_eq!(desc.media_type, Some("text/plain".to_string()));
        assert_eq!(desc.title, Some("My Note".to_string()));
        assert_eq!(desc.description, Some("A description".to_string()));
        assert!(dropped.is_empty(), "no allow-listed field should be dropped");
    }

    #[test]
    fn vd02_from_json_non_object_error() {
        let err = ValueDescription::from_json(&json!("not an object")).expect_err("non-object");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }

    #[test]
    fn vd03_from_json_non_string_field_error() {
        let value = json!({"type_identifier": "Text", "title": 123});
        let err = ValueDescription::from_json(&value).expect_err("non-string field");
        assert_eq!(err.error_type, ErrorType::ParameterError);
    }

    #[test]
    fn vd04_from_json_dropped_fields_sorted() {
        let value = json!({
            "type_identifier": "Text", "extra_field_z": "ignored", "extra_field_a": "ignored", "title": "Title"
        });
        let (_, dropped) = ValueDescription::from_json(&value).expect("from_json");
        assert_eq!(dropped, vec!["extra_field_a".to_string(), "extra_field_z".to_string()], "sorted alphabetically");
    }

    #[test]
    fn vd05_from_params_drops_unknown_parameters() {
        let mut params = HashMap::new();
        params.insert("type_identifier".to_string(), "Bytes".to_string());
        params.insert("data_format".to_string(), "bin".to_string());
        params.insert("title".to_string(), "Binary Data".to_string());
        params.insert("unknown_param".to_string(), "ignored".to_string());

        let (desc, dropped) = ValueDescription::from_params(&params);
        assert_eq!(desc.type_identifier, Some("Bytes".to_string()));
        assert_eq!(desc.data_format, Some("bin".to_string()));
        assert_eq!(desc.title, Some("Binary Data".to_string()));
        assert_eq!(desc.description, None);
        assert_eq!(dropped, vec!["unknown_param".to_string()]);
    }

    #[test]
    fn vd06_or_previous_fills_from_asset_info_never_media_type() {
        // AssetInfo.title/description are String, not Option<String>.
        let previous = AssetInfo {
            status: Status::Source,
            title: "Previous Title".to_string(),
            description: "Previous Desc".to_string(),
            type_identifier: "Text".to_string(),
            data_format: Some("txt".to_string()),
            media_type: "text/plain".to_string(),
            ..AssetInfo::new()
        };
        let desc = ValueDescription {
            type_identifier: None, data_format: None, media_type: None,
            title: Some("New Title".to_string()), description: None,
        };
        let merged = desc.or_previous(Some(&previous));
        assert_eq!(merged.type_identifier, Some("Text".to_string()));
        assert_eq!(merged.data_format, Some("txt".to_string()));
        assert_eq!(merged.title, Some("New Title".to_string()), "explicit field is kept");
        assert_eq!(merged.description, Some("Previous Desc".to_string()), "missing field is filled");
        assert_eq!(merged.media_type, None, "media_type is never filled from previous");
    }

    #[test]
    fn vd07_into_metadata_record_default_type_identifier_is_bytes() {
        let desc = ValueDescription {
            type_identifier: None, data_format: Some("txt".to_string()), media_type: None,
            title: Some("Note".to_string()), description: None,
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.type_identifier, "Bytes", "None defaults to Bytes");
    }

    #[test]
    fn vd08_into_metadata_record_type_name_from_registry() {
        let desc = ValueDescription {
            type_identifier: Some("Text".to_string()), data_format: None, media_type: None,
            title: None, description: None,
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.type_name, "text", "type_name resolved from the registry");
    }

    #[test]
    fn vd09_into_metadata_record_unknown_identifier_is_parameter_error() {
        let desc = ValueDescription {
            type_identifier: Some("UnknownType".to_string()), data_format: None, media_type: None,
            title: None, description: None,
        };
        let err = desc.into_metadata_record(&test_registry()).expect_err("unknown type");
        assert_eq!(err.error_type, ErrorType::ParameterError, "unknown type_identifier is a 400 at the boundary");
    }

    #[test]
    fn vd10_into_metadata_record_untouched_fields_carry_real_defaults() {
        // MetadataRecord::default().status == Status::None (Status::default() = Self::None);
        // `stored` is Option<bool>, default None. into_metadata_record does not decide status —
        // that happens later, in set_binary, from recipe_opt.
        let desc = ValueDescription {
            type_identifier: Some("Bytes".to_string()), data_format: Some("bin".to_string()),
            media_type: Some("application/octet-stream".to_string()),
            title: Some("Data".to_string()), description: Some("Some binary".to_string()),
        };
        let record = desc.into_metadata_record(&test_registry()).expect("into_metadata_record");
        assert_eq!(record.status, Status::None, "into_metadata_record does not decide status");
        assert_eq!(record.stored, None, "stored: Option<bool>; None means true by convention");
        assert!(record.dependencies.is_empty());
    }

    #[test]
    fn vd11_or_previous_fills_type_and_format_only_as_a_pair_from_data() {
        // A never-evaluated recipe key reports type_identifier: "" and the recipe's data_format;
        // neither is inherited (a plain POST data onto a recipe key would otherwise get a 400 or
        // 422). Title is still inherited. A client-named type never inherits a format.
        let recipe_info = AssetInfo {
            status: Status::Recipe, title: "Recipe Title".to_string(),
            type_identifier: String::new(), data_format: Some("txt".to_string()),
            ..AssetInfo::new()
        };
        let merged = ValueDescription::default().or_previous(Some(&recipe_info));
        assert_eq!(merged.type_identifier, None, "empty type_identifier from Recipe is not inherited");
        assert_eq!(merged.data_format, None, "format is not inherited from Recipe");
        assert_eq!(merged.title, Some("Recipe Title".to_string()), "title is still inherited");

        let ready_text = AssetInfo {
            status: Status::Ready, type_identifier: "Text".to_string(), data_format: Some("txt".to_string()),
            ..AssetInfo::new()
        };
        let named = ValueDescription { type_identifier: Some("Bytes".to_string()), ..ValueDescription::default() };
        let merged = named.or_previous(Some(&ready_text));
        assert_eq!(merged.type_identifier, Some("Bytes".to_string()), "client-named type kept");
        assert_eq!(merged.data_format, None, "a client-named type does not inherit a format");
    }
}
