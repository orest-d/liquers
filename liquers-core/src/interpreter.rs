use std::collections::HashSet;
use std::sync::Arc;

use crate::maybe_send::MaybeBoxed;
use futures::FutureExt;

use crate::{
    assets::{AssetManager, AssetRef},
    command_metadata::{CommandKey, PayloadRequirement},
    commands::{CommandArguments, CommandExecutor},
    context::{Context, EnvRef, Environment},
    error::Error,
    metadata::{DependencyRecord, LogEntry, Metadata, Version},
    parse::{SimpleTemplate, SimpleTemplateElement},
    plan::{
        has_expirable_dependencies, has_volatile_dependencies, ParameterValue, Plan, PlanBuilder,
        ResolvedParameterValues, Step,
    },
    query::{CwdCursor, Key, Query, TryToQuery, RELATIVE_WITHOUT_CWD_WARNING},
    recipes::Recipe,
    state::State,
    value::ValueInterface,
};

/// Complete plan setup after `recipe.to_plan()`:
/// - Populates `plan.dependencies` via async `find_dependencies` (which `to_plan` cannot do
///   because it is synchronous).
/// - May update `plan.is_volatile` if any dependency asset is volatile.
/// - May update `plan.expires` based on dependency recipe expiration policies.
/// - Seeds `context.pending_dependencies` with all plan deps at `Version(0)`.
/// - Registers dependency edges in the `DependencyManager` for keyed plans.
///
/// The supplied plan must be fresh and unfinalized. Callers needing another entry CWD must
/// rebuild from the source query or recipe rather than finalizing the same plan twice. This must
/// be called between `recipe.to_plan()` and `apply_plan()` in every `apply_recipe` implementation.
pub async fn finalize_plan<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
    context: &Context<E>,
) -> Result<(), Error> {
    // Freeze before any analysis: once every operand is absolute, the dependency and
    // pre-scheduling walks observe exactly what execution will, instead of each re-deriving the
    // working key with its own cursor.
    let initial_cwd = context.get_cwd_key();
    let (_, defaulted_to_root) = plan.freeze_cwd(initial_cwd.clone())?;
    // Warn only when the fallback was actually used, and only once — the same contract
    // `schedule_plan_dependencies` honours. A plan with no relative operand stays silent.
    if defaulted_to_root && context.install_logical_root_if_unset() {
        context.warning(RELATIVE_WITHOUT_CWD_WARNING)?;
    }
    has_volatile_dependencies(envref.clone(), plan, initial_cwd).await?;
    has_expirable_dependencies(envref.clone(), plan).await?;

    if !plan.is_volatile {
        for plan_dep in &plan.dependencies {
            context
                .add_dependency(DependencyRecord::new(plan_dep.key.clone(), Version::new(0)))
                .await;
        }
        if let Some(key) = context.owner_key().await? {
            let manager = envref.get_asset_manager();
            let _ = manager
                .register_plan_dependencies(&key, &plan.dependencies)
                .await;
        }
    }

    Ok(())
}

pub async fn make_plan<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
) -> Result<Plan, Error> {
    make_plan_with_cwd(envref, query, None).await
}

async fn make_plan_with_cwd<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
    initial_cwd: Option<Key>,
) -> Result<Plan, Error> {
    let rquery = query.try_to_query();
    let cmr = envref.get_command_metadata_registry();
    let mut pb = PlanBuilder::new(rquery?, cmr);

    // Phase 1: Build plan, check commands and 'v' instruction
    let mut plan = pb.build()?;

    // Phase 2: Check asset dependencies for volatility
    has_volatile_dependencies(envref.clone(), &mut plan, initial_cwd).await?;

    // Phase 3: Check asset dependencies for expiration
    has_expirable_dependencies(envref, &mut plan).await?;

    Ok(plan)
}

// TODO: Implement check plan, which would make a quick deep check of the plan and return list of errors or warnings
fn collect_parameter_dependency_queries(
    parameter: &ParameterValue,
    cursor: &mut CwdCursor,
    queries: &mut Vec<(Key, Query)>,
) {
    match parameter {
        ParameterValue::DefaultLink(_, query)
        | ParameterValue::ParameterLink(_, query, _)
        | ParameterValue::OverrideLink(_, query)
        | ParameterValue::EnumLink(_, query, _) => {
            let resolved = cursor.resolve_query_scoped(query);
            if let Some(key) = resolved.key() {
                queries.push((key, resolved));
            }
        }
        ParameterValue::MultipleParameters(_, values) => {
            for value in values {
                collect_parameter_dependency_queries(value, cursor, queries);
            }
        }
        ParameterValue::DefaultValue(_, _)
        | ParameterValue::ParameterValue(_, _, _)
        | ParameterValue::OverrideValue(_, _)
        | ParameterValue::Placeholder(_)
        | ParameterValue::Injected(_)
        | ParameterValue::None => {}
    }
}

fn schedule_plan_dependencies_from<'a, E: Environment>(
    plan: &'a Plan,
    context: &'a Context<E>,
    cursor: &'a mut CwdCursor,
    seen: &'a mut HashSet<Key>,
) -> crate::maybe_send::BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        let absolute_resource_step = plan.absolute_query_resource_step_index();
        for (step_index, step) in plan.steps.iter().enumerate() {
            match step {
                Step::GetAsset(key) | Step::GetAssetBinary(key) | Step::GetAssetMetadata(key) => {
                    let resolved = if absolute_resource_step == Some(step_index) {
                        key.to_absolute(&Key::new())
                    } else {
                        cursor.resolve_key(key)
                    };
                    if seen.insert(resolved.clone()) {
                        context.schedule_dependency_asset(&resolved.into()).await?;
                    }
                }
                Step::Evaluate(query) => {
                    let resolved = cursor.resolve_query_scoped(query);
                    if let Some(key) = resolved.key() {
                        if seen.insert(key) {
                            context.schedule_dependency_asset(&resolved).await?;
                        }
                    }
                }
                Step::Action { parameters, .. } => {
                    let mut queries = Vec::new();
                    for parameter in &parameters.0 {
                        collect_parameter_dependency_queries(parameter, cursor, &mut queries);
                    }
                    for (key, query) in queries {
                        if seen.insert(key) {
                            context.schedule_dependency_asset(&query).await?;
                        }
                    }
                }
                Step::Plan(nested_plan) => {
                    schedule_plan_dependencies_from(nested_plan, context, cursor, seen).await?;
                }
                Step::SetCwd(key) => {
                    if absolute_resource_step == Some(step_index) {
                        let resolved = key.to_absolute(&Key::new());
                        cursor.set_cwd_from(&resolved);
                    } else {
                        cursor.set_cwd_from(key);
                    }
                }
                Step::GetAssetRecipe(_)
                | Step::GetAssetDirectory(_)
                | Step::GetResource(_)
                | Step::GetResourceMetadata(_)
                | Step::GetResourceDirectory(_)
                | Step::UseQueryValue(_)
                | Step::UseKeyValue(_)
                | Step::Filename(_)
                | Step::Info(_)
                | Step::Warning(_)
                | Step::Error(_) => {}
            }
        }
        Ok(())
    })
}

/// Schedules known keyed dependencies without advancing the live Context to the simulated final
/// CWD. A missing entry CWD is installed as logical root only after the local walk releases all
/// cursor state, and its warning is delivered exactly once by the Context that wins installation.
async fn schedule_plan_dependencies<E: Environment>(
    plan: &Plan,
    context: &Context<E>,
) -> Result<(), Error> {
    let mut cursor = CwdCursor::new(context.get_cwd_key());
    let mut seen = HashSet::new();
    let result = schedule_plan_dependencies_from(plan, context, &mut cursor, &mut seen).await;
    if cursor.take_root_fallback() && context.install_logical_root_if_unset() {
        context.warning(RELATIVE_WITHOUT_CWD_WARNING)?;
    }
    result
}

