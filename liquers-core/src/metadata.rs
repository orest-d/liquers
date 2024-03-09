#![allow(unused_imports)]
#![allow(dead_code)]

use serde_json::{self, Value};

use crate::error::Error;
use crate::parse;
use crate::query::{Key, Position, Query};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Status {
    None,
    Submitted,
    EvaluatingParent,
    Evaluation,
    EvaluatingDependencies,
    Error,
    Recipe,
    Ready,
    Expired,
    External,
    SideEffect,
}

impl Default for Status {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LogEntryKind {
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    pub kind: LogEntryKind,
    pub message: String,
    #[serde(default)]
    pub message_html: Option<String>,
    pub timestamp: String,
    #[serde(with = "option_query_format", default)]
    pub query: Option<Query>,
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub traceback: Option<String>,
}

impl LogEntry {
    pub fn new(kind: LogEntryKind, message: String) -> LogEntry {
        LogEntry {
            kind,
            message,
            ..Self::default()
        }
    }
    pub fn info(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Info, message)
    }
    pub fn debug(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Debug, message)
    }
    pub fn warning(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Warning, message)
    }
    pub fn error(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Error, message)
    }
    pub fn with_query(&mut self, query: Query) -> &mut Self {
        self.query = Some(query);
        self
    }
    pub fn with_position(&mut self, position: Position) -> &mut Self {
        self.position = position;
        self
    }
    pub fn with_traceback(&mut self, traceback: String) -> &mut Self {
        self.traceback = Some(traceback);
        self
    }
    pub fn with_message_html(&mut self, message_html: String) -> &mut Self {
        self.message_html = Some(message_html);
        self
    }
    pub fn with_custom_timestamp(&mut self, timestamp: String) -> &mut Self {
        self.timestamp = timestamp;
        self
    }
    pub fn with_timestamp(&mut self) -> &mut Self {
        self.timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self
    }
}

impl Default for LogEntry {
    fn default() -> Self {
        LogEntry {
            kind: LogEntryKind::Info,
            message: "".to_string(),
            message_html: None,
            timestamp: "".to_string(),
            query: None,
            position: Position::default(),
            traceback: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MetadataRecord {
    pub log: Vec<LogEntry>,
    #[serde(with = "query_format")]
    pub query: Query,
    #[serde(with = "option_key_format")]
    pub key: Option<Key>,
    pub status: Status,
    pub type_identifier: String,
    pub message: String,
    pub is_error: bool,
    pub media_type: String,
    pub filename: Option<String>,
}

mod query_format {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(query: &Query, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&query.encode())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Query, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse::parse_query(&s).map_err(de::Error::custom)
    }
}

mod key_format {
    use crate::query::Key;

    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &Key, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&key.encode())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Key, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse::parse_key(&s).map_err(de::Error::custom)
    }
}

mod option_query_format {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(query: &Option<Query>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match query {
            Some(q) => serializer.serialize_str(&q.encode()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Query>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer);
        if s.is_err() {
            return Ok(None);
        } else {
            let s = s.unwrap();
            if s.is_empty() {
                return Ok(Some(Query::new()));
            } else {
                return parse::parse_query(&s).map_err(de::Error::custom).map(Some);
            }
        }
    }
}

mod option_key_format {
    use crate::query::Key;

    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &Option<Key>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match key {
            Some(k) => serializer.serialize_str(&k.encode()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Key>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer);
        if s.is_err() {
            return Ok(None);
        } else {
            let s = s.unwrap();
            if s.is_empty() {
                return Ok(Some(Key::new()));
            } else {
                return parse::parse_key(&s).map_err(de::Error::custom).map(Some);
            }
        }
    }
}

impl MetadataRecord {
    /// Create a new empty MetadataRecord with default values
    pub fn new() -> MetadataRecord {
        MetadataRecord {
            is_error: false,
            ..Self::default()
        }
    }
    /// Set the query of the MetadataRecord
    pub fn with_query(&mut self, query: Query) -> &mut Self {
        self.query = query;
        self
    }
    /*
    pub fn from_query(query: &str) -> Result<Self, Error> {
        let mut metadata = self::MetadataRecord::new();
        metadata.query = query.to_string();
        Ok(metadata)
    }
    */
    pub fn with_key(&mut self, key: Key) -> &mut Self {
        self.key = Some(key);
        self
    }
    pub fn with_status(&mut self, status: Status) -> &mut Self {
        self.status = status;
        self.is_error = status == Status::Error;
        self
    }
    pub fn with_type_identifier(&mut self, type_identifier: String) -> &mut Self {
        self.type_identifier = type_identifier;
        self
    }
    pub fn with_message(&mut self, message: String) -> &mut Self {
        self.message = message;
        self
    }
    pub fn with_media_type(&mut self, media_type: String) -> &mut Self {
        self.media_type = media_type;
        self
    }
    pub fn add_log_entry(&mut self, log_entry: LogEntry) -> &mut Self {
        self.log.push(log_entry);
        self
    }
    pub fn with_filename(&mut self, filename: String) -> &mut Self {
        self.filename = Some(filename);
        //TODO: set media_type from filename extension
        self
    }
    pub fn clean_log(&mut self) -> &mut Self {
        self.log = vec![];
        self
    }
    pub fn info(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::info(message.to_owned()));
        self
    }
    pub fn debug(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::debug(message.to_owned()));
        self
    }
    pub fn warning(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::warning(message.to_owned()));
        self
    }
    pub fn error(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::error(message.to_owned()));
        self
    }
    pub fn type_identifier(&self) -> String {
        self.type_identifier.to_string()
    }
    pub fn filename(&self) -> Option<String> {
        self.filename.clone()
    }
    pub fn set_filename(&mut self, filename: &str) {
        self.filename = Some(filename.to_string());
    }
    pub fn extension(&self) -> Option<String> {
        if let Some(filename) = &self.filename {
            let parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                return Some(parts.last().unwrap().to_string());
            }
        }
        None
    }
    pub fn set_extension(&mut self, extension: &str) {
        if let Some(filename) = &mut self.filename {
            let mut parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                parts.pop();
                parts.push(extension);
                *filename = parts.join(".");
            } else {
                filename.push_str(".");
                filename.push_str(extension);
            }
        }
        else{
            self.filename = Some(format!("file.{}",extension));
        }
    }
}

