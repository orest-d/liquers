use async_trait::async_trait;

use crate::command_metadata::CommandKey;
use crate::commands::{CommandArguments, CommandExecutor, NGCommandArguments, NGCommandExecutor};
use crate::context::{
    ActionContext, Context, ContextInterface, EnvRef, Environment, NGContext, NGEnvRef,
    NGEnvironment,
};
use crate::error::Error;
use crate::parse::{SimpleTemplate, SimpleTemplateElement};
use crate::plan::{Plan, PlanBuilder, Step};
use crate::query::TryToQuery;
use crate::state::State;
use crate::value::ValueInterface;
use futures::future::{BoxFuture, FutureExt};

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
                let state = State::<E::Value>::new()
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
    pub plan: Option<Plan>,
    environment: ER,
    step_number: usize,
    pub state: Option<State<E::Value>>,
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

pub struct NGPlanInterpreter<E: NGEnvironment> {
    plan: Option<Plan>,
    environment: NGEnvRef<E>,
    step_number: usize,
    //state: Option<State<E::Value>>,
}

#[cfg(feature = "async_store")]
impl<E: NGEnvironment> NGPlanInterpreter<E> {
    pub fn new(environment: NGEnvRef<E>) -> Self {
        NGPlanInterpreter {
            plan: None,
            environment,
            step_number: 0,
            //state: None,
        }
    }
    pub fn with_plan(&mut self, plan: Plan) -> &mut Self {
        println!("with plan {:?}", plan);
        self.plan = Some(plan);
        self.step_number = 0;
        self
    }

    pub async fn set_query<Q: TryToQuery>(&mut self, query: Q) -> Result<(), Error> {
        let query = query.try_to_query()?;
        let plan = {
            let env = self.environment.0.read().await;
            let cmr = env.get_command_metadata_registry();
            let mut pb = PlanBuilder::new(query, cmr);
            pb.build()?
        };
        self.with_plan(plan);
        Ok(())
    }

    pub async fn evaluate<Q: TryToQuery>(&mut self, query: Q) -> Result<State<E::Value>, Error> {
        self.set_query(query).await?;
        self.run().await
    }

    pub async fn apply(
        &mut self,
        context: NGContext<E>,
        input_state: State<<E as NGEnvironment>::Value>,
    ) -> Result<State<<E as NGEnvironment>::Value>, Error> {
        //let context = NGContext::new(self.environment.clone()).await;
        async move {
            if self.plan.is_none() {
                Err(Error::general_error("No plan".to_string()))
            } else {
                let mut state = input_state;
                for i in 0..self.len() {
                    let step = self.get_step(i)?.clone();
                    let ctx = context.clone_context();
                    let envref = self.environment.clone();
                    let output_state =
                        async move { Self::do_step(envref, step, state, ctx).await }.await?;
                    state = output_state;
                }
                Ok(state)
            }
        }
        .boxed()
        .await
    }

    pub async fn run(&mut self) -> Result<State<<E as NGEnvironment>::Value>, Error> {
        self.apply(
            NGContext::new(self.environment.clone()).await,
            Self::initial_state(),
        )
        .await
    }

