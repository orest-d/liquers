#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(warnings)]

use std::collections::HashMap;
use std::fmt::{format, Debug};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use nom::Err;

use crate::command_metadata::{self, CommandKey, CommandMetadata, CommandMetadataRegistry};
use crate::context::{Context, Environment};
use crate::error::{Error, ErrorType};
use crate::plan::{ParameterValue, ResolvedParameterValues};
use crate::query::{Key, Position, Query};
use crate::state::State;
use crate::value::ValueInterface;

/// Encapsulates the action parameters, that are passed to the command
/// when it is executed.
#[derive(Debug)]
pub struct CommandArguments<E: Environment> {
    pub(crate) parameters: ResolvedParameterValues,
    pub(crate) values: Vec<Option<Arc<E::Value>>>,
    pub action_position: Position,
}

impl<E: Environment> Clone for CommandArguments<E> {
    fn clone(&self) -> Self {
        CommandArguments {
            parameters: self.parameters.clone(),
            values: self.values.clone(),
            action_position: self.action_position.clone(),
        }
    }
}

impl<E: Environment> CommandArguments<E> {
    pub fn new(parameters: ResolvedParameterValues) -> Self {
        CommandArguments {
            parameters,
            action_position: Position::unknown(),
            values: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.parameters.0.len()
    }

    pub fn set_value(&mut self, i: usize, value: Arc<E::Value>) {
        for j in self.values.len()..=i {
            self.values.push(None);
        }
        self.values[i] = Some(value);
    }
    pub fn get_parameter(&self, i: usize, name: &str) -> Result<&ParameterValue, Error> {
        if let Some(p) = self.parameters.0.get(i) {
            Ok(p)
        } else {
            Err(Error::missing_argument(i, name, &self.action_position))
        }
    }

    pub fn get_name(&self, i: usize) -> Result<Option<String>, Error> {
        if let Some(p) = self.parameters.0.get(i) {
            Ok(p.name())
        } else {
            Err(Error::missing_argument(
                i,
                "<unknown>",
                &self.action_position,
            ))
        }
    }

    pub fn get_value(&self, i: usize, name: &str) -> Result<E::Value, Error> {
        if let Some(Some(v)) = self.values.get(i) {
            Ok((*(v.clone())).clone())
        } else {
            let p = self.get_parameter(i, name)?;
            if let Some(v) = p.value() {
                Ok(E::Value::try_from_json_value(&v)?)
            } else {
                match p {
                    ParameterValue::Placeholder(n) => Err(Error::general_error(format!(
                        "Unresolved placeholder parameter {}: {}",
                        i, n
                    ))
                    .with_position(&self.action_position)),
                    _ => Err(Error::general_error(format!(
                        "Unresolved/unexpected parameter {}: {}",
                        i, p
                    ))
                    .with_position(&self.action_position)),
                }
            }
        }
    }

    pub fn get<T: FromParameterValue<T> + TryFrom<E::Value, Error = Error>>(
        &self,
        i: usize,
        name: &str,
    ) -> Result<T, Error> {
        if let Some(Some(v)) = self.values.get(i) {
            return T::try_from((**v).clone());
        }
        let p = self.get_parameter(i, name)?;

        if let Some(link) = p.link() {
            return Err(
                Error::general_error(format!("Unresolved link parameter {}: {}", i, link))
                    .with_position(&self.action_position),
            );
        }
        if p.is_injected() {
            return Err(Error::general_error(
                "Inconsistent parameter type - injected found, value expected".to_owned(),
            )
            .with_position(&self.action_position));
        }
        T::from_parameter_value(&p)
    }

    /// Returns the elements of a variadic argument, each converted to `T`.
    ///
    /// This is the accessor for an argument declared `multiple` (see
    /// [`ArgumentInfo::set_multiple`](crate::command_metadata::ArgumentInfo::set_multiple) and the
    /// `multiple` flag of `register_command!`). The parameter in slot `i` must be
    /// [`ParameterValue::MultipleParameters`]; anything else means the command metadata and the
    /// resolved plan disagree, and is reported rather than converted.
    ///
    /// An empty argument list yields an empty vector — that is the normal "no parameters supplied"
    /// case, not an error, because a variadic argument has no default other than emptiness.
    ///
    /// # Why the bounds differ from [`CommandArguments::get`]
    ///
    /// `get` additionally requires `T: TryFrom<E::Value, Error = Error>` for its pre-materialised
    /// fast path: when a top-level link parameter has been resolved, the interpreter stores the
    /// resulting value in `self.values[i]` and `get` converts from *that* rather than from the
    /// parameter. A variadic argument never takes that path — the interpreter populates `values`
    /// only where `ParameterValue::link()` is `Some`, and `MultipleParameters::link()` is `None` —
    /// so this deliberately ignores `self.values` and drops the bound. Dropping it is what makes
    /// `Vec<String>` retrievable at all: `Vec<T>` satisfies neither bound of `get`, and a blanket
    /// `FromParameterValue<Vec<T>>` impl would overlap the existing `Vec<V: ValueInterface>` one.
    ///
    /// Links *inside* a variadic argument are resolved element-wise by the interpreter before the
    /// arguments are built, so by the time this runs every element is a value.
    pub fn get_multiple<T: FromParameterValue<T>>(
        &self,
        i: usize,
        name: &str,
    ) -> Result<Vec<T>, Error> {
        let p = self.get_parameter(i, name)?;
        match p {
            ParameterValue::MultipleParameters(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    values.push(Self::convert_multiple_element::<T>(element, i, name)?);
                }
                Ok(values)
            }
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _)
            | ParameterValue::DefaultLink(_, _)
            | ParameterValue::ParameterLink(_, _, _)
            | ParameterValue::OverrideLink(_, _)
            | ParameterValue::EnumLink(_, _, _)
            | ParameterValue::Placeholder(_)
            | ParameterValue::Injected(_)
            | ParameterValue::None => Err(Error::general_error(format!(
                "Argument {} '{}' is declared as multiple, but was not resolved as a parameter list",
                i, name
            ))
            .with_position(&self.action_position)),
        }
    }

    /// Converts one element of a variadic argument.
    ///
    /// The value variants convert through `T::from_parameter_value`. The link variants are an
    /// error: the interpreter materialises links inside a variadic argument before constructing
    /// the arguments, so an unresolved link here means the arguments were built without that step.
    /// The remaining variants cannot occur inside a list — `ParameterValue::pop_value` refuses to
    /// put them there — and are enumerated so that a new variant is a compile error.
    fn convert_multiple_element<T: FromParameterValue<T>>(
        element: &ParameterValue,
        i: usize,
        name: &str,
    ) -> Result<T, Error> {
        match element {
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _) => T::from_parameter_value(element),
            ParameterValue::DefaultLink(_, query)
            | ParameterValue::ParameterLink(_, query, _)
            | ParameterValue::OverrideLink(_, query)
            | ParameterValue::EnumLink(_, query, _) => Err(Error::general_error(format!(
                "Unresolved link parameter in multiple argument {} '{}': {}",
                i,
                name,
                query.encode()
            ))
            .with_position(&element.position())),
            ParameterValue::MultipleParameters(_) => Err(Error::unexpected_error(format!(
                "Nested multiple parameters in argument {} '{}'",
                i, name
            ))),
            ParameterValue::Injected(injected_name) => Err(Error::unexpected_error(format!(
                "Injected parameter '{}' inside multiple argument {} '{}'",
                injected_name, i, name
            ))),
            ParameterValue::Placeholder(placeholder_name) => {
                Err(Error::unexpected_error(format!(
                    "Unresolved placeholder '{}' inside multiple argument {} '{}'",
                    placeholder_name, i, name
                )))
            }
            ParameterValue::None => Err(Error::unexpected_error(format!(
                "Unresolved parameter inside multiple argument {} '{}'",
                i, name
            ))),
        }
    }

    /// Returns the injected parameter as a value of type T
    pub fn get_injected<T: InjectedFromContext<E>>(
        &self,
        i: usize,
        name: &str,
        context: Context<E>,
    ) -> Result<T, Error> {
        let p = self.get_parameter(i, name)?;
        if !p.is_injected() {
            return Err(Error::general_error(
                "Inconsistent parameter type - value found, injected expected".to_owned(),
            ));
        }
        T::from_context(name, context)
    }

    pub fn parameter_position(&self, i: usize) -> Position {
        if let Some(p) = self.parameters.0.get(i) {
            let pos = p.position();
            if pos.is_unknown() {
                self.action_position.clone()
            } else {
                pos
            }
        } else {
            self.action_position.clone()
        }
    }
}

