use futures::FutureExt;

use crate::{
    assets2::{AssetManager, AssetRef},
    command_metadata::CommandKey,
    commands2::{CommandArguments, CommandExecutor},
    context2::{Context, EnvRef, Environment},
    error::Error,
    metadata::Status,
    parse::{SimpleTemplate, SimpleTemplateElement},
    plan::{Plan, PlanBuilder, Step},
    query::{Key, TryToQuery},
    state::State,
    value::ValueInterface,
};
pub fn make_plan<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
) -> Result<Plan, Error> {
    let rquery = query.try_to_query();
    let cmr = envref.get_command_metadata_registry();
    let mut pb = PlanBuilder::new(rquery?, cmr);
    pb.build()
}

// TODO: Implement check plan, which would make a quick deep check of the plan and return list of errors or warnings

pub fn apply_plan<E: Environment>(
    plan: Plan,
    envref: EnvRef<E>,
    context: Context<E>,
    input_state: State<<E as Environment>::Value>,
) -> std::pin::Pin<
    Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send + 'static>,
>
//impl std::future::Future<Output = Result<State<<E as NGEnvironment>::Value>, Error>>
{
    async move {
        let mut state = input_state;
        for i in 0..plan.len() {
            let step = plan[i].clone();
            let ctx = context.clone_context().await;
            let envref1= envref.clone();
            let output_state = async move { 
                do_step(envref1, step, state, ctx).await }.await?;
            state = output_state.with_metadata(context.get_metadata().await?.into());
        }
        if state.status().is_none() {
            // TODO: This is a hack, should be done via context and asset
            state.set_status(Status::Ready)?; // TODO: status should be changed via the context and asset
        }
        Ok(state)
    }
    .boxed()
}

pub fn do_step<E: Environment>(
    envref: EnvRef<E>,
    step: Step,
    input_state: State<<E as Environment>::Value>,
    context: Context<E>,
) -> std::pin::Pin<
    Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send + 'static>,
>
//BoxFuture<'static, Result<State<<E as NGEnvironment>::Value>, Error>>
{
    match step {
        Step::GetResource(key) => async move {
            let store = envref.get_async_store();
            let (data, metadata) = store.get(&key).await?;
            let value = <<E as Environment>::Value as ValueInterface>::from_bytes(data);
            Ok(State::new().with_data(value).with_metadata(metadata))
        }
        .boxed(),
        Step::GetResourceMetadata(key) => async move {
            let store = envref.get_async_store();
            let metadata_value = store.get_metadata(&key).await?;
            if let Some(metadata_value) = metadata_value.metadata_record() {
                let value =
                    <<E as Environment>::Value as ValueInterface>::from_metadata(metadata_value);
                Ok(State::new().with_data(value))
            } else {
                Err(Error::general_error(format!(
                    "Resource metadata is in legacy format: {}",
                    key
                )))
            }
        }
        .boxed(),
        Step::Evaluate(q) => {
            let query = q.clone();
            async move {
                let context = Context::new(
                    AssetRef::new_from_recipe(0, (&query).into(), envref.clone()),
                )
                .await; // TODO: Fix assetref
                let plan = make_plan(envref.clone(), query)?;
                let input_state = State::<<E as Environment>::Value>::new();
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
            let mut arguments = CommandArguments::<E>::new(parameters.clone());
            arguments.action_position = position.clone();
            async move {
                for (i, param) in parameters.0.iter().enumerate() {
                    if let Some(arg_query) = param.link() {
                        let arg_value = envref
                            .get_asset_manager()
                            .get_asset(&arg_query)
                            .await?
                            .get()
                            .await?;
                        arguments.set_value(i, arg_value.data.clone());
                    }
                }
                let ce = envref.get_command_executor();
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
                        context.clone_context().await,
                    )
                    .await;
                match result {
                    Ok(result) => {
                        let state = State::<<E as Environment>::Value>::new()
                            .with_data(result)
                            .with_metadata(context.get_metadata().await?.into());
                        Ok(state)
                    }
                    Err(e) => Err(e),
                }
            }
            .boxed()
        }
        Step::Filename(name) => async move {
            context.set_filename(&name.name);
            Ok(input_state)
        }
        .boxed(),
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
        Step::GetAsset(key) => async move {
            let envref1 = envref.clone();
            let asset_store = envref1.get_asset_manager();
            let asset = asset_store.get(&key).await?;
            let asset_state = asset.get().await?;
            Ok(asset_state)
        }
        .boxed(),
        Step::GetAssetBinary(_key) => todo!(),
        Step::GetAssetMetadata(_key) => todo!(),
        Step::UseKeyValue(_key) => todo!(),
    }
}

