use serde_json;

use liquers_core::{
    command_metadata::CommandMetadata,
    metadata::AssetInfo,
    value::{DefaultValueSerializer, ValueInterface},
};

use liquers_core::error::Error;
use std::{borrow::Cow, convert::TryFrom, result::Result};

/// Extension payload carried by [`CombinedValue::Extended`].
///
/// The thread bounds are the target-conditional [`MaybeSend`]/[`MaybeSync`] markers rather
/// than hard `Send`/`Sync`, matching `liquers_core::value::ValueInterface`. On native they
/// still resolve to `Send + Sync`, so nothing changes there; on `wasm32` they are vacuous,
/// which is what allows an extension to hold a non-`Send` foreign-language handle such as
/// a `JsValue`. See `specs/design/liquers-web/phase1-high-level-design.md` decision 1.
pub trait ValueExtension:
    core::fmt::Debug
    + Clone
    + Sized
    + DefaultValueSerializer
    + liquers_core::maybe_send::MaybeSend
    + liquers_core::maybe_send::MaybeSync
    + 'static
{
    fn try_into_string(&self) -> Result<String, Error> {
        Err(Error::conversion_error(self.identifier(), "string"))
    }

    /// Try to get an `i32` out.
    ///
    /// Default-refuses, exactly as `CombinedValue`'s `ValueInterface::try_into_i32` refused every
    /// extended value before this hook existed — see
    /// `specs/issues/EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS.md`. An extension overrides
    /// this to let its values bind to an `i32` command argument.
    fn try_into_i32(&self) -> Result<i32, Error> {
        Err(Error::conversion_error(self.identifier(), "i32"))
    }

    /// Try to get an `i64` out. See [`ValueExtension::try_into_i32`].
    fn try_into_i64(&self) -> Result<i64, Error> {
        Err(Error::conversion_error(self.identifier(), "i64"))
    }

    /// Try to get an `i64`, allowing an extension to report "no value" instead of erroring.
    ///
    /// Mirrors [`liquers_core::value::ValueInterface::try_into_i64_option`]'s shape; the default
    /// simply wraps [`ValueExtension::try_into_i64`] in `Some`, since an extended value carries no
    /// generic "none" notion of its own (that lives on the base side of `CombinedValue`).
    fn try_into_i64_option(&self) -> Result<Option<i64>, Error> {
        self.try_into_i64().map(Some)
    }

    /// Try to get an `f64` out. See [`ValueExtension::try_into_i32`].
    fn try_into_f64(&self) -> Result<f64, Error> {
        Err(Error::conversion_error(self.identifier(), "f64"))
    }

    /// Try to get an `f64`, allowing an extension to report "no value" instead of erroring.
    /// See [`ValueExtension::try_into_i64_option`].
    fn try_into_f64_option(&self) -> Result<Option<f64>, Error> {
        self.try_into_f64().map(Some)
    }

    /// Try to get a `bool` out. See [`ValueExtension::try_into_i32`].
    fn try_into_bool(&self) -> Result<bool, Error> {
        Err(Error::conversion_error(self.identifier(), "bool"))
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        Err(Error::conversion_error(self.identifier(), "JSON"))
    }
    /// Static self-description of every type this extension can hold.
    ///
    /// Concatenated with the base value's descriptions by `CombinedValue`, so the registry sees
    /// one flat identifier space regardless of which side a variant physically lives on.
    fn type_descriptions() -> Vec<liquers_core::type_system::TypeInfo> {
        Vec::new()
    }
    /// The description of *this* value's type.
    ///
    /// Mirrors [`liquers_core::value::ValueInterface::type_info`]: find this value's identifier
    /// among the extension's own descriptions, and otherwise build a description from the value's
    /// defaults. An extension holding a value whose description is not static — a foreign
    /// language handle — overrides this and delegates to the value itself.
    fn type_info(&self) -> liquers_core::type_system::TypeInfo {
        let identifier = self.identifier();
        Self::type_descriptions()
            .into_iter()
            .find(|info| info.type_identifier == identifier)
            .unwrap_or_else(|| {
                liquers_core::type_system::TypeInfo::new(identifier)
                    .with_type_name(self.type_name())
                    .with_defaults(
                        self.default_extension(),
                        self.default_extension(),
                        self.default_media_type(),
                        self.default_filename(),
                    )
            })
    }

    fn identifier(&self) -> Cow<'static, str>;
    fn type_name(&self) -> Cow<'static, str>;
    fn default_extension(&self) -> Cow<'static, str>;
    fn default_filename(&self) -> Cow<'static, str>;
    fn default_media_type(&self) -> Cow<'static, str>;
}

