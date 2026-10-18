//! Batch intake for `registerCommands` / `validateCommands` (`ENVIRON08`).
//!
//! A batch is a declaration object, an array of them, or a JSON or YAML string in the
//! `CommandMetadataRegistry` format (`specs/command_registry.yaml`). A string is *imported command
//! metadata*: nothing validates it until the batch is checked as a registry, which is the case the
//! guide's `ENVIRON08` is about. Functions are bound by name from `impls`, an object mapping a
//! command name to a function or to `{ run, state?, async? }`.
//!
//! Intake never stops at the first problem. Every refusal becomes an entry in one `IssueReport`, so
//! a page can show the whole list; `environment::validate_batch` then adds the registry check.

use std::collections::HashSet;

use liquers_core::command_metadata::{CommandKey, CommandMetadata};
use liquers_core::issue_report::{Issue, IssueReport, IssueSeverity};
use wasm_bindgen::prelude::*;

use super::spec::JsCommandSpec;

/// What a rebuild replays to reconstruct an accepted command.
///
/// Holding the input rather than the parsed spec keeps replay identical to first registration.
#[derive(Clone)]
pub(crate) enum Retained {
    /// A declaration object, snapshotted so later mutation by the page cannot change it.
    Declaration(JsValue),
    /// Imported metadata and the implementation it was bound to.
    Metadata(CommandMetadata, JsValue),
}

impl Retained {
    /// Reconstructs the spec. Replay of something that was accepted once.
    pub(crate) fn parse(&self) -> Result<JsCommandSpec, liquers_core::error::Error> {
        match self {
            Retained::Declaration(spec) => JsCommandSpec::parse(spec),
            Retained::Metadata(metadata, implementation) => {
                JsCommandSpec::from_metadata(metadata.clone(), implementation).map_err(|issue| {
                    liquers_core::error::Error::from_error(
                        liquers_core::error::ErrorType::ParameterError,
                        issue.message().to_string(),
                    )
                })
            }
        }
    }

    /// The command name, for `unregisterCommand`.
    pub(crate) fn name(&self) -> Option<String> {
        match self {
            Retained::Declaration(spec) => js_sys::Reflect::get(spec, &JsValue::from_str("name"))
                .ok()
                .and_then(|v| v.as_string()),
            Retained::Metadata(metadata, _) => Some(metadata.name.clone()),
        }
    }
}

/// An accepted command of a batch: what to register now, and what to retain for replay.
pub(crate) struct Accepted {
    pub spec: JsCommandSpec,
    pub retained: Retained,
}

/// Reads a batch. Returns the commands that passed intake and every finding about the rest.
pub(crate) fn read_batch(input: &JsValue, impls: &JsValue) -> (Vec<Accepted>, IssueReport) {
    let mut report = IssueReport::default();
    let accepted = if let Some(text) = input.as_string() {
        read_registry(&text, impls, &mut report)
    } else if js_sys::Array::is_array(input) {
        js_sys::Array::from(input)
            .iter()
            .filter_map(|declaration| read_declaration(&declaration, impls, &mut report))
            .collect()
    } else {
        read_declaration(input, impls, &mut report)
            .into_iter()
            .collect()
    };
    let accepted = refuse_duplicates(accepted, &mut report);
    (accepted, report)
}

/// One declaration object. A declaration without `run` takes `impls[name]`, when there is one.
fn read_declaration(
    declaration: &JsValue,
    impls: &JsValue,
    report: &mut IssueReport,
) -> Option<Accepted> {
    let snapshot = with_implementation(snapshot_declaration(declaration), impls);
    let (spec, findings) = JsCommandSpec::check(&snapshot);
    report.append(findings);
    spec.map(|spec| Accepted {
        spec,
        retained: Retained::Declaration(snapshot),
    })
}

/// The parts of a serialized `CommandMetadataRegistry` an import reads.
///
/// Its own type because `CommandMetadataRegistry` requires `default_namespaces`, which an exported
/// registry always has and a hand-written one, holding only `commands`, need not. Unknown fields
/// (`default_namespaces` among them) are ignored, so an exported registry reads as it is.
#[derive(serde::Deserialize)]
struct ImportedRegistry {
    #[serde(default)]
    commands: Vec<CommandMetadata>,
    #[serde(default)]
    global_enums: std::collections::HashMap<String, serde_json::Value>,
}

