use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::command_metadata::CommandKey;
use crate::commands2::{CommandArguments, CommandExecutor, NGCommandArguments, NGCommandExecutor};
use crate::context2::{
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
                let (data, metadata) = store.get(&key)?;
                let value = <<E as Environment>::Value as ValueInterface>::from_bytes(data);
                return Ok(State::new().with_data(value).with_metadata(metadata));
            }
            crate::plan::Step::GetAsset(key) => { // FIXME: This is not correct - just to satisfy old unittests
                let store = self.environment.get_store();
                let (data, metadata) = store.get(&key)?;
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
            Step::SetCwd(key) => todo!(),
            Step::GetAssetBinary(key) => todo!(),
            Step::GetAssetMetadata(key) => todo!(),
            Step::GetResource(key) => todo!(),
            Step::GetResourceMetadata(key) => todo!(),
            Step::GetNamedResource(key) => todo!(),
            Step::GetNamedResourceMetadata(key) => todo!(),
            Step::Evaluate(query) => todo!(),
            Step::Action {
                realm,
                ns,
                action_name,
                position,
                parameters,
            } => todo!(),
            Step::Filename(resource_name) => todo!(),
            Step::Info(_) => todo!(),
            Step::Warning(_) => todo!(),
            Step::Error(_) => todo!(),
            Step::Plan(plan) => todo!(),
            Step::UseKeyValue(key) => todo!(),
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
                let (data, metadata) = store.get(&key).await?;
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
            Step::SetCwd(key) => todo!(),
            Step::GetAsset(key) => todo!(),
            Step::GetAssetBinary(key) => todo!(),
            Step::GetAssetMetadata(key) => todo!(),
            Step::GetResource(key) => todo!(),
            Step::GetResourceMetadata(key) => todo!(),
            Step::GetNamedResource(key) => todo!(),
            Step::GetNamedResourceMetadata(key) => todo!(),
            Step::Evaluate(query) => todo!(),
            Step::Action {
                realm,
                ns,
                action_name,
                position,
                parameters,
            } => todo!(),
            Step::Filename(resource_name) => todo!(),
            Step::Info(_) => todo!(),
            Step::Warning(_) => todo!(),
            Step::Error(_) => todo!(),
            Step::Plan(plan) => todo!(),
            Step::UseKeyValue(key) => todo!(),
        }
        Ok(input_state)
    }
}

pub struct NGPlanInterpreter<E: NGEnvironment> {
    plan: Arc<tokio::sync::Mutex<Option<Plan>>>,
    environment: NGEnvRef<E>,
    step_number: usize,
    //state: Option<State<E::Value>>,
}

#[cfg(feature = "async_store")]
impl<E: NGEnvironment> NGPlanInterpreter<E> {
    pub fn new(environment: NGEnvRef<E>) -> Self {
        NGPlanInterpreter {
            plan: Arc::new(Mutex::new(None)),
            environment,
            step_number: 0,
            //state: None,
        }
    }

    pub async fn set_plan(&mut self, plan: Plan) {
        println!("set plan {:?}", plan);
        let mut p = self.plan.lock().await;
        *p = Some(plan);
        self.step_number = 0;
    }

    pub async fn make_plan<Q: TryToQuery>(envref: NGEnvRef<E>, query: Q) -> Result<Plan, Error> {
        let query = query.try_to_query()?;
        let env = envref.0.read().await;
        let cmr = env.get_command_metadata_registry();
        let mut pb = PlanBuilder::new(query, cmr);
        pb.build()
    }

    pub async fn set_query<Q: TryToQuery>(&mut self, query: Q) -> Result<(), Error> {
        let plan = Self::make_plan(self.environment.clone(), query).await?;
        self.set_plan(plan).await;
        Ok(())
    }

    pub async fn evaluate<Q: TryToQuery>(&mut self, query: Q) -> Result<State<E::Value>, Error> {
        let rquery = query.try_to_query();
        self.set_query(rquery?).await?;
        self.run().await
    }

    pub async fn apply(
        &mut self,
        context: NGContext<E>,
        input_state: State<<E as NGEnvironment>::Value>,
    ) -> Result<State<<E as NGEnvironment>::Value>, Error> {
        //let context = NGContext::new(self.environment.clone()).await;
        let rplan = self.plan.clone();
        async move {
            let p = rplan.lock().await;
            if let Some(plan) = (*p).clone() {
                let mut state = input_state;

                for i in 0..plan.len() {
                    let step = plan[i].clone();
                    let ctx = context.clone_context();
                    let envref = self.environment.clone();
                    let output_state =
                        async move { Self::do_step(envref, step, state, ctx).await }.await?;
                    state = output_state;
                }
                Ok(state)
            } else {
                Err(Error::general_error("No plan".to_string()))
            }
        }
        .boxed()
        .await
    }

