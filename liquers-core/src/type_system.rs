//! The Liquers type system: what a value *is*, independent of how its bytes are encoded.
//!
//! Liquers describes a value on two independent axes:
//!
//! - the **type axis** — [`TypeInfo::type_identifier`], the unique identity of a value variant.
//!   It is the serialization dispatch key and the thing other languages and other realms see.
//! - the **encoding axis** — `data_format` inward (which codec runs) and `media_type` outward
//!   (what the world is told the bytes are), both carried by
//!   [`crate::metadata::MetadataRecord`] rather than here.
//!
//! The governing rule for the boundary between Rust and everything else:
//!
//! > **Rust types in Rust code and in command registration. Type identifiers in the registries**,
//! > which exist to integrate with other languages and other realms.
//!
//! [`to_type_identifier`] is the only crossing, and it goes one way. The reverse — identifier to
//! Rust type — is never needed inside Rust, because Rust code always has the type already.
//! Deserialization is not a counterexample: bytes plus an identifier produce a `Value`, which is
//! dispatch *within* the value type, not resolution of a Rust type.
//!
//! See `specs/design/value-type-system/` and `specs/reference/VALUE_TYPE_SYSTEM.md`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::error::Error;
use crate::value::ValueInterface;

/// The realm a type belongs to when none is named.
///
/// Mirrors `command_metadata`'s treatment of realms: the default realm is stored as the empty
/// string so that single-realm code never has to mention one.
pub const DEFAULT_TYPE_REALM: &str = "";

// There is deliberately **no** error type identifier. An error is a property of the *metadata* —
// `is_error`, `Status::Error`, `error_data` — and not of the value: an errored state holds
// `V::none()`, so its type identifier is the none type's, like any other state holding none.
// The type axis says what a value *is*, and "failed" is not something a value can be.
//
// See `specs/design/foreign-value-type-registration/phase5-documentation.md`.

/// Registry key: a type identifier within a realm.
///
/// Realms exist because a query will eventually span more than one — a `wasm` frontend and a
/// native backend do not support the same types. Keying for that now costs one field; adding a key
/// component after entries exist would rewrite all of them. See
/// `specs/issues/TYPE-REGISTRY-NOT-REALM-AWARE.md` for the behaviour that is *not* yet here.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeKey {
    pub realm: String,
    pub type_identifier: String,
}

impl TypeKey {
    /// Builds a key, normalizing the default realm to the empty string.
    pub fn new(realm: &str, type_identifier: &str) -> Self {
        TypeKey {
            realm: if realm == DEFAULT_TYPE_REALM {
                String::new()
            } else {
                realm.to_owned()
            },
            type_identifier: type_identifier.to_owned(),
        }
    }

    /// A key in the default realm.
    pub fn of(type_identifier: &str) -> Self {
        TypeKey::new(DEFAULT_TYPE_REALM, type_identifier)
    }
}

/// Everything the system knows about one type.
///
/// Construct with [`TypeInfo::new`] and the `with_*` methods rather than a struct literal: a
/// builder is what lets a later field — the per-realm unsupported-type action, for instance — be
/// added without breaking every construction site, including generated ones.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TypeInfo {
    /// Unique, cross-platform identity of the value variant. The serialization dispatch key.
    ///
    /// Naming: `provider.LocalName` — one dot, lowercase provider naming the system the type
    /// belongs to (`polars`, `py`, `js`), CamelCase local name. A **bare** name asserts that
    /// Liquers owns the concept and is reserved for `liquers-core` and `liquers-lib`. Every other
    /// non-alphanumeric character is reserved.
    pub type_identifier: Cow<'static, str>,

    /// Detailed, runtime-oriented name. Informational; never a dispatch key.
    pub type_name: Cow<'static, str>,

    /// Realm. Empty for the default realm.
    pub realm: Cow<'static, str>,

    /// Level-1 seeding defaults, mutually consistent by construction.
    pub default_data_format: Cow<'static, str>,
    pub default_extension: Cow<'static, str>,
    pub default_media_type: Cow<'static, str>,
    pub default_filename: Cow<'static, str>,

    /// Data formats this type can be written to and read from.
    ///
    /// A `data_format` outside this set is what the write path refuses — the check that closes
    /// `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`.
    pub supported_data_formats: Vec<Cow<'static, str>>,
}

