#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(warnings)]

use std::collections::HashMap;
use std::fmt::{format, Debug};
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use nom::Err;

use crate::command_metadata::{self, CommandKey, CommandMetadata, CommandMetadataRegistry};
use crate::context::{Context, Environment};
use crate::error::{Error, ErrorType};
use crate::plan::{ParameterValue, ResolvedParameterValues};
use crate::query::{Position, Query};
use crate::state::State;
use crate::value::ValueInterface;

/// Encapsulates the action parameters, that are passed to the command
/// when it is executed.
#[derive(Debug)]
pub struct CommandArguments<E:Environment> {
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
            let value = v.clone();
            return T::try_from((*value).clone()); // TODO: the clone is not necessary, it should be able to borrow
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
pub trait PayloadType: Clone + Send + Sync + 'static {}

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
        context.get_payload_clone().ok_or(Error::general_error(format!(
            "No payload in context for injected parameter {}", name
        )))
    }
}

#[async_trait]
pub trait CommandExecutor<E:Environment>:
    Send + Sync
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

pub struct CommandRegistry<E: Environment> {
    executors: HashMap<
        CommandKey,
        Arc<
            Box<
                dyn (Fn(&State<E::Value>, CommandArguments<E>, Context<E>) -> Result<E::Value, Error>)
                    + Send
                    + Sync
                    + 'static,
            >,
        >,
    >,
    async_executors: HashMap<
        CommandKey,
        Arc<
            Box<
                dyn (Fn(
                    State<E::Value>,
                    CommandArguments<E>,
                    Context<E>,
                ) -> std::pin::Pin<
                    Box<dyn core::future::Future<Output = Result<E::Value, Error>> + Send + 'static>,
                >) + Send
                    + Sync
                    + 'static,
            >,
        >,
    >,
    pub command_metadata_registry: CommandMetadataRegistry,
    payload: PhantomData<E>, // TODO: Remove if not needed
}

impl<E: Environment> CommandRegistry<E> {
    pub fn new() -> Self {
        CommandRegistry {
            //executors: HashMap::new(),
            executors: HashMap::new(),
            async_executors: HashMap::new(),
            command_metadata_registry: CommandMetadataRegistry::new(),
            payload: PhantomData::default(),
        }
    }
    pub fn register_command<K, F>(&mut self, key: K, f: F) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
        F: (Fn(&State<E::Value>, CommandArguments<E>, Context<E>) -> Result<E::Value, Error>) + Sync + Send + 'static,
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
            ) -> std::pin::Pin<
                Box<dyn core::future::Future<Output = Result<E::Value, Error>> + Send + 'static>,
            >) + Sync
            + Send
            + 'static,
    {
        let key = key.into();
        let command_metadata = CommandMetadata::from_key(key.clone());
        self.command_metadata_registry
            .add_command(&command_metadata);

        let bf: Arc<
            Box<
                dyn (Fn(
                        State<E::Value>,
                        CommandArguments<E>,
                        Context<E>,
                    ) -> std::pin::Pin<
                        Box<dyn core::future::Future<Output = Result<E::Value, Error>> + Send + 'static>,
                    >) + Send
                    + Sync
                    + 'static,
            >,
        > = Arc::new(Box::new(f));
        self.async_executors.insert(key.clone(), bf.clone());
        Ok(self.command_metadata_registry.get_mut(key).unwrap())
    }
}

#[async_trait]
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
    use crate::assets::AssetManager;
    use crate as liquers_core;
    use crate::command_metadata::CommandKey;
    use crate::commands::{CommandArguments, CommandRegistry};
    use crate::context::SimpleEnvironment;
    use crate::state::State;
    use crate::value::Value;
    use liquers_macro::*;


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
        register_command!(cr, fn greet(state, greeting: String) -> result).expect("register_command failed");

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
