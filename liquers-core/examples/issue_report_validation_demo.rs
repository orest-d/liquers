use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata};
use liquers_core::environment_builder::{EnvironmentBuilder, Inline};
use liquers_core::value::Value;

fn main() {
    let mut metadata = CommandMetadata::new("invalid");
    let mut values = ArgumentInfo::argument("values");
    values.multiple = true;
    metadata.arguments.push(values);
    metadata.arguments.push(ArgumentInfo::argument("tail"));

    let mut builder = EnvironmentBuilder::<Value, (), Inline>::new();
    builder
        .command_registry
        .command_metadata_registry
        .add_command(&metadata);
    println!("{}", builder.validate());
    match builder.build() {
        Ok(_) => panic!("invalid metadata must not build"),
        Err(error) => println!("{error}"),
    }
}
