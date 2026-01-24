use std::sync::Arc;

use futures::FutureExt;

use crate::{
    assets::{AssetManager, AssetRef},
    command_metadata::CommandKey,
    commands::{CommandArguments, CommandExecutor},
    context::{Context, EnvRef, Environment},
    error::Error,
    metadata::{LogEntry, Metadata},
    parse::{SimpleTemplate, SimpleTemplateElement},
    plan::{ParameterValue, Plan, PlanBuilder, ResolvedParameterValues, Step},
    query::{Key, Query, TryToQuery},
    recipes::Recipe,
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
    input_state: State<E::Value>,
    context: Context<E>,
    envref: EnvRef<E>,
) -> std::pin::Pin<
    Box<dyn core::future::Future<Output = Result<Arc<E::Value>, Error>> + Send + 'static>,
>
//impl std::future::Future<Output = Result<State<<E as NGEnvironment>::Value>, Error>>
{
    async move {
        let mut state = input_state;
        for i in 0..plan.len() {
            println!("Applying step {}/{}: {:?}", i + 1, plan.len(), &plan[i]);
            let step = plan[i].clone();
            let envref1 = envref.clone();
            let context1 = context.clone();
            let res = async move { do_step(step, state, context1, envref1).await }.await?;
            state = State::new()
                .with_data((*res).clone())
                .with_metadata(context.get_metadata().await?.into());
        }
        Ok(state.data.clone())
    }
    .boxed()
}

pub fn do_step<E: Environment>(
    step: Step,
    input: State<E::Value>,
    context: Context<E>,
    envref: EnvRef<E>,
) -> std::pin::Pin<
    Box<
        dyn core::future::Future<Output = Result<Arc<<E as Environment>::Value>, Error>>
            + Send
            + 'static,
    >,