pub trait FromParameterValue<T> {
    fn from_parameter_value(param: &ParameterValue) -> Result<T, Error>;
}

/// Macro to simplify the implementation of the FromParameterValue trait
macro_rules! impl_from_parameter_value2 {
    ($t:ty, $jsonvalue_to_opt:expr) => {
        impl FromParameterValue<$t> for $t {
            fn from_parameter_value(param: &ParameterValue) -> Result<$t, Error> {
                if let Some(ref p) = param.value() {
                    $jsonvalue_to_opt(p).ok_or(
                        Error::conversion_error_with_message(
                            p,
                            stringify!($t),
                            concat!(stringify!($t), " parameter value expected"),
                        )
                        .with_position(&param.position()),
                    )
                } else {
                    return Err(Error::conversion_error_with_message(
                        param,
                        stringify!($t),
                        "Parameter value expected",
                    ));
                }
            }
        }
    };
}

macro_rules! impl_from_parameter_value2_opt {
    ($t:ty, $jsonvalue_to_opt:expr) => {
        impl FromParameterValue<Option<$t>> for Option<$t> {
            fn from_parameter_value(param: &ParameterValue) -> Result<Option<$t>, Error> {
                if let Some(ref p) = param.value() {
                    if p.is_null() {
                        return Ok(None);
                    }
                    $jsonvalue_to_opt(p).map(|x| Some(x)).ok_or(
                        Error::conversion_error_with_message(
                            p,
                            stringify!($t),
                            concat!(stringify!($t), " parameter value expected"),
                        )
                        .with_position(&param.position()),
                    )
                } else {
                    return Err(Error::conversion_error_with_message(
                        param,
                        stringify!($t),
                        "Parameter value expected",
                    ));
                }
            }
        }
    };
}

