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
use crate::context::{ActionContext, Context, ContextInterface, EnvRef, Environment};
use crate::error::{Error, ErrorType};
use crate::plan::{ParameterValue, ResolvedParameterValues};
use crate::query::{Position, Query};
use crate::state::State;
use crate::value::ValueInterface;

pub struct NoInjection;
pub struct NGNoInjection;

/// Encapsulates the action parameters, that are passed to the command
/// when it is executed.
pub struct CommandArguments {
    pub parameters: ResolvedParameterValues,
    pub action_position: Position,
    pub argument_number: usize,
}

/// Encapsulates the action parameters, that are passed to the command
/// when it is executed.
#[derive(Debug, Clone)]
pub struct NGCommandArguments<V: ValueInterface> {
    pub parameters: ResolvedParameterValues,
    pub values: Vec<Option<Arc<V>>>,
    pub action_position: Position,
    pub argument_number: usize,
}

impl CommandArguments {
    pub fn new(parameters: ResolvedParameterValues) -> Self {
        CommandArguments {
            parameters,
            action_position: Position::unknown(),
            argument_number: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.parameters.0.len()
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

    pub fn get<T: FromParameterValue<T, E>, E: Environment>(
        &mut self,
        context: &impl ContextInterface<E>,
    ) -> Result<T, Error> {
        let p = self.pop_parameter()?;
        if p.is_injected() {
            return Err(Error::general_error(
                "Inconsistent parameter type - injected found, value expected".to_owned(),
            ));
        }
        T::from_parameter_value(p, context)
    }

    /// Returns the injected parameter as a value of type T
    pub fn get_injected<T: InjectedFromContext<T, E>, E: Environment>(
        &mut self,
        name: &str,
        context: &impl ContextInterface<E>,
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

    pub fn get<T: NGFromParameterValue<T> + TryFrom<V, Error = Error>>(
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

pub trait FromParameterValue<T, E: Environment> {
    fn from_parameter_value(
        param: &ParameterValue,
        context: &impl ContextInterface<E>,
    ) -> Result<T, Error>;
}

pub trait NGFromParameterValue<T> {
    fn from_parameter_value(param: &ParameterValue) -> Result<T, Error>;
}

/// Macro to simplify the implementation of the FromParameterValue trait
macro_rules! impl_from_parameter_value {
    ($t:ty, $jsonvalue_to_opt:expr, $stateval_to_res:ident) => {
        impl<E: Environment> FromParameterValue<$t, E> for $t {
            fn from_parameter_value(
                param: &ParameterValue,
                context: &impl ContextInterface<E>,
            ) -> Result<$t, Error> {
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
                    if let Some(link) = param.link().as_ref() {
                        let state = context.evaluate_dependency(link)?;
                        return <E as Environment>::Value::$stateval_to_res(
                            &*(state.read().unwrap()),
                        )
                        .map_err(|e| e.with_query(link));
                    } else {
                        return Err(Error::conversion_error_with_message(
                            param,
                            stringify!($t),
                            "String or link parameter value expected",
                        )); // TODO: check none
                    }
                }
            }
        }
    };
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

impl_from_parameter_value!(
    String,
    (|p: &serde_json::Value| p.as_str().map(|s| s.to_owned())),
    try_into_string
);
impl_ng_from_parameter_value!(
    String,
    (|p: &serde_json::Value| p.as_str().map(|s| s.to_owned()))
);
impl_from_parameter_value!(i64, |p: &serde_json::Value| p.as_i64(), try_into_i64);
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
impl_from_parameter_value!(f64, |p: &serde_json::Value| p.as_f64(), try_into_f64);
impl_ng_from_parameter_value!(f64, |p: &serde_json::Value| p.as_f64());
impl_ng_from_parameter_value!(f32, |p: &serde_json::Value| p.as_f64().map(|x| x as f32));
impl_from_parameter_value!(
    Option<i64>,
    |p: &serde_json::Value| {
        if p.is_null() {
            Some(None)
        } else {
            p.as_i64().map(Some)
        }
    },
    try_into_i64_option
);
impl_ng_from_parameter_value!(Option<i64>, |p: &serde_json::Value| {
    if p.is_null() {
        Some(None)
    } else {
        p.as_i64().map(Some)
    }
});
impl_from_parameter_value!(
    Option<f64>,
    |p: &serde_json::Value| {
        if p.is_null() {
            Some(None)
        } else {
            p.as_f64().map(Some)
        }
    },
    try_into_f64_option
);
impl_ng_from_parameter_value!(Option<f64>, |p: &serde_json::Value| {
    if p.is_null() {
        Some(None)
    } else {
        p.as_f64().map(Some)
    }
});
impl_from_parameter_value!(bool, |p: &serde_json::Value| p.as_bool(), try_into_bool);
impl_ng_from_parameter_value!(bool, |p: &serde_json::Value| p.as_bool());
/*
impl<E: Environment> FromParameterValue<Vec<String>, E> for Vec<String> {
    fn from_parameter_value(
        param: &ParameterValue,
        context: &impl ContextInterface<E>,
    ) -> Result<Vec<String>, Error> {
        fn from_json_value(p: &serde_json::Value) -> Result<Vec<String>, Error> {
            match p {
                serde_json::Value::Null => Ok(vec!["".to_owned()]),
                serde_json::Value::Bool(b) => Ok(vec![if *b {"true".to_owned()} else {"false".to_owned()}]),
                serde_json::Value::Number(n) => Ok(vec![n.to_string()]),
                serde_json::Value::String(s) => Ok(vec![s.to_owned()]),
                serde_json::Value::Array(a) => Ok(a.iter().map(|v| v.to_string()).collect()),
                serde_json::Value::Object(o) => Err(Error::conversion_error_with_message(
                    "Object",
                    "Vec<String>",
                    "Array of strings expected",
                )),
            }
        }
        let from_link = |link:&Query| -> Result<Vec<String>, Error> {
            let state = context.evaluate_dependency(link)?;
            from_json_value(&(state.data.try_into_json_value()?))
        };
        match param{
            ParameterValue::DefaultValue(v) => return from_json_value(v),
            ParameterValue::DefaultLink(link) => return from_link(link),
            ParameterValue::ParameterValue(v, _) => return from_json_value(v),
            ParameterValue::ParameterLink(link, _) =>  return from_link(link),
            ParameterValue::EnumLink(link, _) => return from_link(link),
            ParameterValue::MultipleParameters(p) => {
                return p.iter().map(|pp| String::from_parameter_value(pp, context)).collect();
            }
            ParameterValue::Injected => return Err(Error::general_error("Injected parameter not allowed".to_owned())),
            ParameterValue::None => return Err(Error::general_error("None parameter not allowed".to_owned())),
        }
    }
}
*/

/*
impl<E: Environment> FromParameterValue<E::Value, E> for E::Value {
    fn from_parameter_value(
        param: &ParameterValue,
        context: &impl ContextInterface<E>,
    ) -> Result<E::Value, Error> {
        Ok(E::Value::none())
    }
}
*/

impl<E: Environment> FromParameterValue<Vec<E::Value>, E> for Vec<E::Value> {
    fn from_parameter_value(
        param: &ParameterValue,
        context: &impl ContextInterface<E>,
    ) -> Result<Vec<E::Value>, Error> {
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
        let from_link = |link: &Query| -> Result<E::Value, Error> {
            let state = context.evaluate_dependency(link)?;
            if state.is_error()? {
                return Err(Error::general_error("Error in link".to_owned()).with_query(link));
            }
            let data = state.data.read().unwrap();
            Ok(data.clone())
        };

        match param {
            ParameterValue::DefaultValue(_, v) => return from_json_value(v),
            ParameterValue::OverrideValue(_, v) => return from_json_value(v),
            ParameterValue::DefaultLink(_, link) => return Ok(vec![from_link(link)?]),
            ParameterValue::OverrideLink(_, link) => return Ok(vec![from_link(link)?]),
            ParameterValue::ParameterValue(_, v, pos) => {
                return from_json_value(v).map_err(|e| e.with_position(pos))
            }
            ParameterValue::ParameterLink(_, link, pos) => {
                return Ok(vec![from_link(link).map_err(|e| e.with_position(pos))?])
            }
            ParameterValue::EnumLink(_, link, pos) => {
                return Ok(vec![from_link(link).map_err(|e| e.with_position(pos))?])
            }
            ParameterValue::MultipleParameters(p) => {
                let mut v = Vec::new();
                for pp in p.iter() {
                    v.push(match pp {
                        ParameterValue::DefaultValue(_, value) => {
                            E::Value::try_from_json_value(value)?
                        }
                        ParameterValue::OverrideValue(_, value) => {
                            E::Value::try_from_json_value(value)?
                        }
                        ParameterValue::DefaultLink(_, query) => from_link(query)?,
                        ParameterValue::OverrideLink(_, query) => from_link(query)?,
                        ParameterValue::ParameterValue(_, value, position) => {
                            E::Value::try_from_json_value(value)
                                .map_err(|e| e.with_position(position))?
                        }
                        ParameterValue::ParameterLink(_, query, position) => {
                            from_link(query).map_err(|e| e.with_position(position))?
                        }
                        ParameterValue::EnumLink(_, query, position) => {
                            from_link(query).map_err(|e| e.with_position(position))?
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
                        ParameterValue::Placeholder(name) => {
                            return Err(Error::unexpected_error(format!(
                                "Placeholder parameters ({name}) not allowed inside multi-parameter"
                            )))
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
            ParameterValue::Placeholder(name) => {
                return Err(Error::general_error(format!(
                    "Placeholder parameters ({name}) not allowed"
                )))
            }
        }
        //Ok(vec![E::Value::none()])
    }
}

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

pub trait InjectedFromContext<T, E: Environment> {
    fn from_context(name: &str, context: &impl ContextInterface<E>) -> Result<T, Error>;
}

pub trait NGInjectedFromContext<T, P, V: ValueInterface> {
    fn from_context(name: &str, context: &impl ActionContext<P, V>) -> Result<T, Error>;
}

// TODO: Use CommandKey instead of realm, namespace, command_name
pub trait CommandExecutor<ER: EnvRef<E>, E: Environment, V: ValueInterface> {
    fn execute(
        &self,
        realm: &str,
        namespace: &str,
        command_name: &str,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error>;
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

pub struct CommandRegistry<ER, E, V: ValueInterface>
where
    V: ValueInterface,
    E: Environment,
    ER: EnvRef<E>,
{
    executors: HashMap<
        CommandKey,
        Arc<
            Box<
                dyn (Fn(&State<V>, &mut CommandArguments, Context<ER, E>) -> Result<V, Error>)
                    + Send
                    + Sync
                    + 'static,
            >,
        >,
    >,
    pub command_metadata_registry: CommandMetadataRegistry,
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

impl<ER, E, V> CommandRegistry<ER, E, V>
where
    V: ValueInterface,
    E: Environment,
    ER: EnvRef<E>,
{
    pub fn new() -> Self {
        CommandRegistry {
            //executors: HashMap::new(),
            executors: HashMap::new(),
            command_metadata_registry: CommandMetadataRegistry::new(),
        }
    }
    pub fn register_command<K, F>(&mut self, key: K, f: F) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
        F: (Fn(&State<V>, &mut CommandArguments, Context<ER, E>) -> Result<V, Error>)
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

impl<ER, E, V> CommandExecutor<ER, E, V> for CommandRegistry<ER, E, V>
where
    V: ValueInterface,
    E: Environment,
    ER: EnvRef<E>,
{
    fn execute(
        &self,
        realm: &str,
        namespace: &str,
        command_name: &str,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error> {
        let key = CommandKey::new(realm, namespace, command_name);
        if let Some(command) = self.executors.get(&key) {
            command(state, arguments, context)
        } else {
            Err(Error::unknown_command_executor(
                realm,
                namespace,
                command_name,
                &arguments.action_position,
            ))
        }
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

/*
#[macro_export]
macro_rules! command_wrapper_typed {
    ($name:ident
        ($($argname:ident:$argtype:ty),*)<$V:ty, $ER:ty, $E:ty>) => {
            //stringify!(
            |state: &State<$V>, arguments: &mut CommandArguments, context: Context<$ER, $E>| -> Result<$V, Error> {
                $(
                    let $argname: $argtype = arguments.get::<$argtype, $ER, $E>(&context)?;
                )*
                if arguments.all_parameters_used(){
                    $name($($argname),*)
                }
                else{
                        Err(Error::new(
                            ErrorType::TooManyParameters,
                            format!("Too many parameters: {}; {} excess parameters found", arguments.len(), arguments.excess_parameters()),
                        )
                        .with_position(&arguments.parameter_position()))
                }
            }
        //)
        };
}
*/

#[macro_export]
macro_rules! command_wrapper_parameter_name {
    ($cxpar:ident, $statepar:ident, context) => {
        $cxpar
    };
    ($cxpar:ident, $statepar:ident, state) => {
        $statepar
    };
    ($cxpar:ident, $statepar:ident, injected $argname:ident) => {
        $argname
    };
    ($cxpar:ident, $statepar:ident, multiple $argname:ident) => {
        $argname
    };
    ($cxpar:ident, $statepar:ident, $argname:ident) => {
        $argname
    };
}

#[macro_export]
macro_rules! command_wrapper_parameter_assignment {
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, context) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, context) = $context;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, state) => {
        //let command_wrapper_parameter_name!($cxpar, $statepar, state) = $state;
        ;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, injected $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, injected $argname): $argtype =
            $arguments.get_injected(stringify!($argname), &$context)?;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, $argname): $argtype =
            $arguments.get(&$context)?;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, multiple $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, $argname): std::vec::Vec<$argtype> =
            $arguments.get(&$context)?;
    };
    /*
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, multiple $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, $argname): std::vec::Vec<$argtype> =
            $arguments.get(&$context)?;
    };
    */
}

#[macro_export]
macro_rules! ng_command_wrapper_parameter_assignment {
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, context) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, context) = $context;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, state) => {
        //let command_wrapper_parameter_name!($cxpar, $statepar, state) = $state;
        ;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, injected $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, injected $argname): $argtype =
            $arguments.get_injected(stringify!($argname), &$context)?;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, $argname): $argtype =
            $arguments.get()?;
    };
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, multiple $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, $argname): std::vec::Vec<$argtype> =
            $arguments.get()?;
    };
    /*
    ($cxpar:ident, $statepar:ident, $arguments:ident, $state:ident, $context:ident, multiple $argname:ident:$argtype:ty) => {
        let $crate::command_wrapper_parameter_name!($cxpar, $statepar, $argname): std::vec::Vec<$argtype> =
            $arguments.get(&$context)?;
    };
    */
}

#[macro_export]
macro_rules! command_wrapper {
    ($name:ident
        ($($argname:ident $($argname2:ident)? $(:$argtype:ty)?),*)) => {
            //stringify!(
            |state, arguments, context|{
                let cx_wrapper_parameter = 0;
                //let state_wrapper_parameter = 0;
                $(
                    $crate::command_wrapper_parameter_assignment!(cx_wrapper_parameter, state, arguments, state, context, $argname $($argname2)? $(:$argtype)?);
                )*
                if arguments.all_parameters_used(){
                    Ok($name($($crate::command_wrapper_parameter_name!(cx_wrapper_parameter, state, $argname $($argname2)?)),*)?.into())
                }
                else{
                        Err($crate::error::Error::new(
                            $crate::error::ErrorType::TooManyParameters,
                            format!("Too many parameters: {}; {} excess parameters found", arguments.len(), arguments.excess_parameters()),
                        )
                        .with_position(&arguments.parameter_position()))
                }
            }
        //)
        };
}

#[macro_export]
macro_rules! ng_command_wrapper {
    ($name:ident
        ($($argname:ident $($argname2:ident)? $(:$argtype:ty)?),*)) => {
            //stringify!(
            |state, arguments, context|{
                let cx_wrapper_parameter = 0;
                //let state_wrapper_parameter = 0;
                $(
                    $crate::ng_command_wrapper_parameter_assignment!(cx_wrapper_parameter, state, arguments, state, context, $argname $($argname2)? $(:$argtype)?);
                )*
                if arguments.all_parameters_used(){
                    Ok($name($($crate::command_wrapper_parameter_name!(cx_wrapper_parameter, state, $argname $($argname2)?)),*)?.into())
                }
                else{
                        Err($crate::error::Error::new(
                            $crate::error::ErrorType::TooManyParameters,
                            format!("Too many parameters: {}; {} excess parameters found", arguments.len(), arguments.excess_parameters()),
                        )
                        .with_position(&arguments.parameter_position()))
                }
            }
        //)
        };
}

//TODO: make sure that the macro export is done correctly
#[macro_export]
macro_rules! register_command {
    ($cr:ident, $name:ident ($( $argname:ident $($argname2:ident)? $(:$argtype:ty)?),*)) => {
        {
        let reg_command_metadata = $cr.register_command(stringify!($name), $crate::command_wrapper!($name($($argname $($argname2)? $(:$argtype)?),*)))?
        .with_name(stringify!($name));
        $(
            $crate::register_command!(@arg reg_command_metadata $argname $($argname2)? $(:$argtype)?);
        )*
    }
    };
    (@arg $cm:ident state) =>{
        $cm.with_state_argument($crate::command_metadata::ArgumentInfo::argument("state"));
    };
    (@arg $cm:ident context) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument("context").set_injected());
    };
    (@arg $cm:ident injected $argname:ident:String) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::string_argument(stringify!($argname)).set_injected());
    };
    (@arg $cm:ident $argname:ident:String) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::string_argument(stringify!($argname)));
    };
    (@arg $cm:ident multiple $argname:ident:$argtype:ty) =>{
        /*
        if stringify!($argtype) == "String" {
            println!("multiple String arguments: {}", stringify!($argname));
            $cm.with_argument($crate::command_metadata::ArgumentInfo::string_argument(stringify!($argname)).set_multiple());
        }
        else{
            $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_multiple());
        }
        */
        println!("multiple Any arguments: {}", stringify!($argname));
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_multiple());
    };
    (@arg $cm:ident injected $argname:ident:$argtype:ty) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_injected());
     };
     (@arg $cm:ident $argname:ident:$argtype:ty) =>{
       $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)));
    };

}

