use liquers_core::{
    command_metadata::CommandKey, context2::{Context, Environment, SimpleEnvironment}, error::Error, interpreter2::evaluate, state::State, value::Value
};
use liquers_macro::*;

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
       {
        
            use futures::FutureExt;
            // TODO 4) Insert use futures::FutureExt;
            // TODO: turn off snake case warning
            pub fn REGISTER__greet(
                registry: &mut liquers_core::commands2::CommandRegistry<
                    CommandEnvironment,
                >,
            ) -> core::result::Result<
                &mut liquers_core::command_metadata::CommandMetadata,
                liquers_core::error::Error,
            > {
                fn greet__CMD_(
                    //state: &liquers_core::state::State<CommandEnvironment::Value>,
                    //TODO: 1) state is not a reference
                    //TODO: 2) fix generic argument for extracting the value 
                    state: liquers_core::state::State<<CommandEnvironment as liquers_core::context2::Environment>::Value>,
                    arguments: liquers_core::commands2::CommandArguments<
                        CommandEnvironment,
                    >,
                    context: Context<CommandEnvironment>,
                ) -> core::pin::Pin<
                    std::boxed::Box<
                        dyn core::future::Future<
                            Output = core::result::Result<
                                <CommandEnvironment as liquers_core::context2::Environment>::Value,
                                liquers_core::error::Error,
                            >,
                        > + core::marker::Send + 'static,
                        // TODO: 3) fix the module of Send
//                        > + core::sync::Send + 'static,
                    >,
                > {

                    // TODO: 5) convert to result or find another way - just move command arguments inside
                    async move {
                        let greet__par: String = arguments.get(0usize, "greet")?;
                        let res = greet(state, greet__par).await;
                        res
                    }
                        .boxed()
 //                       .boxed()
                }
                let mut cm = registry
                    .register_async_command(
                        liquers_core::command_metadata::CommandKey::new("", "", "greet"),
                        greet__CMD_,
                    )?;
                cm.with_label("greet");
                cm.arguments = vec![
                        liquers_core::command_metadata::ArgumentInfo {
                            name: "greet".to_string(),
                            label: "greet".to_string(),
                            default: liquers_core::command_metadata::CommandParameterValue::Value(
                                serde_json::Value::String("Hello".to_string()),
                            ),
                            argument_type: liquers_core::command_metadata::ArgumentType::String,
                            multiple: false,
                            injected: false,
                            gui_info: liquers_core::command_metadata::ArgumentGUIInfo::TextField(
                                20usize,
                            ),
                            ..Default::default()
                        },
                    ];
                Ok(cm)
            }
            REGISTER__greet(cr)
        }
            .expect("register_command failed");
 
    let envref = env.to_ref();

    let state = evaluate(envref.clone(), "world/greet", None).await?;

    let value = state.try_into_string()?;
    assert_eq!(value, "Hello, world!");
    Ok(())
}