/// A key written directly in the query text, rather than supplied through a link.
///
/// The link path never reaches here — it converts through `TryFrom<Value>` in
/// [`CommandArguments::get`] — but the bound requires both, and a literal key argument is a
/// reasonable thing to write, so it is parsed rather than rejected.
impl FromParameterValue<Key> for Key {
    fn from_parameter_value(param: &ParameterValue) -> Result<Key, Error> {
        let Some(ref value) = param.value() else {
            return Err(Error::conversion_error_with_message(
                param,
                "Key",
                "Parameter value expected",
            ));
        };
        let Some(text) = value.as_str() else {
            return Err(Error::conversion_error_with_message(
                value,
                "Key",
                "Key parameter value expected",
            )
            .with_position(&param.position()));
        };
        crate::parse::parse_key(text).map_err(|error| error.with_position(&param.position()))
    }
}

impl_from_parameter_value2!(
    String,
    (|p: &serde_json::Value| p.as_str().map(|s| s.to_owned()))
);
impl_from_parameter_value2!(i64, |p: &serde_json::Value| p.as_i64());
impl_from_parameter_value2!(i32, |p: &serde_json::Value| p.as_i64().map(|x| x as i32));
impl_from_parameter_value2!(i16, |p: &serde_json::Value| p.as_i64().map(|x| x as i16));
impl_from_parameter_value2!(i8, |p: &serde_json::Value| p.as_i64().map(|x| x as i8));
impl_from_parameter_value2!(isize, |p: &serde_json::Value| p
    .as_i64()
    .map(|x| x as isize));
impl_from_parameter_value2!(u64, |p: &serde_json::Value| p.as_i64().map(|x| x as u64));
impl_from_parameter_value2!(u32, |p: &serde_json::Value| p.as_i64().map(|x| x as u32));
impl_from_parameter_value2!(u16, |p: &serde_json::Value| p.as_i64().map(|x| x as u16));
impl_from_parameter_value2!(u8, |p: &serde_json::Value| p.as_i64().map(|x| x as u8));
impl_from_parameter_value2!(usize, |p: &serde_json::Value| p
    .as_i64()
    .map(|x| x as usize));
impl_from_parameter_value2!(f64, |p: &serde_json::Value| p.as_f64());
impl_from_parameter_value2!(f32, |p: &serde_json::Value| p.as_f64().map(|x| x as f32));
impl_from_parameter_value2_opt!(i64, |p: &serde_json::Value| p.as_i64());
impl_from_parameter_value2_opt!(i32, |p: &serde_json::Value| p.as_i64().map(|x| x as i32));
impl_from_parameter_value2_opt!(i16, |p: &serde_json::Value| p.as_i64().map(|x| x as i16));
impl_from_parameter_value2_opt!(i8, |p: &serde_json::Value| p.as_i64().map(|x| x as i8));
impl_from_parameter_value2_opt!(isize, |p: &serde_json::Value| p
    .as_i64()
    .map(|x| x as isize));
impl_from_parameter_value2_opt!(u64, |p: &serde_json::Value| p.as_i64().map(|x| x as u64));
impl_from_parameter_value2_opt!(u32, |p: &serde_json::Value| p.as_i64().map(|x| x as u32));
impl_from_parameter_value2_opt!(u16, |p: &serde_json::Value| p.as_i64().map(|x| x as u16));
impl_from_parameter_value2_opt!(u8, |p: &serde_json::Value| p.as_i64().map(|x| x as u8));
impl_from_parameter_value2_opt!(usize, |p: &serde_json::Value| p
    .as_i64()
    .map(|x| x as usize));
