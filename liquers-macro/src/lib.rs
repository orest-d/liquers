use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::parse_macro_input;

#[derive(Debug, Clone, PartialEq)]
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
                        let #var_name: #ty = arguments.get_injected(#name_str, &context)?;
                    }
                } else {
                    quote! {
                        let #var_name: #ty = arguments.get()?;
                    }
                }
            }
            _ => quote! {},
        }
    }

    pub fn parameter_name(&self) -> proc_macro2::TokenStream {
        match self {
            CommandParameter::Param {
                name, ty, injected, ..
            } => {
                let var_name = syn::Ident::new(&format!("{}__par", name), name.span());
                quote! {#var_name}
            }
            CommandParameter::Context => quote! {context},
        }
    }

    pub fn default_value_expression(&self) -> proc_macro2::TokenStream {
        match self {
            CommandParameter::Param {
                default_value: Some(DefaultValue::Str(value)),
                ..
            } => quote! {
                liquers_core::command_metadata::CommandParameterValue::Value(
                    serde_json::Value::String(#value.to_string()))
            },
            CommandParameter::Param {
                default_value: Some(DefaultValue::Bool(value)),
                ..
            } => quote! {
                liquers_core::command_metadata::CommandParameterValue::Value(
                    serde_json::Value::Bool(#value))
            },
            CommandParameter::Param {
                default_value: Some(DefaultValue::Int(value)),
                ..
            } => {
                quote! {
                    liquers_core::command_metadata::CommandParameterValue::Value(
                        serde_json::Value::Number(
                            serde_json::Number::from_i64(#value).unwrap_or(
                                serde_json::Number::from_i64(0).unwrap()
                            )
                        )
                    )
                }
            }
            CommandParameter::Param {
                default_value: Some(DefaultValue::Float(value)),
                ..
            } => quote! {
                liquers_core::command_metadata::CommandParameterValue::Value(
                    serde_json::Value::Number(
                        serde_json::Number::from_f64(#value).unwrap_or(
                            serde_json::Number::from_f64(0.0).unwrap()
                        )
                    )
                )
            },
            _ => {
                quote! {
                    liquers_core::command_metadata::CommandParameterValue::None
                }
            }
        }
    }

    pub fn argument_type_expression(&self) -> proc_macro2::TokenStream {
        match self {
            CommandParameter::Param { ty, .. } => {
                // Refactored helper: returns (is_option, Some(inner_ident)) or (false, Some(ident)) for plain types
                fn is_option_of(ty: &syn::Type) -> (bool, Option<String>) {
                    if let syn::Type::Path(type_path) = ty {
                        if type_path.path.segments.len() == 1
                            && type_path.path.segments[0].ident == "Option"
                        {
                            if let syn::PathArguments::AngleBracketed(ref args) =
                                type_path.path.segments[0].arguments
                            {
                                if let Some(syn::GenericArgument::Type(syn::Type::Path(inner_ty))) =
                                    args.args.first()
                                {
                                    if let Some(inner_ident) = inner_ty.path.get_ident() {
                                        return (true, Some(inner_ident.to_string()));
                                    }
                                }
                            }
                        } else if let Some(ident) = type_path.path.get_ident() {
                            return (false, Some(ident.to_string()));
                        }
                    }
                    (false, None)
                }

                let (is_option, inner) = is_option_of(ty);
                match (is_option, inner.as_deref()) {
                    (false, Some("isize")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Integer }
                    }
                    (true, Some("usize")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::IntegerOption }
                    }
                    (false, Some("i32")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Integer }
                    }
                    (true, Some("i32")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::IntegerOption }
                    }
                    (false, Some("i64")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Integer }
                    }
                    (true, Some("i64")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::IntegerOption }
                    }
                    (false, Some("u32")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Integer }
                    }
                    (true, Some("u32")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::IntegerOption }
                    }
                    (false, Some("u64")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Integer }
                    }
                    (true, Some("u64")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::IntegerOption }
                    }
                    (false, Some("f32")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Float }
                    }
                    (true, Some("f32")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::FloatOpt }
                    }
                    (false, Some("f64")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Float }
                    }
                    (true, Some("f64")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::FloatOpt }
                    }
                    (false, Some("bool")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Boolean }
                    }
                    (false, Some("String")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::String }
                    }
                    (false, Some("Value")) => {
                        quote! { liquers_core::command_metadata::ArgumentType::Any }
                    }
                    _ => quote! { liquers_core::command_metadata::ArgumentType::Any },
                }
            }
            _ => {
                quote! { liquers_core::command_metadata::ArgumentType::None }
            }
        }
    }
    pub fn argument_info_expression(&self) -> Option<proc_macro2::TokenStream> {
        match self {
            CommandParameter::Param {
                name,
                ty,
                injected,
                default_value,
                label,
                gui,
            } => {
                let name_str = name.to_string();
                let default_label = name_str.replace('_', " ");
                let label_str = label
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or(default_label.as_str());
                let argument_type = self.argument_type_expression();
                let gui_str = gui.as_ref().map(|s| s.as_str()).unwrap_or("");
                let default_value_expression = self.default_value_expression();

                Some(quote! {
                    liquers_core::command_metadata::ArgumentInfo{
                        name: #name_str.to_string(),
                        label: #label_str.to_string(),
                        default: #default_value_expression,
                        argument_type: #argument_type,
                        multiple: false,
                        injected: #injected,
                        ..Default::default()
                    }
                })
            }
            _ => None,
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

enum CommandSignatureStatement {
    Label(String),
    Doc(String),
    Namespace(String),
    Realm(String),
}

impl Parse for CommandSignatureStatement {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;
        input.parse::<syn::Token![:]>()?;
        match ident.to_string().as_str() {
            "label" => {
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandSignatureStatement::Label(lit.value()))
            }
            "doc" => {
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandSignatureStatement::Doc(lit.value()))
            }
            "namespace" | "ns" => {
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandSignatureStatement::Namespace(lit.value()))
            }
            "realm" => {
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandSignatureStatement::Realm(lit.value()))
            }
            other => Err(syn::Error::new(
                ident.span(),
                format!("Unknown command signature statement '{}'", other),
            )),
        }
    }
}