> {
    match step {
        Step::GetResource(key) => async move {
            context.add_log_entry(
                LogEntry::info("Getting resource".to_string()).with_query(key.clone().into()),
            )?;
            let store = envref.get_async_store();
            let (data, _metadata) = store.get(&key).await?;
            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_bytes(data),
            ))
        }
        .boxed(),
        Step::GetResourceMetadata(key) => async move {
            context.add_log_entry(
                LogEntry::info("Getting resource metadata".to_string())
                    .with_query(key.clone().into()),
            )?;
            let store = envref.get_async_store();
            match store.get_metadata(&key).await? {
                Metadata::MetadataRecord(mr) => Ok(Arc::new(
                    <<E as Environment>::Value as ValueInterface>::from_metadata(mr),
                )),
                Metadata::LegacyMetadata(json_value) => {
                    context.add_log_entry(
                        LogEntry::warning("Resource metadata is in legacy format".to_string())
                            .with_query(key.into()),
                    )?;
                    let metadata_value =
                        <<E as Environment>::Value as ValueInterface>::try_from_json_value(
                            &json_value,
                        )?;
                    Ok(Arc::new(metadata_value))
                }
            }
        }
        .boxed(),
        Step::GetResourceDirectory(key) => async move {
            println!("Getting resource directory for key {:?}", key);
            let store = envref.get_async_store();
            let d = store.listdir_asset_info(&key).await?;
            println!("Got resource directory: {:?}", d);

            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_asset_info(d),
            ))
        }
        .boxed(),
        Step::Evaluate(q) => {
            let query = q.clone();
            async move {
                let asset = envref.get_asset_manager().get_asset(&query).await?;
                asset.get().await.map(|s| s.data.clone())
            }
            .boxed()
        }
        Step::Action {
            realm,
            ns,
            action_name,
            position,
            parameters,
        } => async move {
            let command_key = CommandKey::new(&realm, &ns, &action_name);
            let mut arguments = CommandArguments::<E>::new(parameters.clone());
            arguments.action_position = position.clone();
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
            ce.execute_async(&command_key, input, arguments, context)
                .await
                .map_err(|e| {
                    // Only set command_key if not already set (to preserve inner command errors)
                    if e.command_key.is_none() {
                        e.with_command_key(&command_key).with_position(&position)
                    } else {
                        e.with_position(&position)
                    }
                })
                .map(|v| Arc::new(v))
        }
        .boxed(),

        Step::Filename(name) => async move {
            context.set_filename(&name.name).await?;
            Ok(input.data.clone())
        }
        .boxed(),
        Step::Info(m) => async move {
            context.info(&m)?;
            Ok(input.data.clone())
        }
        .boxed(),
        Step::Warning(m) => async move {
            context.warning(&m)?;
            Ok(input.data.clone())
        }
        .boxed(),
        Step::Error(m) => async move {
            context.error(&m)?;
            Ok(input.data.clone())
        }
        .boxed(),
        Step::SetCwd(key) => async move {
            context.set_cwd_key(Some(key));
            Ok(input.data.clone())
        }
        .boxed(),
        Step::Plan(plan) => async move {
            todo!("Implement nested plan");
            //let state = apply_plan(plan, envref.clone(), context, input_state).await?;
        }
        .boxed(),
        Step::GetAsset(key) => async move {
            let envref1 = envref.clone();
            let asset_store = envref1.get_asset_manager();
            let asset = asset_store.get(&key).await?;
            let asset_state = asset.get().await?;
            Ok(asset_state.data.clone())
        }
        .boxed(),
        Step::GetAssetBinary(_key) => todo!(),
        Step::GetAssetMetadata(key) => async move {
            let envref1 = envref.clone();
            let asset_store = envref1.get_asset_manager();
            let asset = asset_store.get(&key).await?;
            let asset_state = asset.get().await?;
            match &*asset_state.metadata {
                Metadata::LegacyMetadata(value) => {
                    context.add_log_entry(
                        LogEntry::warning("Asset metadata is in legacy format".to_string())
                            .with_query(key.into()),
                    )?;
                    Ok(Arc::new(
                        <<E as Environment>::Value as ValueInterface>::try_from_json_value(&value)?,
                    ))
                }
                Metadata::MetadataRecord(metadata_record) => Ok(Arc::new(
                    <<E as Environment>::Value as ValueInterface>::from_metadata(
                        metadata_record.clone(),
                    ),
                )),
            }
        }
        .boxed(),
        Step::GetAssetRecipe(key) => async move {
            let envref1 = envref.clone();
            let asset_store = envref1.get_asset_manager();
            if let Some(recipe) = asset_store.recipe_opt(&key).await? {
                let recipe_value =
                    <<E as Environment>::Value as ValueInterface>::from_recipe(recipe);
                Ok(Arc::new(recipe_value))
            } else {
                let none = <<E as Environment>::Value as ValueInterface>::none();
                Ok(Arc::new(none))
            }
        }
        .boxed(),
        Step::GetAssetDirectory(key) => async move {
            let envref1 = envref.clone();
            let asset_manager = envref1.get_asset_manager();
            let d = asset_manager.listdir_asset_info(&key).await?;
            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_asset_info(d),
            ))
        }
        .boxed(),
        Step::UseKeyValue(key) => async move {
            let value = E::Value::from_key(&key);
            Ok(Arc::new(value))
        }
        .boxed(),
    }
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
        let assetref = AssetRef::new_temporary(envref.clone());
        let context = Context::new(assetref).await;
        context.set_cwd_key(cwd_key);
        /*
        apply_plan(
            plan,
            envref.clone(),
            context,
            State::<<E as Environment>::Value>::new(),
        )
        .await
        */
        let input_state = State::<<E as Environment>::Value>::new();
        let res = apply_plan(plan, input_state, context.clone(), envref).await?;
        Ok(State::new()
            .with_data((*res).clone())
            .with_metadata(context.get_metadata().await?.into()))
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

pub(crate) trait IsVolatile<E: Environment> {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error>;
}

impl<E: Environment> IsVolatile<E> for ParameterValue {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        if let Some(link) = self.link() {
            Box::pin(link.is_volatile(env)).await
        } else {
            Ok(false)
        }
    }
}