impl_from_parameter_value2_opt!(f64, |p: &serde_json::Value| p.as_f64());
impl_from_parameter_value2_opt!(f32, |p: &serde_json::Value| p.as_f64().map(|x| x as f32));
impl_from_parameter_value2!(bool, |p: &serde_json::Value| p.as_bool());
/*
impl_from_parameter_value2!(Option<i64>, |p: &serde_json::Value| {
    if p.is_null() {
        Some(None)
    } else {
        p.as_i64().map(Some)
    }
});
impl_from_parameter_value2!(Option<f64>, |p: &serde_json::Value| {
    if p.is_null() {
        Some(None)
    } else {
        p.as_f64().map(Some)
    }
});
*/

impl<V: ValueInterface> FromParameterValue<Vec<V>> for Vec<V> {
    fn from_parameter_value(param: &ParameterValue) -> Result<Vec<V>, Error> {
        fn from_json_value<T: ValueInterface>(p: &serde_json::Value) -> Result<Vec<T>, Error> {
            match p {
                serde_json::Value::Array(a) => {
                    let mut v = Vec::new();
                    for e in a.iter() {
                        v.push(T::try_from_json_value(e)?);
                    }
                    Ok(v)
                }
                _ => Ok(vec![T::try_from_json_value(p)?]),
            }
        }

        match param {
            ParameterValue::DefaultValue(_, v) => return from_json_value(v),
            ParameterValue::ParameterValue(_, v, pos) => {
                return from_json_value(v).map_err(|e| e.with_position(pos))
            }
            ParameterValue::MultipleParameters(p) => {
                let mut v = Vec::new();
                for pp in p.iter() {
                    v.push(match pp {
                        ParameterValue::DefaultValue(_, value) => V::try_from_json_value(value)?,
                        ParameterValue::ParameterValue(_, value, position) => {
                            V::try_from_json_value(value).map_err(|e| e.with_position(position))?
                        }
                        ParameterValue::MultipleParameters(vec) => {
                            return Err(Error::unexpected_error(
                                "Nested multiple parameters not allowed".to_owned(),
                            ))
                        }
                        ParameterValue::Injected(name) => {
                            return Err(Error::unexpected_error(format!(
                                "Injected parameters ({name}) not allowed inside multi-parameter"
                            )))
                        }
                        ParameterValue::None => {
                            return Err(Error::unexpected_error(
                                "None parameter not allowed inside multi-parameter".to_owned(),
                            ))
                        }
                        _ => {
                            return Err(Error::unexpected_error(
                                "Unexpected parameter type inside multi-parameter".to_owned(),
                            ))
                        }
                    });
                }
                Ok(v)
            }
            ParameterValue::Injected(name) => {
                return Err(Error::general_error(format!(
                    "Injected parameters ({name}) not allowed"
                )))
            }
            ParameterValue::None => {
                return Err(Error::general_error(
                    "None parameter not allowed".to_owned(),
                ))
            }
            _ => return Err(Error::general_error("Unexpected parameter type".to_owned())),
        }
        //Ok(vec![E::Value::none()])
    }
}

/// Marker trait to distinguish actual payload types from newtypes that extract from payload.
///
/// Implement this for your payload types. You must also manually implement `InjectedFromContext`
/// for your payload type to enable injection via the `injected` keyword.
///
/// For newtypes that extract from payload, implement `ExtractFromPayload` and `InjectedFromContext`.
pub trait PayloadType:
    Clone + crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync + 'static
{
}

/// Trait for types that can be extracted from a payload.
/// Implement this for newtypes that extract specific fields from a payload.
/// You must also implement `InjectedFromContext` manually for each newtype.
pub trait ExtractFromPayload<P>: Sized {
    fn extract_from_payload(payload: &P) -> Result<Self, Error>;
}

/// Trait for types that can be injected from context.
///
/// # Implementation
///
/// For payload types, implement as:
/// ```ignore
/// impl<E: Environment<Payload = YourPayload>> InjectedFromContext<E> for YourPayload {
///     fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
///         context.get_payload_clone().ok_or(Error::general_error(format!(
///             "No payload in context for injected parameter {}", name
///         )))
///     }
/// }
/// ```
///
/// For newtypes extracting from payload, implement as:
/// ```ignore
/// impl InjectedFromContext<YourEnvironment> for YourNewtype {
///     fn from_context(_name: &str, context: Context<YourEnvironment>) -> Result<Self, Error> {
///         let payload = context.get_payload_clone()
///             .ok_or_else(|| Error::general_error("No payload".to_string()))?;
///         YourNewtype::extract_from_payload(&payload)
///     }
/// }
/// ```
pub trait InjectedFromContext<E: Environment>: Sized {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error>;
}

/// Unit type is a valid payload (for environments without payload)
impl PayloadType for () {}

/// Unit type can be injected (always succeeds with ())
impl<E: Environment<Payload = ()>> InjectedFromContext<E> for () {
    fn from_context(_name: &str, _context: Context<E>) -> Result<Self, Error> {
        Ok(())
    }
}

/// String is a valid payload type (commonly used for simple cases)
impl PayloadType for String {}