/// A serialized registry. Each command binds `impls[name]`; a command with none is an error, and
/// an implementation naming no command is a warning.
fn read_registry(text: &str, impls: &JsValue, report: &mut IssueReport) -> Vec<Accepted> {
    let registry: ImportedRegistry =
        match liquers_core::validate::registry::from_json_or_yaml("the command metadata", text) {
            Ok(registry) => registry,
            Err(e) => {
                report.extend([Issue::generic(IssueSeverity::Error, e.message.clone())]);
                return Vec::new();
            }
        };
    if !registry.global_enums.is_empty() {
        report.extend([Issue::generic(
            IssueSeverity::Warning,
            "The command metadata defines global enums, which are not imported; arguments \
             referring to them will not resolve",
        )]);
    }

    let mut accepted = Vec::new();
    let mut named = HashSet::new();
    for metadata in registry.commands {
        named.insert(metadata.name.clone());
        let implementation = property(impls, &metadata.name);
        if implementation.is_undefined() || implementation.is_null() {
            report.extend([Issue::command_registry(
                IssueSeverity::Error,
                &metadata.realm,
                &metadata.namespace,
                &metadata.name,
                format!("No implementation is given for command {:?}", metadata.name),
            )]);
            continue;
        }
        match JsCommandSpec::from_metadata(metadata.clone(), &implementation) {
            Ok(spec) => accepted.push(Accepted {
                spec,
                retained: Retained::Metadata(metadata, implementation),
            }),
            Err(issue) => report.extend([issue]),
        }
    }

    if impls.is_object() {
        for name in js_sys::Object::keys(&js_sys::Object::from(impls.clone())).iter() {
            let Some(name) = name.as_string() else {
                continue;
            };
            if !named.contains(&name) {
                report.extend([Issue::generic(
                    IssueSeverity::Warning,
                    format!("An implementation is given for {name:?}, which names no command"),
                )]);
            }
        }
    }
    accepted
}

/// Two commands of one batch under the same key would silently replace each other. Every later
/// one is refused.
fn refuse_duplicates(accepted: Vec<Accepted>, report: &mut IssueReport) -> Vec<Accepted> {
    let mut seen: HashSet<CommandKey> = HashSet::new();
    let mut kept = Vec::new();
    for command in accepted {
        if seen.insert(command.spec.key.clone()) {
            kept.push(command);
        } else {
            let key = &command.spec.key;
            report.extend([Issue::command_registry(
                IssueSeverity::Error,
                &key.realm,
                &key.namespace,
                &key.name,
                format!("Command {:?} appears more than once in the batch", key.name),
            )]);
        }
    }
    kept
}

fn property(object: &JsValue, name: &str) -> JsValue {
    if !object.is_object() {
        return JsValue::UNDEFINED;
    }
    js_sys::Reflect::get(object, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
}

/// Sets `run` from `impls[name]` on a declaration that has none. The snapshot is the batch's own
/// copy, so writing to it does not touch the page's object.
fn with_implementation(declaration: JsValue, impls: &JsValue) -> JsValue {
    if !declaration.is_object() || property(&declaration, "run").is_function() {
        return declaration;
    }
    let Some(name) = property(&declaration, "name").as_string() else {
        return declaration;
    };
    let implementation = property(impls, &name);
    if implementation.is_function() {
        let _ = js_sys::Reflect::set(&declaration, &JsValue::from_str("run"), &implementation);
    }
    declaration
}

/// Copies a declaration so that later mutation by the caller cannot change what a rebuild replays.
///
/// `JsValue::clone` clones the *handle*, not the object. Retaining the caller's own object means a
/// page that reuses or mutates a declaration template — plausible in a framework that builds
/// specs from a loop — silently changes an already-registered command the next time a rebuild
/// replays it, or breaks an unrelated registration if a field such as `run` was removed.
///
/// A shallow copy plus a copy of the `arguments` array and its entries covers the mutation a
/// caller can realistically perform. The `run` function is shared by reference, deliberately:
/// copying a closure is not possible and not wanted — it *is* the command. A value that is not an
/// object is returned as it is, for intake to refuse.
pub(crate) fn snapshot_declaration(spec: &JsValue) -> JsValue {
    if !spec.is_object() || js_sys::Array::is_array(spec) {
        return spec.clone();
    }
    let copy = js_sys::Object::new();
    if js_sys::Object::assign(&copy, &js_sys::Object::from(spec.clone())).is_falsy() {
        // `Object.assign` returns its target; a falsy result would mean something is very wrong.
        return spec.clone();
    }

    // The `arguments` array and its entries are the part a caller is most likely to reuse.
    if let Ok(args) = js_sys::Reflect::get(&copy, &JsValue::from_str("arguments")) {
        if js_sys::Array::is_array(&args) {
            let copied = js_sys::Array::new();
            for entry in js_sys::Array::from(&args).iter() {
                if entry.is_object() && !js_sys::Array::is_array(&entry) {
                    let e = js_sys::Object::new();
                    let _ = js_sys::Object::assign(&e, &js_sys::Object::from(entry));
                    copied.push(&e.into());
                } else {
                    copied.push(&entry);
                }
            }
            let _ = js_sys::Reflect::set(&copy, &JsValue::from_str("arguments"), &copied);
        }
    }
    copy.into()
}
