use crate::commands::{CommandArguments, CommandExecutor};
use crate::context::{Context, ContextInterface, EnvRef, Environment};
use crate::error::Error;
use crate::plan::{Plan, PlanBuilder, Step};
use crate::query::TryToQuery;
use crate::state::State;
use crate::value::ValueInterface;

pub struct PlanInterpreter<ER: EnvRef<E>, E: Environment> {
    plan: Option<Plan>,
    environment: ER,
    step_number: usize,
    state: Option<State<E::Value>>,
}

impl<ER: EnvRef<E>, E: Environment<EnvironmentReference = ER>> PlanInterpreter<ER, E> {
    pub fn new(environment: ER) -> Self {
        PlanInterpreter {
            plan: None,
            environment,
            step_number: 0,
            state: None,
        }
    }
    pub fn with_plan(&mut self, plan: Plan) -> &mut Self {
        println!("with plan {:?}", plan);
        self.plan = Some(plan);
        self.step_number = 0;
        self
    }

    pub fn with_query<Q: TryToQuery>(&mut self, query: Q) -> Result<&mut Self, Error> {
        let query = query.try_to_query()?;
        let cmr = self.environment.get().get_command_metadata_registry();
        //println!("Query: {}", query);
        /*
        println!(
            "Command registry:\n{}\n",
            serde_yaml::to_string(cmr).unwrap()
        );
        */
        let mut pb = PlanBuilder::new(query, cmr);
        let plan = pb.build()?;
        Ok(self.with_plan(plan))
    }

    pub fn evaluate<Q: TryToQuery>(&mut self, query: Q) -> Result<State<E::Value>, Error> {
        self.with_query(query)?;
        self.run()?;
        self.state
            .take()
            .ok_or(Error::general_error("No state".to_string()))
    }

    pub fn run(&mut self) -> Result<(), Error> {
        let context = self.environment.new_context();
        if self.plan.is_none() {
            return Err(Error::general_error("No plan".to_string()));
        }
        for i in 0..self.len() {
            let input_state = self.state.take().unwrap_or(self.initial_state());
            let step = self.get_step(i)?;
            let output_state = self.do_step(&step, input_state, context.clone_context())?;
            self.state = Some(output_state);
        }
        Ok(())
    }
    pub fn initial_state(&self) -> State<<E as Environment>::Value> {
        State::new()
    }
    pub fn len(&self) -> usize {
        if let Some(plan) = &self.plan {
            return plan.steps.len();
        }
        0
    }
    pub fn get_step(&self, i: usize) -> Result<&Step, Error> {
        if let Some(plan) = &self.plan {
            if let Some(step) = plan.steps.get(i) {
                return Ok(step);
            } else {
                return Err(Error::general_error(format!(
                    "Step {} requested, plan has {} steps",
                    i,
                    plan.steps.len()
                )));
            }
        }
        Err(Error::general_error("No plan".to_string()))
    }
    pub fn do_step(
        &self,
        step: &Step,
        input_state: State<<E as Environment>::Value>,
        context: Context<ER, E>,
    ) -> Result<State<<E as Environment>::Value>, Error> {
        match step {
            crate::plan::Step::GetResource(key) => {
                let store = self.environment.get_store();
                let (data, metadata) = store
                    .lock()
                    .unwrap()
                    .get(&key)
                    .map_err(|e| Error::general_error(format!("Store error: {}", e)))?; // TODO: use store error type - convert to Error
                let value = <<E as Environment>::Value as ValueInterface>::from_bytes(data);
                return Ok(State::new().with_data(value).with_metadata(metadata));
            }
            crate::plan::Step::GetResourceMetadata(_) => todo!(),
            crate::plan::Step::GetNamedResource(_) => todo!(),
            crate::plan::Step::GetNamedResourceMetadata(_) => todo!(),
            crate::plan::Step::Evaluate(_) => todo!(),
            crate::plan::Step::Action {
                realm,
                ns,
                action_name,
                position,
                parameters,
            } => {
                let mut arguments = CommandArguments::new(parameters.clone());
                arguments.action_position = position.clone();

                let ce = self.environment.get().get_command_executor();
                let result = ce.execute(
                    realm,
                    ns,
                    action_name,
                    &input_state,
                    &mut arguments,
                    context.clone_context(),
                )?;
                let state = State::new()
                    .with_data(result)
                    .with_metadata(context.get_metadata().into());
                /// TODO - reset metadata ?
                return Ok(state);
            }
            crate::plan::Step::Filename(name) => {
                context.set_filename(name.name.clone());
            }
            crate::plan::Step::Info(m) => {
                context.info(&m);
            }
            crate::plan::Step::Warning(m) => {
                context.warning(&m);
            }
            crate::plan::Step::Error(m) => {
                context.error(&m);
            }
            crate::plan::Step::Plan(_) => todo!(),
        }
        Ok(input_state)
    }
}

