use std::fmt::Display;

use proc_macro::TokenStream;
use proc_macro2::extra;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput};
use syn::parse::{Parse, ParseStream};

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
            CommandParameter::Param { name, ty, injected, .. } => {
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
            },
            _ => quote!{},
        }
    }
}

enum StateParameter {
    Value,
    State,
    None,
}

struct CommandSignature {
    is_async: bool,
    name: syn::Ident,
    value_type: Option<syn::Type>,
    state_parameter: StateParameter,
    parameters: Vec<CommandParameter>,
    result_type: syn::Type,
}

impl CommandSignature {
    pub fn extract_all_parameters(&self) -> proc_macro2::TokenStream {
        let extractors: Vec<proc_macro2::TokenStream> = self.parameters.iter().map(|p| p.parameter_extractor()).collect();
        quote! {
            #(#extractors)*
        }
    }
    pub fn wrapper_fn_signature(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let wrapper_name = syn::Ident::new(&format!("{}__CMD_", fn_name), fn_name.span());
        let result_type = &self.result_type;
        let value_type = self.value_type.as_ref();
        if self.is_async {
            if let Some(vt) = value_type {
                quote! {
                    async fn #wrapper_name<P, C: ActionContext<P, #vt>>(
                        state: &State<#vt>,
                        arguments: &mut NGCommandArguments<#vt>,
                        context: C,
                    ) -> Result<#vt, Error>
                }
            } else {
                quote! {
                    async fn #wrapper_name<P, V: ValueInterface, C: ActionContext<P, V>>(
                        state: &State<V>,
                        arguments: &mut NGCommandArguments<V>,
                        context: C,
                    ) -> Result<V, Error>
                }
            }
        } else {
            if let Some(vt) = value_type {
                quote! {
                    fn #wrapper_name<P, C: ActionContext<P, #vt>>(
                        state: &State<#vt>,
                        arguments: &mut NGCommandArguments<#vt>,
                        context: C,
                    ) -> Result<#vt, Error>
                }
            } else {
                quote! {
                    fn #wrapper_name<P, V: ValueInterface, C: ActionContext<P, V>>(
                        state: &State<V>,
                        arguments: &mut NGCommandArguments<V>,
                        context: C,
                    ) -> Result<V, Error>
                }
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
                    return Err(syn::Error::new(name.span(), "Parameter name must not start with underscore (_)."));
                }
                if name_str.contains("__") {
                    return Err(syn::Error::new(name.span(), "Parameter name must not contain double underscore (__)."));
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
                Ok(CommandParameter::Param { name, ty, injected, default_value, label, gui })
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
        let result_type: syn::Type = input.parse()?;
        Ok(CommandSignature {
            is_async,
            name,
            value_type,
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
                },
                "state" => {
                    input.parse::<syn::Ident>()?;
                    Ok(StateParameter::State)
                },
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


#[proc_macro]
pub fn command_signature(input: TokenStream) -> TokenStream {
    let sig = parse_macro_input!(input as CommandSignature);
    let fn_name = &sig.name;
    let signature = sig.wrapper_fn_signature();
    let extract_parameters = sig.extract_all_parameters();
    let gen = if sig.is_async {
        quote! {
            #signature {
                #extract_parameters
                let res = #fn_name(state, arguments, context).await;
            }
        }
    } else {
        quote! {
            #signature {
                #extract_parameters
                let res = #fn_name(state, arguments, context);
            }
        }
    };
    gen.into()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_fn_signature_sync() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state) -> Result<Value, Error>
        };
        let tokens = sig.wrapper_fn_signature();
        let expected = quote! {
            fn test_fn__CMD_<P, V: ValueInterface, C: ActionContext<P, V>>(
                state: &State<V>,
                arguments: &mut NGCommandArguments<V>,
                context: C,
            ) -> Result<V, Error>
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn wrapper_fn_signature_sync_with_value_type() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn<ValueType>(state) -> Result<Value, Error>
        };
        let tokens = sig.wrapper_fn_signature();
        let expected = quote! {
            fn test_fn__CMD_<P, C: ActionContext<P, ValueType>>(
                state: &State<ValueType>,
                arguments: &mut NGCommandArguments<ValueType>,
                context: C,
            ) -> Result<ValueType, Error>
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn wrapper_fn_signature_async() {
        let sig: CommandSignature = syn::parse_quote! {
            async fn test_async(state) -> Result<Value, Error>
        };
        let tokens = sig.wrapper_fn_signature();
        let expected = quote! {
            async fn test_async__CMD_<P, V: ValueInterface, C: ActionContext<P, V>>(
                state: &State<V>,
                arguments: &mut NGCommandArguments<V>,
                context: C,
            ) -> Result<V, Error>
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn extract_all_parameters_basic() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> Result<Value, Error>
        };
        let tokens = sig.extract_all_parameters();
        let expected = quote! {
            let a__par: i32 = args.get()?;
            let b__par: String = args.get_injected("b", &context)?;
            let c__par: f64 = args.get()?;
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

}
