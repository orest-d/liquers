use std::fmt::Display;

use proc_macro::TokenStream;
use proc_macro2::extra;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, DeriveInput};

enum DefaultValue {
    Str(String),
    Bool(bool),
    Int(i64),
    Float(f64),
}

enum CommandParameter {
    Param {
        name: syn::Ident,
        ty: syn::Type,
        injected: bool,
        default_value: Option<DefaultValue>,
        label: Option<String>,
        gui: Option<String>,
    },
    Context,
}

impl CommandParameter {
    pub fn parameter_extractor(&self) -> proc_macro2::TokenStream {
        match self {
            CommandParameter::Param {
                name, ty, injected, ..
            } => {
                let var_name = syn::Ident::new(&format!("{}__par", name), name.span());
                if *injected {
                    let name_str = name.to_string();
                    quote! {
                        let #var_name: #ty = args.get_injected(#name_str, &context)?;
                    }
                } else {
                    quote! {
                        let #var_name: #ty = args.get()?;
                    }
                }
            }
            _ => quote! {},
        }
    }
}

enum StateParameter {
    Value,
    State,
    None,
}

enum ResultType {
    Value,
    Result,
}

struct CommandSignature {
    is_async: bool,
    name: syn::Ident,
    state_parameter: StateParameter,
    parameters: Vec<CommandParameter>,
    result_type: ResultType,
}

impl CommandSignature {
    pub fn extract_all_parameters(&self) -> proc_macro2::TokenStream {
        let extractors: Vec<proc_macro2::TokenStream> = self
            .parameters
            .iter()
            .map(|p| p.parameter_extractor())
            .collect();
        quote! {
            #(#extractors)*
        }
    }
    pub fn wrapper_fn_signature(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let wrapper_name = syn::Ident::new(&format!("{}__CMD_", fn_name), fn_name.span());
        if self.is_async {
            quote! {
                fn #wrapper_name(
                    state: &liquers_core::state::State<CommandValue>,
                    arguments: &mut liquers_core::command::NGCommandArguments<CommandValue>,
                    context: CommandContext,
                ) ->
                core::pin::Pin<
                  std::boxed::Box<
                    dyn core::future::Future<
                      Output = core::result::Result<CommandValue, liquers_core::error::Error>
                    > + core::sync::Send  + 'static
                  >
                >
            }
        } else {
            quote! {
                fn #wrapper_name(
                    state: &liquers_core::state::State<CommandValue>,
                    arguments: &mut liquers_core::command::NGCommandArguments<CommandValue>,
                    context: CommandContext,
                ) -> core::result::Result<CommandValue, liquers_core::error::Error>
            }
        }
    }
    pub fn command_call(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let ret = self.result_type.convert_result();
        if self.is_async {
            quote! {
                async move {
                    let res = #fn_name(state, arguments, context).await;
                    #ret
                }.boxed()
            }
        } else {
            quote! {
                let res = #fn_name(state, arguments, context);
                #ret
            }
        }
    }
    pub fn command_wrapper(&self) -> proc_macro2::TokenStream {
        let signature = self.wrapper_fn_signature();
        let extract_parameters = self.extract_all_parameters();
        let call = self.command_call();
        quote! {
            #signature {
                #extract_parameters
                #call
            }
        }
    }

}

impl Parse for CommandParameter {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::Ident) {
            let fork = input.fork();
            let ident: syn::Ident = fork.parse()?;
            if ident == "context" || ident == "Context" {
                input.parse::<syn::Ident>()?;
                Ok(CommandParameter::Context)
            } else {
                let name: syn::Ident = input.parse()?;
                let name_str = name.to_string();
                if name_str.starts_with('_') {
                    return Err(syn::Error::new(
                        name.span(),
                        "Parameter name must not start with underscore (_).",
                    ));
                }
                if name_str.contains("__") {
                    return Err(syn::Error::new(
                        name.span(),
                        "Parameter name must not contain double underscore (__).",
                    ));
                }
                input.parse::<syn::Token![:]>()?;
                let ty: syn::Type = input.parse()?;
                let injected = if input.peek(syn::Ident) {
                    let flag: syn::Ident = input.parse()?;
                    flag == "injected"
                } else {
                    false
                };
                let default_value = if input.peek(syn::Token![=]) {
                    input.parse::<syn::Token![=]>()?;
                    Some(DefaultValue::parse(input)?)
                } else {
                    None
                };
                let mut label = None;
                let mut gui = None;
                if input.peek(syn::LitStr) {
                    let lit: syn::LitStr = input.parse()?;
                    label = Some(lit.value());
                }
                if input.peek(syn::Token![/]) {
                    input.parse::<syn::Token![/]>()?;
                    let gui_lit: syn::LitStr = input.parse()?;
                    gui = Some(gui_lit.value());
                }
                Ok(CommandParameter::Param {
                    name,
                    ty,
                    injected,
                    default_value,
                    label,
                    gui,
                })
            }
        } else {
            Err(input.error("Expected parameter or 'context'"))
        }
    }
}

