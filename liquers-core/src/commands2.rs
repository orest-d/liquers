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
use crate::context2::{ActionContext};
use crate::error::{Error, ErrorType};
use crate::plan::{ParameterValue, ResolvedParameterValues};
use crate::query::{Position, Query};
use crate::state::State;
use crate::value::ValueInterface;

/// Encapsulates the action parameters, that are passed to the command
/// when it is executed.
#[derive(Debug, Clone)]
pub struct NGCommandArguments<V: ValueInterface> {
    pub parameters: ResolvedParameterValues,
    pub values: Vec<Option<Arc<V>>>,
    pub action_position: Position,
    pub argument_number: usize,
}

impl<V: ValueInterface> NGCommandArguments<V> {
    pub fn new(parameters: ResolvedParameterValues) -> Self {
        NGCommandArguments {
            parameters,
            action_position: Position::unknown(),
            argument_number: 0,
            values: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.parameters.0.len()
    }

    pub fn pop_value(&mut self) -> Result<Option<Arc<V>>, Error> {
        if let Some(v) = self.values.get(self.argument_number) {
            self.argument_number += 1;
            if let Some(v) = v {
                Ok(Some(v.clone()))
            } else {
                Ok(None)
            }
        } else {
            Err(Error::missing_argument(
                self.argument_number,
                "?",
                &self.action_position,
            ))
        }
    }

    pub fn pop_parameter(&mut self) -> Result<&ParameterValue, Error> {
        if let Some(p) = self.parameters.0.get(self.argument_number) {
            self.argument_number += 1;
            Ok(p)
        } else {
            Err(Error::missing_argument(
                self.argument_number,
                "?",
                &self.action_position,
            ))
        }
    }

    pub fn get<T: NGFromParameterValue<T> + TryFrom<V, Error = Error>>( // TODO: BAD DESIGN, the TryFrom should not be here
        &mut self,
    ) -> Result<T, Error> {
        let argnum = self.argument_number;
        let p = self.pop_parameter()?.to_owned();
        if let Some(link) = p.link() {
            let resolved = self.values.get(argnum);
            return match resolved {
                Some(Some(v)) => {
                    let value = v.clone();
                    return T::try_from((*value).clone()); // TODO: the clone is not necessary, it should be able to borrow
                                                          /*
                                                                              if let Ok(v) = value.try_into_json_value(){
                                                                                  return T::from_parameter_value(&ParameterValue::ParameterValue(v, p.position().or(self.action_position.clone())))
                                                                              }
                                                                              return Err(Error::general_error(format!(
                                                                                  "Failed to convert link parameter {}: {}",
                                                                                  argnum,
                                                                                  link
                                                                              )).with_position(&p.position().or(self.action_position.clone())));
                                                          */
                }
                Some(None) => Err(Error::general_error(format!(
                    "Unresolved link parameter {}: {}",
                    argnum, link
                ))
                .with_position(&self.action_position)),
                None => Err(Error::general_error(format!(
                    "Unresolved link parameter {}: {} (resolved links too short: {})",
                    argnum,
                    link,
                    self.values.len()
                ))
                .with_position(&self.action_position)),
            };
        }
        if p.is_injected() {
            return Err(Error::general_error(
                "Inconsistent parameter type - injected found, value expected".to_owned(),
            )
            .with_position(&self.action_position));
        }
        T::from_parameter_value(&p)
    }
    // TODO: Implement get_value, use as a quicker way to get the value (any)
    /*
    pub fn get_value(&mut self) -> Result<Option<Arc<V>>, Error> {
        let argnum=self.argument_number;
        let p = self.pop_parameter()?.to_owned();
        let value = self.values.get(argnum);

        if let Some(link) = p.link() {
            match value {
                Some(Some(state)) => Ok(state.clone()),
                Some(None) => Err(Error::general_error(format!(
                    "Unresolved link parameter {}: {}",
                    argnum,
                    link
                )).with_position(&self.action_position)),
                None => Err(Error::general_error(format!(
                    "Unresolved link parameter {}: {} (resolved links too short: {})",
                    argnum,
                    link,
                    self.values.len()
                )).with_position(&self.action_position)),
            }
        }
        else{
            Err(Error::general_error(format!(
                "Link parameter expected, value found, parameter {}",argnum
            )).with_position(&self.action_position))
        }
    }
    */

    /// Returns the injected parameter as a value of type T
    pub fn get_injected<P, T: NGInjectedFromContext<T, P, V>>(
        &mut self,
        name: &str,
        context: &impl ActionContext<P, V>,
    ) -> Result<T, Error> {
        let p = self.pop_parameter()?;
        if !p.is_injected() {
            return Err(Error::general_error(
                "Inconsistent parameter type - value found, injected expected".to_owned(),
            ));
        }
        T::from_context(name, context)
    }

    /// Returns true if all parameters have been used
    /// This is checked during the command execution
    pub fn all_parameters_used(&self) -> bool {
        self.argument_number == self.parameters.0.len()
    }
    /// Returns the number of parameters that have not been used
    pub fn excess_parameters(&self) -> usize {
        self.parameters.len() - self.argument_number
    }
    pub fn parameter_position(&self) -> Position {
        if let Some(p) = self.parameters.0.get(self.argument_number) {
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


pub trait NGFromParameterValue<T> {
    fn from_parameter_value(param: &ParameterValue) -> Result<T, Error>;
}


/// Macro to simplify the implementation of the FromParameterValue trait
macro_rules! impl_ng_from_parameter_value {
    ($t:ty, $jsonvalue_to_opt:expr) => {
        impl NGFromParameterValue<$t> for $t {
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

impl_ng_from_parameter_value!(
    String,
    (|p: &serde_json::Value| p.as_str().map(|s| s.to_owned()))
);
impl_ng_from_parameter_value!(i64, |p: &serde_json::Value| p.as_i64());
impl_ng_from_parameter_value!(i32, |p: &serde_json::Value| p.as_i64().map(|x| x as i32));
impl_ng_from_parameter_value!(i16, |p: &serde_json::Value| p.as_i64().map(|x| x as i16));
impl_ng_from_parameter_value!(i8, |p: &serde_json::Value| p.as_i64().map(|x| x as i8));
impl_ng_from_parameter_value!(isize, |p: &serde_json::Value| p.as_i64().map(|x| x as isize));
impl_ng_from_parameter_value!(u64, |p: &serde_json::Value| p.as_i64().map(|x| x as u64));
impl_ng_from_parameter_value!(u32, |p: &serde_json::Value| p.as_i64().map(|x| x as u32));
impl_ng_from_parameter_value!(u16, |p: &serde_json::Value| p.as_i64().map(|x| x as u16));
impl_ng_from_parameter_value!(u8, |p: &serde_json::Value| p.as_i64().map(|x| x as u8));
impl_ng_from_parameter_value!(usize, |p: &serde_json::Value| p.as_i64().map(|x| x as usize));
impl_ng_from_parameter_value!(f64, |p: &serde_json::Value| p.as_f64());
impl_ng_from_parameter_value!(f32, |p: &serde_json::Value| p.as_f64().map(|x| x as f32));
impl_ng_from_parameter_value!(Option<i64>, |p: &serde_json::Value| {
    if p.is_null() {
        Some(None)
    } else {
        p.as_i64().map(Some)
    }
});
impl_ng_from_parameter_value!(Option<f64>, |p: &serde_json::Value| {
    if p.is_null() {
        Some(None)
    } else {
        p.as_f64().map(Some)
    }
});
impl_ng_from_parameter_value!(bool, |p: &serde_json::Value| p.as_bool());

impl<V: ValueInterface> NGFromParameterValue<Vec<V>> for Vec<V> {
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

pub trait NGInjectedFromContext<T, P, V: ValueInterface> {
    fn from_context(name: &str, context: &impl ActionContext<P, V>) -> Result<T, Error>;
}

#[async_trait]
pub trait NGCommandExecutor<P, V: ValueInterface, C: ActionContext<P, V> + Send + 'static>:
    Send + Sync
{
    fn execute(
        &self,
        command_key: &CommandKey,
        state: &State<V>,
        arguments: &mut NGCommandArguments<V>,
        context: C,
    ) -> Result<V, Error>;

    async fn execute_async(
        &self,
        command_key: &CommandKey,
        state: State<V>,
        mut arguments: NGCommandArguments<V>,
        context: C,
    ) -> Result<V, Error> {
        self.execute(command_key, &state, &mut arguments, context)
    }
}

pub struct NGCommandRegistry<P, V: ValueInterface, C: ActionContext<P, V>> {
    executors: HashMap<
        CommandKey,
        Arc<
            Box<
                dyn (Fn(&State<V>, &mut NGCommandArguments<V>, C) -> Result<V, Error>)
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
                        State<V>,
                        NGCommandArguments<V>,
                        C,
                    ) -> std::pin::Pin<
                        Box<
                            dyn core::future::Future<Output = Result<V, Error>>
                                + Send
                                + 'static,
                        >,
                    >) + Send
                    + Sync
                    + 'static,
            >,
        >,
    >,
    pub command_metadata_registry: CommandMetadataRegistry,
    payload: PhantomData<P>,
}

impl<P, V: ValueInterface, C: ActionContext<P, V>> NGCommandRegistry<P, V, C> {
    pub fn new() -> Self {
        NGCommandRegistry {
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
        F: (Fn(&State<V>, &mut NGCommandArguments<V>, C) -> Result<V, Error>)
            + Sync
            + Send
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
                State<V>,
                NGCommandArguments<V>,
                C,
            ) -> std::pin::Pin<
                Box<dyn core::future::Future<Output = Result<V, Error>> + Send  + 'static>,
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
                        State<V>,
                        NGCommandArguments<V>,
                        C,
                    ) -> std::pin::Pin<
                        Box<
                            dyn core::future::Future<Output = Result<V, Error>>
                                + Send
                                + 'static,
                        >,
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
impl<P: Send + Sync, V: ValueInterface, C: ActionContext<P, V> + Send + 'static>
    NGCommandExecutor<P, V, C> for NGCommandRegistry<P, V, C>
{
    fn execute(
        &self,
        key: &CommandKey,
        state: &State<V>,
        arguments: &mut NGCommandArguments<V>,
        context: C,
    ) -> Result<V, Error> {
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
        state: State<V>,
        mut arguments: NGCommandArguments<V>,
        context: C,
    ) -> Result<V, Error> {
        if let Some(command) = self.async_executors.get(&key) {
            command(state, arguments, context).await
        } else {
            self.execute(key, &state, &mut arguments, context)
        }
    }
}