/// String payload can be injected
impl<E: Environment<Payload = String>> InjectedFromContext<E> for String {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
        context
            .get_payload_clone()
            .ok_or(Error::general_error(format!(
                "No payload in context for injected parameter {}",
                name
            )))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CommandExecutor<E: Environment>:
    crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync
{
    fn execute(
        &self,
        command_key: &CommandKey,
        state: &State<E::Value>,
        arguments: CommandArguments<E>,
        context: Context<E>,
    ) -> Result<E::Value, Error>;

    async fn execute_async(
        &self,
        command_key: &CommandKey,
        state: State<E::Value>,
        arguments: CommandArguments<E>,
        context: Context<E>,
    ) -> Result<E::Value, Error> {
        self.execute(command_key, &state, arguments, context)
    }
}

// Stored command-executor closure types. Trait-object additional bounds cannot use the
// MaybeSend/MaybeSync markers (only auto-traits may follow the principal trait — E0225), so
// the whole `dyn Fn ...` type is aliased per target: `+ Send + Sync` on native, bare on wasm.
#[cfg(not(target_arch = "wasm32"))]
type SyncExecutorFn<E> = dyn Fn(
        &State<<E as Environment>::Value>,
        CommandArguments<E>,
        Context<E>,
    ) -> Result<<E as Environment>::Value, Error>
    + Send
    + Sync
    + 'static;
#[cfg(target_arch = "wasm32")]
type SyncExecutorFn<E> = dyn Fn(
        &State<<E as Environment>::Value>,
        CommandArguments<E>,
        Context<E>,
    ) -> Result<<E as Environment>::Value, Error>
    + 'static;

#[cfg(not(target_arch = "wasm32"))]
type AsyncExecutorFn<E> = dyn Fn(
        State<<E as Environment>::Value>,
        CommandArguments<E>,
        Context<E>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<<E as Environment>::Value, Error>>
    + Send
    + Sync
    + 'static;
#[cfg(target_arch = "wasm32")]
type AsyncExecutorFn<E> = dyn Fn(
        State<<E as Environment>::Value>,
        CommandArguments<E>,
        Context<E>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<<E as Environment>::Value, Error>>
    + 'static;

pub struct CommandRegistry<E: Environment> {
    executors: HashMap<CommandKey, Arc<Box<SyncExecutorFn<E>>>>,
    async_executors: HashMap<CommandKey, Arc<Box<AsyncExecutorFn<E>>>>,
    pub command_metadata_registry: CommandMetadataRegistry,
}

impl<E: Environment> CommandRegistry<E> {
    pub fn new() -> Self {
        CommandRegistry {
            //executors: HashMap::new(),
            executors: HashMap::new(),
            async_executors: HashMap::new(),
            command_metadata_registry: CommandMetadataRegistry::new(),
        }
    }
    pub fn register_command<K, F>(&mut self, key: K, f: F) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
        F: (Fn(&State<E::Value>, CommandArguments<E>, Context<E>) -> Result<E::Value, Error>)
            + crate::maybe_send::MaybeSync
            + crate::maybe_send::MaybeSend
            + 'static,
    {
        let key = key.into();
        let command_metadata = CommandMetadata::from_key(key.clone());
        self.command_metadata_registry
            .add_command(&command_metadata);
        self.executors.insert(key.clone(), Arc::new(Box::new(f)));
        Ok(self.command_metadata_registry.get_mut(key).unwrap())
    }
    pub fn register_async_command<K, F>(
        &mut self,
        key: K,
        f: F,
    ) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
        F: (Fn(
                State<E::Value>,
                CommandArguments<E>,
                Context<E>,
            ) -> crate::maybe_send::BoxFuture<'static, Result<E::Value, Error>>)
            + crate::maybe_send::MaybeSync
            + crate::maybe_send::MaybeSend
            + 'static,
    {
        let key = key.into();
        let command_metadata = CommandMetadata::from_key(key.clone());
        self.command_metadata_registry
            .add_command(&command_metadata);

        let bf: Arc<Box<AsyncExecutorFn<E>>> = Arc::new(Box::new(f));
        self.async_executors.insert(key.clone(), bf.clone());
        Ok(self.command_metadata_registry.get_mut(key).unwrap())
    }

    /// Removes a command's sync executor, async executor and metadata.
    ///
    /// Returns `true` if anything was removed. Idempotent: unregistering a command that was
    /// never registered returns `false` rather than an error.
    ///
    /// All three stores are cleared together by design. Planning consults the metadata registry
    /// while execution consults the executor maps, so removing only the executors would leave a
    /// command that plans successfully and then fails at execution, and removing only the
    /// metadata would leave an unreachable executor.
    ///
    /// Note that this discards the `impl_version` that [`CommandMetadataRegistry::add_command`]
    /// preserves when *replacing* a command: re-registering after `unregister` starts from a
    /// fresh version, which expires assets computed by the earlier command.
    pub fn unregister<K>(&mut self, key: K) -> bool
    where
        K: Into<CommandKey>,
    {
        let key: CommandKey = key.into();
        let had_sync = self.executors.remove(&key).is_some();
        let had_async = self.async_executors.remove(&key).is_some();
        let had_metadata = self.command_metadata_registry.remove_command(key).is_some();
        had_sync || had_async || had_metadata
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> CommandExecutor<E> for CommandRegistry<E> {
    fn execute(
        &self,
        key: &CommandKey,
        state: &State<E::Value>,
        arguments: CommandArguments<E>,
        context: Context<E>,
    ) -> Result<E::Value, Error> {
        if let Some(command) = self.executors.get(&key) {
            command(state, arguments, context)
        } else {
            Err(Error::unknown_command_executor(
                &key.realm,
                &key.namespace,
                &key.name,
                &arguments.action_position,
            ))
        }
    }

    async fn execute_async(
        &self,
        key: &CommandKey,
        state: State<E::Value>,
        arguments: CommandArguments<E>,
        context: Context<E>,
    ) -> Result<E::Value, Error> {
        if let Some(command) = self.async_executors.get(&key) {
            command(state, arguments.clone(), context).await
        } else {
            self.execute(key, &state, arguments, context)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as liquers_core;
    use crate::assets::AssetManager;
    use crate::command_metadata::CommandKey;
    use crate::commands::{CommandArguments, CommandRegistry};
    use crate::context::SimpleEnvironment;
    use crate::state::State;
    use crate::value::Value;
    use liquers_macro::*;

    /// Tests for `CommandArguments::get_multiple`, the retrieval half of variadic arguments.
    ///
    /// See `specs/design/variadic-arguments-declaration/`. These construct `CommandArguments`
    /// directly rather than going through the interpreter, which is deliberate: it also covers
    /// the states the interpreter never produces.
    mod get_multiple {
        use super::*;
        use crate::plan::{ParameterValue, ResolvedParameterValues};
        use crate::query::Position;

        type TestEnv = SimpleEnvironment<Value>;

        fn element(name: &str, v: serde_json::Value, offset: usize) -> ParameterValue {
            ParameterValue::ParameterValue(
                name.to_string(),
                v,
                Position::new(offset, 1, offset + 1),
            )
        }

        fn args_of(parameter: ParameterValue) -> CommandArguments<TestEnv> {
            CommandArguments::<TestEnv>::new(ResolvedParameterValues(vec![parameter]))
        }

        /// U1 - three elements convert, in order.
        #[test]
        fn returns_elements_in_order() -> Result<(), Error> {
            let args = args_of(ParameterValue::MultipleParameters(vec![
                element("columns", "a".into(), 21),
                element("columns", "b".into(), 23),
                element("columns", "c".into(), 25),
            ]));

            let columns: Vec<String> = args.get_multiple(0, "columns")?;
            assert_eq!(columns, vec!["a", "b", "c"]);
            Ok(())
        }

        /// U2 - an empty variadic argument is an empty vector, NOT an error. This is the
        /// `select_columns` with no parameters case, which the plan builder produces legitimately
        /// because a variadic argument has no default other than emptiness.
        #[test]
        fn empty_list_is_ok() -> Result<(), Error> {
            let args = args_of(ParameterValue::MultipleParameters(Vec::new()));

            let columns: Vec<String> = args.get_multiple(0, "columns")?;
            assert!(columns.is_empty());
            Ok(())
        }

        /// U3 - metadata says `multiple`, the plan resolved a scalar. Report it, never convert.
        #[test]
        fn scalar_slot_is_an_error() {
            let args = args_of(element("columns", "a".into(), 21));

            let err = args
                .get_multiple::<String>(0, "columns")
                .expect_err("a scalar slot must not satisfy get_multiple");
            assert!(err.message.contains("columns"), "message: {}", err.message);
            assert!(err.message.contains("multiple"), "message: {}", err.message);
        }

        /// U4 - an unresolved link element. Unreachable through the interpreter, reachable
        /// through `CommandArguments::new`, so the message must say what is wrong.
        #[test]
        fn unresolved_link_element_is_an_error() -> Result<(), Error> {
            let link = crate::parse::parse_query("-R/config/colname.txt")?;
            let args = args_of(ParameterValue::MultipleParameters(vec![
                ParameterValue::ParameterLink(
                    "columns".to_string(),
                    link,
                    Position::new(21, 1, 22),
                ),
            ]));

            let err = args
                .get_multiple::<String>(0, "columns")
                .expect_err("an unresolved link must not convert");
            assert!(
                err.message.to_lowercase().contains("link"),
                "message: {}",
                err.message
            );
            Ok(())
        }

        /// U5 - the error points at the offending ELEMENT, not at the action. Each element
        /// carries its own position, which is what makes a per-element diagnostic possible.
        #[test]
        fn conversion_error_carries_element_position() {
            let mut args = args_of(ParameterValue::MultipleParameters(vec![
                element("rows", 1.into(), 21),
                element("rows", "x".into(), 23),
            ]));
            args.action_position = Position::new(6, 1, 7);

            let err = args
                .get_multiple::<i64>(0, "rows")
                .expect_err("\"x\" is not an i64");
            assert_eq!(
                err.position.offset, 23,
                "must point at the element, not at the action"
            );
        }

        /// U6 - non-string element types, the case that motivates deriving `ArgumentType` from
        /// the `Vec` element type rather than leaving it `Any`.
        #[test]
        fn converts_integers() -> Result<(), Error> {
            let args = args_of(ParameterValue::MultipleParameters(vec![
                element("rows", 1.into(), 21),
                element("rows", 2.into(), 23),
            ]));

            let rows: Vec<i64> = args.get_multiple(0, "rows")?;
            assert_eq!(rows, vec![1, 2]);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_command_registry_execute() {
        // Create a registry
        let mut registry = CommandRegistry::<SimpleEnvironment<Value>>::new();

        // Register a simple command that returns a constant value
        let key = CommandKey::new("realm", "namespace", "name");
        registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from(42)))
            .expect("register_command failed");

        // Prepare state and arguments
        let state = State::new();
        let parameters = ResolvedParameterValues::new();
        let args = CommandArguments::new(parameters);
        let envref = SimpleEnvironment::<Value>::new().to_ref();
        let assetref = envref.get_asset_manager().create_dummy_asset();
        let context = assetref.create_context().await;

        // Execute the command
        let result = registry.execute(&key, &state, args, context);

        // Assert the result
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value, Value::from(42));
    }

    #[tokio::test]
    async fn test_command_registry_execute_greet() {
        // Create a registry
        let mut registry = CommandRegistry::<SimpleEnvironment<Value>>::new();

        // Register a simple command that returns a constant value
        let key = CommandKey::new_name("greet");
        registry
            .register_command(key.clone(), |state, args, _| {
                let input = state.try_into_string()?;
                let greeting: String = args.get(0, "greeting")?;
                Ok(Value::from(format!("{}, {}!", greeting, input)))
            })
            .expect("register_command failed");

        // Prepare state and arguments
        let state = State::new().with_string("world");
        let parameters = ResolvedParameterValues::new();
        let mut args = CommandArguments::new(parameters);
        args.set_value(0, Arc::new(Value::from("Hello")));
        let envref = SimpleEnvironment::<Value>::new().to_ref();
        let assetref = envref.get_asset_manager().create_dummy_asset();
        let context = assetref.create_context().await;

        // Execute the command
        let result = registry.execute(&key, &state, args, context);

        // Assert the result
        assert!(result.is_ok());
        let value = result.unwrap().try_into_string().unwrap();
        assert_eq!(value, "Hello, world!");
    }

    #[tokio::test]
    async fn test_command_registry_execute_greet_macroregistration() {
        use crate::context::*;
        // Create a registry
        let mut registry = CommandRegistry::<SimpleEnvironment<Value>>::new();

        // Register a simple command that returns a constant value
        let key = CommandKey::new_name("greet");
        type CommandEnvironment = SimpleEnvironment<Value>;

        fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
            let input = state.try_into_string()?;
            Ok(Value::from(format!("{}, {}!", greeting, input)))
        }
        let mut cr = &mut registry;
        register_command!(cr, fn greet(state, greeting: String) -> result)
            .expect("register_command failed");

        // Prepare state and arguments
        let state = State::new().with_string("world");
        let parameters = ResolvedParameterValues::new();
        let mut args = CommandArguments::new(parameters);
        args.set_value(0, Arc::new(Value::from("Hello")));
        let envref = SimpleEnvironment::<Value>::new().to_ref();
        let assetref = envref.get_asset_manager().create_dummy_asset();
        let context = assetref.create_context().await;

        // Execute the command
        let result = registry.execute(&key, &state, args, context);

        // Assert the result
        assert!(result.is_ok());
        let value = result.unwrap().try_into_string().unwrap();
        assert_eq!(value, "Hello, world!");
    }
}

#[cfg(test)]
mod unregister_tests {
    use super::*;
    use crate::command_metadata::CommandKey;
    use crate::context::SimpleEnvironment;
    use crate::parse::parse_query;
    use crate::plan::PlanBuilder;
    use crate::value::Value;

    type Env = SimpleEnvironment<Value>;

    fn registry_with_hello() -> CommandRegistry<Env> {
        let mut registry = CommandRegistry::<Env>::new();
        registry
            .register_command(CommandKey::new("", "", "hello"), |_, _, _| {
                Ok(Value::from("Hello, world!"))
            })
            .expect("register_command failed");
        registry
    }

    /// unregister01 — metadata *and* both executor maps are cleared together.
    ///
    /// The sharp assertion is *which layer* fails afterwards. Planning consults the metadata
    /// registry; execution consults the executor maps. So if only the executors were removed,
    /// the query would still plan and fail later at execution — and this test must reject that,
    /// not accept it as "an error was returned".
    #[test]
    fn unregister01_removes_metadata_and_executors() -> Result<(), Error> {
        let mut registry = registry_with_hello();
        let key = CommandKey::new("", "", "hello");

        // Before: the query plans.
        let query = parse_query("hello")?;
        let mut builder = PlanBuilder::new(query.clone(), &registry.command_metadata_registry);
        assert!(
            builder.build().is_ok(),
            "precondition: `hello` should plan while registered"
        );

        assert!(
            registry.unregister(key.clone()),
            "unregister reported no removal"
        );

        // After: planning itself must fail. A plan that still builds would mean the metadata
        // survived, which is exactly the partial-removal bug this test exists to catch.
        let mut builder = PlanBuilder::new(query, &registry.command_metadata_registry);
        let planned = builder.build();
        assert!(
            planned.is_err(),
            "after unregister the query still planned — metadata was not removed, so the failure \
             would surface at execution instead"
        );

        // And the executors are gone too.
        assert!(
            registry.executors.get(&key).is_none(),
            "sync executor survived"
        );
        assert!(
            registry.async_executors.get(&key).is_none(),
            "async executor survived"
        );
        assert!(
            registry.command_metadata_registry.get(key).is_none(),
            "metadata survived"
        );
        Ok(())
    }

    /// unregister02 — unregistering an absent command is `false`, not an error.
    #[test]
    fn unregister02_absent_is_false_not_error() {
        let mut registry = registry_with_hello();
        assert!(!registry.unregister(CommandKey::new("", "", "nonexistent")));
        // Idempotent: a second unregister of a real command is also false.
        assert!(registry.unregister(CommandKey::new("", "", "hello")));
        assert!(!registry.unregister(CommandKey::new("", "", "hello")));
    }

    /// unregister03 — re-registering after unregister starts from a fresh `impl_version`.
    ///
    /// `add_command` deliberately *preserves* `impl_version` when replacing a command, so that a
    /// replace does not expire dependent assets. `unregister` discards that history, so this
    /// documents the difference rather than leaving it to be discovered as mysterious cache
    /// invalidation.
    #[test]
    fn unregister03_reregister_resets_impl_version() {
        let mut registry = registry_with_hello();
        let key = CommandKey::new("", "", "hello");

        if let Some(meta) = registry.command_metadata_registry.get_mut(key.clone()) {
            meta.impl_version = crate::metadata::Version::new(42);
        }
        assert_eq!(
            registry
                .command_metadata_registry
                .get(key.clone())
                .map(|m| m.impl_version.clone()),
            Some(crate::metadata::Version::new(42))
        );

        // A *replace* preserves it.
        registry
            .register_command(key.clone(), |_, _, _| Ok(Value::none()))
            .expect("replace failed");
        assert_eq!(
            registry
                .command_metadata_registry
                .get(key.clone())
                .map(|m| m.impl_version.clone()),
            Some(crate::metadata::Version::new(42)),
            "replace should preserve impl_version"
        );

        // An unregister/re-register does not.
        assert!(registry.unregister(key.clone()));
        registry
            .register_command(key.clone(), |_, _, _| Ok(Value::none()))
            .expect("re-register failed");
        assert_ne!(
            registry
                .command_metadata_registry
                .get(key)
                .map(|m| m.impl_version.clone()),
            Some(crate::metadata::Version::new(42)),
            "unregister should discard impl_version history"
        );
    }

    /// unregister04 — a command registered on *both* paths has both executors removed.
    #[test]
    fn unregister04_async_and_sync_both_removed() {
        let mut registry = CommandRegistry::<Env>::new();
        let key = CommandKey::new("", "", "both");
        registry
            .register_command(key.clone(), |_, _, _| Ok(Value::none()))
            .expect("register_command failed");
        registry
            .register_async_command(key.clone(), |_, _, _| Box::pin(async { Ok(Value::none()) }))
            .expect("register_async_command failed");

        assert!(registry.executors.contains_key(&key));
        assert!(registry.async_executors.contains_key(&key));

        assert!(registry.unregister(key.clone()));

        assert!(
            !registry.executors.contains_key(&key),
            "sync executor survived"
        );
        assert!(
            !registry.async_executors.contains_key(&key),
            "async executor survived"
        );
    }
}
