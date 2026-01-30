use serde_json;

use liquers_core::{
    metadata::{AssetInfo},
    value::{DefaultValueSerializer, ValueInterface},
};

use liquers_core::error::Error;
use std::{
    borrow::Cow,
    convert::TryFrom,
    result::Result,
};

pub trait ValueExtension:core::fmt::Debug + Clone + Sized + DefaultValueSerializer + Send + Sync + 'static {
    fn try_into_string(&self) -> Result<String, Error> {
        Err(Error::conversion_error(self.identifier(), "string"))
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error>{
        Err(Error::conversion_error(self.identifier(), "JSON"))
    }
    fn identifier(&self) -> Cow<'static, str>;
    fn type_name(&self) -> Cow<'static, str>;
    fn default_extension(&self) -> Cow<'static, str>;
    fn default_filename(&self) -> Cow<'static, str>;
    fn default_media_type(&self) -> Cow<'static, str>;
}

#[derive(Debug, Clone)]
pub enum CombinedValue<BaseValue:ValueInterface + Default, Ext:ValueExtension> {
    Base(BaseValue),
    Extended(Ext),
}

impl<BaseValue:ValueInterface + Default, Ext:ValueExtension> CombinedValue<BaseValue, Ext> {
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

impl<BaseValue: ValueInterface + Default, Ext: ValueExtension> Default for CombinedValue<BaseValue, Ext> {
    fn default() -> Self {
        CombinedValue::Base(BaseValue::default())
    }
}


impl<BaseValue: ValueInterface + Default, Ext: ValueExtension> ValueInterface for CombinedValue<BaseValue, Ext> {
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
            _ => Err(Error::conversion_error("extended value", "i32")),
        }
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_json_value(),
            CombinedValue::Extended(ext) => ext.try_into_json_value(),
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
            _ => "ext".into(),
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
            _ => Err(Error::conversion_error(self.type_name(), "i64")),
        }
    }

    fn try_into_bool(&self) -> Result<bool, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_bool(),
            _ => Err(Error::conversion_error(self.type_name(), "bool")),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_f64(),
            _ => Err(Error::conversion_error(self.type_name(), "f64")),
        }
    }
    fn try_into_key(&self) -> Result<liquers_core::query::Key, Error> {
        match self {
            CombinedValue::Base(base) => base.try_into_key(),
            _ => Err(Error::conversion_error(self.type_name(), "Key")),
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

impl<B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for i32
where i32: TryFrom<B, Error = Error>
{
    type Error = Error;
    fn try_from(value: CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => i32::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl<B:ValueInterface + Default + From<i32>,E:ValueExtension> From<i32> for CombinedValue<B,E> 
{
    fn from(value: i32) -> CombinedValue<B,E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> From<()> for CombinedValue<B,E> {
    fn from(_value: ()) -> CombinedValue<B,E> {
        CombinedValue::none()
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for i64
where i64: TryFrom<B, Error = Error>
{
    type Error = Error;
    fn try_from(value: CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => i64::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "i64")),
        }
    }
}

impl<B:ValueInterface + Default + From<i64>,E:ValueExtension> From<i64> for CombinedValue<B,E> {
    fn from(value: i64) -> CombinedValue<B,E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for f64
where f64: TryFrom<B, Error = Error>
{
    type Error = Error;
    fn try_from(value: CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => f64::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "f64")),
        }
    }
}

impl<B:ValueInterface + Default + From<f64>,E:ValueExtension> From<f64> for CombinedValue<B,E> {
    fn from(value: f64) -> CombinedValue<B,E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for f32
where f32: TryFrom<B, Error = Error>
{
    type Error = Error;
    fn try_from(value: CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => f32::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "f32")),
        }
    }
}

impl<B:ValueInterface + Default + From<f32>,E:ValueExtension> From<f32> for CombinedValue<B,E> {
    fn from(value: f32) -> CombinedValue<B,E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for bool
where bool: TryFrom<B, Error = Error>
{
    type Error = Error;
    fn try_from(value: CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => bool::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}

impl <B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for u32 
where u32: TryFrom<B, Error = Error>
{
    type Error = Error; 

    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => u32::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "u32")),
        }
    }
}

impl <B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for u8
where u8: TryFrom<B, Error = Error>
{
    type Error = Error; 

    fn try_from(value: CombinedValue<B, E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => u8::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "u8")),
        }
    }
}

impl<B:ValueInterface + Default + From<bool>,E:ValueExtension> From<bool> for CombinedValue<B,E> {
    fn from(value: bool) -> CombinedValue<B,E> {
        CombinedValue::Base(B::from(value))
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> TryFrom<CombinedValue<B,E>> for String
where String: TryFrom<B, Error = Error>
{
    type Error = Error;
    fn try_from(value: CombinedValue<B,E>) -> Result<Self, Self::Error> {
        match value {
            CombinedValue::Base(base) => String::try_from(base),
            _ => Err(Error::conversion_error(value.type_name(), "string")),
        }
    }
}   



impl<B:ValueInterface + Default,E:ValueExtension> From<String> for CombinedValue<B,E> {
    fn from(value: String) -> CombinedValue<B,E> {
        CombinedValue::Base(B::new(&value))
    }
}

impl<B:ValueInterface + Default,E:ValueExtension> From<&str> for CombinedValue<B,E> {
    fn from(value: &str) -> CombinedValue<B,E> {
        CombinedValue::Base(B::new(value))
    }
}


impl<B:ValueInterface + Default,E:ValueExtension> DefaultValueSerializer for CombinedValue<B,E> {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match self{
            CombinedValue::Base(x) => x.as_bytes(format),
            CombinedValue::Extended(x) => x.as_bytes(format),
        }
    }
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        // TODO: use type identifier to find out whether this is base or extended 
        Ok(CombinedValue::Base(
            B::deserialize_from_bytes(b, type_identifier, fmt)?,
        ))
    }
}

