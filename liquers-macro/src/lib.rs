use proc_macro::TokenStream;

mod registration;
mod versioning;

#[proc_macro]
pub fn register_command(input: TokenStream) -> TokenStream {
    registration::register_command_impl(input)
}

#[proc_macro_attribute]
pub fn command_version(attr: TokenStream, item: TokenStream) -> TokenStream {
    versioning::command_version_impl(attr, item)
}