#[derive(Debug, Clone)]
pub enum CombinedValue<BaseValue: ValueInterface + Default, Ext: ValueExtension> {
    Base(BaseValue),
    Extended(Ext),
}

impl<BaseValue: ValueInterface + Default, Ext: ValueExtension> CombinedValue<BaseValue, Ext> {
    pub fn new_base(value: BaseValue) -> Self {
        CombinedValue::Base(value)
    }

    pub fn new_extended(value: Ext) -> Self {
        CombinedValue::Extended(value)
    }

    pub fn is_extended(&self) -> bool {
        matches!(self, CombinedValue::Extended(_))
    }

    pub fn as_extended(&self) -> Option<&Ext> {
        match self {
            CombinedValue::Extended(ext) => Some(ext),
            _ => None,
        }
    }

    pub fn is_base(&self) -> bool {
        matches!(self, CombinedValue::Base(_))
    }

    pub fn as_base(&self) -> Option<&BaseValue> {
        match self {
            CombinedValue::Base(base) => Some(base),
            _ => None,
        }
    }
}

impl<BaseValue: ValueInterface + Default, Ext: ValueExtension> Default
    for CombinedValue<BaseValue, Ext>
{
    fn default() -> Self {
        CombinedValue::Base(BaseValue::default())
    }
}