#[macro_export]
macro_rules! ng_register_command {
    ($cr:ident, $name:ident ($( $argname:ident $($argname2:ident)? $(:$argtype:ty)?),*)) => {
        {
        let reg_command_metadata = $cr.register_command(stringify!($name), $crate::ng_command_wrapper!($name($($argname $($argname2)? $(:$argtype)?),*)))?
        .with_name(stringify!($name));
        $(
            $crate::ng_register_command!(@arg reg_command_metadata $argname $($argname2)? $(:$argtype)?);
        )*
    }
    };
    (@arg $cm:ident state) =>{
        $cm.with_state_argument($crate::command_metadata::ArgumentInfo::argument("state"));
    };
    (@arg $cm:ident context) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument("context").set_injected());
    };
    (@arg $cm:ident injected $argname:ident:String) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::string_argument(stringify!($argname)).set_injected());
    };
    (@arg $cm:ident $argname:ident:String) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::string_argument(stringify!($argname)));
    };
    (@arg $cm:ident multiple $argname:ident:$argtype:ty) =>{
        /*
        if stringify!($argtype) == "String" {
            println!("multiple String arguments: {}", stringify!($argname));
            $cm.with_argument($crate::command_metadata::ArgumentInfo::string_argument(stringify!($argname)).set_multiple());
        }
        else{
            $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_multiple());
        }
        */
        println!("multiple Any arguments: {}", stringify!($argname));
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_multiple());
    };
    (@arg $cm:ident injected $argname:ident:$argtype:ty) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_injected());
     };
     (@arg $cm:ident $argname:ident:$argtype:ty) =>{
       $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)));
    };

}