#[derive(Debug, Clone)]
pub enum Metadata {
    LegacyMetadata(serde_json::Value),
    MetadataRecord(MetadataRecord),
}

impl Metadata {
    pub fn new() -> Metadata {
        Metadata::MetadataRecord(MetadataRecord::new())
    }

    pub fn with_query(&mut self, query: Query) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("query".to_string(), Value::String(query.encode()));
                self
            }
            Metadata::MetadataRecord(m) => {
                m.with_query(query);
                self
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.query = query;
                *self = Metadata::MetadataRecord(m);
                self
            }

            _ => {
                panic!("Cannot set query on unsupported legacy metadata")
            }
        }
    }

    pub fn from_json(json: &str) -> serde_json::Result<Metadata> {
        match serde_json::from_str::<MetadataRecord>(json) {
            Ok(m) => Ok(Metadata::MetadataRecord(m)),
            Err(_) => match serde_json::from_str::<serde_json::Value>(json) {
                Ok(v) => Ok(Metadata::LegacyMetadata(v)),
                Err(e) => Err(e),
            },
        }
    }

    pub fn from_json_value(json: serde_json::Value) -> serde_json::Result<Metadata> {
        match serde_json::from_value::<MetadataRecord>(json.clone()) {
            Ok(m) => Ok(Metadata::MetadataRecord(m)),
            Err(_) => match serde_json::from_value::<serde_json::Value>(json) {
                Ok(v) => Ok(Metadata::LegacyMetadata(v)),
                Err(e) => Err(e),
            },
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        match self {
            Metadata::LegacyMetadata(v) => serde_json::to_string(v),
            Metadata::MetadataRecord(m) => serde_json::to_string(m),
        }
    }

    pub fn get_media_type(&self) -> String {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(mimetype) = o.get("mimetype") {
                    return mimetype.to_string();
                }
                if let Some(media_type) = o.get("media_type") {
                    return media_type.to_string();
                }
                return "application/octet-stream".to_string();
            }
            Metadata::MetadataRecord(m) => m.media_type.to_string(),
            _ => "application/octet-stream".to_string(),
        }
    }

    pub fn query(&self) -> Result<Query, crate::error::Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(query)) = o.get("query") {
                    return parse::parse_query(query);
                }
                return Err(Error::general_error(
                    "Query not found in legacy metadata".to_string(),
                ));
            }
            Metadata::MetadataRecord(m) => Ok(m.query.to_owned()),
            _ => Err(Error::general_error(
                "Query not found in unsupported legacy metadata".to_string(),
            )),
        }
    }
    pub fn with_type_identifier(&mut self, type_identifier: String) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert(
                    "type_identifier".to_string(),
                    Value::String(type_identifier),
                );
                self
            }
            Metadata::MetadataRecord(m) => {
                m.with_type_identifier(type_identifier);
                self
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.type_identifier = type_identifier;
                *self = Metadata::MetadataRecord(m);
                self
            }

            _ => {
                panic!("Cannot set type_identifier on unsupported legacy metadata")
            }
        }
    }
    pub fn type_identifier(&self) -> Result<String, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(type_identifier)) = o.get("type_identifier") {
                    Ok(type_identifier.to_string())
                } else {
                    let mut error = Error::general_error(
                        "type_identifier not found in legacy metadata".to_string(),
                    );
                    if let Ok(query) = self.query() {
                        Err(error.with_query(&query))
                    } else {
                        Err(error)
                    }
                }
            }
            Metadata::MetadataRecord(m) => Ok(m.type_identifier()),
            _ => {
                let mut error = Error::general_error(
                    "type_identifier is not defined in non-object legacy metadata".to_string(),
                );
                if let Ok(query) = self.query() {
                    Err(error.with_query(&query))
                } else {
                    Err(error)
                }
            }
        }
    }
    pub fn filename(&self) -> Option<String> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(filename)) = o.get("filename") {
                    Some(filename.to_string())
                } else {
                    self.query()
                        .unwrap_or(Query::new())
                        .filename()
                        .map(|f| f.encode().to_string())
                }
            }
            Metadata::MetadataRecord(m) => m.filename(),
            _ => None,
        }
    }
    pub fn extension(&self) -> Option<String> {
        if let Some(filename) = self.filename() {
            let parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                return Some(parts.last().unwrap().to_string());
            }
        }
        None
    }
    
    pub fn set_extension(&mut self, extension: &str)->Result<&mut Self, Error> {
        match self {
            Metadata::LegacyMetadata(_) => {
                let mut error = Error::general_error(
                    "Cannot set extension on legacy metadata".to_string(),
                );
                if let Ok(query) = self.query() {
                    Err(error.with_query(&query))
                } else {
                    Err(error)
                }        
            }
            Metadata::MetadataRecord(m) => {
                m.set_extension(extension);
                Ok(self)
            }
        }   
    }
}

impl From<MetadataRecord> for Metadata {
    fn from(m: MetadataRecord) -> Self {
        Metadata::MetadataRecord(m)
    }
}