fn resolve_absolute_query_resource_step(step: Step) -> Step {
    let resolve = |key: Key| key.to_absolute(&Key::new());
    match step {
        Step::GetAsset(key) => Step::GetAsset(resolve(key)),
        Step::GetAssetBinary(key) => Step::GetAssetBinary(resolve(key)),
        Step::GetAssetMetadata(key) => Step::GetAssetMetadata(resolve(key)),
        Step::GetAssetRecipe(key) => Step::GetAssetRecipe(resolve(key)),
        Step::GetAssetDirectory(key) => Step::GetAssetDirectory(resolve(key)),
        Step::GetResource(key) => Step::GetResource(resolve(key)),
        Step::GetResourceMetadata(key) => Step::GetResourceMetadata(resolve(key)),
        Step::GetResourceDirectory(key) => Step::GetResourceDirectory(resolve(key)),
        Step::SetCwd(key) => Step::SetCwd(resolve(key)),
        Step::UseKeyValue(key) => Step::UseKeyValue(resolve(key)),
        Step::Evaluate(query) => Step::Evaluate(query),
        Step::UseQueryValue(query) => Step::UseQueryValue(query),
        Step::Action {
            realm,
            ns,
            action_name,
            position,
            parameters,
        } => Step::Action {
            realm,
            ns,
            action_name,
            position,
            parameters,
        },
        Step::Filename(filename) => Step::Filename(filename),
        Step::Info(message) => Step::Info(message),
        Step::Warning(message) => Step::Warning(message),
        Step::Error(message) => Step::Error(message),
        Step::Plan(plan) => Step::Plan(plan),
    }
}

pub fn apply_plan<E: Environment>(
    mut plan: Plan,
    input_state: State<E::Value>,
    context: Context<E>,
    envref: EnvRef<E>,
) -> crate::maybe_send::BoxFuture<'static, Result<Arc<E::Value>, Error>>
//impl std::future::Future<Output = Result<State<<E as NGEnvironment>::Value>, Error>>
{
    if let Some(step_index) = plan.absolute_query_resource_step_index() {
        plan.steps[step_index] =
            resolve_absolute_query_resource_step(plan.steps[step_index].clone());
    }
    async move {
        // A plan declared `payload: required` must not run without one. This is the
        // authoritative gate: it covers every execution path — top-level `EnvRef::evaluate`,
        // keyed evaluation, and nested scheduling alike — where the per-entry-point checks
        // cover only the nested ones. Without it, a command that reads the payload through
        // `Context` and tolerates its absence would run despite declaring that it cannot.
        if plan.payload_required.is_required() && !context.has_payload() {
            return Err(Error::general_error(format!(
                "Query '{}' requires an evaluation payload, but the evaluation was started \
                 without one. Use EnvRef::evaluate_immediately to supply a payload, or remove \
                 the 'payload: required' declaration from the commands involved.",
                plan.query.encode()
            ))
            .with_query(&plan.query));
        }

        // Pre-pass: schedule known dependencies before executing steps (concurrency +
        // one inline drain so at-capacity dependencies begin executing up front).
        schedule_plan_dependencies(&plan, &context).await?;
        context.evaluate_local_queue().await?;
        let mut state = input_state;
        for i in 0..plan.len() {
            eprintln!("Applying step {}/{}: {:?}", i + 1, plan.len(), &plan[i]);
            let step = plan[i].clone();
            let envref1 = envref.clone();
            let context1 = context.clone();
            let res = async move { do_step(step, state, context1, envref1).await }.await?;
            state = State::new()
                .with_data((*res).clone())
                .with_metadata(context.get_metadata().await?.into());
        }
        state.value()
    }
    .maybe_boxed()
}

fn materialize_link_json<'a, E: Environment>(
    context: &'a Context<E>,
    query: &'a Query,
    position: Option<&'a crate::query::Position>,
) -> crate::maybe_send::BoxFuture<'a, Result<serde_json::Value, Error>> {
    Box::pin(async move {
        let attach_context = |error: Error| match position {
            Some(position) => error.with_position(position),
            None => error.with_query(query),
        };
        let resolved = context
            .resolve_query_from_cwd(query)
            .map_err(attach_context)?;
        let state = context
            .get_dependency_state(&resolved)
            .await
            .map_err(attach_context)?;
        let value = state.value().map_err(attach_context)?;
        value.try_into_json_value().map_err(attach_context)
    })
}

fn materialize_nested_parameter<'a, E: Environment>(
    context: &'a Context<E>,
    parameter: &'a ParameterValue,
) -> crate::maybe_send::BoxFuture<'a, Result<ParameterValue, Error>> {
    Box::pin(async move {
        match parameter {
            ParameterValue::DefaultLink(name, query) => Ok(ParameterValue::DefaultValue(
                name.clone(),
                materialize_link_json(context, query, None).await?,
            )),
            ParameterValue::ParameterLink(name, query, position) => {
                Ok(ParameterValue::ParameterValue(
                    name.clone(),
                    materialize_link_json(context, query, Some(position)).await?,
                    position.clone(),
                ))
            }
            ParameterValue::OverrideLink(name, query) => Ok(ParameterValue::OverrideValue(
                name.clone(),
                materialize_link_json(context, query, None).await?,
            )),
            ParameterValue::EnumLink(name, query, position) => Ok(ParameterValue::ParameterValue(
                name.clone(),
                materialize_link_json(context, query, Some(position)).await?,
                position.clone(),
            )),
            ParameterValue::MultipleParameters(name, values) => {
                let mut materialized = Vec::with_capacity(values.len());
                for value in values {
                    materialized.push(materialize_nested_parameter(context, value).await?);
                }
                Ok(ParameterValue::MultipleParameters(name.clone(), materialized))
            }
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _)
            | ParameterValue::Placeholder(_)
            | ParameterValue::Injected(_)
            | ParameterValue::None => Ok(parameter.clone()),
        }
    })
}

fn attach_parameter_link_context(error: Error, parameter: &ParameterValue, query: &Query) -> Error {
    match parameter {
        ParameterValue::DefaultLink(_, _) | ParameterValue::OverrideLink(_, _) => {
            error.with_query(query)
        }
        ParameterValue::ParameterLink(_, _, position)
        | ParameterValue::EnumLink(_, _, position) => error.with_position(position),
        ParameterValue::DefaultValue(_, _)
        | ParameterValue::ParameterValue(_, _, _)
        | ParameterValue::OverrideValue(_, _)
        | ParameterValue::MultipleParameters(_, _)
        | ParameterValue::Placeholder(_)
        | ParameterValue::Injected(_)
        | ParameterValue::None => error,
    }
}