    pub async fn apply_plan(
        plan: Plan,
        envref: NGEnvRef<E>,
        context: NGContext<E>,
        input_state: State<<E as NGEnvironment>::Value>,
    ) -> Result<State<<E as NGEnvironment>::Value>, Error> {
        async move {
            let mut state = input_state;
            for i in 0..plan.len() {
                let step = plan[i].clone();
                let ctx = context.clone_context();
                let envref_copy = envref.clone();
                let output_state =
                    async move { Self::do_step(envref_copy, step, state, ctx).await }.await?;
                state = output_state;
            }
            Ok(state)
        }
        .boxed()
        .await
    }

    pub async fn run_plan(
        plan: Plan,
        envref: NGEnvRef<E>,
    ) -> Result<State<<E as NGEnvironment>::Value>, Error> {
        Self::apply_plan(
            plan,
            envref.clone(),
            NGContext::new(envref).await,
            Self::initial_state(),
        )
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
                crate::plan::Step::GetAsset(key) => { // FIXME: This is not correct - just to satisfy old unittests
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
                Step::SetCwd(key) => todo!(),
                Step::GetAssetBinary(key) => todo!(),
                Step::GetAssetMetadata(key) => todo!(),
                Step::GetResource(key) => todo!(),
                Step::GetResourceMetadata(key) => todo!(),
                Step::GetNamedResource(key) => todo!(),
                Step::GetNamedResourceMetadata(key) => todo!(),
                Step::Evaluate(query) => todo!(),
                Step::Action {
                    realm,
                    ns,
                    action_name,
                    position,
                    parameters,
                } => todo!(),
                Step::Filename(resource_name) => todo!(),
                Step::Info(_) => todo!(),
                Step::Warning(_) => todo!(),
                Step::Error(_) => todo!(),
                Step::Plan(plan) => todo!(),
                Step::UseKeyValue(key) => todo!(),
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

pub mod ngi {
    use futures::FutureExt;

    use crate::{
        assets2::AssetInterface, command_metadata::CommandKey, commands2::{NGCommandArguments, NGCommandExecutor}, context2::{ActionContext, NGContext, NGEnvRef, NGEnvironment}, error::Error, parse::{SimpleTemplate, SimpleTemplateElement}, plan::{Plan, PlanBuilder, Step}, query::{Key, TryToQuery}, state::State, value::ValueInterface
    };

    pub fn make_plan<E: NGEnvironment, Q: TryToQuery>(
        envref: NGEnvRef<E>,
        query: Q,
    ) -> std::pin::Pin<Box<dyn core::future::Future<Output = Result<Plan, Error>> + Send + 'static>>
//impl std::future::Future<Output = Result<Plan, Error>>
    {
        let rquery = query.try_to_query();
        async move {
            let env = envref.0.read().await;
            let cmr = env.get_command_metadata_registry();
            let mut pb = PlanBuilder::new(rquery?, cmr);
            pb.build()
        }
        .boxed()
    }

    pub fn apply_plan<E: NGEnvironment>(
        plan: Plan,
        envref: NGEnvRef<E>,
        context: NGContext<E>,
        input_state: State<<E as NGEnvironment>::Value>,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send + 'static>,
    >
//impl std::future::Future<Output = Result<State<<E as NGEnvironment>::Value>, Error>>
    {
        async move {
            let mut state = input_state;
            for i in 0..plan.len() {
                let step = plan[i].clone();
                let ctx = context.clone_context();
                let envref_copy = envref.clone();
                let output_state =
                    async move { do_step(envref_copy, step, state, ctx).await }.await?;
                state = output_state.with_metadata(context.get_metadata().into());
            }
            Ok(state)
        }
        .boxed()
    }

    pub fn do_step<E: NGEnvironment>(
        envref: NGEnvRef<E>,
        step: Step,
        input_state: State<<E as NGEnvironment>::Value>,
        context: NGContext<E>,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send + 'static>,
    >
//BoxFuture<'static, Result<State<<E as NGEnvironment>::Value>, Error>>
    {
        match step {
            Step::GetResource(key) => async move {
                let store = envref.get_async_store().await;
                let (data, metadata) = store.get(&key).await?;
                let value = <<E as NGEnvironment>::Value as ValueInterface>::from_bytes(data);
                Ok(State::new().with_data(value).with_metadata(metadata))
            }
            .boxed(),
            Step::GetResourceMetadata(_) => todo!(),
            Step::GetNamedResource(_) => todo!(),
            Step::GetNamedResourceMetadata(_) => todo!(),
            Step::Evaluate(q) => {
                let query = q.clone();
                async move {
                    let context = NGContext::new(envref.clone()).await;
                    let plan = make_plan(envref.clone(), query).await?;
                    let input_state = State::<<E as NGEnvironment>::Value>::new();
                    apply_plan(plan, envref.clone(), context, input_state).await
                }
                .boxed()
            }
            Step::Action {
                ref realm,
                ref ns,
                ref action_name,
                position,
                parameters,
            } => {
                let commannd_key = CommandKey::new(realm, ns, action_name);
                let mut arguments =
                    NGCommandArguments::<<E as NGEnvironment>::Value>::new(parameters.clone());
                arguments.action_position = position.clone();
                async move {
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
                    let result = ce
                        .execute_async(
                            &commannd_key,
                            input_state,
                            arguments,
                            context.clone_context(),
                        )
                        .await;
                    match result {
                        Ok(result) => {
                            let state = State::<<E as NGEnvironment>::Value>::new()
                                .with_data(result)
                                .with_metadata(context.get_metadata().into());
                            Ok(state)
                        }
                        Err(e) => Err(e),
                    }
                }
                .boxed()
            }
            Step::Filename(name) => {
                context.set_filename(name.name.clone());
                async move { Ok(input_state) }.boxed()
            }
            Step::Info(m) => {
                context.info(&m);
                async move { Ok(input_state) }.boxed()
            }
            Step::Warning(m) => {
                context.warning(&m);
                async move { Ok(input_state) }.boxed()
            }
            Step::Error(m) => {
                context.error(&m);
                async move { Ok(input_state) }.boxed()
            }
            Step::SetCwd(key) => {
                context.set_cwd_key(Some(key.clone()));
                async move { Ok(input_state) }.boxed()
            }
            Step::Plan(plan) => async move {
                let state = apply_plan(plan, envref.clone(), context, input_state).await?;
                Ok(state)
            }
            .boxed(),
            Step::GetAsset(key) => {
                async move {
                    let envref1 = envref.clone();
                    let asset_store = envref1.0.read().await.get_asset_store();
                    let asset = asset_store.get(&key).await?;
                    let asset_state = asset.get_state(envref.clone()).await?;
                    Ok(asset_state)
                }
                .boxed()
            },
            Step::GetAssetBinary(key) => todo!(),
            Step::GetAssetMetadata(key) => todo!(),
            Step::UseKeyValue(key) => todo!(),
        }
    }

    pub fn evaluate_plan<E: NGEnvironment>(
        plan: Plan,
        envref: NGEnvRef<E>,
        cwd_key: Option<Key>,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send + 'static>,
    >
//Result<State<<E as NGEnvironment>::Value>, Error>
    {
        async move {
            let mut context = NGContext::new(envref.clone()).await;
            context.set_cwd_key(cwd_key);
            apply_plan(
                plan,
                envref.clone(),
                context,
                State::<<E as NGEnvironment>::Value>::new(),
            )
            .await
        }
        .boxed()
    }

    pub fn evaluate<E: NGEnvironment, Q: TryToQuery>(
        envref: NGEnvRef<E>,
        query: Q,
        cwd_key: Option<Key>,
    ) -> std::pin::Pin<Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send>>
    {
        let rquery = query.try_to_query();
        async move {
            let plan = make_plan(envref.clone(), rquery?).await?;
            let mut context = NGContext::new(envref.clone()).await;
            context.set_cwd_key(cwd_key);
            apply_plan(
                plan,
                envref.clone(),
                NGContext::new(envref.clone()).await,
                State::<<E as NGEnvironment>::Value>::new(),
            )
            .await
        }
        .boxed()
    }

    pub fn evaluate_simple_template<E: NGEnvironment>(
        envref: NGEnvRef<E>,
        template: SimpleTemplate,
        cwd_key: Option<Key>,
    ) -> std::pin::Pin<Box<dyn core::future::Future<Output = Result<String, Error>> + Send>> {
        let mut result = String::new();
        async move {
            for element in template.0.iter() {
                match element {
                    SimpleTemplateElement::Text(t) => {
                        result.push_str(t);
                    }
                    SimpleTemplateElement::ExpandQuery(q) => {
                        let state = evaluate(envref.clone(), q, cwd_key.clone()).await?;
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
        .boxed()
    }
}
