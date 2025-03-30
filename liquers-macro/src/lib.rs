use std::fmt::Display;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput};
use syn::parse::{Parse, ParseStream};

struct CommandRegistration {
    is_async: bool,
    name: syn::Ident,
}

impl Display for CommandRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}fn {}", (if self.is_async {"async "} else {""}), self.name)
    }
}

impl Parse for CommandRegistration {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let is_async = input.parse::<syn::Token![async]>().is_ok();
        input.parse::<syn::Token![fn]>()?;
        let name = input.parse::<syn::Ident>()?;
        Ok(CommandRegistration { is_async, name })
    }
}

impl ToTokens for CommandRegistration {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let is_async = self.is_async;
        let name = &self.name;
        if is_async {
            tokens.extend(quote! {
                println!("Async command {}", stringify!(#name));
            });
        } else {
            tokens.extend(quote! {
                println!("Command {}", stringify!(#name));
            });
        }
    }
}


#[proc_macro]
pub fn register_commands(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as CommandRegistration);
    let output = quote! {
        #input
    };
    output.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello1() {
        let input:CommandRegistration = syn::parse_quote! {
            fn hello
        };
        assert_eq!(input.to_string(), "fn hello");
    }
}

