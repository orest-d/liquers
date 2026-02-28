use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemFn};

pub(crate) fn command_version_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "command_version does not accept attribute arguments",
        )
        .to_compile_error()
        .into();
    }

    let item_string = item.to_string();
    let function = parse_macro_input!(item as ItemFn);
    let function_name = &function.sig.ident;
    let visibility = &function.vis;
    let version_function_name = format_ident!("{}__VERSION_", function_name);
    let hash = blake3::hash(item_string.as_bytes()).to_hex().to_string();

    quote! {
        #function

        #[allow(non_snake_case)]
        #visibility fn #version_function_name() -> &'static str {
            #hash
        }
    }
    .into()
}
