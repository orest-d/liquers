#![allow(unused_imports)]
#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

use nom::Err;

use crate::command_metadata::{self, CommandKey, CommandMetadata, CommandMetadataRegistry};
use crate::context::{self, Context, EnvRef, Environment};
use crate::error::{Error, ErrorType};
use crate::plan::{Parameter, ResolvedParameters};
use crate::query::Position;
use crate::state::State;
use crate::value::ValueInterface;


pub struct NoInjection;


pub struct CommandArguments {
    pub parameters: ResolvedParameters,
    pub action_position: Position,
    pub argument_number: usize,
}

impl CommandArguments{
    pub fn new(parameters: ResolvedParameters) -> Self {
        CommandArguments {
            parameters,
            action_position: Position::unknown(),
            argument_number: 0,
        }
    }
    
    pub fn has_no_parameters(&self) -> bool {
        self.parameters.parameters.is_empty()
    }
    pub fn len(&self) -> usize {
        self.parameters.parameters.len()
    }
    pub fn get_parameter(&mut self)->Result<&Parameter,Error>{
        if let Some(p) = self.parameters.parameters.get(self.argument_number) {
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
    pub fn get<T: FromCommandArguments<T, ER, E>,ER:EnvRef<E>,E:Environment>(&mut self, context:&Context<ER,E>) -> Result<T, Error> {
        T::from_arguments(self, context)
    }
    /// Returns true if all parameters have been used
    /// This is checked during the command execution
    pub fn all_parameters_used(&self) -> bool {
        self.argument_number == self.parameters.parameters.len()
    }

    /// Returns the number of parameters that have not been used
    pub fn excess_parameters(&self) -> usize {
        self.parameters.parameters.len() - self.argument_number
    }
    pub fn parameter_position(&self) -> Position {
        if let Some(p) = self.parameters.parameters.get(self.argument_number) {
            p.position.clone()
        } else {
            self.action_position.clone()
        }
    }
}

/// Command trait
/// This trait encapsulates a command that can be executed,
/// typically a function
/// Command depends on three traits:
/// - value V encapsulating the main value type
/// - environment E encapsulating the environment
/// - EnvRef<E> specifies how the environment is referenced
pub trait Command<ER: EnvRef<E>, E:Environment, V: ValueInterface> {
    fn execute(
        &self,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error>;

    /// Returns the default metadata of the command
    /// This may be modified or overriden inside the command registry
    fn command_metadata(&self) -> Option<CommandMetadata> {
        None
    }
}

/*
impl<F, R, Injection, V> Command<Injection, V> for F
where
    F: Fn() -> R,
    V: ValueInterface + From<R>,
{
    fn execute(
        &mut self,
        _state: &State<V>,
        arguments: &mut CommandArguments<'_, Injection>,
    ) -> Result<V, Error> {
        if arguments.has_no_parameters() {
            let result = self();
            Ok(V::from(result))
        } else {
            Err(
                Error::new(ErrorType::TooManyParameters, format!("Too many parameters ({}); none expected", arguments.len()))
                    .with_position(&arguments.action_position),
            )
        }
    }
}
*/

#[derive(Clone)]
pub struct Command0<R, F>
where
    F: Fn() -> R,
{
    f: F,
    result: PhantomData<R>,
}

impl<R, F> From<F> for Command0<R, F>
where
    F: Fn() -> R,
{
    fn from(f: F) -> Self {
        Command0 {
            f,
            result: Default::default(),
        }
    }
}

impl<F, ER, E, V, R> Command<ER, E, V> for Command0<R, F>
where
    F: Fn() -> R,
    R:Clone,
    F:Clone,
    V: ValueInterface + From<R>,
    E:Environment,
    ER:EnvRef<E>,
{
    fn execute(
        &self,
        _state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error> {
        if arguments.has_no_parameters() {
            let result = (self.f)();
            Ok(V::from(result))
        } else {
            Err(Error::new(
                ErrorType::TooManyParameters,
                format!("Too many parameters ({}) - none expected", arguments.len()),
            )
            .with_position(&arguments.action_position))
        }
    }
}

#[derive(Clone)]
pub struct Command0c<ER, E, R, F>
where
    F: Fn(Context<ER,E>) -> R,
    E:Environment,
    ER:EnvRef<E>,
{
    f: F,
    envref: PhantomData<ER>,
    environment: PhantomData<E>,
    result: PhantomData<R>,
}

impl<ER, E, R, F> From<F> for Command0c<ER, E, R, F>
where
    F: Fn(Context<ER,E>) -> R,
    E:Environment,
    ER:EnvRef<E>,
{
    fn from(f: F) -> Self {
        Command0c {
            f,
            envref: Default::default(),
            environment: Default::default(),
            result: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct Command1<S, R, F>
where
    F: Fn(S) -> R,
{
    f: F,
    state: PhantomData<S>,
    result: PhantomData<R>,
}

impl<S, R, F> From<F> for Command1<S, R, F>
where
    F: Fn(S) -> R,
{
    fn from(f: F) -> Self {
        Command1 {
            f,
            state: Default::default(),
            result: Default::default(),
        }
    }
}

impl<F, ER, E, V, R> Command<ER, E, V> for Command1<&State<V>, R, F>
where
    F: Fn(&State<V>) -> R,
    R:Clone,
    F:Clone,
    V: ValueInterface + From<R>,
    E:Environment,
    ER:EnvRef<E>,
{
    fn execute(
        &self,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error> {

        if !arguments.all_parameters_used(){
            Err(Error::new(
                ErrorType::TooManyParameters,
                format!("Too many parameters: {} - no parameters expected, {} excess parameters found", arguments.len(), arguments.excess_parameters()),
            )
            .with_position(&arguments.parameter_position()))
        }
        else{
            let result = (self.f)(state);
            Ok(V::from(result))
        }
    }
}
/*
impl<F: Fn(&State<V>) -> R + 'static, Injection, V, R> From<F> for Box<dyn Command<Injection, V>>
where
R: 'static,
V: ValueInterface + From<R> + 'static,
{
    fn from(f: F) -> Self {
        Box::new(Command1::from(f))
    }
}
*/

#[derive(Clone)]
pub struct Command2<S, T, R, F>
where
    F: Fn(S, T) -> R,
{
    f: F,
    state: PhantomData<S>,
    argument: PhantomData<T>,
    result: PhantomData<R>,
}

impl<S, T, R, F> From<F> for Command2<S, T, R, F>
where
    F: Fn(S, T) -> R,
{
    fn from(f: F) -> Self {
        Command2 {
            f,
            state: Default::default(),
            result: Default::default(),
            argument: Default::default(),
        }
    }
}

impl<F, ER, E, V, T, R> Command<ER, E, V> for Command2<&State<V>, T, R, F>
where
    F: Fn(&State<V>, T) -> R,
    R:Clone,
    F:Clone,
    T:Clone,
    V: ValueInterface + From<R>,
    T: FromCommandArguments<T, ER, E>,
    E:Environment,
    ER:EnvRef<E>,
{
    fn execute(
        &self,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error> {
        let argument: T = arguments.get(&context)?;
        if !arguments.all_parameters_used(){
            Err(Error::new(
                ErrorType::TooManyParameters,
                format!("Too many parameters: {}; {} excess parameters found", arguments.len(), arguments.excess_parameters()),
            )
            .with_position(&arguments.parameter_position()))
        }
        else{
            let result = (self.f)(state, argument);
            Ok(V::from(result))
        }
    }
}

pub trait FromParameter<T> {
    fn from_parameter(param: &Parameter) -> Result<T, Error>;
}

impl FromParameter<String> for String {
    fn from_parameter(param: &Parameter) -> Result<String, Error> {
        if let Some(p) = param.value.as_str() {
            Ok(p.to_owned())
        } else {
            Err(Error::conversion_error_at_position(
                param.value.clone(),
                "string",
                &param.position,
            ))
        }
    }
}

pub trait FromCommandArguments<T, ER: EnvRef<E>, E:Environment> {
    fn from_arguments(args: &mut CommandArguments, context:&Context<ER, E> ) -> Result<T, Error>;
    fn is_injected() -> bool;
}

impl<T, ER: EnvRef<E>, E:Environment> FromCommandArguments<T, ER, E> for T
where
    T: FromParameter<T>,
    E:Environment,
{
    fn from_arguments<'e>(args: &mut CommandArguments, _context:&Context<ER, E>) -> Result<T, Error> {
        T::from_parameter(args.get_parameter()?)
    }
    fn is_injected() -> bool {
        false
    }
}

// TODO: Use CommandKey instead of realm, namespace, command_name
pub trait CommandExecutor<ER: EnvRef<E>, E:Environment, V: ValueInterface> {
    fn execute<'e>(
        &self,
        realm: &str,
        namespace: &str,
        command_name: &str,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context: Context<ER, E>,
    ) -> Result<V, Error>;
}

impl<ER: EnvRef<E>, E:Environment, V: ValueInterface> CommandExecutor<ER, E, V> for HashMap<CommandKey, Box<dyn Command<ER, E, V>>> {
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
        if let Some(command) = self.get(&key) {
            command.execute(state, arguments, context)
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

pub struct CommandRegistry<ER, E, V: ValueInterface> {
    executors: HashMap<CommandKey, Box<dyn Command<ER, E, V>>>,
    pub command_metadata_registry: CommandMetadataRegistry,
}

impl<ER: EnvRef<E>, E:Environment, V: ValueInterface> CommandRegistry<ER, E, V> {
    pub fn new() -> Self {
        CommandRegistry {
            executors: HashMap::new(),
            command_metadata_registry: CommandMetadataRegistry::new(),
        }
    }
    pub fn register_boxed_command<K>(
        &mut self,
        key: K,
        executor: Box<dyn Command<ER, E, V>>,
    ) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
    {
        let key = key.into();
        let command_metadata = executor
            .command_metadata()
            .map(|cm| {
                let mut cm = cm.clone();
                cm.with_realm(&key.realm)
                    .with_namespace(&key.namespace)
                    .with_name(&key.name);
                cm
            })
            .unwrap_or((&key).into());
        self.command_metadata_registry
            .add_command(&command_metadata);

        self.executors.insert(key.clone(), executor);
        Ok(self.command_metadata_registry.get_mut(key).unwrap())
    }
    pub fn register_command<K, T>(&mut self, key: K, f: T) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
        T: Command<ER, E, V> + 'static,
    {
        let key = key.into();
        let command: Box<dyn Command<ER, E, V>> = Box::new(f);
        self.register_boxed_command(key, command)
    }
    /*
    pub fn register<K, T>(&mut self, key: K, f: T) -> Result<&mut CommandMetadata, Error>
    where
        K: Into<CommandKey>,
        T: Into<Box<dyn Command<I, V>>> + 'static,
    {
        let key = key.into();
        let command: Box<dyn Command<I, V>> = f.into();
        self.register_boxed_command(key, command)
    }
    */
}

impl<ER: EnvRef<E>, E:Environment, V: ValueInterface> CommandExecutor<ER, E, V> for CommandRegistry<ER, E, V> {
    fn execute(
        &self,
        realm: &str,
        namespace: &str,
        command_name: &str,
        state: &State<V>,
        arguments: &mut CommandArguments,
        context:Context<ER, E>
    ) -> Result<V, Error> {
        let key = CommandKey::new(realm, namespace, command_name);
        if let Some(command) = self.executors.get(&key) {
            command.execute(state, arguments, context)
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

#[cfg(test)]
mod tests {
    use self::context::StatEnvRef;

    use super::*;
    use crate::{state, value::Value};

    struct TestExecutor;
    impl CommandExecutor<StatEnvRef<NoInjection>, NoInjection, Value> for TestExecutor {
        fn execute<'e>(
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
            Command0::from(|| -> String { "Hello".into() }).execute(state, arguments, context)
        }
    }
    #[test]
    fn first_test() {
        let p = Parameter {
            value: "Hello".into(),
            ..Parameter::default()
        };
        let s: String = String::from_parameter(&p).unwrap();
        assert_eq!(s, "Hello");
    }
    #[test]
    fn test_command_arguments() {
        let mut rp = ResolvedParameters::new();
        rp.parameters.push(Parameter {
            value: "Hello".into(),
            ..Parameter::default()
        });
        let injection= Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(rp);
        let s: String = ca.get(&mut context).unwrap();
        assert_eq!(s, "Hello");
    }
    #[test]
    fn test_execute_command() -> Result<(), Error> {
        let c = Command0::from(|| -> String { "Hello".into() });
        let injection= Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(ResolvedParameters::new());
        let state: State<Value> = State::new();
        let s: Value = c.execute(&state, &mut ca, context).unwrap();
        assert_eq!(s.try_into_string()?, "Hello");
        Ok(())
    }

    #[test]
    fn test_command_executor() -> Result<(), Error> {
        let injection= Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(ResolvedParameters::new());
        let state = State::new();
        let s = TestExecutor
            .execute("", "", "test", &state, &mut ca, context)
            .unwrap();
        assert_eq!(s.try_into_string()?, "Hello");
        Ok(())
    }
    #[test]
    fn test_hashmap_command_executor() -> Result<(), Error> {
        let mut hm = HashMap::<CommandKey, Box<dyn Command<StatEnvRef<NoInjection>, NoInjection, Value>>>::new();
        hm.insert(
            CommandKey::new("", "", "test"),
            Box::new(Command0::from(|| -> String { "Hello1".into() })),
        );
        hm.insert(
            CommandKey::new("", "", "test2"),
            Box::new(Command0::from(|| -> String { "Hello2".into() })),
        );

        let state = State::new();
        let injection= Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(ResolvedParameters::new());
        let s = hm.execute("", "", "test", &state, &mut ca, context.clone_context()).unwrap();
        assert_eq!(s.try_into_string()?, "Hello1");
        let mut ca = CommandArguments::new(ResolvedParameters::new());
        let s = hm.execute("", "", "test2", &state, &mut ca, context.clone_context()).unwrap();
        assert_eq!(s.try_into_string()?, "Hello2");
        Ok(())
    }
    #[test]
    fn test_command_registry() -> Result<(), Error> {
        let mut cr = CommandRegistry::<StatEnvRef<NoInjection>, NoInjection, Value>::new();
        cr.register_command("test", Command0::from(|| -> String { "Hello1".into() }))?;
        cr.register_command("test2", Command0::from(|| -> String { "Hello2".into() }))?;
        cr.register_command(
            "stest1",
            Command1::from(|_s: &State<Value>| -> String { "STest1".into() }),
        )?;
        println!("{:?}", cr.command_metadata_registry);

        let state = State::new();

        let injection= Box::leak(Box::new(NoInjection));
        let envref = StatEnvRef(injection);
        let mut context = Context::new(envref);
        let mut ca = CommandArguments::new(ResolvedParameters::new());
        let s = cr.execute("", "", "test", &state, &mut ca, context.clone_context()).unwrap();
        assert_eq!(s.try_into_string()?, "Hello1");
        let mut ca = CommandArguments::new(ResolvedParameters::new());
        let s = cr.execute("", "", "test2", &state, &mut ca, context.clone_context()).unwrap();
        assert_eq!(s.try_into_string()?, "Hello2");
        Ok(())
    }
}