pub fn do_step<E: Environment>(
    step: Step,
    input: State<E::Value>,
    context: Context<E>,
    envref: EnvRef<E>,
) -> crate::maybe_send::BoxFuture<'static, Result<Arc<<E as Environment>::Value>, Error>> {
    match step {
        Step::GetResource(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            context.add_log_entry(
                LogEntry::info("Getting resource".to_string()).with_query(key.clone().into()),
            )?;
            let store = envref.get_async_store();
            let (data, _metadata) = store.get(&key).await?;
            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_bytes(data),
            ))
        }
        .maybe_boxed(),
        Step::GetResourceMetadata(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
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
        .maybe_boxed(),
        Step::GetResourceDirectory(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            eprintln!("Getting resource directory for key {:?}", key);
            let store = envref.get_async_store();
            let d = store.listdir_asset_info(&key).await?;
            eprintln!("Got resource directory: {:?}", d);

            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_asset_info(d),
            ))
        }
        .maybe_boxed(),
        Step::Evaluate(q) => {
            let query = q.clone();
            async move {
                let query = context.resolve_query_from_cwd(&query)?;
                // Claim-aware wait (drain the parent's local queue + direct-claim before
                // blocking) so an at-capacity dependency never deadlocks the parent.
                context
                    .get_dependency_state(&query)
                    .await
                    .and_then(|s| s.value())
            }
            .maybe_boxed()
        }
        Step::UseQueryValue(query) => async move {
            let query = context.resolve_query_from_cwd(&query)?;
            let value = E::Value::from_query(&query);
            Ok(Arc::new(value))
        }
        .maybe_boxed(),
        Step::Action {
            realm,
            ns,
            action_name,
            position,
            parameters,
        } => async move {
            let command_key = CommandKey::new(&realm, &ns, &action_name);
            let mut materialized_parameters = parameters.clone();
            for parameter in &mut materialized_parameters.0 {
                if matches!(parameter, ParameterValue::MultipleParameters(_, _)) {
                    *parameter = materialize_nested_parameter(&context, parameter).await?;
                }
            }
            let mut arguments = CommandArguments::<E>::new(materialized_parameters);
            arguments.action_position = position.clone();
            for (i, param) in parameters.0.iter().enumerate() {
                if let Some(arg_query) = param.link() {
                    let resolved = context
                        .resolve_query_from_cwd(&arg_query)
                        .map_err(|error| attach_parameter_link_context(error, param, &arg_query))?;
                    let arg_value = context
                        .get_dependency_state(&resolved)
                        .await
                        .map_err(|error| attach_parameter_link_context(error, param, &arg_query))?;
                    arguments.set_value(
                        i,
                        arg_value.value().map_err(|error| {
                            attach_parameter_link_context(error, param, &arg_query)
                        })?,
                    );
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
        .maybe_boxed(),

        Step::Filename(name) => async move {
            context.set_filename(&name.name).await?;
            input.value()
        }
        .maybe_boxed(),
        Step::Info(m) => async move {
            context.info(&m)?;
            input.value()
        }
        .maybe_boxed(),
        Step::Warning(m) => async move {
            context.warning(&m)?;
            input.value()
        }
        .maybe_boxed(),
        Step::Error(m) => async move {
            context.error(&m)?;
            input.value()
        }
        .maybe_boxed(),
        Step::SetCwd(key) => async move {
            context.set_cwd_from_key(&key)?;
            input.value()
        }
        .maybe_boxed(),
        Step::Plan(plan) => {
            async move { apply_plan(plan, input, context, envref).await }.maybe_boxed()
        }
        Step::GetAsset(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            let asset_state = context.get_dependency_state(&key.into()).await?;
            asset_state.value()
        }
        .maybe_boxed(),
        Step::GetAssetBinary(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            let asset = context.schedule_dependency_asset(&key.into()).await?;
            // Serialize the state the dependency resolution just returned, rather than issuing a
            // second, independently-gated read of the same asset. Re-reading made this step fail
            // where its neighbour `Step::GetAsset` succeeds — the dependency contract decides what
            // a dependent may use, and both steps must honour the same answer.
            let state = context.wait_for_dependency(&asset).await?;
            let binary = state.as_bytes()?;
            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_bytes(binary),
            ))
        }
        .maybe_boxed(),
        Step::GetAssetMetadata(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            let asset = context
                .schedule_dependency_asset(&key.clone().into())
                .await?;
            let asset_state = context.wait_for_dependency(&asset).await?;
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
        .maybe_boxed(),
        Step::GetAssetRecipe(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
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
        .maybe_boxed(),
        Step::GetAssetDirectory(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            let envref1 = envref.clone();
            let asset_manager = envref1.get_asset_manager();
            let d = asset_manager.listdir_asset_info(&key).await?;
            Ok(Arc::new(
                <<E as Environment>::Value as ValueInterface>::from_asset_info(d),
            ))
        }
        .maybe_boxed(),
        Step::UseKeyValue(key) => async move {
            let key = context.resolve_key_from_cwd(&key)?;
            let value = E::Value::from_key(&key);
            Ok(Arc::new(value))
        }
        .maybe_boxed(),
    }
}

pub fn evaluate<E: Environment, Q: TryToQuery>(
    // TODO: this should be docommissioned in favor of environment evaluate methods
    envref: EnvRef<E>,
    query: Q,
    cwd_key: Option<Key>,
) -> crate::maybe_send::BoxFuture<'static, Result<State<E::Value>, Error>> {
    let rquery = query.try_to_query();
    async move {
        let query = rquery?;
        let plan = make_plan_with_cwd(envref.clone(), query.clone(), cwd_key.clone()).await?;
        let assetref = AssetRef::new_temporary(envref.clone());
        let context = Context::new(assetref, plan.is_volatile).await;
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
        let observed_deps = context.take_pending_dependencies().await;
        let mut metadata = context.get_metadata().await?;
        for dep in observed_deps {
            let _ = metadata.add_dependency(dep);
        }
        Ok(State::new()
            .with_data((*res).clone())
            .with_metadata(metadata.into()))
    }
    .maybe_boxed()
}