impl<E: Environment> IsVolatile<E> for ResolvedParameterValues {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        for param in self.0.iter() {
            if param.is_volatile(env.clone()).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<E: Environment> IsVolatile<E> for Plan {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        for step in self.steps.iter() {
            if Box::pin(step.is_volatile(env.clone())).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<E: Environment> IsVolatile<E> for Recipe {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        if self.volatile {
            return Ok(true);
        }
        let plan = self.to_plan(env.get_command_metadata_registry())?;
        plan.is_volatile(env).await
    }
}

impl<E: Environment> IsVolatile<E> for Query {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        let plan = make_plan(env.clone(), self.clone())?;
        plan.is_volatile(env).await
    }
}

impl<E: Environment> IsVolatile<E> for Step {
    async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
        match self {
            Step::Action {
                realm,
                ns,
                action_name,
                position: _,
                parameters,
            } => {
                if let Some(cmd) =
                    env.get_command_metadata_registry()
                        .find_command(&realm, &ns, action_name)
                {
                    if cmd.volatile {
                        return Ok(true);
                    }
                    if parameters.is_volatile(env).await? {
                        return Ok(true);
                    }
                    return Ok(false);
                } else {
                    Ok(false)
                }
            }
            Step::GetAsset(key) => env.get_asset_manager().is_volatile(&key).await,
            Step::GetAssetBinary(key) => env.get_asset_manager().is_volatile(&key).await,
            Step::GetAssetMetadata(key) => env.get_asset_manager().is_volatile(&key).await,
            Step::GetAssetRecipe(key) => {
                env.get_asset_manager().is_volatile(&key).await // TODO: not sure when a recipe itself is volatile
            }
            Self::GetAssetDirectory(_) => {
                Ok(true) // TODO: Is asset directory volatilile?
            }
            Step::GetResource(key) => {
                eprintln!("ADD SUPPORT FOR RESOURCE VOLATILITY CHECK! (A)");
                // TODO: support for resource volatility check
                Ok(false)
            }
            Step::GetResourceMetadata(key) => {
                eprintln!("ADD SUPPORT FOR RESOURCE VOLATILITY CHECK! (B)");
                // TODO: support for resource volatility check
                Ok(false)
            }
            Self::GetResourceDirectory(_) => {
                Ok(true) // TODO: Is directory volatilile?
            }
            Step::Evaluate(query) => query.is_volatile(env).await,
            Step::Filename(_) => Ok(false),
            Step::Info(_) => Ok(false),
            Step::Warning(_) => Ok(false),
            Step::Error(_) => Ok(false),
            Step::Plan(plan) => plan.is_volatile(env).await,
            Step::SetCwd(_) => Ok(false),
            Step::UseKeyValue(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use crate as liquers_core;
    use crate::command_metadata::CommandKey;
    use crate::context::{SimpleEnvironment, SimpleEnvironmentWithPayload};
    use crate::metadata::ProgressEntry;
    use crate::parse::parse_query;
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
        register_command!(cr, fn world(state) -> result).expect("register_command failed");
        register_command!(cr, fn greet(state, greet: String = "Hello") -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let state = evaluate(envref.clone(), "world/greet", None).await?;

        let value = state.try_into_string()?;
        assert_eq!(value, "Hello, world!");
        Ok(())
    }

    #[tokio::test]
    async fn test_generic_hello_world() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = SimpleEnvironment<Value>;
        let mut env = SimpleEnvironment::<Value>::new();

        // Register "hello" command
        fn world<E: Environment>(
            _state: &State<E::Value>,
            context: Context<E>,
        ) -> Result<Value, Error> {
            context.info("Generating 'world'")?;
            Ok(Value::from("world"))
        }
        fn greet<E: Environment>(
            state: &State<E::Value>,
            greet: String,
            context: Context<E>,
        ) -> Result<Value, Error> {
            let what = state.try_into_string()?;
            context.info(&format!("Greeting {what}"))?;
            Ok(Value::from(format!("{greet}, {what}!")))
        }
        let cr = &mut env.command_registry;
        register_command!(cr, fn world(state, context) -> result).expect("register_command failed");
        register_command!(cr, fn greet(state, greet: String = "Hello", context) -> result)
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
        register_command!(cr, fn world(state) -> result).expect("register_command failed");
        register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let state = evaluate(envref.clone(), "world/greet", None).await?;

        let value = state.try_into_string()?;
        assert_eq!(value, "Hello, world!");
        Ok(())
    }

    #[tokio::test]
    async fn test_context_evaluate() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = SimpleEnvironment<Value>;
        let mut env = SimpleEnvironment::<Value>::new();

        // Register "hello" command
        fn world(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("world"))
        }
        fn moon(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("moon"))
        }
        async fn greet(
            state: State<Value>,
            greet: String,
            context: Context<CommandEnvironment>,
        ) -> Result<Value, Error> {
            let what = state.try_into_string()?;
            context.info(&format!("Greeting {what}"))?;
            let moon = context.evaluate(&parse_query("moon").unwrap()).await?;
            let moon_text = moon.get().await?.try_into_string()?;
            Ok(Value::from(format!("{greet}, {what} from {moon_text}!")))
        }
        let cr = &mut env.command_registry;
        register_command!(cr, fn world(state) -> result).expect("register_command failed");
        register_command!(cr, fn moon(state) -> result).expect("register_command failed");
        register_command!(cr, async fn greet(state, greet: String = "Hello", context) -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let state = evaluate(envref.clone(), "world/greet", None).await?;

        let value = state.try_into_string()?;
        assert_eq!(value, "Hello, world from moon!");
        Ok(())
    }