impl<BaseValue: ValueInterface + Default, Ext: ValueExtension> ValueInterface
    for CombinedValue<BaseValue, Ext>
{
    fn try_into_query(&self) -> Result<liquers_core::query::Query, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_query(),
            CombinedValue::Extended(_ext) => {
                Err(Error::conversion_error("extended value", "Query"))
            }
        }
    }
    fn none() -> Self {
        CombinedValue::Base(BaseValue::none())
    }

    fn is_none(&self) -> bool {
        if let CombinedValue::Base(base) = self {
            base.is_none()
        } else {
            false
        }
    }

    fn new(txt: &str) -> Self {
        CombinedValue::Base(BaseValue::new(txt))
    }

    fn try_into_string(&self) -> Result<String, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_string(),
            CombinedValue::Extended(ext) => ext.try_into_string(),
        }
    }

    fn try_into_i32(&self) -> Result<i32, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_i32(),
            CombinedValue::Extended(ext) => ext.try_into_i32(),
        }
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_json_value(),
            CombinedValue::Extended(ext) => ext.try_into_json_value(),
        }
    }

    fn type_descriptions() -> Vec<liquers_core::type_system::TypeInfo> {
        // Variant *placement* carries no type-system meaning: whether a type lives in the base
        // value or the extension is an implementation detail, so the two description sets are
        // simply concatenated into one flat identifier space.
        let mut descriptions = BaseValue::type_descriptions();
        descriptions.extend(Ext::type_descriptions());
        descriptions
    }

    /// Routes to whichever side holds the value, rather than searching the concatenated
    /// descriptions.
    ///
    /// For every statically described type the two are the same answer. They differ for a value
    /// whose description is only known at runtime: the default would miss it and fall back to a
    /// derivation with **no** supported formats, so a foreign value that can serialize would
    /// report `supports_data_format(..) == false` against a registry saying otherwise.
    fn type_info(&self) -> liquers_core::type_system::TypeInfo {
        match self {
            CombinedValue::Base(base) => base.type_info(),
            CombinedValue::Extended(ext) => ext.type_info(),
        }
    }

    fn identifier(&self) -> Cow<'static, str> {
        match self {
            CombinedValue::Base(base) => base.identifier(),
            CombinedValue::Extended(ext) => ext.identifier(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            CombinedValue::Base(base) => base.type_name(),
            CombinedValue::Extended(ext) => ext.type_name(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            CombinedValue::Base(base) => base.default_extension(),
            CombinedValue::Extended(ext) => ext.default_extension(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            CombinedValue::Base(base) => base.default_filename(),
            CombinedValue::Extended(ext) => ext.default_filename(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            CombinedValue::Base(base) => base.default_media_type(),
            CombinedValue::Extended(ext) => ext.default_media_type(),
        }
    }

    fn from_string(txt: String) -> Self {
        CombinedValue::Base(BaseValue::from_string(txt))
    }

    fn from_i32(n: i32) -> Self {
        CombinedValue::Base(BaseValue::from_i32(n))
    }

    fn from_i64(n: i64) -> Self {
        CombinedValue::Base(BaseValue::from_i64(n))
    }

    fn from_f64(n: f64) -> Self {
        CombinedValue::Base(BaseValue::from_f64(n))
    }

    fn from_bool(b: bool) -> Self {
        CombinedValue::Base(BaseValue::from_bool(b))
    }

    fn from_bytes(b: Vec<u8>) -> Self {
        CombinedValue::Base(BaseValue::from_bytes(b))
    }

    fn try_from_json_value(value: &serde_json::Value) -> Result<Self, Error> {
        Ok(CombinedValue::Base(BaseValue::try_from_json_value(value)?))
    }

    fn try_into_i64(&self) -> Result<i64, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_i64(),
            CombinedValue::Extended(ext) => ext.try_into_i64(),
        }
    }

    fn try_into_bool(&self) -> Result<bool, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_bool(),
            CombinedValue::Extended(ext) => ext.try_into_bool(),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_f64(),
            CombinedValue::Extended(ext) => ext.try_into_f64(),
        }
    }
    fn try_into_key(&self) -> Result<liquers_core::query::Key, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_key(),
            _ => Err(Error::conversion_error(self.type_name(), "Key")),
        }
    }

    fn try_into_command_metadata(&self) -> Result<CommandMetadata, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_command_metadata(),
            _ => Err(Error::conversion_error(self.type_name(), "CommandMetadata")),
        }
    }

    fn try_into_bytes(&self) -> Result<Vec<u8>, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_bytes(),
            _ => Err(Error::conversion_error(self.type_name(), "bytes")),
        }
    }

    fn from_metadata(metadata: liquers_core::metadata::MetadataRecord) -> Self {
        CombinedValue::Base(BaseValue::from_metadata(metadata))
    }

    fn from_asset_info(asset_info: Vec<AssetInfo>) -> Self {
        CombinedValue::Base(BaseValue::from_asset_info(asset_info))
    }

    fn from_recipe(recipe: liquers_core::recipes::Recipe) -> Self {
        CombinedValue::Base(BaseValue::from_recipe(recipe))
    }

    fn from_command_metadata(command_metadata: CommandMetadata) -> Self {
        CombinedValue::Base(BaseValue::from_command_metadata(command_metadata))
    }

    fn from_query(query: &liquers_core::query::Query) -> Self {
        CombinedValue::Base(BaseValue::from_query(query))
    }

    fn from_key(key: &liquers_core::query::Key) -> Self {
        CombinedValue::Base(BaseValue::from_key(key))
    }
}

/*
impl<'a, B:ValueInterface + Default,E:ValueExtension> TryFrom<&'a CombinedValue<B,E>> for i32
where i32 : TryFrom<&'a B>
{
    type Error = Error;
    fn try_from(value: &CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => i32::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}
*/

/// Narrows an `f64` to `f32`, refusing rather than silently overflowing to infinity.
///
/// Precision loss from the narrowing itself is expected and not an error (that is what `as f32`
/// is for); what must not happen silently is a finite `f64` landing outside `f32`'s range. A
/// value that was already non-finite (`NaN`/`±inf`) narrows to its `f32` counterpart as usual.
fn narrow_f64_to_f32(value: f64) -> Result<f32, Error> {
    let narrowed = value as f32;
    if value.is_finite() && !narrowed.is_finite() {
        Err(Error::conversion_error(value, "f32"))
    } else {
        Ok(narrowed)
    }
}

/// Narrows an `i64` to `u32`, refusing out-of-range values instead of truncating.
fn narrow_i64_to_u32(value: i64) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| Error::conversion_error(value, "u32"))
}