    pub fn initial_state() -> State<<E as NGEnvironment>::Value> {
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
        envref: NGEnvRef<E>,
        step: Step,
        input_state: State<<E as NGEnvironment>::Value>,
        context: NGContext<E>,
    ) -> BoxFuture<'static, Result<State<<E as NGEnvironment>::Value>, Error>> {
        async move {
            match step {
                crate::plan::Step::GetResource(key) => {
                    let store = envref.get_async_store().await;
                    let (data, metadata) = store.get(&key).await?;
                    let value = <<E as NGEnvironment>::Value as ValueInterface>::from_bytes(data);
                    return Ok(State::new().with_data(value).with_metadata(metadata));
                }
                crate::plan::Step::GetResourceMetadata(_) => todo!(),
                crate::plan::Step::GetNamedResource(_) => todo!(),
                crate::plan::Step::GetNamedResourceMetadata(_) => todo!(),
                crate::plan::Step::Evaluate(q) => {
                    //                todo!()  //TODO: ! evaluate

                    let query = q.clone();
                    return async move {
                        let context = NGContext::new(envref.clone()).await;
                        let mut interpreter = Self::new(envref);
                        interpreter.set_query(query).await?;
                        interpreter.apply(context, Self::initial_state()).await
                    }
                    .boxed()
                    .await;
                }
                crate::plan::Step::Action {
                    ref realm,
                    ref ns,
                    ref action_name,
                    position,
                    parameters,
                } => {
                    let mut arguments =
                        NGCommandArguments::<<E as NGEnvironment>::Value>::new(parameters.clone());
                    arguments.action_position = position.clone();
                    let result = {
                        #[cfg(not(feature = "tokio_exec"))]
                        {
                            let env = envref.0.read().await;
                            let ce = env.get_command_executor();
                            /*
                            ce.execute(
                                &CommandKey::new(realm, ns, action_name),
                                &input_state,
                                &mut arguments,
                                context.clone_context(),
                            )?
                            */
                            ce.execute_async(
                                &CommandKey::new(realm, ns, action_name),
                                input_state,
                                arguments,
                                context.clone_context(),                                
                            )
                            .await?
                        }
                        // TODO: ! tokio_exec
                        #[cfg(feature = "tokio_exec")]
                        {
                            !todo!("Tokio exec");
                            /*
                                                    let ce = {
                                                        let env = envref.0.read().await;
                                                         env.get_command_executor()
                                                    };
                                                    let res = tokio::task::spawn_blocking(move || {
                                                        let res = ce.execute(
                                                            &CommandKey::new(realm, ns, action_name),
                                                            &input_state,
                                                            &mut arguments,
                                                            context.clone_context(),
                                                        );
                                                        tokio::sync::Mutex::new(Ok(E::Value::none()))
                                                    }).await.map_err(|e| Error::general_error(format!("Tokio task error: {}", e)))?;
                                                    let x  = (*(res.lock().await))?;
                                                    x
                            */
                        }
                    };

                    let state = State::<<E as NGEnvironment>::Value>::new()
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
        .boxed()
    }

    pub async fn evaluate_simple_template(
        &self,
        template: &SimpleTemplate,
    ) -> Result<String, Error> {
        let mut result = String::new();
        for element in template.0.iter() {
            match element {
                SimpleTemplateElement::Text(t) => {
                    result.push_str(t);
                }
                SimpleTemplateElement::ExpandQuery(q) => {
                    let mut pi = Self::new(self.environment.clone());
                    let state = pi.evaluate(q.clone()).await?;
                    if state.is_error()? {
                        return Err(Error::general_error("Error in template".to_string())
                            .with_query(&q)
                            .with_position(&q.position()));
                    }
                    result.push_str(&state.try_into_string()?);
                }
            }
        }
        Ok(result)
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
        store: Arc<Box<dyn crate::store::Store>>,
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

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            self.store.clone()
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }
        #[cfg(feature = "async_store")]
        fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
            Arc::new(Box::new(crate::store::NoAsyncStore))
        }
    }

    struct NGInjectionTest {
        variable: InjectedVariable,
        cr: NGCommandRegistry<NGEnvRef<Self>, Value, NGContext<Self>>,
        store: Arc<Box<dyn crate::store::Store>>,
    }

    impl NGEnvironment for NGInjectionTest {
        type Value = Value;

        type CommandExecutor = NGCommandRegistry<NGEnvRef<Self>, Value, NGContext<Self>>;

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

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            self.store.clone()
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }

        fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
            Arc::new(Box::new(crate::store::NoAsyncStore))
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

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            panic!("NoInjection has no store")
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }

        #[cfg(feature = "async_store")]
        fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
            Arc::new(Box::new(crate::store::NoAsyncStore))
        }
    }

    struct MutableInjectionTest {
        variable: Arc<Mutex<InjectedVariable>>,
        cr: CommandRegistry<StatEnvRef<Self>, Self, Value>,
        store: Arc<Box<dyn crate::store::Store>>,
    }

    impl NGEnvironment for NGNoInjection {
        type Value = Value;

        type CommandExecutor = NGTestExecutor;

        fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
            panic!("NGNoInjection has no command metadata registry")
        }

        fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
            panic!("NGNoInjection has non-mutable command metadata registry")
        }

        fn get_command_executor(&self) -> &Self::CommandExecutor {
            &NGTestExecutor
        }

        fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor {
            panic!("NGNoInjection has non-mutable command executor")
        }

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            panic!("NGNoInjection has no store")
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            panic!("NGNoInjection has no cache")
        }

        fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
            panic!("NGNoInjection has no async store")
        }
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

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            self.store.clone()
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }

        #[cfg(feature = "async_store")]
        fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
            Arc::new(Box::new(crate::store::NoAsyncStore))
        }
    }

    struct NGMutableInjectionTest {
        variable: Arc<Mutex<InjectedVariable>>,
        cr: NGCommandRegistry<NGEnvRef<Self>, Value, NGContext<Self>>,
        store: Arc<Box<dyn crate::store::Store>>,
    }

    impl NGEnvironment for NGMutableInjectionTest {
        type Value = Value;

        type CommandExecutor = NGCommandRegistry<NGEnvRef<Self>, Value, NGContext<Self>>;

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

        fn get_store(&self) -> Arc<Box<dyn crate::store::Store>> {
            self.store.clone()
        }

        fn get_cache(&self) -> Arc<Mutex<Box<dyn crate::cache::Cache<Self::Value>>>> {
            Arc::new(Mutex::new(Box::new(NoCache::new())))
        }

        fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
            Arc::new(Box::new(crate::store::NoAsyncStore))
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

    pub struct NGTestExecutor;

    impl NGCommandExecutor<NGEnvRef<NGNoInjection>, Value, NGContext<NGNoInjection>>
        for NGTestExecutor
    {
        fn execute(
            &self,
            key: &CommandKey,
            state: &State<Value>,
            arguments: &mut NGCommandArguments<Value>,
            context: NGContext<NGNoInjection>,
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
        assert_eq!(pi.state.as_ref().unwrap().try_into_string()?, "Hello");
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
                let greeting = state.try_into_string().unwrap();
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
            pi.state.as_ref().unwrap().try_into_string()?,
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
            fn greet(state: &State<Value>, who: Vec<Value>) -> Result<String, Error> {
                let greeting = state.try_into_string().unwrap();
                Ok(format!(
                    "{} {}!",
                    greeting,
                    who.iter()
                        .map(|x| x.try_into_string().unwrap_or("?".to_string()))
                        .collect::<Vec<String>>()
                        .join(" ")
                ))
            }
            register_command!(cr, hello());
            register_command!(cr, greet(state, multiple who:Value));
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
            pi.state.as_ref().unwrap().try_into_string()?,
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

        fn injected(_state: &State<Value>, what: InjectedVariable) -> Result<String, Error> {
            Ok(format!("Hello {}", what.0))
        }
        register_command!(cr, injected(state, injected what:InjectedVariable));

        let cmr = cr.command_metadata_registry.clone();

        let env = Box::leak(Box::new(InjectionTest {
            variable: InjectedVariable("injected string".to_string()),
            cr: cr,
            store: Arc::new(Box::new(crate::store::NoStore)),
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
            pi.state.as_ref().unwrap().try_into_string()?,
            "Hello injected string"
        );
        Ok(())
    }
    #[test]
    fn test_interpreter_with_mutable_injection() -> Result<(), Error> {
        let mut cr: CommandRegistry<StatEnvRef<MutableInjectionTest>, MutableInjectionTest, Value> =
            CommandRegistry::new();
        impl InjectedFromContext<Arc<Mutex<InjectedVariable>>, MutableInjectionTest>
            for Arc<Mutex<InjectedVariable>>
        {
            fn from_context(
                _name: &str,
                context: &impl ContextInterface<MutableInjectionTest>,
            ) -> Result<Arc<Mutex<InjectedVariable>>, Error> {
                Ok(context.get_environment().variable.clone())
            }
        }

        fn injected(
            _state: &State<Value>,
            what: Arc<Mutex<InjectedVariable>>,
        ) -> Result<String, Error> {
            let res = format!("Hello {}", what.lock().unwrap().0);
            (*what.lock().unwrap()).0 = "changed".to_owned();
            Ok(res)
        }
        register_command!(cr, injected(state, injected what:Arc<Mutex<InjectedVariable>>));

        let env = Box::leak(Box::new(MutableInjectionTest {
            variable: Arc::new(Mutex::new(InjectedVariable("injected string".to_string()))),
            cr: cr,
            store: Arc::new(Box::new(crate::store::NoStore)),
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
            pi.state.as_ref().unwrap().try_into_string()?,
            "Hello injected string"
        );
        assert_eq!(pi.environment.get().variable.lock().unwrap().0, "changed");
        Ok(())
    }

    #[test]
    fn test_resource_interpreter() -> Result<(), Error> {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_store(Box::new(crate::store::MemoryStore::new(&Key::new())));
        {
            let store = env.get_store();
            store
                .set(
                    &parse_key("hello.txt").unwrap(),
                    "Hello TEXT".as_bytes(),
                    &Metadata::new(),
                )
                .unwrap();
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                let greeting = state.try_into_string().unwrap();
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
            pi.state.as_ref().unwrap().try_into_string()?,
            "Hello TEXT world!"
        );
        Ok(())
    }
    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_async_resource_interpreter() -> Result<(), Error> {
        use crate::store::*;

        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let store = MemoryStore::new(&Key::new());
        store.set(
            &parse_key("hello.txt").unwrap(),
            "Hello TEXT".as_bytes(),
            &Metadata::new(),
        )?;

        env.with_async_store(Box::new(crate::store::AsyncStoreWrapper(store)));
        {
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                let greeting = state.try_into_string().unwrap();
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
            pi.state.as_ref().unwrap().try_into_string()?,
            "Hello TEXT world!"
        );
        Ok(())
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_nginterpreter() -> Result<(), Error> {
        use crate::{context::SimpleNGEnvironment, store::*};

        let mut env: SimpleNGEnvironment<Value> = SimpleNGEnvironment::new();
        let store = MemoryStore::new(&Key::new());
        store.set(
            &parse_key("hello.txt").unwrap(),
            "Hello TEXT".as_bytes(),
            &Metadata::new(),
        )?;

        env.with_async_store(Box::new(crate::store::AsyncStoreWrapper(store)));
        {
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                let greeting = state.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who))
            }
            ng_register_command!(cr, greet(state, who:String));
        }

        let mut pi = NGPlanInterpreter::new(env.to_ref());
        pi.set_query("hello.txt/-/greet-world").await?;
        println!(
            "############################ PLAN ############################\n{}\n",
            serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
        );
        let state = pi.run().await?;
        assert_eq!(state.try_into_string()?, "Hello TEXT world!");
        Ok(())
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_template() -> Result<(), Error> {
        use crate::{context::SimpleNGEnvironment, parse::parse_simple_template, store::*};

        let mut env: SimpleNGEnvironment<Value> = SimpleNGEnvironment::new();
        let store = MemoryStore::new(&Key::new());
        store.set(
            &parse_key("hello.txt").unwrap(),
            "Hello TEXT".as_bytes(),
            &Metadata::new(),
        )?;

        env.with_async_store(Box::new(crate::store::AsyncStoreWrapper(store)));
        {
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                println!("GREET {:?}", state.data);
                let greeting = state.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who))
            }
            ng_register_command!(cr, greet(state, who:String));
        }

        let mut pi = NGPlanInterpreter::new(env.to_ref());

        let template = parse_simple_template("*** $-R/hello.txt/-/greet-world$ ***")?;
        let result = pi.evaluate_simple_template(&template).await?;
        assert_eq!(result, "*** Hello TEXT world! ***");
        Ok(())
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_template_command() -> Result<(), Error> {
        use crate::{context::SimpleNGEnvironment, parse::parse_simple_template, store::*};

        let mut env: SimpleNGEnvironment<Value> = SimpleNGEnvironment::new();
        let store = MemoryStore::new(&Key::new());
        store.set(
            &parse_key("hello.txt").unwrap(),
            "Hello TEXT".as_bytes(),
            &Metadata::new(),
        )?;

        env.with_async_store(Box::new(crate::store::AsyncStoreWrapper(store)));
        {
            let cr = env.get_mut_command_executor();
            fn greet(state: &State<Value>, who: String) -> Result<String, Error> {
                println!("GREET {:?}", state.data);
                let greeting = state.try_into_string().unwrap();
                Ok(format!("{} {}!", greeting, who))
            }
            ng_register_command!(cr, greet(state, who:String));

            fn template(state:State<Value>, mut args: NGCommandArguments<Value>, context:NGContext<SimpleNGEnvironment<Value>>)
            ->  std::pin::Pin<
            Box<dyn core::future::Future<Output = Result<Value, Error>> + Send + Sync + 'static>,
            >{
                Box::pin(async move{
                    
                let template = state.try_into_string()?;
                let template = parse_simple_template(template)?;
                let envref = context.clone_payload();
                //let result = NGPlanInterpreter::new(envref).evaluate_simple_template(&template).await?;
                //Ok(Value::from_string(result))
                Ok(Value::none())
                })
            }

            cr.register_async_command("template", template);
        }

        let mut pi = NGPlanInterpreter::new(env.to_ref());

        let template = parse_simple_template("*** $-R/hello.txt/-/greet-world$ ***")?;
        let result = pi.evaluate_simple_template(&template).await?;
        assert_eq!(result, "*** Hello TEXT world! ***");
        Ok(())
    }

}