enum CommandParameterStatement {
    Label(String),
    Gui(String),
    Hint(String, String),
}

impl Parse for CommandParameterStatement {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;
        match ident.to_string().as_str() {
            "label" => {
                input.parse::<syn::Token![:]>()?;
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandParameterStatement::Label(lit.value()))
            }
            "gui" => {
                input.parse::<syn::Token![:]>()?;
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandParameterStatement::Gui(lit.value()))
            }
            "hint" => {
                // Parse: hint key_identifier: "Some hint"
                let key: syn::Ident = input.parse()?;
                input.parse::<syn::Token![:]>()?;
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandParameterStatement::Hint(key.to_string(), lit.value()))
            }
            other => Err(syn::Error::new(
                ident.span(),
                format!("Unknown command parameter statement '{}'", other),
            )),
        }
    }
}

struct CommandSignature {
    pub is_async: bool,
    pub name: syn::Ident,
    pub label: Option<String>,
    pub doc: Option<String>,
    pub state_parameter: StateParameter,
    pub parameters: Vec<CommandParameter>,
    pub result_type: ResultType,
    pub namespace: String,
    pub realm: String,
    pub command_statements: Vec<CommandSignatureStatement>,
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
    pub fn state_argument_parameter(&self) -> Option<proc_macro2::TokenStream> {
        match self.state_parameter {
            StateParameter::Value => Some(quote! {(&*(state.data)).clone()}),
            StateParameter::State => Some(quote! {state}),
            StateParameter::None => None,
        }
    }

    pub fn wrapper_arguments(&self) -> proc_macro2::TokenStream {
        let state_param = self.state_argument_parameter();
        let mut args = vec![];
        if let Some(state_param) = state_param {
            args.push(state_param);
        }
        for param in &self.parameters {
            args.push(param.parameter_name());
        }
        quote! {
            #(#args),*
        }
    }

    /// Name of the wrapper function, that can be registered as a command
    pub fn wrapper_fn_name(&self) -> syn::Ident {
        syn::Ident::new(&format!("{}__CMD_", self.name), self.name.span())
    }