#[cfg(test)]
mod tests {
    use crate::{context::StatEnvRef, metadata::MetadataRecord, query::Key};

    use super::*;
    use crate::{state, value::Value};

    struct TestExecutor;
    struct NGTestExecutor;
    impl CommandExecutor<StatEnvRef<NoInjection>, NoInjection, Value> for TestExecutor {
        fn execute(
            &self,
            realm: &str,
            namespace: &str,
            command_name: &str,
            state: &State<Value>,
            arguments: &mut CommandArguments,
            context: Context<StatEnvRef<NoInjection>, NoInjection>,
        ) -> Result<Value, Error> {
            assert_eq!(realm, "");
            assert_eq!(namespace, "");
            assert_eq!(command_name, "test");
            assert!(state.is_none());
            Ok(Value::from_string("Hello".into()))
        }
    }
    struct TrivialContext;
    impl ActionContext<NoInjection, Value> for TrivialContext {
        fn borrow_payload(&self) -> &NoInjection {
            panic!("borrow_payload not needed")
        }

        fn clone_payload(&self) -> NoInjection {
            NoInjection
        }

        fn evaluate_dependency<Q: crate::query::TryToQuery>(
            &self,
            query: Q,
        ) -> Result<State<Value>, Error> {
            panic!("evaluate_dependency not needed")
        }

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            Arc::new(Box::new(crate::store::NoStore))
        }