    #[tokio::test]
    async fn test_context_apply() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = SimpleEnvironment<Value>;
        let mut env = SimpleEnvironment::<Value>::new();

        // Register "hello" command
        fn world(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("world"))
        }
        fn upper(state: &State<Value>) -> Result<Value, Error> {
            let txt = state.try_into_string()?;
            Ok(Value::from(txt.to_uppercase()))
        }
        async fn greet(
            state: State<Value>,
            greet: String,
            context: Context<CommandEnvironment>,
        ) -> Result<Value, Error> {
            println!("greet command called");
            let what = state.try_into_string()?;
            context.info(&format!("Greeting {what}"))?;
            let upper = context
                .apply(&parse_query("upper").unwrap(), what.into())
                .await?;
            let upper_text = upper.get().await?.try_into_string()?;
            context.progress(ProgressEntry::done("OK, done".to_owned()))?;
            Ok(Value::from(format!("{greet}, {upper_text}!")))
        }
        let cr = &mut env.command_registry;
        register_command!(cr, fn world(state) -> result).expect("register_command failed");
        register_command!(cr, fn upper(state) -> result).expect("register_command failed");
        register_command!(cr, async fn greet(state, greet: String = "Hello", context) -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let asset = envref.evaluate("world/greet-Ciao").await?;
        let state = asset.get().await?;
        println!("Metadata: {:?}", state.metadata);

        let value = state.try_into_string()?;
        assert_eq!(value, "Ciao, WORLD!");
        assert!(state.metadata.primary_progress().is_done());
        Ok(())
    }

    #[tokio::test]
    async fn test_evaluate_immediately() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = SimpleEnvironmentWithPayload<Value, String>;
        let mut env = SimpleEnvironmentWithPayload::<Value, String>::new();

