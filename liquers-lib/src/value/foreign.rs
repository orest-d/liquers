//! Opaque values belonging to an *integrated language* runtime.
//!
//! A language integration — JavaScript in `liquers-web`, Starlark, Python — retains a handle to
//! one of its own values inside a Liquers value by implementing [`ForeignValue`] and storing it in
//! [`crate::value::ExtValue::Foreign`]. There is deliberately **one** variant for all languages
//! rather than one per language:
//!
//! - `liquers-lib` contains no language-specific code, and every `match` arm on the variant is a
//!   one-line delegation to this trait;
//! - adding a language costs no new variant and no new match arms, so an integration never has to
//!   edit this crate;
//! - languages are separated at *downcast* time via [`ForeignValue::as_any`], with
//!   [`ForeignValue::origin`] naming the runtime a value came from so a failed downcast produces a
//!   diagnostic rather than a bare conversion error.
//!
//! The thread bounds are the target-conditional `MaybeSend`/`MaybeSync` markers. On native they
//! resolve to `Send + Sync`, which `Py<PyAny>` and a frozen Starlark value satisfy; on `wasm32`
//! they are vacuous, which is what allows a `JsValue` — never `Send` on any target — to be stored.
//! Supertrait transitivity carries those bounds to the trait object, so `Arc<dyn ForeignValue>` is
//! `Send + Sync` on native and the variant needs no target gate.
//!
//! See `specs/design/liquers-web/phase2-architecture.md`, "Where the foreign value lives".

use liquers_core::error::{Error, ErrorType};
use std::borrow::Cow;