/// Narrows an `i64` to `u8`, refusing out-of-range values instead of truncating.
fn narrow_i64_to_u8(value: i64) -> Result<u8, Error> {
    u8::try_from(value).map_err(|_| Error::conversion_error(value, "u8"))
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for i32
where
    i32: TryFrom<B, Error = Error>,
{
    type Error = Error;
    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => i32::try_from(base),
            CombinedValue::Extended(ext) => ext.try_into_i32(),
        }
    }
}

impl<B: ValueInterface + Default + From<i32>, E: ValueExtension> From<i32> for CombinedValue<B, E> {
    fn from(value: i32) -> CombinedValue<B, E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> From<()> for CombinedValue<B, E> {
    fn from(_value: ()) -> CombinedValue<B, E> {
        CombinedValue::none()
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for i64
where
    i64: TryFrom<B, Error = Error>,
{
    type Error = Error;
    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => i64::try_from(base),
            CombinedValue::Extended(ext) => ext.try_into_i64(),
        }
    }
}

impl<B: ValueInterface + Default + From<i64>, E: ValueExtension> From<i64> for CombinedValue<B, E> {
    fn from(value: i64) -> CombinedValue<B, E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B: ValueInterface + Default + From<Vec<i64>>, E: ValueExtension> From<Vec<i64>>
    for CombinedValue<B, E>
{
    fn from(value: Vec<i64>) -> CombinedValue<B, E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for f64
where
    f64: TryFrom<B, Error = Error>,
{
    type Error = Error;
    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => f64::try_from(base),
            CombinedValue::Extended(ext) => ext.try_into_f64(),
        }
    }
}

impl<B: ValueInterface + Default + From<f64>, E: ValueExtension> From<f64> for CombinedValue<B, E> {
    fn from(value: f64) -> CombinedValue<B, E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for f32
where
    f32: TryFrom<B, Error = Error>,
{
    type Error = Error;
    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => f32::try_from(base),
            CombinedValue::Extended(ext) => narrow_f64_to_f32(ext.try_into_f64()?),
        }
    }
}

impl<B: ValueInterface + Default + From<f32>, E: ValueExtension> From<f32> for CombinedValue<B, E> {
    fn from(value: f32) -> CombinedValue<B, E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for bool
where
    bool: TryFrom<B, Error = Error>,
{
    type Error = Error;
    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => bool::try_from(base),
            CombinedValue::Extended(ext) => ext.try_into_bool(),
        }
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for u32
where
    u32: TryFrom<B, Error = Error>,
{
    type Error = Error;

    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => u32::try_from(base),
            CombinedValue::Extended(ext) => narrow_i64_to_u32(ext.try_into_i64()?),
        }
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for u8
where
    u8: TryFrom<B, Error = Error>,
{
    type Error = Error;

    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => u8::try_from(base),
            CombinedValue::Extended(ext) => narrow_i64_to_u8(ext.try_into_i64()?),
        }
    }
}

