use proc_macro::TokenStream;

mod registration;

#[proc_macro]
pub fn register_command(input: TokenStream) -> TokenStream {
    registration::register_command_impl(input)
}