impl TypeInfo {
    /// A minimally-populated description. The defaults are placeholders until `with_defaults`
    /// is called; `new` alone is only useful for a type with no serialized form.
    pub fn new(type_identifier: impl Into<Cow<'static, str>>) -> Self {
        let type_identifier = type_identifier.into();
        TypeInfo {
            type_name: type_identifier.clone(),
            type_identifier,
            realm: Cow::Borrowed(DEFAULT_TYPE_REALM),
            default_data_format: Cow::Borrowed("bin"),
            default_extension: Cow::Borrowed("bin"),
            default_media_type: Cow::Borrowed("application/octet-stream"),
            default_filename: Cow::Borrowed("data.bin"),
            supported_data_formats: Vec::new(),
        }
    }

    pub fn with_type_name(mut self, type_name: impl Into<Cow<'static, str>>) -> Self {
        self.type_name = type_name.into();
        self
    }

    pub fn with_realm(mut self, realm: impl Into<Cow<'static, str>>) -> Self {
        self.realm = realm.into();
        self
    }

    /// Sets the level-1 defaults together, because they must agree with each other.
    pub fn with_defaults(
        mut self,
        data_format: impl Into<Cow<'static, str>>,
        extension: impl Into<Cow<'static, str>>,
        media_type: impl Into<Cow<'static, str>>,
        filename: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.default_data_format = data_format.into();
        self.default_extension = extension.into();
        self.default_media_type = media_type.into();
        self.default_filename = filename.into();
        self
    }

    /// Appends a supported data format.
    pub fn with_data_format(mut self, data_format: impl Into<Cow<'static, str>>) -> Self {
        self.supported_data_formats.push(data_format.into());
        self
    }

    /// Appends several supported data formats.
    pub fn with_data_formats<I, S>(mut self, formats: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Cow<'static, str>>,
    {
        self.supported_data_formats
            .extend(formats.into_iter().map(Into::into));
        self
    }

    /// The description of a Rust type that names its own identifier within value type `V`.
    pub fn of<V, T>() -> Self
    where
        T: TypeIdentifiedIn<V>,
    {
        T::type_info()
    }

    /// This type's registry key.
    pub fn key(&self) -> TypeKey {
        TypeKey::new(&self.realm, &self.type_identifier)
    }

    /// Whether this type can be written to, and read from, `data_format`.
    ///
    /// A refinement matches its base: a type supporting `csv` supports `csv:comma`.
    pub fn supports_data_format(&self, data_format: &str) -> bool {
        let base = base_format(data_format);
        self.supported_data_formats
            .iter()
            .any(|supported| base_format(supported) == base)
    }
}

/// Strips a data-format refinement: `csv:comma` -> `csv`.
///
/// A refinement narrows a format without changing which parser reads it.
fn base_format(data_format: &str) -> &str {
    match data_format.split_once(':') {
        Some((base, _refinement)) => base,
        None => data_format,
    }
}

/// A Rust type that carries a Liquers type identifier, within the value type `V`.
///
/// **`V` is not decoration.** A bare `TypeIdentified` would have to be implemented in
/// `liquers-lib` for `polars::frame::DataFrame` — a foreign trait for a foreign type, which the
/// orphan rule rejects (E0117), and the same applies to `image::DynamicImage`,
/// `chrono::NaiveDate` and every other type that matters. Parameterising by the value type puts a
/// **local** type into the impl head, which RFC 2451 permits:
///
/// ```ignore
/// impl TypeIdentifiedIn<ExtValue> for polars::frame::DataFrame { .. }  // ExtValue is local
/// ```
///
/// A welcome consequence: the mapping is relative to a value type, so two crates that each define
/// their own value type may both name `polars::frame::DataFrame` without a coherence conflict —
/// which matches the registry being a property of the build rather than of the universe.
pub trait TypeIdentifiedIn<V> {
    /// The type identifier. Resolved at compile time; never looked up.
    const TYPE_IDENTIFIER: &'static str;

    /// The full description, for registration.
    fn type_info() -> TypeInfo;
}

impl<V, T: TypeIdentifiedIn<V>> TypeIdentifiedIn<V> for std::sync::Arc<T> {
    const TYPE_IDENTIFIER: &'static str = T::TYPE_IDENTIFIER;
    fn type_info() -> TypeInfo {
        T::type_info()
    }
}

impl<V, T: TypeIdentifiedIn<V>> TypeIdentifiedIn<V> for &T {
    const TYPE_IDENTIFIER: &'static str = T::TYPE_IDENTIFIER;
    fn type_info() -> TypeInfo {
        T::type_info()
    }
}

/// The bridge from a Rust type to its Liquers type identifier.
///
/// Resolved by the compiler at the call site; it performs no lookup and cannot fail. A type with
/// no [`TypeIdentifiedIn`] impl is a compile error where it is used, not a runtime miss.
pub const fn to_type_identifier<V, T: TypeIdentifiedIn<V>>() -> &'static str {
    T::TYPE_IDENTIFIER
}