#[cfg(feature = "async_store")]
pub struct AsyncPlanInterpreter<ER: EnvRef<E>, E: Environment> {
    plan: Option<Plan>,
    environment: ER,
    step_number: usize,
    state: Option<State<E::Value>>,
}

#[cfg(feature = "async_store")]
impl<ER: EnvRef<E>, E: Environment<EnvironmentReference = ER>> AsyncPlanInterpreter<ER, E> {
    pub fn new(environment: ER) -> Self {
        AsyncPlanInterpreter {
            plan: None,
            environment,
            step_number: 0,
            state: None,
        }
    }
    pub fn with_plan(&mut self, plan: Plan) -> &mut Self {
        println!("with plan {:?}", plan);
        self.plan = Some(plan);
        self.step_number = 0;
        self
    }

    pub fn with_query<Q: TryToQuery>(&mut self, query: Q) -> Result<&mut Self, Error> {
        let query = query.try_to_query()?;
        let cmr = self.environment.get().get_command_metadata_registry();
        //println!("Query: {}", query);
        /*
        println!(
            "Command registry:\n{}\n",
            serde_yaml::to_string(cmr).unwrap()
        );
        */
        let mut pb = PlanBuilder::new(query, cmr);
        let plan = pb.build()?;
        Ok(self.with_plan(plan))
    }

    pub async fn evaluate<Q: TryToQuery>(&mut self, query: Q) -> Result<State<E::Value>, Error> {
        self.with_query(query)?;
        self.run().await?;
        self.state
            .take()
            .ok_or(Error::general_error("No state".to_string()))
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        let context = self.environment.new_context();
        if self.plan.is_none() {
            return Err(Error::general_error("No plan".to_string()));
        }
        for i in 0..self.len() {
            let input_state = self.state.take().unwrap_or(self.initial_state());
            let step = self.get_step(i)?;
            let output_state = self
                .do_step(&step, input_state, context.clone_context())
                .await?;
            self.state = Some(output_state);
        }
        Ok(())
    }
    pub fn initial_state(&self) -> State<<E as Environment>::Value> {
        State::new()
    }
    pub fn len(&self) -> usize {
        if let Some(plan) = &self.plan {
            return plan.steps.len();
        }
        0
    }
    pub fn get_step(&self, i: usize) -> Result<&Step, Error> {
        if let Some(plan) = &self.plan {
            if let Some(step) = plan.steps.get(i) {
                return Ok(step);
            } else {
                return Err(Error::general_error(format!(
                    "Step {} requested, plan has {} steps",
                    i,
                    plan.steps.len()
                )));
            }
        }
        Err(Error::general_error("No plan".to_string()))
    }
    pub async fn do_step(
        &self,
        step: &Step,
        input_state: State<<E as Environment>::Value>,
        context: Context<ER, E>,
    ) -> Result<State<<E as Environment>::Value>, Error> {
        match step {
            crate::plan::Step::GetResource(key) => {
                let store = self.environment.get_async_store();
                let (data, metadata) = store
                    .lock()
                    .unwrap()
                    .get(&key)
                    .await
                    .map_err(|e| Error::general_error(format!("Store error: {}", e)))?; // TODO: use store error type - convert to Error
                let value = <<E as Environment>::Value as ValueInterface>::from_bytes(data);
                return Ok(State::new().with_data(value).with_metadata(metadata));
            }
            crate::plan::Step::GetResourceMetadata(_) => todo!(),
            crate::plan::Step::GetNamedResource(_) => todo!(),
            crate::plan::Step::GetNamedResourceMetadata(_) => todo!(),
            crate::plan::Step::Evaluate(_) => todo!(),
            crate::plan::Step::Action {
                realm,
                ns,
                action_name,
                position,
                parameters,
            } => {
                let mut arguments = CommandArguments::new(parameters.clone());
                arguments.action_position = position.clone();

                let ce = self.environment.get().get_command_executor();
                let result = ce.execute(
                    realm,
                    ns,
                    action_name,
                    &input_state,
                    &mut arguments,
                    context.clone_context(),
                )?;
                let state = State::new()
                    .with_data(result)
                    .with_metadata(context.get_metadata().into());
                /// TODO - reset metadata ?
                return Ok(state);
            }
            crate::plan::Step::Filename(name) => {
                context.set_filename(name.name.clone());
            }
            crate::plan::Step::Info(m) => {
                context.info(&m);
            }
            crate::plan::Step::Warning(m) => {
                context.warning(&m);
            }
            crate::plan::Step::Error(m) => {
                context.error(&m);
            }
            crate::plan::Step::Plan(_) => todo!(),
        }
        Ok(input_state)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fmt::format;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::*;
    use crate::cache::NoCache;
    use crate::command_metadata::ArgumentInfo;
    use crate::command_metadata::CommandMetadata;
    use crate::command_metadata::CommandMetadataRegistry;

    use crate::commands::*;
    use crate::context;
    use crate::context::SimpleEnvironment;
    use crate::context::StatEnvRef;
    use crate::metadata::Metadata;
    use crate::parse::parse_key;
    use crate::query::Key;
    use crate::value::{Value, ValueInterface};
    pub struct TestExecutor;

    #[derive(Debug, Clone)]
    struct InjectedVariable(String);
    struct InjectionTest {
        variable: InjectedVariable,
        cr: CommandRegistry<StatEnvRef<Self>, Self, Value>,
        store: Arc<Mutex<Box<dyn crate::store::Store>>>,
    }

    impl Environment for InjectionTest {
        fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
            &self.cr.command_metadata_registry
        }
        fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
            &mut self.cr.command_metadata_registry
        }
        type Value = Value;
        type CommandExecutor = CommandRegistry<Self::EnvironmentReference, Self, Value>;
        type EnvironmentReference = StatEnvRef<Self>;
        type Context = context::Context<Self::EnvironmentReference, Self>;