pub fn evaluate_plan<E: Environment>(
    plan: Plan,
    envref: EnvRef<E>,
    assetref: AssetRef<E>,
) -> std::pin::Pin<
    Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send + 'static>,
> {
    async move {
        let context = Context::new(assetref).await;
        apply_plan(
            plan,
            envref.clone(),
            context,
            State::<<E as Environment>::Value>::new(),
        )
        .await
    }
    .boxed()
}

pub fn evaluate<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
    cwd_key: Option<Key>,
) -> std::pin::Pin<Box<dyn core::future::Future<Output = Result<State<E::Value>, Error>> + Send>> {
    let rquery = query.try_to_query();
    async move {
        let query = rquery?;
        let plan = make_plan(envref.clone(), query.clone())?;
        let assetref = AssetRef::new_from_recipe(0, (&query).into(), envref.clone());
        let context = Context::new(assetref).await;
        context.set_cwd_key(cwd_key);
        apply_plan(
            plan,
            envref.clone(),
            context,
            State::<<E as Environment>::Value>::new(),
        )
        .await
    }
    .boxed()
}

pub fn evaluate_simple_template<E: Environment>(
    envref: EnvRef<E>,
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
                            .with_query(q)
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

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use crate as liquers_core;
    use crate::command_metadata::CommandKey;
    use crate::context2::SimpleEnvironment;
    use crate::state::State;
    use crate::value::Value;
    use liquers_macro::*;

    #[tokio::test]
    async fn test_simple() -> Result<(), Box<dyn std::error::Error>> {
        let mut env = SimpleEnvironment::<Value>::new();

        // Register a command in the registry
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();

        let state = evaluate(envref.clone(), "test", None).await?;

        assert_eq!(state.try_into_string().unwrap(), "Hello, world!");
        Ok(())
    }

    #[tokio::test]
    async fn test_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = SimpleEnvironment<Value>;
        let mut env = SimpleEnvironment::<Value>::new();

        // Register "hello" command
        fn world(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("world"))
        }
        fn greet(state: &State<Value>, greet: String) -> Result<Value, Error> {
            let what = state.try_into_string()?;
            Ok(Value::from(format!("{greet}, {what}!")))
        }
        let cr = &mut env.command_registry;
        register_command_v2!(cr, fn world(state) -> result).expect("register_command failed");
        register_command_v2!(cr, fn greet(state, greet: String = "Hello") -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let state = evaluate(envref.clone(), "world/greet", None).await?;

        let value = state.try_into_string()?;
        assert_eq!(value, "Hello, world!");
        Ok(())
    }

    #[tokio::test]
    async fn test_async_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = SimpleEnvironment<Value>;
        let mut env = SimpleEnvironment::<Value>::new();

        // Register "hello" command
        fn world(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("world"))
        }
        async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
            let what = state.try_into_string()?;
            Ok(Value::from(format!("{greet}, {what}!")))
        }
        let cr = &mut env.command_registry;
        register_command_v2!(cr, fn world(state) -> result).expect("register_command failed");
        register_command_v2!(cr, async fn greet(state, greet: String = "Hello") -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let state = evaluate(envref.clone(), "world/greet", None).await?;

        let value = state.try_into_string()?;
        assert_eq!(value, "Hello, world!");
        Ok(())
    }

}