impl Parse for CommandSignature {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let is_async = input.parse::<syn::Token![async]>().is_ok();
        input.parse::<syn::Token![fn]>()?;
        let name: syn::Ident = input.parse()?;
        let value_type = if input.peek(syn::Token![<]) {
            input.parse::<syn::Token![<]>()?;
            let vt: syn::Type = input.parse()?;
            input.parse::<syn::Token![>]>()?;
            Some(vt)
        } else {
            None
        };
        let content;
        syn::parenthesized!(content in input);
        let state_parameter = if content.peek(syn::Ident) {
            let ident: syn::Ident = content.parse()?;
            match ident.to_string().as_str() {
                "value" => StateParameter::Value,
                "state" => StateParameter::State,
                _ => StateParameter::None,
            }
        } else {
            StateParameter::None
        };
        let mut parameters = Vec::new();
        while content.peek(syn::Token![,]) {
            content.parse::<syn::Token![,]>()?;
            parameters.push(content.parse()?);
        }
        input.parse::<syn::Token![->]>()?;
        let result_type = input.parse()?;
        Ok(CommandSignature {
            is_async,
            name,
            state_parameter,
            parameters,
            result_type,
        })
    }
}

impl Parse for StateParameter {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::Ident) {
            let fork = input.fork();
            let ident: syn::Ident = fork.parse()?;
            match ident.to_string().as_str() {
                "value" => {
                    input.parse::<syn::Ident>()?;
                    Ok(StateParameter::Value)
                }
                "state" => {
                    input.parse::<syn::Ident>()?;
                    Ok(StateParameter::State)
                }
                _ => Ok(StateParameter::None), // do not consume
            }
        } else {
            Ok(StateParameter::None)
        }
    }
}

impl Parse for DefaultValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::LitStr) {
            let lit: syn::LitStr = input.parse()?;
            Ok(DefaultValue::Str(lit.value()))
        } else if input.peek(syn::LitBool) {
            let lit: syn::LitBool = input.parse()?;
            Ok(DefaultValue::Bool(lit.value))
        } else if input.peek(syn::LitInt) {
            let lit: syn::LitInt = input.parse()?;
            let val = lit.base10_parse::<i64>()?;
            Ok(DefaultValue::Int(val))
        } else if input.peek(syn::LitFloat) {
            let lit: syn::LitFloat = input.parse()?;
            let val = lit.base10_parse::<f64>()?;
            Ok(DefaultValue::Float(val))
        } else {
            Err(input.error("Unsupported default value type"))
        }
    }
}

impl Parse for ResultType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::Ident) {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "value" => Ok(ResultType::Value),
                "result" => Ok(ResultType::Result),
                other => Err(syn::Error::new(ident.span(), format!("Unknown result type '{}'", other))),
            }
        } else {
            Err(input.error("Expected result type identifier (Value or Result)"))
        }
    }
}

impl ResultType {
    pub fn convert_result(&self) -> proc_macro2::TokenStream {
        match self {
            ResultType::Result => quote! { res },
            ResultType::Value => quote! { Ok(res) },
        }
    }
}

#[proc_macro]
pub fn command_wrapper(input: TokenStream) -> TokenStream {
    let sig = parse_macro_input!(input as CommandSignature);
    let signature = sig.wrapper_fn_signature();
    let extract_parameters = sig.extract_all_parameters();
    let call = sig.command_call();
    let gen = quote! {
        #signature {
            #extract_parameters
            #call
        }
    };
    gen.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn extract_all_parameters_basic() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> result
        };
        let tokens = sig.extract_all_parameters();
        let expected = quote! {
            let a__par: i32 = args.get()?;
            let b__par: String = args.get_injected("b", &context)?;
            let c__par: f64 = args.get()?;
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn wrapper_fn_signature_sync() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state) -> result
        };
        let tokens = sig.wrapper_fn_signature();
        let expected = quote! {
            fn test_fn__CMD_(
                state: &liquers_core::state::State<CommandValue>,
                arguments: &mut liquers_core::command::NGCommandArguments<CommandValue>,
                context: CommandContext,
            ) -> core::result::Result<CommandValue, liquers_core::error::Error>
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn wrapper_fn_signature_async() {
        let sig: CommandSignature = syn::parse_quote! {
            async fn test_fn(state) -> result
        };
        let tokens = sig.wrapper_fn_signature();
        let expected = quote! {
            fn test_fn__CMD_(
                state: &liquers_core::state::State<CommandValue>,
                arguments: &mut liquers_core::command::NGCommandArguments<CommandValue>,
                context: CommandContext,
            ) ->
            core::pin::Pin<
                std::boxed::Box<
                    dyn core::future::Future<
                        Output = core::result::Result<CommandValue, liquers_core::error::Error>
                    > + core::sync::Send  + 'static
                >
            >
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }


    #[test]
    fn command_wrapper_basic() {
        let input: proc_macro2::TokenStream = quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> result
        };
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> result
        };
        let expanded = sig.command_wrapper();
        let expected = quote! {
            fn test_fn__CMD_(
                state: &liquers_core::state::State<CommandValue>,
                arguments: &mut liquers_core::command::NGCommandArguments<CommandValue>,
                context: CommandContext,
            ) -> core::result::Result<CommandValue, liquers_core::error::Error>
            {
                let a__par: i32 = args.get()?;
                let b__par: String = args.get_injected("b", &context)?;
                let c__par: f64 = args.get()?;
                let res = test_fn(state, arguments, context);
                res
            }
        };
        assert_eq!(
            expanded.to_string().replace(" ", ""),
            expected.to_string().replace(" ", "")
        );
    }
}