/// What a build knows about types, keyed by realm and identifier.
///
/// Built once — from the value type's own descriptions, plus any registrations an integration
/// crate adds — and read-only thereafter, which is why it is a `BTreeMap` and not a concurrent
/// map: no lock is needed, and deterministic iteration keeps any listing stable.
#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    types: BTreeMap<TypeKey, TypeInfo>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        TypeRegistry {
            types: BTreeMap::new(),
        }
    }

    /// Seeds a registry from a value type's own static self-description.
    ///
    /// Infallible, because an `Environment` constructor is: a duplicate here means a value type
    /// described the same identifier twice, which is a bug in that type rather than a runtime
    /// condition a caller can act on. The first description wins and the collision is reported on
    /// stderr; `register` remains fallible for callers that can handle it, and
    /// `type_descriptions_match_identifier` catches the mistake in tests.
    pub fn from_value_type<V: ValueInterface>() -> Self {
        let mut registry = TypeRegistry::new();
        let mut add = |info: TypeInfo| {
            if let Err(error) = registry.register(info) {
                eprintln!("liquers: type registry construction: {error}");
            }
        };
        for info in V::type_descriptions() {
            add(info);
        }
        registry
    }

    /// Adds one description.
    ///
    /// A duplicate key is an error rather than an overwrite: two crates claiming `Image` must
    /// fail, not resolve by load order.
    pub fn register(&mut self, info: TypeInfo) -> Result<(), Error> {
        let key = info.key();
        if let Some(existing) = self.types.get(&key) {
            return Err(Error::general_error(format!(
                "Type identifier '{}' is already registered in realm '{}' (as '{}'); \
                 two types cannot claim the same identifier",
                key.type_identifier, key.realm, existing.type_name
            )));
        }
        self.types.insert(key, info);
        Ok(())
    }

    /// Looks a type up in the default realm.
    pub fn get(&self, type_identifier: &str) -> Option<&TypeInfo> {
        self.types.get(&TypeKey::of(type_identifier))
    }

    /// Looks a type up in a named realm.
    pub fn get_in_realm(&self, realm: &str, type_identifier: &str) -> Option<&TypeInfo> {
        self.types.get(&TypeKey::new(realm, type_identifier))
    }

    pub fn contains(&self, type_identifier: &str) -> bool {
        self.get(type_identifier).is_some()
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&TypeKey, &TypeInfo)> {
        self.types.iter()
    }

    /// Whether `data_format` is usable for `type_identifier` in the default realm.
    ///
    /// An unknown type supports nothing — the caller must distinguish "unknown type" from
    /// "unsupported format" with [`TypeRegistry::contains`] when the difference matters.
    pub fn supports_data_format(&self, type_identifier: &str, data_format: &str) -> bool {
        self.get(type_identifier)
            .is_some_and(|info| info.supports_data_format(data_format))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_info() -> TypeInfo {
        TypeInfo::new("Text")
            .with_type_name("text")
            .with_defaults("txt", "txt", "text/plain", "text.txt")
            .with_data_formats(["txt", "json"])
    }

    /// `vts4.2` — a duplicate identifier is refused, never silently overwritten.
    #[test]
    fn duplicate_registration_is_an_error() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = TypeRegistry::new();
        registry.register(text_info())?;
        let err = registry
            .register(text_info())
            .expect_err("a duplicate identifier must be refused");
        assert!(
            err.message.contains("Text"),
            "message names the identifier: {err}"
        );
        Ok(())
    }

    /// `fvt1.1` — a base registry can be extended with a type the value type cannot describe.
    ///
    /// This is the registration mechanism in full: `from_value_type` seeds what the build knows,
    /// `register` adds what only an integration knows, and the result goes to an environment
    /// constructor. Nothing writes to the registry after that.
    #[test]
    fn a_base_registry_can_be_extended() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = TypeRegistry::from_value_type::<crate::value::Value>();
        let before = registry.len();

        registry.register(
            TypeInfo::new("js.Value")
                .with_type_name("JsValue")
                .with_defaults("json", "json", "application/json", "value.json"),
        )?;

        assert!(registry.contains("js.Value"), "the added type is present");
        assert!(
            registry.contains("Text") && registry.contains("None"),
            "and the base types survived, which an empty registry would have lost"
        );
        assert_eq!(registry.len(), before + 1);
        Ok(())
    }

    /// `fvt1.2` — registering the same identifier twice is refused, and the message names it.
    ///
    /// Two integrations claiming one identifier must fail loudly at environment construction
    /// rather than resolve by load order.
    #[test]
    fn a_duplicate_foreign_registration_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = TypeRegistry::from_value_type::<crate::value::Value>();
        let info = TypeInfo::new("js.Value").with_type_name("JsValue");
        registry.register(info.clone())?;

        let error = registry
            .register(info)
            .expect_err("a second registration of the same identifier must be refused");
        assert!(
            error.message.contains("js.Value"),
            "the message names the identifier: {error}"
        );
        Ok(())
    }

    /// `vts4.3` — format support follows the declared list, and a refinement matches its base.
    #[test]
    fn supports_data_format_matches_the_list() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = TypeRegistry::new();
        registry.register(text_info())?;

        assert!(registry.supports_data_format("Text", "txt"));
        assert!(registry.supports_data_format("Text", "json"));
        assert!(
            registry.supports_data_format("Text", "txt:utf8"),
            "a refinement is supported wherever its base is"
        );
        assert!(!registry.supports_data_format("Text", "parquet"));
        assert!(
            !registry.supports_data_format("NoSuchType", "txt"),
            "an unknown type supports nothing"
        );
        Ok(())
    }

    /// `vts4.4` — realms isolate lookups, and the default realm is unaffected by them.
    #[test]
    fn realm_keying_isolates_lookups() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = TypeRegistry::new();
        registry.register(text_info())?;
        registry.register(TypeInfo::new("polars.DataFrame").with_realm("backend"))?;

        assert!(registry.contains("Text"));
        assert!(
            !registry.contains("polars.DataFrame"),
            "a type registered in another realm is not in the default one"
        );
        assert!(registry
            .get_in_realm("backend", "polars.DataFrame")
            .is_some());
        assert!(registry
            .get_in_realm("frontend", "polars.DataFrame")
            .is_none());

        // The same identifier may exist in two realms.
        registry.register(TypeInfo::new("polars.DataFrame"))?;
        assert!(registry.contains("polars.DataFrame"));
        Ok(())
    }

    /// `vts4.5` — the builder produces defaults that agree with each other.
    #[test]
    fn builder_produces_consistent_defaults() {
        let info = text_info();
        assert_eq!(info.default_data_format, "txt");
        assert!(info
            .default_filename
            .ends_with(&format!(".{}", info.default_extension)));
        assert!(
            info.supports_data_format(&info.default_data_format),
            "a type must support its own default format"
        );
    }

    /// There is **no** error type. An errored state holds `V::none()`, so it is typed `None` like
    /// any other state holding none; "failed" lives in the metadata, not on the type axis.
    #[test]
    fn there_is_no_error_type() -> Result<(), Box<dyn std::error::Error>> {
        let registry = TypeRegistry::from_value_type::<crate::value::Value>();
        assert!(
            !registry.contains("error"),
            "error is a metadata property, not a value type"
        );
        assert!(
            registry.contains("None"),
            "and the type an errored state actually reports is registered like any other"
        );
        Ok(())
    }

    /// `vts4.6` — every registered identifier follows the naming rule.
    ///
    /// Bare names are reserved for types whose concept Liquers owns; everything else carries a
    /// lowercase provider and exactly one dot. No other non-alphanumeric character is allowed —
    /// they are reserved for future structure such as generics.
    #[test]
    fn identifier_naming_rule_holds() -> Result<(), Box<dyn std::error::Error>> {
        let registry = TypeRegistry::from_value_type::<crate::value::Value>();
        for (key, _) in registry.iter() {
            let id = &key.type_identifier;
            assert!(!id.is_empty(), "an identifier must not be empty");
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'),
                "identifier {id:?} may contain only alphanumerics and a single dot"
            );
            assert!(
                id.matches('.').count() <= 1,
                "identifier {id:?} may carry at most one dot"
            );
            if let Some((provider, local)) = id.split_once('.') {
                assert!(
                    !provider.is_empty()
                        && provider
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                    "provider of {id:?} must be lowercase"
                );
                assert!(!local.is_empty(), "local name of {id:?} must not be empty");
            }
        }
        Ok(())
    }
}