        fn get_metadata(&self) -> crate::metadata::MetadataRecord {
            MetadataRecord::new()
        }

        fn set_filename(&self, filename: String) {}

        fn debug(&self, message: &str) {}

        fn info(&self, message: &str) {}

        fn warning(&self, message: &str) {}

        fn error(&self, message: &str) {}

        fn clone_context(&self) -> Self {
            TrivialContext
        }
        
        fn get_cwd_key(&self) -> Option<Key> {
            None
        }
        
        fn set_cwd_key(&mut self, key: std::option::Option<Key>) {
        }
    }

    impl NGCommandExecutor<NoInjection, Value, TrivialContext> for NGTestExecutor {
        fn execute(
            &self,
            key: &CommandKey,
            state: &State<Value>,
            arguments: &mut NGCommandArguments<Value>,
            context: TrivialContext,
        ) -> Result<Value, Error> {
            assert_eq!(key.realm, "");
            assert_eq!(key.namespace, "");
            assert_eq!(key.name, "test");
            assert!(state.is_none());
            Ok(Value::from_string("Hello".into()))
        }
    }
    #[test]
    fn test_command_arguments() {
        let mut rp = ResolvedParameterValues::new();
        rp.0.push(ParameterValue::ParameterValue(
            "arg".into(),
            "Hello".into(),
            Position::unknown(),
        ));
        let injection = Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(rp);
        let s: String = ca.get(&mut context).unwrap();
        assert_eq!(s, "Hello");
    }
    #[test]
    fn test_ng_command_arguments() {
        let mut rp = ResolvedParameterValues::new();
        rp.0.push(ParameterValue::ParameterValue(
            "arg".into(),
            "Hello".into(),
            Position::unknown(),
        ));
        let mut ca: NGCommandArguments<Value> = NGCommandArguments::new(rp);
        let s: String = ca.get().unwrap();
        assert_eq!(s, "Hello");
    }
    #[test]
    fn test_command_executor() -> Result<(), Error> {
        let injection = Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(ResolvedParameterValues::new());
        let state = State::new();
        let s = TestExecutor
            .execute("", "", "test", &state, &mut ca, context)
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello");
        Ok(())
    }

    #[test]
    fn test_ng_command_executor() -> Result<(), Error> {
        let mut context = TrivialContext;
        let mut ca = NGCommandArguments::new(ResolvedParameterValues::new());
        let state = State::new();
        let s = NGTestExecutor
            .execute(&CommandKey::new("", "", "test"), &state, &mut ca, context)
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello");
        Ok(())
    }

    #[test]
    fn test_macro_wrapper() -> Result<(), Error> {
        #![feature(trace_macros)]
        //println!("Macro: {}", make_command_wrapper!(Test1, a:A, b:B));
        //println!("Command wrapper: {}", command_wrapper!(xx-yy-zz(a:A, b:B)));
        //trace_macros!(true);
        fn test1() -> Result<Value, Error> {
            Ok(Value::from_string("Hello1".into()))
        }
        fn test2(context: Context<StatEnvRef<NoInjection>, NoInjection>) -> Result<Value, Error> {
            context.info("test2 called");
            Ok(Value::from_string(format!("Hello2")))
        }
        let mut cr = CommandRegistry::<StatEnvRef<NoInjection>, NoInjection, Value>::new();
        //cr.register_command("test1", command_wrapper1!(test1()<Value, StatEnvRef<NoInjection>, NoInjection>))?;
        cr.register_command("test1a", command_wrapper!(test1()))?;
        register_command!(cr, test1());
        register_command!(cr, test2(context));
        serde_yaml::to_writer(std::io::stdout(), &cr.command_metadata_registry)
            .expect("cr yaml error");

        //trace_macros!(false);

        let injection = Box::leak(Box::new(NoInjection));

        let envref = StatEnvRef(injection);
        let state = State::new();
        let mut context: Context<StatEnvRef<NoInjection>, NoInjection> = Context::new(envref);
        let mut ca = CommandArguments::new(ResolvedParameterValues::new());

        let s = cr
            .execute("", "", "test1a", &state, &mut ca, context.clone_context())
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello1");

        let s = cr
            .execute("", "", "test1", &state, &mut ca, context.clone_context())
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello1");
        assert_eq!(context.get_metadata().log.len(), 0);

        let s = cr
            .execute("", "", "test2", &state, &mut ca, context.clone_context())
            .unwrap();

        //        serde_yaml::to_writer(std::io::stdout(), &context.get_metadata()).expect("yaml error");
        assert_eq!(context.get_metadata().log[0].message, "test2 called");

        Ok(())
    }

    #[test]
    fn test_ng_macro_wrapper() -> Result<(), Error> {
        #![feature(trace_macros)]
        //println!("Macro: {}", make_command_wrapper!(Test1, a:A, b:B));
        //println!("Command wrapper: {}", command_wrapper!(xx-yy-zz(a:A, b:B)));
        //trace_macros!(true);
        fn test1() -> Result<Value, Error> {
            Ok(Value::from_string("Hello1".into()))
        }
        fn test2(context: TrivialContext) -> Result<Value, Error> {
            context.info("test2 called");
            Ok(Value::from_string(format!("Hello2")))
        }
        let mut cr = NGCommandRegistry::<NoInjection, Value, TrivialContext>::new();
        //cr.register_command("test1", command_wrapper1!(test1()<Value, StatEnvRef<NoInjection>, NoInjection>))?;
        cr.register_command("test1a", ng_command_wrapper!(test1()))?;
        ng_register_command!(cr, test1());
        ng_register_command!(cr, test2(context));
        serde_yaml::to_writer(std::io::stdout(), &cr.command_metadata_registry)
            .expect("cr yaml error");

        //trace_macros!(false);

        let state = State::new();
        let mut ca = NGCommandArguments::new(ResolvedParameterValues::new());

        let s = cr
            .execute(
                &CommandKey::new("", "", "test1a"),
                &state,
                &mut ca,
                TrivialContext,
            )
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello1");

        let s = cr
            .execute(
                &CommandKey::new("", "", "test1"),
                &state,
                &mut ca,
                TrivialContext,
            )
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello1");
        //assert_eq!(context.get_metadata().log.len(), 0);

        let s = cr
            .execute(
                &CommandKey::new("", "", "test2"),
                &state,
                &mut ca,
                TrivialContext,
            )
            .unwrap();

        //        serde_yaml::to_writer(std::io::stdout(), &context.get_metadata()).expect("yaml error");
        //assert_eq!(context.get_metadata().log[0].message, "test2 called");

        Ok(())
    }
}