        fn get_command_executor(&self) -> &Self::CommandExecutor {
            &self.cr
        }

        fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor {
            &mut self.cr
        }

        fn get_store(&self) -> Arc<Mutex<Box<dyn crate::store::Store>>> {
            self.store.clone()
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }
        #[cfg(feature = "async_store")]
        fn get_async_store(&self) -> Arc<Mutex<Box<dyn crate::store::AsyncStore>>> {
            Arc::new(Mutex::new(Box::new(crate::store::NoAsyncStore)))
        }
    }

    impl Environment for NoInjection {
        fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
            panic!("NoInjection has no command metadata registry")
        }

        fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
            panic!("NoInjection has no command metadata registry")
        }
        type Value = Value;
        type CommandExecutor = TestExecutor;
        type EnvironmentReference = StatEnvRef<Self>;
        type Context = context::Context<Self::EnvironmentReference, Self>;

        fn get_command_executor(&self) -> &Self::CommandExecutor {
            &TestExecutor
        }

        fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor {
            panic!("NoInjection has non-mutable command executor")
        }

        fn get_store(&self) -> Arc<Mutex<Box<dyn crate::store::Store>>> {
            panic!("NoInjection has no store")
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }

        #[cfg(feature = "async_store")]
        fn get_async_store(&self) -> Arc<Mutex<Box<dyn crate::store::AsyncStore>>> {
            Arc::new(Mutex::new(Box::new(crate::store::NoAsyncStore)))
        }
    }

    struct MutableInjectionTest {
        variable: Rc<RefCell<InjectedVariable>>,
        cr: CommandRegistry<StatEnvRef<Self>, Self, Value>,
        store: Arc<Mutex<Box<dyn crate::store::Store>>>,
    }

    impl Environment for MutableInjectionTest {
        type Value = Value;
        type CommandExecutor = CommandRegistry<StatEnvRef<Self>, Self, Value>;
        type EnvironmentReference = StatEnvRef<Self>;
        type Context = context::Context<Self::EnvironmentReference, Self>;

        fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
            &self.cr.command_metadata_registry
        }
        fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
            &mut self.cr.command_metadata_registry
        }

        fn get_command_executor(&self) -> &Self::CommandExecutor {
            &self.cr
        }

        fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor {
            &mut self.cr
        }

        fn get_store(&self) -> std::sync::Arc<std::sync::Mutex<Box<dyn crate::store::Store>>> {
            self.store.clone()
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }

        #[cfg(feature = "async_store")]
        fn get_async_store(&self) -> Arc<Mutex<Box<dyn crate::store::AsyncStore>>> {
            Arc::new(Mutex::new(Box::new(crate::store::NoAsyncStore)))
        }
    }

    impl<ER: EnvRef<E>, E: Environment> CommandExecutor<ER, E, Value> for TestExecutor {
        fn execute(
            &self,
            realm: &str,
            namespace: &str,
            command_name: &str,
            state: &State<Value>,
            arguments: &mut CommandArguments,
            context: Context<ER, E>,
        ) -> Result<Value, Error> {
            todo!()
        }
    }
    #[test]
    fn test_plan_interpreter() -> Result<(), Error> {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.get_mut_command_metadata_registry()
            .add_command(&CommandMetadata::new("test"));
        fn test() -> Result<String, Error> {
            Ok("Hello".to_string())
        }
        let cr = env.get_mut_command_executor();
        register_command!(cr, test());
        let envref = env.to_ref();

        let mut pi = PlanInterpreter::new(envref);
        pi.with_query("test").unwrap();
        //println!("{:?}", pi.plan);
        pi.run()?;
        assert_eq!(pi.state.as_ref().unwrap().data.try_into_string()?, "Hello");
        Ok(())
    }
    #[test]
    fn test_hello_world_interpreter() -> Result<(), Error> {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        {
            let mut cr = env.get_mut_command_executor();
            fn hello() -> Result<String, Error> {
                Ok("Hello".to_string())
            }
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                let greeting = state.data.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who))
            }
            register_command!(cr, hello());
            register_command!(cr, greet(state, who:String));
        }

        let mut pi = PlanInterpreter::new(env.to_ref());
        pi.with_query("hello/greet-world").unwrap();
        //println!("{:?}", pi.plan);
        println!(
            "############################ PLAN ############################\n{}\n",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        pi.run()?;
        assert_eq!(
            pi.state.as_ref().unwrap().data.try_into_string()?,
            "Hello world!"
        );
        Ok(())
    }

    #[test]
    fn test_hello_world_multiple_interpreter() -> Result<(), Error> {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        {
            let mut cr = env.get_mut_command_executor();
            fn hello() -> Result<String, Error> {
                Ok("Hello".to_string())
            }
            fn greet(state: &State<Value>, who: Vec<String>) -> Result<String, Error> {
                let greeting = state.data.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who.join(" ")))
            }
            register_command!(cr, hello());
            register_command!(cr, greet(state, multiple who:String));
            /*
            for x in cr.command_metadata_registry.get_mut("greet").unwrap().arguments.iter_mut(){
                //x.multiple = true;
                println!("{:?}", x);
            }
            */
        }

        /*
        println!(
            "############################ COMMANDS ############################\n{}\n",
            serde_yaml::to_string(env.get_command_metadata_registry()).unwrap()
        );
        */
        let mut pi = PlanInterpreter::new(env.to_ref());
        pi.with_query("hello/greet-world-and-sun").unwrap();
        //println!("{:?}", pi.plan);
        println!(
            "############################ PLAN ############################\n{}\n",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        pi.run()?;
        assert_eq!(
            pi.state.as_ref().unwrap().data.try_into_string()?,
            "Hello world and sun!"
        );
        Ok(())
    }

    #[test]
    fn test_interpreter_with_value_injection() -> Result<(), Error> {
        let mut cr: CommandRegistry<StatEnvRef<InjectionTest>, InjectionTest, Value> =
            CommandRegistry::new();
        impl InjectedFromContext<InjectedVariable, InjectionTest> for InjectedVariable {
            fn from_context(
                _name: &str,
                context: &impl ContextInterface<InjectionTest>,
            ) -> Result<InjectedVariable, Error> {
                Ok(context.get_environment().variable.to_owned())
            }
        }

        fn injected(state: &State<Value>, what: InjectedVariable) -> Result<String, Error> {
            Ok(format!("Hello {}", what.0))
        }
        register_command!(cr, injected(state, injected what:InjectedVariable));

        let cmr = cr.command_metadata_registry.clone();

        let env = Box::leak(Box::new(InjectionTest {
            variable: InjectedVariable("injected string".to_string()),
            cr: cr,
            store: Arc::new(Mutex::new(Box::new(crate::store::NoStore))),
        }));
        let envref = StatEnvRef(env);
        let mut pi = PlanInterpreter::new(envref);
        pi.with_query("injected")?;
        println!(
            "{}",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        pi.run()?;
        assert_eq!(
            pi.state.as_ref().unwrap().data.try_into_string()?,
            "Hello injected string"
        );
        Ok(())
    }
    #[test]
    fn test_interpreter_with_mutable_injection() -> Result<(), Error> {
        let mut cr: CommandRegistry<StatEnvRef<MutableInjectionTest>, MutableInjectionTest, Value> =
            CommandRegistry::new();
        impl InjectedFromContext<Rc<RefCell<InjectedVariable>>, MutableInjectionTest>
            for Rc<RefCell<InjectedVariable>>
        {
            fn from_context(
                _name: &str,
                context: &impl ContextInterface<MutableInjectionTest>,
            ) -> Result<Rc<RefCell<InjectedVariable>>, Error> {
                Ok(context.get_environment().variable.clone())
            }
        }

        fn injected(
            state: &State<Value>,
            what: Rc<RefCell<InjectedVariable>>,
        ) -> Result<String, Error> {
            let res = format!("Hello {}", what.borrow().0);
            what.borrow_mut().0 = "changed".to_owned();
            Ok(res)
        }
        register_command!(cr, injected(state, injected what:Rc<RefCell<InjectedVariable>>));

        let env = Box::leak(Box::new(MutableInjectionTest {
            variable: Rc::new(RefCell::new(InjectedVariable(
                "injected string".to_string(),
            ))),
            cr: cr,
            store: Arc::new(Mutex::new(Box::new(crate::store::NoStore))),
        }));
        let envref = StatEnvRef(env);
        let mut pi = PlanInterpreter::new(envref);
        pi.with_query("injected")?;
        println!(
            "{}",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        pi.run()?;
        assert_eq!(
            pi.state.as_ref().unwrap().data.try_into_string()?,
            "Hello injected string"
        );
        assert_eq!(pi.environment.get().variable.borrow().0, "changed");
        Ok(())
    }

    #[test]
    fn test_resource_interpreter() -> Result<(), Error> {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_store(Box::new(crate::store::MemoryStore::new(&Key::new())));
        {
            let store = env.get_store();
            let mut store = store.lock().unwrap();
            store
                .set(
                    &parse_key("hello.txt").unwrap(),
                    "Hello TEXT".as_bytes(),
                    &Metadata::new(),
                )
                .unwrap();
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                let greeting = state.data.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who))
            }
            register_command!(cr, greet(state, who:String));
        }

        let mut pi = PlanInterpreter::new(env.to_ref());
        pi.with_query("hello.txt/-/greet-world").unwrap();
        //println!("{:?}", pi.plan);
        println!(
            "############################ PLAN ############################\n{}\n",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        pi.run()?;
        assert_eq!(
            pi.state.as_ref().unwrap().data.try_into_string()?,
            "Hello TEXT world!"
        );
        Ok(())
    }
    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_async_resource_interpreter() -> Result<(), Error> {
        use crate::store::*;

        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let mut store = MemoryStore::new(&Key::new());
        store.set(
            &parse_key("hello.txt").unwrap(),
            "Hello TEXT".as_bytes(),
            &Metadata::new(),
        )?;

        env.with_async_store(Box::new(crate::store::AsyncStoreWrapper(store)));
        {
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                let greeting = state.data.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who))
            }
            register_command!(cr, greet(state, who:String));
        }

        let mut pi = AsyncPlanInterpreter::new(env.to_ref());
        pi.with_query("hello.txt/-/greet-world").unwrap();
        //println!("{:?}", pi.plan);
        println!(
            "############################ PLAN ############################\n{}\n",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        pi.run().await?;
        assert_eq!(
            pi.state.as_ref().unwrap().data.try_into_string()?,
            "Hello TEXT world!"
        );
        Ok(())
    }
}