        fn word(_state: &State<Value>, payload: String) -> Result<Value, Error> {
            Ok(Value::from(format!("{payload}")))
        }
        fn upper(state: &State<Value>) -> Result<Value, Error> {
            let txt = state.try_into_string()?;
            Ok(Value::from(txt.to_uppercase()))
        }
        async fn greet(
            state: State<Value>,
            greet: String,
            context: Context<CommandEnvironment>,
        ) -> Result<Value, Error> {
            let what = state.try_into_string()?;
            context.info(&format!("Greeting {what}"))?;
            let upper = context
                .apply(&parse_query("upper").unwrap(), what.into())
                .await?;
            let upper_text = upper.get().await?.try_into_string()?;
            context.progress(ProgressEntry::done("OK, done".to_owned()))?;
            Ok(Value::from(format!("{greet}, {upper_text}!")))
        }
        let cr = &mut env.command_registry;
        register_command!(cr, fn word(state, payload: String injected) -> result)
            .expect("register_command failed");
        register_command!(cr, fn upper(state) -> result).expect("register_command failed");
        register_command!(cr, async fn greet(state, greet: String = "Hello", context) -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let asset = envref
            .evaluate_immediately("word/greet-Ciao", "Earth".into())
            .await?;
        let state = asset.get().await?;
        println!("Metadata: {:?}", state.metadata);

        let value = state.try_into_string()?;
        assert_eq!(value, "Ciao, EARTH!");
        assert!(state.metadata.primary_progress().is_done());
        Ok(())
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_dir() {
        use crate::context::{EnvRef, Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::store::{AsyncStoreWrapper, MemoryStore, Store};
        use crate::value::Value;

        // Create a MemoryStore and populate it with recipes.yaml
        let memory_store = MemoryStore::new(&Key::new());

        // Create a recipe list
        let mut recipe_list = crate::recipes::RecipeList::new();
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/hello/test.txt/-/file1.txt".to_string(),
                "Test Recipe".to_string(),
                "A test recipe".to_string(),
            )
            .unwrap(),
        );
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/data/another.json/-/file2.txt".to_string(),
                "Another Recipe".to_string(),
                "Another test recipe".to_string(),
            )
            .unwrap(),
        );

        // Serialize to YAML
        let yaml_content = serde_yaml::to_string(&recipe_list).unwrap();
        println!("recipes.yaml content:\n{}", yaml_content);

        // Store the recipes.yaml in the MemoryStore at folder/recipes.yaml
        let recipes_key = parse_key("folder/recipes.yaml").unwrap();
        let metadata = Metadata::new();
        memory_store
            .set(&recipes_key, yaml_content.as_bytes(), &metadata)
            .unwrap();
        memory_store
            .set(
                &parse_key("hello/test.txt").unwrap(),
                "Hello, world!".as_bytes(),
                &metadata,
            )
            .unwrap();

        // Wrap the MemoryStore with AsyncStoreWrapper
        let async_store = AsyncStoreWrapper(memory_store);

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(async_store));
        env.with_recipe_provider(Box::new(crate::recipes::DefaultRecipeProvider));

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir/folder").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //println!("Directory listing:\n{:?}", &*s.data);
        //println!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &*s.data {
            assert_eq!(a.len(), 3);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //println!("Names: {:?}", names);
            assert!(names.contains(&"recipes.yaml".to_string()));
            assert!(names.contains(&"file1.txt".to_string()));
            assert!(names.contains(&"file2.txt".to_string()));
        } else {
            panic!("Expected AssetInfo value");
        }
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_dir_no_recipe_yaml() {
        use crate::context::{EnvRef, Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::store::{AsyncStoreWrapper, MemoryStore, Store};
        use crate::value::Value;

        // Create a MemoryStore and populate it with recipes.yaml
        let memory_store = MemoryStore::new(&Key::new());

        memory_store
            .set(
                &parse_key("folder/test.txt").unwrap(),
                "Hello, world!".as_bytes(),
                &Metadata::new(),
            )
            .unwrap();

        // Wrap the MemoryStore with AsyncStoreWrapper
        let async_store = AsyncStoreWrapper(memory_store);

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(async_store));
        env.with_recipe_provider(Box::new(crate::recipes::DefaultRecipeProvider));

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir/folder").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //println!("Directory listing:\n{:?}", &*s.data);
        //println!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &*s.data {
            assert_eq!(a.len(), 1);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //println!("Names: {:?}", names);
            assert!(names.contains(&"test.txt".to_string()));
        } else {
            panic!("Expected AssetInfo value");
        }

        let a = envref.evaluate("-R-dir").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //println!("Directory listing:\n{:?}", &*s.data);
        //println!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &*s.data {
            assert_eq!(a.len(), 1);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //println!("Names: {:?}", names);
            assert!(names.contains(&"folder".to_string()));
        } else {
            panic!("Expected AssetInfo value");
        }

    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_dir_no_recipe_provider() {
        use crate::context::{EnvRef, Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::store::{AsyncStore, AsyncStoreWrapper, MemoryStore, Store};
        use crate::value::Value;

        // Create a MemoryStore and populate it with recipes.yaml
        let memory_store = MemoryStore::new(&Key::new());


        // Wrap the MemoryStore with AsyncStoreWrapper
        let async_store = AsyncStoreWrapper(memory_store);
        async_store.set(
            &parse_key("file1.txt").unwrap(),
            "File 1 contents".as_bytes(),
            &Metadata::new(),
        ).await.unwrap();

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(async_store));

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //println!("Directory listing:\n{:?}", &*s.data);
        //println!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &*s.data {
            assert_eq!(a.len(), 1);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //println!("Names: {:?}", names);
            assert!(names.contains(&"file1.txt".to_string()));
        } else {
            panic!("Expected AssetInfo value");
        }
    }

}