impl<B: ValueInterface + Default + From<bool>, E: ValueExtension> From<bool>
    for CombinedValue<B, E>
{
    fn from(value: bool) -> CombinedValue<B, E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> TryFrom<CombinedValue<B, E>> for String
where
    String: TryFrom<B, Error = Error>,
{
    type Error = Error;
    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => String::try_from(base),
            CombinedValue::Extended(ext) => ext.try_into_string(),
        }
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> From<String> for CombinedValue<B, E> {
    fn from(value: String) -> CombinedValue<B, E> {
        CombinedValue::Base(B::new(&value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> From<&str> for CombinedValue<B, E> {
    fn from(value: &str) -> CombinedValue<B, E> {
        CombinedValue::Base(B::new(value))
    }
}

impl<B: ValueInterface + Default, E: ValueExtension> DefaultValueSerializer
    for CombinedValue<B, E>
{
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match self {
            CombinedValue::Base(x) => x.as_bytes(format),
            CombinedValue::Extended(x) => x.as_bytes(format),
        }
    }
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        match B::deserialize_from_bytes(b, type_identifier, fmt) {
            Ok(base) => Ok(CombinedValue::Base(base)),
            Err(base_err) => match E::deserialize_from_bytes(b, type_identifier, fmt) {
                Ok(ext) => Ok(CombinedValue::Extended(ext)),
                Err(ext_err) => {
                    if type_identifier == "polars.DataFrame" {
                        Err(ext_err)
                    } else {
                        Err(base_err)
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{ExtValue, Value};

    /// RUNTIME01 — the native adapter still satisfies the required thread bounds.
    ///
    /// `ValueExtension` was relaxed from a hard `Send + Sync` to the target-conditional
    /// `MaybeSend`/`MaybeSync` markers so that a `wasm32` build can carry a non-`Send`
    /// foreign-language handle. On native those markers must still resolve to `Send + Sync`;
    /// if the relaxation ever weakens the native build, this stops compiling.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn runtime01_native_adapter_satisfies_thread_bounds() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ExtValue>();
        assert_send_sync::<Value>();
        // The trait object behind ExtValue::Foreign must carry the bounds too, via
        // supertrait transitivity — this is what lets the variant be ungated.
        assert_send_sync::<std::sync::Arc<dyn crate::value::foreign::ForeignValue>>();
    }

    /// `fvt4.1` — a statically described type still resolves to its declared description.
    ///
    /// The routing added for foreign values must not change the answer for anything else.
    #[test]
    fn type_info_still_finds_a_described_type() {
        use crate::value::ExtValueInterface;
        use liquers_core::value::ValueInterface;

        let image = Value::new_extended(ExtValue::from_image(std::sync::Arc::new(
            image::DynamicImage::new_rgb8(1, 1),
        )));
        let info = image.type_info();

        assert_eq!(info.type_identifier, "Image");
        assert_eq!(info.default_data_format, "png");
        assert!(
            info.supports_data_format("png"),
            "the declared formats survive the delegation"
        );
    }

    /// A test-local extension that never overrides a scalar hook, so every scalar conversion
    /// falls through to `ValueExtension`'s default-refusing body
    /// (`EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS`).
    #[derive(Debug, Clone)]
    struct RefusingExtension;

    impl DefaultValueSerializer for RefusingExtension {
        fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
            Err(Error::conversion_error("RefusingExtension", format))
        }
        fn deserialize_from_bytes(
            _b: &[u8],
            type_identifier: &str,
            _fmt: &str,
        ) -> Result<Self, Error> {
            Err(Error::conversion_error(type_identifier, "RefusingExtension"))
        }
    }

    impl ValueExtension for RefusingExtension {
        fn identifier(&self) -> Cow<'static, str> {
            Cow::Borrowed("test.RefusingExtension")
        }
        fn type_name(&self) -> Cow<'static, str> {
            Cow::Borrowed("RefusingExtension")
        }
        fn default_extension(&self) -> Cow<'static, str> {
            Cow::Borrowed("bin")
        }
        fn default_filename(&self) -> Cow<'static, str> {
            Cow::Borrowed("value.bin")
        }
        fn default_media_type(&self) -> Cow<'static, str> {
            Cow::Borrowed("application/octet-stream")
        }
    }

    /// A test-local extension carrying an `i64`, exercising the scalar hooks an extension can
    /// implement to opt into binding as a scalar command argument.
    #[derive(Debug, Clone)]
    struct ScalarExtension(i64);

    impl DefaultValueSerializer for ScalarExtension {
        fn as_bytes(&self, _format: &str) -> Result<Vec<u8>, Error> {
            Ok(self.0.to_string().into_bytes())
        }
        fn deserialize_from_bytes(
            _b: &[u8],
            type_identifier: &str,
            _fmt: &str,
        ) -> Result<Self, Error> {
            Err(Error::conversion_error(type_identifier, "ScalarExtension"))
        }
    }

    impl ValueExtension for ScalarExtension {
        fn try_into_string(&self) -> Result<String, Error> {
            Ok(self.0.to_string())
        }
        fn try_into_i32(&self) -> Result<i32, Error> {
            i32::try_from(self.0).map_err(|_| Error::conversion_error(self.0, "i32"))
        }
        fn try_into_i64(&self) -> Result<i64, Error> {
            Ok(self.0)
        }
        fn try_into_f64(&self) -> Result<f64, Error> {
            Ok(self.0 as f64)
        }
        fn try_into_bool(&self) -> Result<bool, Error> {
            Ok(self.0 != 0)
        }
        fn identifier(&self) -> Cow<'static, str> {
            Cow::Borrowed("test.ScalarExtension")
        }
        fn type_name(&self) -> Cow<'static, str> {
            Cow::Borrowed("ScalarExtension")
        }
        fn default_extension(&self) -> Cow<'static, str> {
            Cow::Borrowed("bin")
        }
        fn default_filename(&self) -> Cow<'static, str> {
            Cow::Borrowed("value.bin")
        }
        fn default_media_type(&self) -> Cow<'static, str> {
            Cow::Borrowed("application/octet-stream")
        }
    }

    type RefusingValue = CombinedValue<crate::value::SimpleValue, RefusingExtension>;
    type ScalarValue = CombinedValue<crate::value::SimpleValue, ScalarExtension>;

    /// `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS`: a refusing extension errors identically
    /// through `ValueInterface` and through `TryFrom`, for every scalar type.
    #[test]
    fn refusing_extension_agrees_on_i32_via_value_interface_and_try_from() {
        let value = RefusingValue::new_extended(RefusingExtension);
        let via_interface = value.try_into_i32();
        let via_try_from = i32::try_from(value);
        assert!(via_interface.is_err());
        assert!(via_try_from.is_err());
    }

    #[test]
    fn refusing_extension_agrees_on_i64_via_value_interface_and_try_from() {
        let value = RefusingValue::new_extended(RefusingExtension);
        let via_interface = value.try_into_i64();
        let via_try_from = i64::try_from(value);
        assert!(via_interface.is_err());
        assert!(via_try_from.is_err());
    }

    #[test]
    fn refusing_extension_agrees_on_f64_via_value_interface_and_try_from() {
        let value = RefusingValue::new_extended(RefusingExtension);
        let via_interface = value.try_into_f64();
        let via_try_from = f64::try_from(value);
        assert!(via_interface.is_err());
        assert!(via_try_from.is_err());
    }

    #[test]
    fn refusing_extension_agrees_on_bool_via_value_interface_and_try_from() {
        let value = RefusingValue::new_extended(RefusingExtension);
        let via_interface = value.try_into_bool();
        let via_try_from = bool::try_from(value);
        assert!(via_interface.is_err());
        assert!(via_try_from.is_err());
    }

    #[test]
    fn refusing_extension_agrees_on_string_via_value_interface_and_try_from() {
        let value = RefusingValue::new_extended(RefusingExtension);
        let via_interface = value.try_into_string();
        let via_try_from = String::try_from(value);
        assert!(via_interface.is_err());
        assert!(via_try_from.is_err());
    }

    /// A refusing extension also refuses the narrower `f32`/`u32`/`u8` `TryFrom` impls, which
    /// route through the `f64`/`i64` hooks.
    #[test]
    fn refusing_extension_refuses_narrower_scalar_try_from_impls() {
        assert!(f32::try_from(RefusingValue::new_extended(RefusingExtension)).is_err());
        assert!(u32::try_from(RefusingValue::new_extended(RefusingExtension)).is_err());
        assert!(u8::try_from(RefusingValue::new_extended(RefusingExtension)).is_err());
    }

    /// An extension that implements the scalar hooks binds through both paths, and the two
    /// agree — the contract the issue is named after.
    #[test]
    fn scalar_extension_agrees_on_i32_via_value_interface_and_try_from() {
        let via_interface = ScalarValue::new_extended(ScalarExtension(42)).try_into_i32();
        let via_try_from = i32::try_from(ScalarValue::new_extended(ScalarExtension(42)));
        assert_eq!(via_interface.unwrap(), 42);
        assert_eq!(via_try_from.unwrap(), 42);
    }

    #[test]
    fn scalar_extension_agrees_on_i64_via_value_interface_and_try_from() {
        let via_interface = ScalarValue::new_extended(ScalarExtension(42)).try_into_i64();
        let via_try_from = i64::try_from(ScalarValue::new_extended(ScalarExtension(42)));
        assert_eq!(via_interface.unwrap(), 42);
        assert_eq!(via_try_from.unwrap(), 42);
    }

    #[test]
    fn scalar_extension_agrees_on_f64_via_value_interface_and_try_from() {
        let via_interface = ScalarValue::new_extended(ScalarExtension(42)).try_into_f64();
        let via_try_from = f64::try_from(ScalarValue::new_extended(ScalarExtension(42)));
        assert_eq!(via_interface.unwrap(), 42.0);
        assert_eq!(via_try_from.unwrap(), 42.0);
    }

    #[test]
    fn scalar_extension_agrees_on_bool_via_value_interface_and_try_from() {
        let via_interface = ScalarValue::new_extended(ScalarExtension(1)).try_into_bool();
        let via_try_from = bool::try_from(ScalarValue::new_extended(ScalarExtension(1)));
        assert!(via_interface.unwrap());
        assert!(via_try_from.unwrap());
    }

    #[test]
    fn scalar_extension_agrees_on_string_via_value_interface_and_try_from() {
        let via_interface = ScalarValue::new_extended(ScalarExtension(42)).try_into_string();
        let via_try_from = String::try_from(ScalarValue::new_extended(ScalarExtension(42)));
        assert_eq!(via_interface.unwrap(), "42");
        assert_eq!(via_try_from.unwrap(), "42");
    }

    /// The narrower `f32`/`u32`/`u8` `TryFrom` impls also reach the extension, via the
    /// `f64`/`i64` hooks with a checked narrowing conversion.
    #[test]
    fn scalar_extension_binds_through_the_narrower_try_from_impls() {
        assert_eq!(
            f32::try_from(ScalarValue::new_extended(ScalarExtension(42))).unwrap(),
            42.0f32
        );
        assert_eq!(
            u32::try_from(ScalarValue::new_extended(ScalarExtension(42))).unwrap(),
            42u32
        );
        assert_eq!(
            u8::try_from(ScalarValue::new_extended(ScalarExtension(42))).unwrap(),
            42u8
        );
    }

    /// A narrower `TryFrom` impl still refuses an out-of-range value rather than truncating it
    /// silently — the checked-narrowing half of the contract.
    #[test]
    fn scalar_extension_narrowing_refuses_out_of_range_values_instead_of_truncating() {
        // 2^40 does not fit in u32, and is nowhere near a "silently truncated" small number.
        let too_big: i64 = 1i64 << 40;
        assert!(u32::try_from(ScalarValue::new_extended(ScalarExtension(too_big))).is_err());
        // 300 does not fit in u8 (max 255); a silent truncation would give 44.
        assert!(u8::try_from(ScalarValue::new_extended(ScalarExtension(300))).is_err());

        // f64::MAX does not fit in f32; a silent truncation would give infinity with no error.
        // `ScalarExtension` carries an `i64`, so the f32 narrowing itself (shared by every
        // extension through `narrow_f64_to_f32`) is exercised directly here.
        assert!(narrow_f64_to_f32(f64::MAX).is_err());
        assert!(narrow_f64_to_f32(1.0).is_ok());
    }

    /// The `_option` hooks default to wrapping the non-option hook in `Some`, so a refusing
    /// extension still errors (there is no "none" reading to fall back to) and an implementing
    /// extension's answer round-trips through `Some`.
    #[test]
    fn option_hooks_default_to_wrapping_the_scalar_hook() {
        assert!(RefusingExtension.try_into_i64_option().is_err());
        assert!(RefusingExtension.try_into_f64_option().is_err());
        assert_eq!(ScalarExtension(42).try_into_i64_option().unwrap(), Some(42));
        assert_eq!(
            ScalarExtension(42).try_into_f64_option().unwrap(),
            Some(42.0)
        );
    }

    /// `fvt4.2` — a foreign value reports its own description, not the generic fallback.
    ///
    /// The fallback would declare no supported formats. This mock declares one, so the two
    /// answers are distinguishable and the test fails if the delegation is removed.
    #[test]
    fn type_info_delegates_to_the_foreign_value() {
        use crate::value::foreign::ForeignValue;
        use liquers_core::value::ValueInterface;
        use std::borrow::Cow;

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
        }

        let value = Value::new_extended(ExtValue::Foreign {
            value: std::sync::Arc::new(Serializable),
        });
        let info = value.type_info();

        assert_eq!(info.type_identifier, "mock.Serializable");
        assert_eq!(info.type_name, "MockSerializable");
        assert!(
            info.supports_data_format("json"),
            "the foreign value's own declared formats must reach the caller; the generic \
             fallback would have declared none"
        );
        assert!(
            value.supports_data_format("json"),
            "and ValueInterface::supports_data_format, which consults type_info, agrees"
        );
    }
}