    /// Name of a function that does the registration of the command
    pub fn register_fn_name(&self) -> syn::Ident {
        syn::Ident::new(&format!("REGISTER__{}", self.name), self.name.span())
    }

    pub fn wrapper_fn_signature(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let wrapper_name = self.wrapper_fn_name();
        if self.is_async {
            quote! {
                fn #wrapper_name(
                    state: &liquers_core::state::State<CommandValue>,
                    arguments: &mut liquers_core::commands::NGCommandArguments<CommandValue>,
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
                    arguments: &mut liquers_core::commands::NGCommandArguments<CommandValue>,
                    context: CommandContext,
                ) -> core::result::Result<CommandValue, liquers_core::error::Error>
            }
        }
    }
    pub fn command_call(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let ret = self.result_type.convert_result();
        let wrapper_args = self.wrapper_arguments();
        if self.is_async {
            quote! {
                async move {
                    let res = #fn_name(#wrapper_args).await;
                    #ret
                }.boxed()
            }
        } else {
            quote! {
                let res = #fn_name(#wrapper_args);
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

    pub fn register_command(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let wrapper_fn_name = self.wrapper_fn_name();
        let key = self.command_key_expression();
        if self.is_async {
            quote! {
                 registry.register_async_command(#key, #wrapper_fn_name)?
            }
        } else {
            quote! {
                 registry.register_command(#key, #wrapper_fn_name)?
            }
        }
    }

    pub fn command_registration(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let register_fn_name = self.register_fn_name();
        let command_wrapper = self.command_wrapper();
        let wrapper_fn_name = self.wrapper_fn_name();
        let reg_command = self.register_command();
        let default_label = fn_name.to_string().replace('_', " ");
        let label = self
            .label
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(default_label.as_str());
        let doc = self.doc.as_ref().map(|s| s.as_str()).unwrap_or("");
        let doc_cmd = if doc.is_empty() {
            quote! {}
        } else {
            quote! { cm.with_doc(#doc); }
        };
        let arguments = self.command_arguments_expression();

        quote! {
            pub fn #register_fn_name(
                registry: &mut liquers_core::commands::NGCommandRegistry<CommandPayload, CommandValue, CommandContext>)
                -> core::result::Result<&mut liquers_core::command_metadata::CommandMetadata, liquers_core::error::Error>
            {
                #command_wrapper

                let mut cm = #reg_command;
                cm.with_label(#label);
                #doc_cmd
                cm.arguments = #arguments;

                Ok(cm)
            }
        }
    }

    /// Returns a TokenStream expression constructing a CommandKey from realm, namespace, and function name.
    pub fn command_key_expression(&self) -> proc_macro2::TokenStream {
        let realm = &self.realm;
        let namespace = &self.namespace;
        let name = self.name.to_string();
        quote! {
            liquers_core::command_metadata::CommandKey::new(#realm, #namespace, #name)
        }
    }

    /// Generates a TokenStream that creates a Vec of ArgumentInfo for all defined arguments.
    pub fn command_arguments_expression(&self) -> proc_macro2::TokenStream {
        let args: Vec<proc_macro2::TokenStream> = self
            .parameters
            .iter()
            .filter_map(|p| p.argument_info_expression())
            .collect();
        quote! {
            vec![
                #(#args),*
            ]
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
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    while !content.is_empty() {
                        let stmt: CommandParameterStatement = content.parse()?;
                        match stmt {
                            CommandParameterStatement::Label(l) => label = Some(l),
                            CommandParameterStatement::Gui(g) => gui = Some(g),
                            CommandParameterStatement::Hint(_, _) => {} // TODO: handle hints
                        }
                        if content.peek(syn::Token![,]) {
                            content.parse::<syn::Token![,]>()?;
                        }
                    }
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

        // Parse command signature statements (e.g. label: "foo", doc: "bar")
        let mut command_statements = Vec::new();
        while input.peek(syn::Ident) {
            command_statements.push(input.parse()?);
        }

        // Optionally, set namespace/realm from statements if present
        let mut namespace = String::new();
        let mut realm = String::new();
        let mut label = None;
        let mut doc = None;
        for stmt in &command_statements {
            match stmt {
                CommandSignatureStatement::Namespace(ns) => namespace = ns.clone(),
                CommandSignatureStatement::Realm(r) => realm = r.clone(),
                CommandSignatureStatement::Label(l) => label = Some(l.clone()),
                CommandSignatureStatement::Doc(d) => doc = Some(d.clone()),
            }
        }

        Ok(CommandSignature {
            is_async,
            name,
            label,
            doc,
            state_parameter,
            parameters,
            result_type,
            namespace,
            realm,
            command_statements,
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
                other => Err(syn::Error::new(
                    ident.span(),
                    format!("Unknown result type '{}'", other),
                )),
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

struct CommandSignatureExt {
    cr: syn::Ident,
    sig: CommandSignature,
}

impl Parse for CommandSignatureExt {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse the identifier for the command registry (e.g., cr)
        let cr: syn::Ident = input.parse()?;
        // Parse a comma
        input.parse::<syn::Token![,]>()?;
        // Parse the command signature
        let sig: CommandSignature = input.parse()?;
        Ok(CommandSignatureExt { cr, sig })
    }
}

#[proc_macro]
pub fn register_command(input: TokenStream) -> TokenStream {
    let sig = parse_macro_input!(input as CommandSignatureExt);
    let register_fn = sig.sig.command_registration();
    let register_fn_name = sig.sig.register_fn_name();
    let cr = sig.cr;
    let gen = quote! {
        {
            #register_fn
            #register_fn_name(#cr)
        }
    };
    gen.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn extract_all_parameters_basic() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> result
        };
        let tokens = sig.extract_all_parameters();
        let expected = quote! {
            let a__par: i32 = arguments.get()?;
            let b__par: String = arguments.get_injected("b", &context)?;
            let c__par: f64 = arguments.get()?;
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
                arguments: &mut liquers_core::commands::NGCommandArguments<CommandValue>,
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
                arguments: &mut liquers_core::commands::NGCommandArguments<CommandValue>,
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
                arguments: &mut liquers_core::commands::NGCommandArguments<CommandValue>,
                context: CommandContext,
            ) -> core::result::Result<CommandValue, liquers_core::error::Error>
            {
                let a__par: i32 = arguments.get()?;
                let b__par: String = arguments.get_injected("b", &context)?;
                let c__par: f64 = arguments.get()?;
                let res = test_fn(state, a__par, b__par, c__par);
                res
            }
        };
        assert_eq!(
            expanded.to_string().replace(" ", ""),
            expected.to_string().replace(" ", "")
        );
    }

    #[test]
    fn test_argument_type_expression_i32() {
        let param = CommandParameter::Param {
            name: parse_quote! { a },
            ty: parse_quote! { i32 },
            injected: false,
            default_value: None,
            label: None,
            gui: None,
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::Integer };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_argument_type_expression_option_i32() {
        let param = CommandParameter::Param {
            name: parse_quote! { a },
            ty: parse_quote! { Option<i32> },
            injected: false,
            default_value: None,
            label: None,
            gui: None,
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::IntegerOption };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_argument_type_expression_f64() {
        let param = CommandParameter::Param {
            name: parse_quote! { a },
            ty: parse_quote! { f64 },
            injected: false,
            default_value: None,
            label: None,
            gui: None,
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::Float };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_argument_type_expression_option_f64() {
        let param = CommandParameter::Param {
            name: parse_quote! { a },
            ty: parse_quote! { Option<f64> },
            injected: false,
            default_value: None,
            label: None,
            gui: None,
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::FloatOpt };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_argument_type_expression_string() {
        let param = CommandParameter::Param {
            name: parse_quote! { a },
            ty: parse_quote! { String },
            injected: false,
            default_value: None,
            label: None,
            gui: None,
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::String };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_argument_type_expression_value() {
        let param = CommandParameter::Param {
            name: parse_quote! { a },
            ty: parse_quote! { Value },
            injected: false,
            default_value: None,
            label: None,
            gui: None,
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::Any };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_command_signature_with_label() {
        use syn::parse_quote;

        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32) -> result
            label: "Test label"
        };

        assert_eq!(sig.label, Some("Test label".to_string()));
    }

    #[test]
    fn test_command_signature_with_default_value() {
        use syn::parse_quote;

        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32 = 42) -> result
        };


        // Find the parameter 'a'
        let param = sig.parameters.iter().find_map(|p| {
            if let CommandParameter::Param {
                name,
                default_value,
                ..
            } = p
            {
                if name == "a" {
                    return Some(default_value);
                }
            }
            None
        });

        assert_eq!(param, Some(&Some(DefaultValue::Int(42))));

        let param = sig
            .parameters
            .iter()
            .find(|p| {
                if let CommandParameter::Param { name, .. } = p {
                    name == "a"
                } else {
                    false
                }
            })
            .unwrap();

        let info_tokens = param.argument_info_expression().unwrap();
        let info_string = info_tokens.to_string();
        println!("info_string: {}", info_string);

        assert!(info_string.contains("name : \"a\""));
        assert!(info_string.contains(
            "default : liquers_core :: command_metadata :: CommandParameterValue :: Value"
        ));
        assert!(info_string.contains("Value (serde_json :: Value :: Number (serde_json :: Number :: from_i64 (42i64) . unwrap_or (serde_json :: Number :: from_i64 (0) . unwrap ())))"));
        assert!(info_string.contains(
            "argument_type : liquers_core :: command_metadata :: ArgumentType :: Integer"
        ));
    }

    #[test]
    fn test_command_registration_generates_function() {
        use syn::parse_quote;

        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32) -> result
            label: "Test label"
        };

        let tokens = sig.command_registration();

        let expected = r#"
            pub fn REGISTER__test_fn(
                registry: &mut liquers_core::commands::NGCommandRegistry<
                    CommandPayload,
                    CommandValue,
                    CommandContext
                >
            ) -> core::result::Result<
                &mut liquers_core::command_metadata::CommandMetadata,
                liquers_core::error::Error
            > {
                fn test_fn__CMD_(
                    state: &liquers_core::state::State<CommandValue>,
                    arguments: &mut liquers_core::commands::NGCommandArguments<CommandValue>,
                    context: CommandContext,
                ) -> core::result::Result<CommandValue, liquers_core::error::Error>
                {
                    let a__par: i32 = arguments.get()?;
                    let res = test_fn(state, a__par);
                    res
                }
                let mut cm = registry.register_command(
                    liquers_core::command_metadata::CommandKey::new("", "", "test_fn"),
                    test_fn__CMD_
                )?;
                cm.with_label("Test label");
                cm.arguments = vec![liquers_core::command_metadata::ArgumentInfo {
                    name: "a".to_string(),
                    label: "a".to_string(),
                    default: liquers_core::command_metadata::CommandParameterValue::None,
                    argument_type: liquers_core::command_metadata::ArgumentType::Integer,
                    multiple: false,
                    injected: false,
                    ..Default::default()
                }];
                Ok(cm)
            }
        
        "#;

        //println!("Generated tokens: {}", tokens.to_string());
        fn fuzzy(s: &str) -> String {
            s.replace(' ', "").replace('\n', "")
        }
        assert!(tokens.to_string().contains("pub fn"));
        assert!(fuzzy(&tokens.to_string()).contains("cm.with_label"));
        for (a, b) in fuzzy(&tokens.to_string())
            .split("::")
            .zip(fuzzy(expected).split("::"))
        {
            assert_eq!(a, b);
        }
        assert_eq!(fuzzy(&tokens.to_string()), fuzzy(expected));
    }

    #[test]
    fn test_command_parameter_with_label_argument_info() {
        use syn::parse_quote;
        let param: CommandParameter = syn::parse_quote! {
            a: i32 = 42 (label: "My Argument")
        };

        // Generate ArgumentInfo token stream
        let info_tokens = param.argument_info_expression().unwrap();
        let info_string = info_tokens.to_string();

        // Check that the label is present and correct
        assert!(info_string.contains("label : \"My Argument\""));
        // Check that the name is present and correct
        assert!(info_string.contains("name : \"a\""));
        // Check that the argument type is correct
        assert!(info_string.contains("argument_type : liquers_core :: command_metadata :: ArgumentType :: Integer"));
    }
}
