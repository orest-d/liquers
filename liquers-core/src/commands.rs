#![allow(unused_imports)]
#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

use nom::Err;

use crate::command_metadata::{self, CommandKey, CommandMetadata, CommandMetadataRegistry};
use crate::context::{Context, ContextInterface, EnvRef, Environment};
use crate::error::{Error, ErrorType};
use crate::plan::{ParameterValue, ResolvedParameterValues};
use crate::query::Position;
use crate::state::State;
use crate::value::ValueInterface;

pub struct NoInjection;


/// Encapsulates the action parameters, that are passed to the command
/// when it is executed.
pub struct CommandArguments {
    pub parameters: ResolvedParameterValues,
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
        if p.is_injected(){
            return Err(Error::general_error("Inconsistent parameter type - injected found, value expected".to_owned()));
        }
        T::from_parameter_value(p, context)
    }

    /// Returns the injected parameter as a value of type T
    pub fn get_injected<T: InjectedFromContext<T, E>, E: Environment>(
        &mut self,
        name:&str,
        context: &impl ContextInterface<E>,
    ) -> Result<T, Error> {
        let p = self.pop_parameter()?;
        if !p.is_injected(){
            return Err(Error::general_error("Inconsistent parameter type - value found, injected expected".to_owned()));
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
                    if let Some(link) = param.link() {
                        let state = context.evaluate_dependency(link)?;
                        return <E as Environment>::Value::$stateval_to_res(&*(state.data));
                    } else {
                        return Err(Error::conversion_error_with_message(
                            param,
                            "string",
                            "String parameter value expected",
                        )); // TODO: check none
                    }
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
impl_from_parameter_value!(i64, |p: &serde_json::Value| p.as_i64(), try_into_i64);
impl_from_parameter_value!(f64, |p: &serde_json::Value| p.as_f64(), try_into_f64);
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
impl_from_parameter_value!(bool, |p: &serde_json::Value| p.as_bool(), try_into_bool);


pub trait InjectedFromContext<T, E: Environment> {
    fn from_context(name:&str, context: &impl ContextInterface<E>) -> Result<T, Error>;
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

pub struct CommandRegistry<ER, E, V: ValueInterface>
where
    V: ValueInterface,
    E: Environment,
    ER: EnvRef<E>,
{
    executors: HashMap<
        CommandKey,
        Box<dyn (Fn(&State<V>, &mut CommandArguments, Context<ER, E>) -> Result<V, Error>) + Send + 'static>,
    >,
    pub command_metadata_registry: CommandMetadataRegistry,
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
        F: (Fn(&State<V>, &mut CommandArguments, Context<ER, E>) -> Result<V, Error>) + Send + 'static,
    {
        let key = key.into();
        let command_metadata = CommandMetadata::from_key(key.clone());
        self.command_metadata_registry
            .add_command(&command_metadata);
        self.executors.insert(key.clone(), Box::new(f));
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
    (@arg $cm:ident injected $argname:ident:$argtype:ty) =>{
        $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)).set_injected());
     };
     (@arg $cm:ident $argname:ident:$argtype:ty) =>{
       $cm.with_argument($crate::command_metadata::ArgumentInfo::argument(stringify!($argname)));
    };

}

#[cfg(test)]
mod tests {
    use crate::context::StatEnvRef;

    use super::*;
    use crate::{state, value::Value};

    struct TestExecutor;
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
            assert!(state.data.is_none());
            Ok(Value::from_string("Hello".into()))
        }
    }
    #[test]
    fn test_command_arguments() {
        let mut rp = ResolvedParameterValues::new();
        rp.0.push(ParameterValue::ParameterValue("Hello".into(), Position::unknown()));
        let injection = Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(rp);
        let s: String = ca.get(&mut context).unwrap();
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
}