/// A value owned by an *integrated language* runtime, retained opaquely by Liquers.
///
/// Implementors live in the integration crate, not here.
pub trait ForeignValue:
    core::fmt::Debug
    + liquers_core::maybe_send::MaybeSend
    + liquers_core::maybe_send::MaybeSync
    + 'static
{
    /// Which language runtime produced this value — `"javascript"`, `"starlark"`, `"python"`.
    ///
    /// Carried so that a failed downcast can report where the value actually came from instead of
    /// reporting only that a conversion failed.
    fn origin(&self) -> &'static str;

    /// Object-safe downcast hook. An integration recovers its own concrete type with
    /// `value.as_any().downcast_ref::<MyOpaque>()`; `None` means the value belongs to a different
    /// language runtime.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Stable type identifier, in the sense of `ValueInterface::identifier`.
    fn identifier(&self) -> Cow<'static, str>;

    /// Human-readable type name, typically the language's own name for the value's type.
    fn type_name(&self) -> Cow<'static, str>;

    /// Default file extension used when the value is written out.
    fn default_extension(&self) -> Cow<'static, str>;

    /// Default filename used when the value is written out.
    fn default_filename(&self) -> Cow<'static, str>;

    /// Default media type used when the value is served.
    fn default_media_type(&self) -> Cow<'static, str>;

    /// This value's type description — what the registry holds and what the write path checks.
    ///
    /// The default derives everything from the methods above and declares **no** data formats,
    /// which is right for a handle with no byte form: the write path exempts a formatless type
    /// from the format check exactly as it exempts a UI element. An implementation whose
    /// [`ForeignValue::as_bytes`] does produce bytes overrides this and adds
    /// `.with_data_formats([…])`, or the registry and the value would disagree about what the
    /// type can be written as.
    ///
    /// **This is the instance side of a fact with two spellings.** The registry is built before
    /// any value exists, so an integration also needs the same description *without* an
    /// instance — a free function in the integration crate. Both must read one identifier
    /// constant, and a unit test must assert they agree; the type system cannot enforce it,
    /// because a default body is type-checked with `Self: ?Sized` and so cannot call an
    /// associated function. See `specs/design/foreign-value-type-registration/`.
    fn type_info(&self) -> liquers_core::type_system::TypeInfo {
        liquers_core::type_system::TypeInfo::new(self.identifier())
            .with_type_name(self.type_name())
            .with_defaults(
                // `default_data_format` derives from the extension, as `ValueInterface` does.
                self.default_extension(),
                self.default_extension(),
                self.default_media_type(),
                self.default_filename(),
            )
    }

    /// Text conversion, if the language can provide a faithful one.
    ///
    /// The default refuses: a coercion such as JavaScript's `String(obj)` is lossy and usually not
    /// what a caller means by "the text of this value".
    fn try_into_string(&self) -> Result<String, Error> {
        Err(Error::conversion_error(
            self.identifier().as_ref(),
            "string",
        ))
    }

    /// JSON conversion, if the language can provide a faithful one. Refuses by default.
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        Err(Error::conversion_error(self.identifier().as_ref(), "JSON"))
    }

    /// Byte serialization. Bytes plus a media type are the only sanctioned path across a store,
    /// a process or another language runtime.
    ///
    /// Refusing is a legitimate and expected implementation: the asset layer already tolerates it,
    /// falling back to a time-based version and to metadata-only persistence, so an unserializable
    /// foreign value degrades instead of failing evaluation.
    ///
    /// The refusal is a [`ErrorType::SerializationError`], not a `ConversionError`: this is the
    /// byte-serialization boundary, and the design assigns those two error types to different
    /// boundaries deliberately — a *structural* conversion refusal (`try_into_string`,
    /// `try_into_json_value`) is a `ConversionError`, while failing to produce bytes is a
    /// serialization failure. Keeping them distinct is what lets a caller tell "this value has no
    /// text form" apart from "this value cannot be persisted".
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        Err(Error::from_error(
            ErrorType::SerializationError,
            format!(
                "Serialization to {} not supported by {}",
                format,
                self.type_name()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    const MOCK_TYPE_IDENTIFIER: &str = "mock.Value";

    /// A foreign value with no byte form — the ordinary case, and what `JsOpaque` is.
    #[derive(Debug)]
    struct Opaque;

    impl ForeignValue for Opaque {
        fn origin(&self) -> &'static str {
            "mock"
        }
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
        fn identifier(&self) -> Cow<'static, str> {
            MOCK_TYPE_IDENTIFIER.into()
        }
        fn type_name(&self) -> Cow<'static, str> {
            "MockObject".into()
        }
        fn default_extension(&self) -> Cow<'static, str> {
            "json".into()
        }
        fn default_filename(&self) -> Cow<'static, str> {
            "value.json".into()
        }
        fn default_media_type(&self) -> Cow<'static, str> {
            "application/json".into()
        }
    }

    /// A foreign value that *can* serialize, and therefore overrides `type_info`.
    #[derive(Debug)]
    struct Serializable;

    impl ForeignValue for Serializable {
        fn origin(&self) -> &'static str {
            "mock"
        }
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
        fn identifier(&self) -> Cow<'static, str> {
            "mock.Serializable".into()
        }
        fn type_name(&self) -> Cow<'static, str> {
            "MockSerializable".into()
        }
        fn default_extension(&self) -> Cow<'static, str> {
            "json".into()
        }
        fn default_filename(&self) -> Cow<'static, str> {
            "value.json".into()
        }
        fn default_media_type(&self) -> Cow<'static, str> {
            "application/json".into()
        }
        fn type_info(&self) -> liquers_core::type_system::TypeInfo {
            liquers_core::type_system::TypeInfo::new(self.identifier())
                .with_type_name(self.type_name())
                .with_defaults("json", "json", "application/json", "value.json")
                .with_data_formats(["json"])
        }
        fn as_bytes(&self, _format: &str) -> Result<Vec<u8>, Error> {
            Ok(b"{}".to_vec())
        }
    }

    /// `fvt3.1` — the default derives from the value's own methods and declares no formats.
    #[test]
    fn the_default_type_info_derives_from_the_value() {
        let info = Opaque.type_info();

        assert_eq!(info.type_identifier, MOCK_TYPE_IDENTIFIER);
        assert_eq!(info.type_name, "MockObject");
        assert_eq!(info.default_data_format, "json");
        assert_eq!(info.default_extension, "json");
        assert_eq!(info.default_media_type, "application/json");
        assert_eq!(info.default_filename, "value.json");
        assert!(
            info.supported_data_formats.is_empty(),
            "a handle with no byte form declares no formats, which is what exempts it from the \
             write path's format check"
        );
        assert!(
            !info.supports_data_format("json"),
            "and it therefore supports nothing, even its own default"
        );
    }

    /// `fvt3.2` — an implementation that serializes can declare what `as_bytes` accepts.
    #[test]
    fn an_implementation_that_serializes_can_declare_formats() {
        let info = Serializable.type_info();

        assert!(info.supports_data_format("json"));
        assert!(!info.supports_data_format("parquet"));
    }

    /// The identifier reaches the description unchanged — the property the registry depends on.
    #[test]
    fn the_identifier_and_the_description_agree() {
        assert_eq!(Opaque.type_info().type_identifier, Opaque.identifier());
        assert_eq!(
            Serializable.type_info().type_identifier,
            Serializable.identifier()
        );
    }
}