pub fn evaluate_simple_template<E: Environment>(
    envref: EnvRef<E>,
    template: SimpleTemplate,
    cwd_key: Option<Key>,
) -> crate::maybe_send::BoxFuture<'static, Result<String, Error>> {
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
    .maybe_boxed()
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
    async fn is_volatile(&self, _env: EnvRef<E>) -> Result<bool, Error> {
        // Return cached value - Plan.is_volatile is always set during plan building (make_plan)
        // No need to re-check steps; volatility is computed during Phase 1 and Phase 2
        Ok(self.is_volatile)
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
        let plan = make_plan(env.clone(), self.clone()).await?;
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
            Step::UseQueryValue(query) => Ok(false),
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

/// Whether a plan, query, recipe or step needs an evaluation payload to run.
///
/// Mirrors [`IsVolatile`], with one deliberate difference: **keys are a payload boundary**.
/// A keyed asset is evaluated independently through the asset manager and never inherits a
/// caller's payload, because keys are global while a payload is per-evaluation. The keyed
/// step arms below therefore return [`PayloadRequirement::None`] unconditionally instead of
/// consulting the asset manager as `IsVolatile` does. A keyed recipe that does require a
/// payload is rejected where the plan for that key is built.
///
/// Because no arm consults the asset manager, this traversal is cheaper than `IsVolatile`:
/// its only asynchronous work is `make_plan` in the [`Query`] implementation.
pub(crate) trait RequiresPayload<E: Environment> {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error>;
}

impl<E: Environment> RequiresPayload<E> for ParameterValue {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error> {
        if let Some(link) = self.link() {
            Box::pin(link.requires_payload(env)).await
        } else {
            Ok(PayloadRequirement::None)
        }
    }
}

impl<E: Environment> RequiresPayload<E> for ResolvedParameterValues {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error> {
        for param in self.0.iter() {
            if param.requires_payload(env.clone()).await?.is_required() {
                return Ok(PayloadRequirement::Required);
            }
        }
        Ok(PayloadRequirement::None)
    }
}

impl<E: Environment> RequiresPayload<E> for Plan {
    async fn requires_payload(&self, _env: EnvRef<E>) -> Result<PayloadRequirement, Error> {
        // Return cached value - Plan.payload_required is set during plan building.
        Ok(self.payload_required)
    }
}

impl<E: Environment> RequiresPayload<E> for Recipe {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error> {
        // Note: unlike `volatile`, Recipe has no author-declarable payload override.
        // The requirement is always derived from the commands the recipe uses.
        let plan = self.to_plan(env.get_command_metadata_registry())?;
        plan.requires_payload(env).await
    }
}

impl<E: Environment> RequiresPayload<E> for Query {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error> {
        let plan = make_plan(env.clone(), self.clone()).await?;
        plan.requires_payload(env).await
    }
}

impl<E: Environment> RequiresPayload<E> for Step {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error> {
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
                    if cmd.payload_required.is_required() {
                        return Ok(PayloadRequirement::Required);
                    }
                    return parameters.requires_payload(env).await;
                } else {
                    Ok(PayloadRequirement::None)
                }
            }
            // Keys are a payload boundary: a keyed asset is resolved through the manager
            // without a payload, so a payload requirement never propagates through one.
            Step::GetAsset(_) => Ok(PayloadRequirement::None),
            Step::GetAssetBinary(_) => Ok(PayloadRequirement::None),
            Step::GetAssetMetadata(_) => Ok(PayloadRequirement::None),
            Step::GetAssetRecipe(_) => Ok(PayloadRequirement::None),
            Step::GetAssetDirectory(_) => Ok(PayloadRequirement::None),
            Step::GetResource(_) => Ok(PayloadRequirement::None),
            Step::GetResourceMetadata(_) => Ok(PayloadRequirement::None),
            Step::GetResourceDirectory(_) => Ok(PayloadRequirement::None),
            Step::Evaluate(query) => query.requires_payload(env).await,
            Step::UseQueryValue(_) => Ok(PayloadRequirement::None),
            Step::Filename(_) => Ok(PayloadRequirement::None),
            Step::Info(_) => Ok(PayloadRequirement::None),
            Step::Warning(_) => Ok(PayloadRequirement::None),
            Step::Error(_) => Ok(PayloadRequirement::None),
            Step::Plan(plan) => plan.requires_payload(env).await,
            Step::SetCwd(_) => Ok(PayloadRequirement::None),
            Step::UseKeyValue(_) => Ok(PayloadRequirement::None),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use crate as liquers_core;
    use crate::assets::{AssetManager, AssetRef};
    use crate::command_metadata::CommandKey;
    use crate::context::{ImmediateEnvironment, SimpleEnvironment, SimpleEnvironmentWithPayload};
    use crate::error::ErrorType;
    use crate::metadata::{
        DependencyKey, LogEntryKind, Metadata, MetadataRecord, ProgressEntry, Status, Version,
    };
    use crate::parse::{parse_key, parse_query};
    use crate::query::Position;
    use crate::state::State;
    use crate::store::{AsyncMemoryStore, AsyncStore};
    use crate::value::Value;
    use liquers_macro::*;

    async fn immediate_context(
        envref: EnvRef<ImmediateEnvironment<Value>>,
        cwd: Option<Key>,
    ) -> Context<ImmediateEnvironment<Value>> {
        let asset = AssetRef::new_temporary(envref);
        let context = Context::new(asset, false).await;
        context.set_cwd_key(cwd);
        context
    }

    async fn immediate_environment_with_text(
        entries: &[(&str, &str)],
    ) -> Result<EnvRef<ImmediateEnvironment<Value>>, Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        for (key_text, value) in entries {
            let key = parse_key(key_text)?;
            let mut metadata = MetadataRecord::new();
            metadata
                .with_key(key.clone())
                .with_status(Status::Source)
                .with_type_identifier("Text".to_owned());
            store
                .set(&key, value.as_bytes(), &Metadata::MetadataRecord(metadata))
                .await?;
        }
        let mut environment = ImmediateEnvironment::<Value>::new();
        environment.with_async_store(Box::new(store));
        Ok(environment.to_ref())
    }

    fn root_fallback_warning_count(metadata: &Metadata) -> usize {
        let Metadata::MetadataRecord(record) = metadata else {
            std::panic!("expected structured metadata");
        };
        record
            .log
            .iter()
            .filter(|entry| {
                entry.kind == LogEntryKind::Warning && entry.message == RELATIVE_WITHOUT_CWD_WARNING
            })
            .count()
    }

    #[tokio::test]
    async fn relative_operand_without_cwd_warns_and_uses_root(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();

        let asset = envref.evaluate("-R-key/./root.txt").await?;
        let state = asset.get().await?;
        assert_eq!(state.value()?.try_into_key()?.encode(), "root.txt");

        let metadata = asset.get_metadata().await?;
        assert_eq!(root_fallback_warning_count(&metadata), 1);
        Ok(())
    }

    #[tokio::test]
    async fn deterministic_context_clones_share_root_and_one_warning(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let observations = Arc::new(std::sync::Mutex::new(Vec::new()));
        let command_observations = observations.clone();
        let mut environment = ImmediateEnvironment::<Value>::new();
        environment.command_registry.register_command(
            CommandKey::new_name("resolve_on_context_clones"),
            move |_state, _arguments, context| {
                let first_context = context.clone();
                let second_context = context.clone();
                let first = first_context.resolve_key_from_cwd(&parse_key("./first.txt")?)?;
                let second = second_context.resolve_key_from_cwd(&parse_key("./second.txt")?)?;
                command_observations
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push((
                        first.encode(),
                        second.encode(),
                        first_context.get_cwd_key(),
                        second_context.get_cwd_key(),
                    ));
                Ok(Value::from("resolved"))
            },
        )?;
        let envref = environment.to_ref();

        let asset = envref.evaluate("resolve_on_context_clones").await?;
        let state = asset.get().await?;
        assert_eq!(state.try_into_string()?, "resolved");

        let observations = observations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].0, "first.txt");
        assert_eq!(observations[0].1, "second.txt");
        assert_eq!(observations[0].2, Some(Key::new()));
        assert_eq!(observations[0].3, Some(Key::new()));
        drop(observations);

        let metadata = asset.get_metadata().await?;
        assert_eq!(root_fallback_warning_count(&metadata), 1);
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn concurrent_root_fallback_warns_once() -> Result<(), Box<dyn std::error::Error>> {
        let mut environment = ImmediateEnvironment::<Value>::new();
        environment.command_registry.register_async_command(
            CommandKey::new_name("resolve_root_concurrently"),
            move |_state, _arguments, context| {
                Box::pin(async move {
                    let barrier = Arc::new(tokio::sync::Barrier::new(2));
                    let first_barrier = barrier.clone();
                    let first_context = context.clone();
                    let first = tokio::spawn(async move {
                        first_barrier.wait().await;
                        let key = first_context.resolve_key_from_cwd(&parse_key("./first.txt")?)?;
                        Ok::<_, Error>((key, first_context.get_cwd_key()))
                    });
                    let second_barrier = barrier.clone();
                    let second_context = context.clone();
                    let second = tokio::spawn(async move {
                        second_barrier.wait().await;
                        let key =
                            second_context.resolve_key_from_cwd(&parse_key("./second.txt")?)?;
                        Ok::<_, Error>((key, second_context.get_cwd_key()))
                    });

                    let (first, second) =
                        tokio::time::timeout(std::time::Duration::from_secs(2), async move {
                            let first = first.await.map_err(|error| {
                                Error::general_error(format!(
                                    "first root-resolution task failed: {error}"
                                ))
                            })??;
                            let second = second.await.map_err(|error| {
                                Error::general_error(format!(
                                    "second root-resolution task failed: {error}"
                                ))
                            })??;
                            Ok::<_, Error>((first, second))
                        })
                        .await
                        .map_err(|_| {
                            Error::general_error("concurrent root resolution timed out".to_owned())
                        })??;

                    let mut keys = [first.0.encode(), second.0.encode()];
                    keys.sort();
                    if keys != ["first.txt", "second.txt"]
                        || first.1 != Some(Key::new())
                        || second.1 != Some(Key::new())
                    {
                        return Err(Error::general_error(format!(
                            "context clones did not share logical root: {first:?}, {second:?}"
                        )));
                    }
                    Ok(Value::from("resolved concurrently"))
                })
            },
        )?;
        let envref = environment.to_ref();

        let asset = envref.evaluate("resolve_root_concurrently").await?;
        let state = asset.get().await?;
        assert_eq!(state.try_into_string()?, "resolved concurrently");

        let metadata = asset.get_metadata().await?;
        assert_eq!(root_fallback_warning_count(&metadata), 1);
        Ok(())
    }

    #[tokio::test]
    async fn resolves_ordered_cwd_changes() -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let context = immediate_context(envref.clone(), Some(parse_key("a/b")?)).await;
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("../c")?),
            Step::SetCwd(parse_key("./d")?),
            Step::UseKeyValue(parse_key("../result.txt")?),
        ];

        let result = apply_plan(plan, State::new(), context.clone(), envref).await?;

        assert_eq!(result.try_into_key()?.encode(), "a/c/result.txt");
        assert_eq!(
            context.get_cwd_key().map(|key| key.encode()).as_deref(),
            Some("a/c/d")
        );
        Ok(())
    }

    #[tokio::test]
    async fn chained_cwd_updates_resolve_key_value() -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let state = evaluate(
            envref,
            "-R-cwd/../-R-cwd/./c/-R-key/.",
            Some(parse_key("a/b")?),
        )
        .await?;

        assert_eq!(state.value()?.try_into_key()?.encode(), "a/c");
        Ok(())
    }

    #[tokio::test]
    async fn absolute_key_instruction_ignores_entry_cwd() -> Result<(), Box<dyn std::error::Error>>
    {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();

        let state = evaluate(envref, "/-R-key/./root-key", Some(parse_key("a/c")?)).await?;

        assert_eq!(state.value()?.try_into_key()?.encode(), "root-key");
        Ok(())
    }

    #[tokio::test]
    async fn absolute_source_cwd_skips_relative_recipe_prefix(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let mut recipe = Recipe::default();
        recipe.query = "/-R-cwd/./source/-R-key/.".to_owned();
        recipe.cwd = Some("../recipe".to_owned());
        let plan = recipe.to_plan(envref.get_command_metadata_registry())?;

        assert_eq!(plan.absolute_query_resource_step_index(), Some(1));
        assert!(matches!(
            &plan.steps[0],
            Step::SetCwd(key) if key.encode() == "../recipe"
        ));
        assert!(matches!(
            &plan.steps[1],
            Step::SetCwd(key) if key.encode() == "./source"
        ));

        let context = immediate_context(envref.clone(), Some(parse_key("a/b")?)).await;
        let result = apply_plan(plan.clone(), State::new(), context.clone(), envref).await?;

        assert_eq!(result.try_into_key()?.encode(), "source");
        assert_eq!(
            context.get_cwd_key().map(|key| key.encode()).as_deref(),
            Some("source")
        );
        assert!(matches!(
            &plan.steps[0],
            Step::SetCwd(key) if key.encode() == "../recipe"
        ));
        assert!(matches!(
            &plan.steps[1],
            Step::SetCwd(key) if key.encode() == "./source"
        ));
        Ok(())
    }

    #[tokio::test]
    async fn absolute_outer_resource_keeps_relative_link_on_live_cwd(
    ) -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = ImmediateEnvironment<Value>;

        fn use_link(state: &State<Value>, value: String) -> Result<Value, Error> {
            Ok(Value::from(format!("{}|{value}", state.try_into_string()?)))
        }

        let store = AsyncMemoryStore::new(&Key::new());
        for (key_text, value) in [
            ("data", "root-data"),
            ("a/c/data", "wrong-data"),
            ("a/c/hello.txt", "linked"),
        ] {
            let key = parse_key(key_text)?;
            let mut metadata = MetadataRecord::new();
            metadata
                .with_key(key.clone())
                .with_status(Status::Source)
                .with_type_identifier("Text".to_owned());
            store
                .set(&key, value.as_bytes(), &Metadata::MetadataRecord(metadata))
                .await?;
        }
        let mut environment = CommandEnvironment::new();
        let registry = &mut environment.command_registry;
        register_command!(registry, fn use_link(state, value: String) -> result)?;
        environment.with_async_store(Box::new(store));
        let envref = environment.to_ref();

        let mut recipe = Recipe::default();
        recipe.query = "/-R/./data/-/use_link-~X~-R/./hello.txt~E".to_owned();
        recipe.cwd = Some("a/c".to_owned());
        let mut plan = recipe.to_plan(envref.get_command_metadata_registry())?;
        let context = immediate_context(envref.clone(), None).await;

        // The index maps source query segments onto steps positionally, so it is meaningful only
        // while the operands are still source-relative. Freezing consumes it.
        assert_eq!(plan.absolute_query_resource_step_index(), Some(1));

        finalize_plan(envref.clone(), &mut plan, &context).await?;

        assert!(plan
            .dependencies
            .iter()
            .any(|dependency| dependency.key.as_str() == "-R/data"));
        assert!(!plan
            .dependencies
            .iter()
            .any(|dependency| dependency.key.as_str() == "-R/a/c/data"));
        assert!(plan
            .dependencies
            .iter()
            .any(|dependency| dependency.key.as_str() == "-R/a/c/hello.txt"));
        // Frozen: the absolute query's own resource resolved against logical root, not against the
        // recipe CWD. Before freezing this step read `./data` and was resolved afresh by each pass.
        assert!(matches!(
            &plan.steps[1],
            Step::GetAsset(key) if key.encode() == "data"
        ));

        let result = apply_plan(plan.clone(), State::new(), context.clone(), envref).await?;

        assert_eq!(result.try_into_string()?, "root-data|linked");
        assert_eq!(
            context.get_cwd_key().map(|key| key.encode()).as_deref(),
            Some("a/c")
        );
        // Execution does not mutate the frozen plan.
        assert!(matches!(
            &plan.steps[1],
            Step::GetAsset(key) if key.encode() == "data"
        ));
        Ok(())
    }

    #[tokio::test]
    async fn dependency_preschedule_tracks_cwd_without_mutating_context(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = immediate_environment_with_text(&[("a/c/input.txt", "input")]).await?;
        let context = immediate_context(envref.clone(), Some(parse_key("a/b")?)).await;
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("../c")?),
            Step::GetAsset(parse_key("./input.txt")?),
        ];

        schedule_plan_dependencies(&plan, &context).await?;

        assert_eq!(
            context.get_cwd_key().map(|key| key.encode()).as_deref(),
            Some("a/b")
        );
        let manager = envref.get_asset_manager();
        assert!(manager
            .lookup_key_asset(&parse_key("a/c/input.txt")?)
            .is_some());
        assert!(manager
            .lookup_key_asset(&parse_key("a/b/input.txt")?)
            .is_none());
        Ok(())
    }

    #[tokio::test]
    async fn evaluate_cwd_applies_before_dependency_analysis(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = immediate_environment_with_text(&[("a/b/input.txt", "input")]).await?;

        let state = evaluate(envref.clone(), "-R/./input.txt", Some(parse_key("a/b")?)).await?;

        assert_eq!(state.try_into_string()?, "input");
        let manager = envref.get_asset_manager();
        assert!(manager
            .lookup_key_asset(&parse_key("a/b/input.txt")?)
            .is_some());
        assert!(manager.lookup_key_asset(&parse_key("input.txt")?).is_none());
        Ok(())
    }

    #[tokio::test]
    async fn manual_plan_resolves_relative_steps_from_context(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = immediate_environment_with_text(&[("a/c/input.txt", "input")]).await?;
        let context = immediate_context(envref.clone(), Some(parse_key("a/c")?)).await;
        let relative_key = parse_key("./input.txt")?;
        let relative_query = parse_query("-R/./input.txt")?;
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::GetResource(relative_key.clone()),
            Step::GetResourceMetadata(relative_key.clone()),
            Step::GetResourceDirectory(parse_key(".")?),
            Step::GetAsset(relative_key.clone()),
            Step::GetAssetBinary(relative_key.clone()),
            Step::GetAssetMetadata(relative_key.clone()),
            Step::GetAssetRecipe(relative_key.clone()),
            Step::GetAssetDirectory(parse_key(".")?),
            Step::Evaluate(relative_query.clone()),
            Step::UseQueryValue(relative_query),
            Step::UseKeyValue(relative_key),
        ];

        let result = apply_plan(plan, State::new(), context.clone(), envref.clone()).await?;

        assert_eq!(result.try_into_key()?.encode(), "a/c/input.txt");
        assert_eq!(
            context.get_cwd_key().map(|key| key.encode()).as_deref(),
            Some("a/c")
        );
        assert!(envref
            .get_asset_manager()
            .lookup_key_asset(&parse_key("input.txt")?)
            .is_none());
        Ok(())
    }

    #[tokio::test]
    async fn nested_plan_inherits_and_updates_cwd() -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let context = immediate_context(envref.clone(), Some(parse_key("a/b")?)).await;
        let mut nested = Plan::new();
        nested.steps = vec![
            Step::SetCwd(parse_key("./child")?),
            Step::UseKeyValue(parse_key("./inside.txt")?),
        ];
        let mut outer = Plan::new();
        outer.steps = vec![
            Step::Plan(nested),
            Step::UseKeyValue(parse_key("./outside.txt")?),
        ];

        let result = apply_plan(outer, State::new(), context.clone(), envref).await?;

        assert_eq!(result.try_into_key()?.encode(), "a/b/child/outside.txt");
        assert_eq!(
            context.get_cwd_key().map(|key| key.encode()).as_deref(),
            Some("a/b/child")
        );
        Ok(())
    }

    #[tokio::test]
    async fn action_observes_current_cwd() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = ImmediateEnvironment<Value>;

        fn cwd(
            _state: &State<Value>,
            context: Context<CommandEnvironment>,
        ) -> Result<Value, Error> {
            Ok(Value::from(
                context
                    .get_cwd_key()
                    .map(|key| key.encode())
                    .unwrap_or_else(|| "<none>".to_owned()),
            ))
        }

        let mut environment = CommandEnvironment::new();
        let registry = &mut environment.command_registry;
        register_command!(registry, fn cwd(state, context) -> result)?;
        let state = evaluate(
            environment.to_ref(),
            "-R-cwd/../c/-/cwd",
            Some(parse_key("a/b")?),
        )
        .await?;

        assert_eq!(state.try_into_string()?, "a/c");
        Ok(())
    }

    #[tokio::test]
    async fn multiple_parameter_links_materialize_in_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        fn render_parameter(parameter: &ParameterValue) -> Result<String, Error> {
            fn text(value: &serde_json::Value) -> Result<&str, Error> {
                value.as_str().ok_or_else(|| {
                    Error::general_error("Expected materialized string value".to_owned())
                })
            }

            match parameter {
                ParameterValue::DefaultValue(_, value) => Ok(format!("D:{}", text(value)?)),
                ParameterValue::ParameterValue(_, value, _) => Ok(format!("P:{}", text(value)?)),
                ParameterValue::OverrideValue(_, value) => Ok(format!("O:{}", text(value)?)),
                ParameterValue::MultipleParameters(_, values) => {
                    let rendered = values
                        .iter()
                        .map(render_parameter)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(format!("[{}]", rendered.join("|")))
                }
                other => Err(Error::general_error(format!(
                    "Unexpected unresolved parameter: {other:?}"
                ))),
            }
        }

        let mut environment = ImmediateEnvironment::<Value>::new();
        for (name, result) in [
            ("default_value", "default"),
            ("parameter_value", "parameter"),
            ("override_value", "override"),
            ("enum_value", "enum"),
        ] {
            let result = result.to_owned();
            environment
                .command_registry
                .register_command(CommandKey::new_name(name), move |_, _, _| {
                    Ok(Value::from(result.clone()))
                })?;
        }
        environment.command_registry.register_command(
            CommandKey::new_name("collect_materialized"),
            |_, arguments, _| {
                let rendered = render_parameter(arguments.get_parameter(0, "items")?)?;
                let lossless_key = arguments.get_value(1, "lossless")?.try_into_key()?;
                Ok(Value::from(format!(
                    "{rendered}|key={}",
                    lossless_key.encode()
                )))
            },
        )?;
        let envref = environment.to_ref();
        let context = immediate_context(envref.clone(), Some(parse_key("a/b")?)).await;
        let mut plan = Plan::new();
        plan.steps.push(Step::Action {
            realm: String::new(),
            ns: String::new(),
            action_name: "collect_materialized".to_owned(),
            position: Position::unknown(),
            parameters: ResolvedParameterValues(vec![
                ParameterValue::MultipleParameters("items".to_owned(), vec![
                    ParameterValue::DefaultLink("items".to_owned(), parse_query("default_value")?),
                    ParameterValue::ParameterLink(
                        "items".to_owned(),
                        parse_query("parameter_value")?,
                        Position::new(10, 1, 11),
                    ),
                    ParameterValue::MultipleParameters("items".to_owned(), vec![
                        ParameterValue::OverrideLink(
                            "items".to_owned(),
                            parse_query("override_value")?,
                        ),
                        ParameterValue::EnumLink(
                            "items".to_owned(),
                            parse_query("enum_value")?,
                            Position::new(20, 1, 21),
                        ),
                    ]),
                ]),
                ParameterValue::ParameterLink(
                    "lossless".to_owned(),
                    parse_query("-R-key/./lossless")?,
                    Position::new(30, 1, 31),
                ),
            ]),
        });

        let result = apply_plan(plan, State::new(), context, envref).await?;

        assert_eq!(
            result.try_into_string()?,
            "[D:default|P:parameter|[O:override|P:enum]]|key=a/b/lossless"
        );
        Ok(())
    }

    #[tokio::test]
    async fn multiple_parameter_non_json_link_reports_positioned_conversion_error(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let context = immediate_context(envref.clone(), Some(parse_key("a/b")?)).await;
        let position = Position::new(17, 1, 18);
        let mut plan = Plan::new();
        plan.steps.push(Step::Action {
            realm: String::new(),
            ns: String::new(),
            action_name: "must_not_execute".to_owned(),
            position: Position::unknown(),
            parameters: ResolvedParameterValues(vec![ParameterValue::MultipleParameters("items".to_owned(), vec![
                ParameterValue::ParameterLink(
                    "items".to_owned(),
                    parse_query("-R-key/./target")?,
                    position.clone(),
                ),
            ])]),
        });

        let error = apply_plan(plan, State::new(), context, envref)
            .await
            .expect_err("non-JSON variadic link should fail conversion");

        assert_eq!(error.error_type, ErrorType::ConversionError);
        assert_eq!(error.position, position);
        Ok(())
    }

    #[tokio::test]
    async fn missing_resolved_resource_preserves_key_not_found_for_absolute_key(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = immediate_environment_with_text(&[]).await?;
        let context = immediate_context(envref.clone(), Some(parse_key("a/c")?)).await;

        let error = do_step(
            Step::GetResource(parse_key("./missing.txt")?),
            State::new(),
            context,
            envref,
        )
        .await
        .expect_err("the resolved resource is intentionally absent");

        assert_eq!(error.error_type, ErrorType::KeyNotFound);
        assert!(
            error.message.contains("a/c/missing.txt"),
            "resolved key missing from error: {error:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn resolver_scopes_nested_links() -> Result<(), Box<dyn std::error::Error>> {
        type CommandEnvironment = ImmediateEnvironment<Value>;

        fn cwd(
            _state: &State<Value>,
            context: Context<CommandEnvironment>,
        ) -> Result<Value, Error> {
            Ok(Value::from(
                context
                    .get_cwd_key()
                    .map(|key| key.encode())
                    .unwrap_or_else(|| "<none>".to_owned()),
            ))
        }

        fn pass(_state: &State<Value>, value: String) -> Result<Value, Error> {
            Ok(Value::from(value))
        }

        fn append_cwd(
            state: &State<Value>,
            context: Context<CommandEnvironment>,
        ) -> Result<Value, Error> {
            let value = state.try_into_string()?;
            let cwd = context
                .get_cwd_key()
                .map(|key| key.encode())
                .unwrap_or_else(|| "<none>".to_owned());
            Ok(Value::from(format!("{value}|{cwd}")))
        }

        let mut environment = CommandEnvironment::new();
        let registry = &mut environment.command_registry;
        register_command!(registry, fn cwd(state, context) -> result)?;
        register_command!(registry, fn pass(state, value: String) -> result)?;
        register_command!(registry, fn append_cwd(state, context) -> result)?;
        let envref = environment.to_ref();

        let state = evaluate(
            envref,
            "pass-~X~-R-cwd/./child/-/cwd~E/append_cwd",
            Some(parse_key("a/b")?),
        )
        .await?;

        assert_eq!(state.try_into_string()?, "a/b/child|a/b");
        Ok(())
    }

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
    async fn make_plan_with_cwd_uses_one_entry_snapshot() -> Result<(), Box<dyn std::error::Error>>
    {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();

        let plan =
            make_plan_with_cwd(envref.clone(), "-R/./input.txt", Some(parse_key("a/b")?)).await?;

        assert!(plan.dependencies.iter().any(|dependency| {
            dependency.key.as_str() == "-R/a/b/input.txt"
                && dependency.relation == crate::dependencies::DependencyRelation::StateArgument
        }));
        assert!(matches!(
            plan.steps.first(),
            Some(Step::GetAsset(key)) if key.encode() == "./input.txt"
        ));
        assert!(!plan.init_steps.iter().any(
            |step| matches!(step, Step::Warning(message) if message == crate::query::RELATIVE_WITHOUT_CWD_WARNING)
        ));

        let root_plan = make_plan_with_cwd(envref, "-R/./input.txt", None).await?;
        assert!(root_plan
            .dependencies
            .iter()
            .any(|dependency| dependency.key.as_str() == "-R/input.txt"));
        assert_eq!(
            root_plan
                .init_steps
                .iter()
                .filter(|step| matches!(step, Step::Warning(message) if message == crate::query::RELATIVE_WITHOUT_CWD_WARNING))
                .count(),
            1
        );
        Ok(())
    }

    #[tokio::test]
    async fn finalize_relative_plan_uses_context_owner_key(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let manager = envref.get_asset_manager();
        let owner_key = parse_key("owners/result.txt")?;
        let owner = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            owner_key.clone().into(),
            envref.clone(),
        );
        assert!(
            manager
                .try_insert_key_asset(&owner_key, owner.clone())
                .await
        );
        let context = Context::new(owner, false).await;
        context.set_cwd_key(Some(parse_key("a/b")?));

        let resolved_dependency_key = parse_key("a/b/input.txt")?;
        let resolved_dependency = DependencyKey::from(&resolved_dependency_key);
        let _ = manager
            .dependency_manager()
            .register_version(&resolved_dependency, Version::new(1))
            .await;

        let mut plan = Plan::new();
        plan.query = parse_query("-R/./raw-query-owner.txt")?;
        plan.steps.push(Step::GetAsset(parse_key("./input.txt")?));
        finalize_plan(envref, &mut plan, &context).await?;

        assert!(plan
            .dependencies
            .iter()
            .any(|dependency| dependency.key == resolved_dependency));
        let owner_dependency = DependencyKey::from(&owner_key);
        assert!(
            manager
                .dependency_manager()
                .would_create_cycle(&resolved_dependency, &owner_dependency)
                .await
        );
        let raw_query_owner = DependencyKey::from(&parse_key("./raw-query-owner.txt")?);
        assert!(
            !manager
                .dependency_manager()
                .would_create_cycle(&resolved_dependency, &raw_query_owner)
                .await
        );
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
        let moon_dependency_key = crate::metadata::DependencyKey::from(&parse_query("moon")?);
        let deps = state.metadata.get_dependencies();
        assert!(
            deps.iter().any(|d| d.key == moon_dependency_key),
            "context.evaluate() should record the runtime dependency in result metadata"
        );
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
            eprintln!("greet command called");
            let what = state.try_into_string()?;
            context.info(&format!("Greeting {what}"))?;
            let upper = context
                .apply(
                    &parse_query("upper").unwrap(),
                    State::new().with_data(what.into()),
                )
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
        eprintln!("Metadata: {:?}", state.metadata);

        let value = state.try_into_string()?;
        assert_eq!(value, "Ciao, WORLD!");
        assert!(state.metadata.primary_progress().is_off()); // Progress should be off (not done) at the end of execution
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
                .apply(
                    &parse_query("upper").unwrap(),
                    State::new().with_data(what.into()),
                )
                .await?;
            let upper_text = upper.get().await?.try_into_string()?;
            context.progress(ProgressEntry::done("OK, done".to_owned()))?;
            Ok(Value::from(format!("{greet}, {upper_text}!")))
        }
        let cr = &mut env.command_registry;
        // `payload: required` is what carries the payload across an evaluation boundary. Without
        // it this command works only at top level, where `evaluate_immediately` installs the
        // payload directly, and silently receives none as a nested dependency.
        register_command!(cr, fn word(state, payload: String injected) -> result payload: required)
            .expect("register_command failed");
        register_command!(cr, fn upper(state) -> result).expect("register_command failed");
        register_command!(cr, async fn greet(state, greet: String = "Hello", context) -> result)
            .expect("register_command failed");

        let envref = env.to_ref();

        let asset = envref
            .evaluate_immediately("word/greet-Ciao", "Earth".into())
            .await?;
        let state = asset.get().await?;
        eprintln!("Metadata: {:?}", state.metadata);

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
        use crate::store::{AsyncMemoryStore, AsyncStore};
        use crate::value::Value;

        // Create an async memory store and populate it with recipes.yaml
        let memory_store = AsyncMemoryStore::new(&Key::new());

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
        eprintln!("recipes.yaml content:\n{}", yaml_content);

        // Store the recipes.yaml in memory at folder/recipes.yaml
        let recipes_key = parse_key("folder/recipes.yaml").unwrap();
        let metadata = Metadata::new();
        memory_store
            .set(&recipes_key, yaml_content.as_bytes(), &metadata)
            .await
            .unwrap();
        memory_store
            .set(
                &parse_key("hello/test.txt").unwrap(),
                "Hello, world!".as_bytes(),
                &metadata,
            )
            .await
            .unwrap();

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(memory_store));
        env.with_recipe_provider(Box::new(crate::recipes::DefaultRecipeProvider));

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir/folder").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //eprintln!("Directory listing:\n{:?}", &**s.data_unchecked());
        //eprintln!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &**s.data_unchecked() {
            assert_eq!(a.len(), 3);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //eprintln!("Names: {:?}", names);
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
        use crate::store::{AsyncMemoryStore, AsyncStore};
        use crate::value::Value;

        // Create an async memory store and populate it with recipes.yaml
        let memory_store = AsyncMemoryStore::new(&Key::new());

        memory_store
            .set(
                &parse_key("folder/test.txt").unwrap(),
                "Hello, world!".as_bytes(),
                &Metadata::new(),
            )
            .await
            .unwrap();

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(memory_store));
        env.with_recipe_provider(Box::new(crate::recipes::DefaultRecipeProvider));

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir/folder").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //eprintln!("Directory listing:\n{:?}", &**s.data_unchecked());
        //eprintln!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &**s.data_unchecked() {
            assert_eq!(a.len(), 1);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //eprintln!("Names: {:?}", names);
            assert!(names.contains(&"test.txt".to_string()));
        } else {
            panic!("Expected AssetInfo value");
        }

        let a = envref.evaluate("-R-dir").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //eprintln!("Directory listing:\n{:?}", &**s.data_unchecked());
        //eprintln!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &**s.data_unchecked() {
            assert_eq!(a.len(), 1);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //eprintln!("Names: {:?}", names);
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
        use crate::store::{AsyncMemoryStore, AsyncStore};
        use crate::value::Value;

        // Create an async memory store and populate it with test data
        let async_store = AsyncMemoryStore::new(&Key::new());
        async_store
            .set(
                &parse_key("file1.txt").unwrap(),
                "File 1 contents".as_bytes(),
                &Metadata::new(),
            )
            .await
            .unwrap();

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(async_store));

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");

        //eprintln!("Directory listing:\n{:?}", &**s.data_unchecked());
        //eprintln!("Directory metadata:\n{:?}", &*s.metadata);
        assert!(!s.is_error().unwrap());
        if let Value::AssetInfo(a) = &**s.data_unchecked() {
            assert_eq!(a.len(), 1);
            let names: std::collections::HashSet<String> = a
                .iter()
                .map(|x| x.filename.as_ref().unwrap().clone())
                .collect();
            //eprintln!("Names: {:?}", names);
            assert!(names.contains(&"file1.txt".to_string()));
        } else {
            panic!("Expected AssetInfo value");
        }
    }

    // --- cut/expand equivalence ------------------------------------------------------------

    /// Evaluates one recipe twice from the same finalized plan: once expanded, once with the
    /// predecessor cut into a `Step::Evaluate` boundary.
    ///
    /// The two forms are meant to differ only in *where* work happens, never in what comes out.
    /// This harness is what makes that claim checkable rather than argued: every difference found
    /// so far — a predecessor action running twice, a dependency's error being replaced by "did
    /// not produce a value" — was invisible to analysis and obvious to a comparison.
    async fn evaluate_both_ways(
        envref: EnvRef<ImmediateEnvironment<Value>>,
        recipe: Recipe,
        cwd: Option<Key>,
    ) -> (Result<String, Error>, Result<String, Error>, bool) {
        let cmr = envref.get_command_metadata_registry();
        let mut plan = match recipe.to_plan(cmr) {
            Ok(plan) => plan,
            Err(error) => return (Err(error.clone()), Err(error), false),
        };
        let context = immediate_context(envref.clone(), cwd.clone()).await;
        if let Err(error) = finalize_plan(envref.clone(), &mut plan, &context).await {
            return (Err(error.clone()), Err(error), false);
        }

        let mut cut = plan.clone();
        let was_cut = cut
            .cut_predecessor(envref.get_command_metadata_registry())
            .unwrap_or(false);

        let run = |plan: Plan, context: Context<ImmediateEnvironment<Value>>| {
            let envref = envref.clone();
            async move {
                apply_plan(plan, State::new(), context, envref)
                    .await
                    .and_then(|value| value.try_into_string())
            }
        };

        let expanded = run(plan, context).await;
        let cut_context = immediate_context(envref.clone(), cwd).await;
        let cut_result = run(cut, cut_context).await;
        (expanded, cut_result, was_cut)
    }

    fn describe(result: &Result<String, Error>) -> String {
        match result {
            Ok(value) => format!("ok:{value}"),
            Err(error) => format!("err:{}", error.message),
        }
    }

    /// A chain of plain actions produces the same value whether or not the predecessor is cut.
    #[tokio::test]
    async fn equivalence_transform_chain() -> Result<(), Box<dyn std::error::Error>> {
        fn seed(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("seed"))
        }
        fn upper(state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        }
        type CommandEnvironment = ImmediateEnvironment<Value>;
        let mut environment = ImmediateEnvironment::<Value>::new();
        let registry = &mut environment.command_registry;
        register_command!(registry, fn seed(state) -> result)?;
        register_command!(registry, fn upper(state) -> result)?;
        let envref = environment.to_ref();

        let recipe = Recipe::new("seed/upper".to_owned(), String::new(), String::new())?;
        let (expanded, cut, was_cut) = evaluate_both_ways(envref, recipe, None).await;

        assert!(was_cut, "this shape has a predecessor to cut");
        assert_eq!(describe(&expanded), "ok:SEED");
        assert_eq!(
            describe(&expanded),
            describe(&cut),
            "cutting the predecessor must not change the result"
        );
        Ok(())
    }

    /// A trailing filename is not an action: cutting must leave the real last action in the
    /// parent, or a recipe's overrides would have nothing to patch.
    #[tokio::test]
    async fn equivalence_keeps_last_action_before_a_filename(
    ) -> Result<(), Box<dyn std::error::Error>> {
        fn seed(_state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from("seed"))
        }
        fn upper(state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        }
        type CommandEnvironment = ImmediateEnvironment<Value>;
        let mut environment = ImmediateEnvironment::<Value>::new();
        let registry = &mut environment.command_registry;
        register_command!(registry, fn seed(state) -> result)?;
        register_command!(registry, fn upper(state) -> result)?;
        let envref = environment.to_ref();

        let recipe = Recipe::new("seed/upper/out.txt".to_owned(), String::new(), String::new())?;
        let cmr = envref.get_command_metadata_registry();
        let mut plan = recipe.to_plan(cmr)?;
        let context = immediate_context(envref.clone(), None).await;
        finalize_plan(envref.clone(), &mut plan, &context).await?;
        plan.cut_predecessor(envref.get_command_metadata_registry())?;

        assert!(
            matches!(plan.steps.last(), Some(Step::Filename(_))),
            "the filename stays in the parent: {:?}",
            plan.steps
        );
        assert!(
            plan.steps
                .iter()
                .any(|step| matches!(step, Step::Action { action_name, .. } if action_name == "upper")),
            "the last action stays in the parent: {:?}",
            plan.steps
        );
        assert_eq!(
            plan.steps
                .iter()
                .filter(|step| matches!(step, Step::Action { .. }))
                .count(),
            1,
            "the predecessor's own action moved into the boundary rather than being duplicated"
        );
        Ok(())
    }

    /// A failure reports the same cause either way, which is only true because a dependency's
    /// error is chained into its dependent.
    #[tokio::test]
    async fn equivalence_failure_reports_the_same_cause(
    ) -> Result<(), Box<dyn std::error::Error>> {
        fn boom(_state: &State<Value>) -> Result<Value, Error> {
            Err(Error::general_error("the real reason".to_owned()))
        }
        fn upper(state: &State<Value>) -> Result<Value, Error> {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        }
        type CommandEnvironment = ImmediateEnvironment<Value>;
        let mut environment = ImmediateEnvironment::<Value>::new();
        let registry = &mut environment.command_registry;
        register_command!(registry, fn boom(state) -> result)?;
        register_command!(registry, fn upper(state) -> result)?;
        let envref = environment.to_ref();

        let recipe = Recipe::new("boom/upper".to_owned(), String::new(), String::new())?;
        let (expanded, cut, _) = evaluate_both_ways(envref, recipe, None).await;

        for result in [&expanded, &cut] {
            let Err(error) = result else {
                panic!("both forms must fail, got {}", describe(result));
            };
            assert!(
                error.message.contains("the real reason"),
                "the cause must survive either form, got: {error}"
            );
        }
        Ok(())
    }

}
