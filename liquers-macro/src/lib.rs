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
    Query(String),
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
        } else if input.peek(syn::Ident) {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "query" => {
                    let lit: syn::LitStr = input.parse()?; // TODO: Validate query
                    Ok(DefaultValue::Query(lit.value()))
                }
                id @ _ => Err(input.error(format!(
                    "Unsupported default value type starting with literal {id}"
                ))),
            }
        } else {
            Err(input.error("Unsupported default value type"))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CommandPreset {
    action: String,
    label: String,
    description: String,
}
use quote::ToTokens;

impl ToTokens for CommandPreset {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let action = &self.action;
        let label = &self.label;
        let description = &self.description;
        tokens.extend(quote! {
            liquers_core::command_metadata::CommandPreset::new(#action, #label, #description)?
        });
    }
}

impl Parse for CommandPreset {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse the action as a string literal
        if !input.peek(syn::LitStr) {
            return Err(input.error("string expected as a preset action"));
        }
        let action_lit: syn::LitStr = input.parse()?;
        let action = action_lit.value();

        let mut label = action.clone();
        let mut description = String::new();

        // Optionally parse (label: "...", description: "...")
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            while !content.is_empty() {
                let ident: syn::Ident = content.parse()?;
                if !content.peek(syn::Token![:]) {
                    return Err(
                        input.error(format!("colon expected after '{ident}' in command preset"))
                    );
                }
                content.parse::<syn::Token![:]>()?;
                let value: syn::LitStr = content.parse()?;
                match ident.to_string().as_str() {
                    "label" => label = value.value(),
                    "description" => description = value.value(),
                    other => {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!("Unknown field '{}' in CommandPreset", other),
                        ))
                    }
                }
                if content.peek(syn::Token![,]) {
                    content.parse::<syn::Token![,]>()?;
                }
            }
        }

        Ok(CommandPreset {
            action,
            label,
            description,
        })
    }
}

/// This is a copy of ArgumentGUIInfo from liquers-core
enum ArgumentGUIInfo {
    /// Text field for entering a short text, e.g. a name or a title.
    /// Argument is a width hint specified in characters.
    /// UI may interpret the hints differently, e.g. as width_pixels = 10*width.
    TextField(usize),
    CodeField(usize, String),
    /// Text area for entering of a larger text with a width and height hints.
    /// Width and hight should be specified in characters.
    /// UI may interpret the hints differently, e.g. as width_pixels = 10*width.
    TextArea(usize, usize),
    CodeArea(usize, usize, String),
    IntegerField,
    /// Integer range with min and max values, unspecified how it should be rendered
    IntegerRange {
        min: i64,
        max: i64,
    },
    /// Integer range with min and max values, should be rendered as a slider
    IntegerSlider {
        min: i64,
        max: i64,
        step: i64,
    },
    /// Float entry field    
    FloatField,
    /// Float range with min and max values, should be rendered as a slider
    FloatSlider {
        min: f64,
        max: f64,
        step: f64,
    },
    /// Used to enter boolean values, should be presented as a checkbox.
    Checkbox,
    /// Used to enter boolean values, presentable as radio buttons with custom labels for true and false.
    RadioBoolean {
        true_label: String,
        false_label: String,
    },
    /// Used to enter enum values, arranged horizontally.
    /// This is to be used when only up to 3-4 alternatives are expected with short enum labels.
    HorizontalRadioEnum,
    /// Used to enter enum values, arranged vertically.
    /// This is to be used when many alternatives are expected or if enum labels are long.
    VerticalRadioEnum,
    /// Select enum from a dropdown list.
    EnumSelector,
    /// Color picker for a color value
    /// Should edit the color in form of a color name or hex code RGB or RGBA.
    /// The hex code does NOT start with `#`, but is just a string of 6 or 8 hexadecimal digits.
    ColorString,
    DateField(usize),
    /// Parameter should not appear in the GUI
    Hide,
    /// No GUI information
    None,
}

enum CommandParameter {
    Param {
        name: syn::Ident,
        ty: syn::Type,
        injected: bool,
        default_value: Option<DefaultValue>,
        label: Option<String>,
        gui: ArgumentGUIInfo,
    },
    Context,
}

impl CommandParameter {
    pub fn parameter_extractor(&self, i: usize) -> proc_macro2::TokenStream {
        match self {
            CommandParameter::Param {
                name, ty, injected, ..
            } => {
                let var_name = syn::Ident::new(&format!("{}__par", name), name.span());

                // Helper to check if type is Value or Any
                fn is_value_or_any(ty: &syn::Type) -> bool {
                    if let syn::Type::Path(type_path) = ty {
                        if let Some(ident) = type_path.path.get_ident() {
                            let name = ident.to_string().to_lowercase();
                            name == "value" || name == "any" || name == "commandvalue"
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }

                let name_str = name.to_string();
                if *injected {
                    quote! {
                        let #var_name: #ty = arguments.get_injected(#i, #name_str, context.clone())?;
                    }
                } else {
                    if is_value_or_any(ty) {
                        quote! {
                            let #var_name = arguments.get_value(#i, #name_str)?;
                        }
                    } else {
                        quote! {
                            let #var_name: #ty = arguments.get(#i, #name_str)?;
                        }
                    }
                }
            }
            _ => quote! {},
        }
    }

    pub fn parameter_name(&self) -> proc_macro2::TokenStream {
        match self {
            CommandParameter::Param {
                name, ..
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
            CommandParameter::Param {
                default_value: Some(DefaultValue::Query(value)),
                ..
            } => quote! {
                liquers_core::command_metadata::CommandParameterValue::Query(
                    liquers_core::query::TryToQuery::try_to_query(#value)?
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
                ty:_ty,
                injected,
                default_value: _default_value,
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
                let default_value_expression = self.default_value_expression();

                Some(quote! {
                    liquers_core::command_metadata::ArgumentInfo{
                        name: #name_str.to_string(),
                        label: #label_str.to_string(),
                        default: #default_value_expression,
                        argument_type: #argument_type,
                        multiple: false,
                        injected: #injected,
                        gui_info: #gui,
                        ..Default::default()
                    }
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum StateParameter {
    Value,
    State,
    Text,
    None,
}

enum ResultType {
    Value,
    Result,
}

enum CommandSignatureStatement {
    Volatile(bool),
    Label(String),
    Doc(String),
    Namespace(String),
    Realm(String),
    Preset(CommandPreset),
    Next(CommandPreset),
    Filename(String),
}

impl Parse for CommandSignatureStatement {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;
        if !input.peek(syn::Token![:]) {
            return Err(input.error(format!(
                "colon expected after '{ident}' in command signature statement"
            )));
        }

        input.parse::<syn::Token![:]>()?;
        match ident.to_string().as_str() {
            "volatile" =>{
                let lit: syn::LitBool = input.parse()?;
                Ok(CommandSignatureStatement::Volatile(lit.value()))
            }
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
            "preset" => {
                let preset: CommandPreset = input.parse()?;
                Ok(CommandSignatureStatement::Preset(preset))
            }
            "next" => {
                let next: CommandPreset = input.parse()?;
                Ok(CommandSignatureStatement::Next(next))
            }
            "filename" => {
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandSignatureStatement::Filename(lit.value()))
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
    Gui(ArgumentGUIInfo),
    Hint(String, String), // TODO: Implement hints
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
                let gui_info: ArgumentGUIInfo = input.parse()?;
                Ok(CommandParameterStatement::Gui(gui_info))
            }
            "hint" => {
                // Parse: hint key_identifier: "Some hint"
                let key: syn::Ident = input.parse()?;
                input.parse::<syn::Token![:]>()?;
                let lit: syn::LitStr = input.parse()?;
                Ok(CommandParameterStatement::Hint(
                    key.to_string(),
                    lit.value(),
                ))
            }
            other => Err(syn::Error::new(
                ident.span(),
                format!("Unknown command parameter statement '{}'", other),
            )),
        }
    }
}

enum WrapperVersion {
    V2,
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
    pub presets: Vec<CommandPreset>,
    pub next: Vec<CommandPreset>,
    pub filename: String,
    pub volatile: bool,
    pub wrapper_version: WrapperVersion,
}

impl CommandSignature {
    pub fn extract_all_parameters(&self) -> proc_macro2::TokenStream {

        let extractors: Vec<proc_macro2::TokenStream> = match self.wrapper_version {
            WrapperVersion::V2 => self
                .parameters
                .iter().enumerate()
                .map(|(i, p)| p.parameter_extractor(i))
                .collect(),
        };
        quote! {
            #(#extractors)*
        }
    }
    pub fn state_argument_parameter(&self) -> Option<proc_macro2::TokenStream> {
        match self.state_parameter {
            StateParameter::Value => Some(quote! {(&*(state.data)).clone()}),
            StateParameter::State => Some(quote! {state}),
            StateParameter::Text => Some(quote! {state.try_into_string()?.as_str()}),
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
        let wrapper_name = self.wrapper_fn_name();
        if self.is_async {
            quote! {
                #[allow(non_snake_case)]
                fn #wrapper_name(
                    state: liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                    arguments: liquers_core::commands::CommandArguments<CommandEnvironment>,
                    context: Context<CommandEnvironment>,
                ) ->
                core::pin::Pin<
                  std::boxed::Box<
                    dyn core::future::Future<
                      Output = core::result::Result<<CommandEnvironment  as liquers_core::context::Environment>::Value, liquers_core::error::Error>
                    > + core::marker::Send  + 'static
                  >
                >
            }
        } else {
            quote! {
                #[allow(non_snake_case)]
                fn #wrapper_name(
                    state: &liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                    arguments: liquers_core::commands::CommandArguments<CommandEnvironment>,
                    context: Context<CommandEnvironment>,
                ) -> core::result::Result<<CommandEnvironment as liquers_core::context::Environment>::Value, liquers_core::error::Error>
            }
        }
    }

    pub fn command_call(&self) -> proc_macro2::TokenStream {
        let fn_name = &self.name;
        let ret = self.result_type.convert_result();
        let wrapper_args = self.wrapper_arguments();
        if self.is_async {
            quote! {
                let res = #fn_name(#wrapper_args).await;
                #ret
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
        if self.is_async {
            quote! {
                #signature {
                    async move {
                        #extract_parameters
                        #call
                    }.boxed()
                }
            }
        } else {
            quote! {
                #signature {
                    #extract_parameters
                    #call
                }
            }
        }
    }

    pub fn register_command(&self) -> proc_macro2::TokenStream {
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
        let presets_code = if self.presets.is_empty() {
            // If no presets are defined, we can use an empty vector
            quote! {}
        } else {
            let presets = self.presets_expression();
            quote! { cm.presets = #presets; }
        };
        let filename: proc_macro2::TokenStream = {
            let filename_str = &self.filename;
            quote! { #filename_str }
        };
        let next_code = if self.next.is_empty() {
            // If no next commands are defined, we can use an empty vector
            quote! {}
        } else {
            let next = self.next_expression();
            quote! { cm.next = #next; }
        };


        let registry_type = match self.wrapper_version {
            WrapperVersion::V2 => quote! {
                liquers_core::commands::CommandRegistry<CommandEnvironment>
            },
        };
        let volatile_code = if self.volatile {
            quote!(cm.volatile = true;)
        }
        else{
            quote!()
        };
        quote! {
            #[allow(non_snake_case)]
            pub fn #register_fn_name(
                registry: &mut #registry_type
            ) -> core::result::Result<&mut liquers_core::command_metadata::CommandMetadata, liquers_core::error::Error>
            {
                #command_wrapper

                let mut cm = #reg_command;
                cm.with_label(#label);
                #doc_cmd
                cm.arguments = #arguments;
                #presets_code
                #next_code
                cm.with_filename(#filename);
                #volatile_code
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
    /// Generates a TokenStream that creates a Vec of CommandPreset for all defined presets.
    pub fn presets_expression(&self) -> proc_macro2::TokenStream {
        let presets: Vec<proc_macro2::TokenStream> =
            self.presets.iter().map(|p| quote! { #p }).collect();
        quote! {
            vec![
                #(#presets),*
            ]
        }
    }
    /// Generates a TokenStream that creates a Vec of CommandPreset for all defined presets.
    pub fn next_expression(&self) -> proc_macro2::TokenStream {
        let next: Vec<proc_macro2::TokenStream> =
            self.next.iter().map(|p| quote! { #p }).collect();
        quote! {
            vec![
                #(#next),*
            ]
        }   
    }
}

impl Parse for ArgumentGUIInfo {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;
        match ident.to_string().as_str() {
            "TextField" => {
                let width: syn::LitInt = input.parse()?;
                Ok(ArgumentGUIInfo::TextField(width.base10_parse()?))
            }
            "CodeField" => {
                let width: syn::LitInt = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                let lang: syn::LitStr = input.parse()?;
                Ok(ArgumentGUIInfo::CodeField(
                    width.base10_parse()?,
                    lang.value(),
                ))
            }
            "TextArea" => {
                let width: syn::LitInt = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                let height: syn::LitInt = input.parse()?;
                Ok(ArgumentGUIInfo::TextArea(
                    width.base10_parse()?,
                    height.base10_parse()?,
                ))
            }
            "CodeArea" => {
                let width: syn::LitInt = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                let height: syn::LitInt = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                let lang: syn::LitStr = input.parse()?;
                Ok(ArgumentGUIInfo::CodeArea(
                    width.base10_parse()?,
                    height.base10_parse()?,
                    lang.value(),
                ))
            }
            "IntegerField" => Ok(ArgumentGUIInfo::IntegerField),
            "IntegerRange" => {
                let content;
                syn::parenthesized!(content in input);
                let min: syn::LitInt = content.parse()?;
                content.parse::<syn::Token![,]>()?;
                let max: syn::LitInt = content.parse()?;
                Ok(ArgumentGUIInfo::IntegerRange {
                    min: min.base10_parse()?,
                    max: max.base10_parse()?,
                })
            }
            "IntegerSlider" => {
                let content;
                syn::parenthesized!(content in input);
                let min: syn::LitInt = content.parse()?;
                content.parse::<syn::Token![,]>()?;
                let max: syn::LitInt = content.parse()?;
                content.parse::<syn::Token![,]>()?;
                let step: syn::LitInt = content.parse()?;
                Ok(ArgumentGUIInfo::IntegerSlider {
                    min: min.base10_parse()?,
                    max: max.base10_parse()?,
                    step: step.base10_parse()?,
                })
            }
            "FloatField" => Ok(ArgumentGUIInfo::FloatField),
            "FloatSlider" => {
                let content;
                syn::parenthesized!(content in input);
                let min: syn::LitFloat = content.parse()?;
                content.parse::<syn::Token![,]>()?;
                let max: syn::LitFloat = content.parse()?;
                content.parse::<syn::Token![,]>()?;
                let step: syn::LitFloat = content.parse()?;
                Ok(ArgumentGUIInfo::FloatSlider {
                    min: min.base10_parse()?,
                    max: max.base10_parse()?,
                    step: step.base10_parse()?,
                })
            }
            "Checkbox" => Ok(ArgumentGUIInfo::Checkbox),
            "RadioBoolean" => {
                let content;
                syn::parenthesized!(content in input);
                let true_label: syn::LitStr = content.parse()?;
                content.parse::<syn::Token![,]>()?;
                let false_label: syn::LitStr = content.parse()?;
                Ok(ArgumentGUIInfo::RadioBoolean {
                    true_label: true_label.value(),
                    false_label: false_label.value(),
                })
            }
            "HorizontalRadioEnum" => Ok(ArgumentGUIInfo::HorizontalRadioEnum),
            "VerticalRadioEnum" => Ok(ArgumentGUIInfo::VerticalRadioEnum),
            "EnumSelector" => Ok(ArgumentGUIInfo::EnumSelector),
            "ColorString" => Ok(ArgumentGUIInfo::ColorString),
            "DateField" => {
                let width: syn::LitInt = input.parse()?;
                Ok(ArgumentGUIInfo::DateField(width.base10_parse()?))
            }
            "Hide" => Ok(ArgumentGUIInfo::Hide),
            "None" => Ok(ArgumentGUIInfo::None),
            other => Err(syn::Error::new(
                ident.span(),
                format!("Unknown ArgumentGUIInfo variant '{}'", other),
            )),
        }
    }
}

impl ToTokens for ArgumentGUIInfo {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let gui = match self {
            ArgumentGUIInfo::TextField(n) => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::TextField(#n) }
            }
            ArgumentGUIInfo::CodeField(w, lang) => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::CodeField(#w, #lang.to_string()) }
            }
            ArgumentGUIInfo::TextArea(w, h) => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::TextArea(#w, #h) }
            }
            ArgumentGUIInfo::CodeArea(w, h, lang) => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::CodeArea(#w, #h, #lang.to_string()) }
            }
            ArgumentGUIInfo::IntegerField => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::IntegerField }
            }
            ArgumentGUIInfo::IntegerRange { min, max } => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::IntegerRange { min: #min, max: #max } }
            }
            ArgumentGUIInfo::IntegerSlider { min, max, step } => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::IntegerSlider { min: #min, max: #max, step: #step } }
            }
            ArgumentGUIInfo::FloatField => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::FloatField }
            }
            ArgumentGUIInfo::FloatSlider { min, max, step } => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::FloatSlider { min: #min, max: #max, step: #step } }
            }
            ArgumentGUIInfo::Checkbox => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::Checkbox }
            }
            ArgumentGUIInfo::RadioBoolean {
                true_label,
                false_label,
            } => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::RadioBoolean { true_label: #true_label.to_string(), false_label: #false_label.to_string() } }
            }
            ArgumentGUIInfo::HorizontalRadioEnum => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::HorizontalRadioEnum }
            }
            ArgumentGUIInfo::VerticalRadioEnum => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::VerticalRadioEnum }
            }
            ArgumentGUIInfo::EnumSelector => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::EnumSelector }
            }
            ArgumentGUIInfo::ColorString => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::ColorString }
            }
            ArgumentGUIInfo::DateField(width) => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::DateField(#width) }
            }
            ArgumentGUIInfo::Hide => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::Hide }
            }
            ArgumentGUIInfo::None => {
                quote! { liquers_core::command_metadata::ArgumentGUIInfo::None }
            }
        };
        gui.to_tokens(tokens);
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
                let mut gui = ArgumentGUIInfo::TextField(20);
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    while !content.is_empty() {
                        let stmt: CommandParameterStatement = content.parse()?;
                        match stmt {
                            CommandParameterStatement::Label(l) => label = Some(l),
                            CommandParameterStatement::Gui(g) => gui = g,
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
        let content;
        syn::parenthesized!(content in input);

        let state_parameter = StateParameter::parse(&content)?;

        let mut parameters = Vec::new();
        while (parameters.is_empty() && state_parameter == StateParameter::None) || content.peek(syn::Token![,]) {
            if (!parameters.is_empty()) || state_parameter != StateParameter::None{
                content.parse::<syn::Token![,]>()?;
            }
            if content.is_empty() {
                break;
            }
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
        let mut presets = Vec::new();
        let mut next = Vec::new();
        let mut filename = String::new();
        let mut volatile: bool = false;
        for stmt in &command_statements {
            match stmt {
                CommandSignatureStatement::Namespace(ns) => namespace = ns.clone(),
                CommandSignatureStatement::Realm(r) => realm = r.clone(),
                CommandSignatureStatement::Label(l) => label = Some(l.clone()),
                CommandSignatureStatement::Doc(d) => doc = Some(d.clone()),
                CommandSignatureStatement::Preset(command_preset) => {
                    presets.push(command_preset.clone());
                }
                CommandSignatureStatement::Next(command_preset) => {
                    next.push(command_preset.clone());
                },
                CommandSignatureStatement::Filename(f) => filename = f.clone(),
                CommandSignatureStatement::Volatile(b) => {
                    volatile = *b;
                },
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
            presets,
            next,
            filename,
            volatile,
            wrapper_version: WrapperVersion::V2,
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
                "text" => {
                    input.parse::<syn::Ident>()?;
                    Ok(StateParameter::Text)
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
    let mut sig = parse_macro_input!(input as CommandSignatureExt);
    sig.sig.wrapper_version = WrapperVersion::V2;
    let register_fn = sig.sig.command_registration();
    let register_fn_name = sig.sig.register_fn_name();
    let cr = sig.cr;
    let gen = quote! {
        {
            use futures::FutureExt; 
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
            let a__par: i32 = arguments.get(0usize, "a")?;
            let b__par: String = arguments.get_injected(1usize, "b", context.clone())?;
            let c__par: f64 = arguments.get(2usize, "c")?;
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
            #[allow(non_snake_case)]
            fn test_fn__CMD_(
                state : &liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                arguments : liquers_core::commands::CommandArguments<CommandEnvironment>,
                context : Context<CommandEnvironment>,
            ) -> core::result::Result<<CommandEnvironment as liquers_core::context::Environment>::Value, liquers_core::error::Error>
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn wrapper_fn_signature_text() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(text) -> result
        };
        let tokens = sig.state_argument_parameter().unwrap();
        let expected = quote! {
            state.try_into_string()?.as_str()
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
            #[allow(non_snake_case)]
            fn test_fn__CMD_(
                state : liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                arguments : liquers_core::commands::CommandArguments<CommandEnvironment>,
                context : Context<CommandEnvironment>,
            ) ->
            core::pin::Pin<
                std::boxed::Box<
                    dyn core::future::Future<
                        Output = core::result::Result<<CommandEnvironment as liquers_core::context::Environment>::Value,
                        liquers_core::error::Error> > + core::marker::Send + 'static > >
        };
        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn command_wrapper_basic() {
        let _input: proc_macro2::TokenStream = quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> result
        };
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32, b: String injected, c: f64) -> result
        };
        let expanded = sig.command_wrapper();
        /*
        let expected1 = quote! {
            fn test_fn__CMD_(
                state: &liquers_core::state::State<CommandValue>,
                arguments: &mut liquers_core::commands::CommandArguments<CommandValue>,
                context: CommandContext,
            ) -> core::result::Result<CommandValue, liquers_core::error::Error>
            {
                let a__par: i32 = arguments.get()?;
                let b__par: String = arguments.get_injected("b", context.clone())?;
                let c__par: f64 = arguments.get()?;
                let res = test_fn(state, a__par, b__par, c__par);
                res
            }
        };
        */
        let expected = quote!{
            #[allow(non_snake_case)]
            fn test_fn__CMD_(
                state : &liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                arguments : liquers_core::commands::CommandArguments<CommandEnvironment>,
                context : Context<CommandEnvironment>,
            ) -> core::result::Result<<CommandEnvironment as liquers_core::context::Environment>::Value, liquers_core::error::Error>
            {
                let a__par:i32 = arguments.get(0usize , "a")?;
                let b__par:String = arguments.get_injected (1usize, "b", context.clone())?;
                let c__par : f64 = arguments.get(2usize , "c")?;
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
            gui: ArgumentGUIInfo::TextField(20),
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
            gui: ArgumentGUIInfo::TextField(20),
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
            gui: ArgumentGUIInfo::TextField(20),
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
            gui: ArgumentGUIInfo::TextField(20),
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
            gui: ArgumentGUIInfo::TextField(20),
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
            gui: ArgumentGUIInfo::TextField(20),
        };
        let tokens = param.argument_type_expression();
        let expected = quote! { liquers_core::command_metadata::ArgumentType::Any };
        assert_eq!(tokens.to_string(), expected.to_string());
    }


    #[test]
    fn test_parse_preset_command_signature_statement() {
        let stmt: CommandSignatureStatement = syn::parse_quote! {
            preset: "action1" (label: "Preset 1", description: "First preset")
        };

        match stmt {
            CommandSignatureStatement::Preset(preset) => {
                assert_eq!(preset.action, "action1");
                assert_eq!(preset.label, "Preset 1");
                assert_eq!(preset.description, "First preset");
            }
            _ => panic!("Expected CommandSignatureStatement::Preset"),
        }
    }

    #[test]
    fn test_command_signature_with_label() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32) -> result
            label: "Test label"
        };

        assert_eq!(sig.label, Some("Test label".to_string()));
    }

    #[test]
    fn test_command_signature_with_default_value() {
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
    fn test_command_signature_with_default_query_value() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: String = query "abc/def") -> result
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

        assert_eq!(
            param,
            Some(&Some(DefaultValue::Query("abc/def".to_string())))
        );

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

        assert!(info_string.contains("name : \"a\""));
        assert!(info_string.contains(
            "default : liquers_core :: command_metadata :: CommandParameterValue :: Query"
        ));
        assert!(info_string.contains("\"abc/def\""));
        assert!(info_string.contains(
            "argument_type : liquers_core :: command_metadata :: ArgumentType :: String"
        ));
    }

    #[test]
    fn test_command_signature_with_default_query_value_any() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: Value = query "abc/def") -> result
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

        assert_eq!(
            param,
            Some(&Some(DefaultValue::Query("abc/def".to_string())))
        );

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

        assert!(info_string.contains("name : \"a\""));
        assert!(info_string.contains(
            "default : liquers_core :: command_metadata :: CommandParameterValue :: Query"
        ));
        assert!(info_string.contains("\"abc/def\""));
        assert!(info_string.contains(
            "argument_type : liquers_core :: command_metadata :: ArgumentType :: Any"
        ));
    }

    fn fuzzy(s: &str) -> String {
        s.replace(' ', "").replace('\n', "")
    }

    #[test]
    fn test_command_registration_generates_function() {
        let mut sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32) -> result
            label: "Test label"
        };

        sig.wrapper_version = WrapperVersion::V2;

        let tokens = sig.command_registration();

        let expected = r#"
        #[allow(non_snake_case)]
        pub fn REGISTER__test_fn(
            registry: &mut liquers_core::commands::CommandRegistry<CommandEnvironment>
        ) -> core::result::Result<
            &mut liquers_core::command_metadata::CommandMetadata,
            liquers_core::error::Error
        > {
            #[allow(non_snake_case)]
            fn test_fn__CMD_(
                state: &liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                arguments: liquers_core::commands::CommandArguments<CommandEnvironment>,
                context: Context<CommandEnvironment>,
            ) -> core::result::Result<<CommandEnvironment as liquers_core::context::Environment>::Value, liquers_core::error::Error> {
                let a__par: i32 = arguments.get(0usize, "a")?;
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
                gui_info: liquers_core::command_metadata::ArgumentGUIInfo::TextField(20usize),
                ..Default::default()
            }];
            cm.with_filename("");
            Ok(cm)
        }
        "#;

        println!("Generated tokens: {}", tokens.to_string());
        for (a, b) in fuzzy(&tokens.to_string())
            .split(",")
            .zip(fuzzy(expected).split(","))
        {
            assert_eq!(a, b);
        }
        assert!(tokens.to_string().contains("pub fn"));
        assert!(fuzzy(&tokens.to_string()).contains("cm.with_label"));
        assert!(fuzzy(&tokens.to_string()).contains("arguments.get(0usize,\"a\")?"));
        assert_eq!(fuzzy(&tokens.to_string()), fuzzy(expected));
    }

     #[test]
    fn test_command_registration_generates_async_function() {
        let mut sig: CommandSignature = syn::parse_quote! {
            async fn test_fn(state, a: i32) -> result
            label: "Test label"
        };

        sig.wrapper_version = WrapperVersion::V2;

        let tokens = sig.command_registration();

        let expected = r#"
        #[allow(non_snake_case)]
        pub fn REGISTER__test_fn(
            registry: &mut liquers_core::commands::CommandRegistry<CommandEnvironment>
        ) -> core::result::Result<
            &mut liquers_core::command_metadata::CommandMetadata,
            liquers_core::error::Error
        > {
            #[allow(non_snake_case)]
            fn test_fn__CMD_(
                state: liquers_core::state::State<<CommandEnvironment as liquers_core::context::Environment>::Value>,
                arguments: liquers_core::commands::CommandArguments<CommandEnvironment>,
                context: Context<CommandEnvironment>,
            ) -> core::pin::Pin<
                std::boxed::Box<
                    dyn core::future::Future<
                            Output = core::result::Result<
                                <CommandEnvironment as liquers_core::context::Environment>::Value,
                                liquers_core::error::Error
                            >
                        > + core::marker::Send
                        + 'static
                >
            > {
                async move {
                    let a__par: i32 = arguments.get(0usize, "a")?;
                    let res = test_fn(state, a__par).await;
                    res
                }
                .boxed()
            }
            let mut cm = registry.register_async_command(
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
                gui_info: liquers_core::command_metadata::ArgumentGUIInfo::TextField(20usize),
                ..Default::default()
            }];
            cm.with_filename("");
            Ok(cm)
        }
        "#;

        println!("Generated tokens: {}", tokens.to_string());
        for (a, b) in fuzzy(&tokens.to_string())
            .split(",")
            .zip(fuzzy(expected).split(","))
        {
            assert_eq!(a, b);
        }
        assert!(tokens.to_string().contains("pub fn"));
        assert!(fuzzy(&tokens.to_string()).contains("cm.with_label"));
        assert!(fuzzy(&tokens.to_string()).contains("arguments.get(0usize,\"a\")?"));
        assert_eq!(fuzzy(&tokens.to_string()), fuzzy(expected));
    }
   
    #[test]
    fn test_command_registration_with_doc() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32) -> result
            label: "Test label"
            doc: "This is a test function"
        };

        let tokens = sig.command_registration();

        let expected_doc = "cm . with_doc (\"This is a test function\") ;";
        let expected_label = "cm . with_label (\"Test label\") ;";

        let tokens_str = tokens.to_string();
        //println!();
        //println!("Generated tokens: {}", tokens_str);

        assert!(&tokens_str.contains("pub fn REGISTER__test_fn"));
        assert!(&tokens_str.contains(expected_label));
        assert!(&tokens_str.contains(expected_doc));
    }

    #[test]
    fn test_command_registration_with_presets() {
        let sig: CommandSignature = syn::parse_quote! {
            fn test_fn(state, a: i32) -> result
            label: "Test label"
            doc: "This is a test function"
            preset: "action1"
            preset: "action2-x-y" (label: "Preset 2", description: "Second preset")
            next: "nextaction1"
        };

        let tokens = sig.command_registration();

        let tokens_str = tokens.to_string();

        // Check that the generated code contains the presets vector and both presets
        assert!(tokens_str.contains("cm . presets = vec ! ["));
        assert!(tokens_str.contains("liquers_core :: command_metadata :: CommandPreset :: new (\"action1\" , \"action1\" , \"\")"));
        assert!(tokens_str.contains("liquers_core :: command_metadata :: CommandPreset :: new (\"action2-x-y\" , \"Preset 2\" , \"Second preset\")"));
        assert!(tokens_str.contains("cm . with_label (\"Test label\") ;"));
        assert!(tokens_str.contains("cm . with_doc (\"This is a test function\") ;"));
        assert!(tokens_str.contains("cm . next = vec ! ["));
        assert!(tokens_str.contains("liquers_core :: command_metadata :: CommandPreset :: new (\"nextaction1\" , \"nextaction1\" , \"\")"));
    }

    #[test]
    fn test_nostate_command_registration1() {
        let sig: CommandSignature = syn::parse_quote! {
            fn nostate() -> result
        };

        let tokens = sig.command_registration();

        let tokens_str = tokens.to_string();
        //println!("{}",tokens_str)
    }

        #[test]
    fn test_nostate_command_registration2() {
        let sig: CommandSignature = syn::parse_quote! {
            fn nostate(context) -> result
        };

        let tokens = sig.command_registration();

        let tokens_str = tokens.to_string();
        println!("{}",tokens_str)

    }

    
    #[test]
    fn test_config_command_registration() {
        let sig: CommandSignature = syn::parse_quote! {
            async fn config(
                default: Value = query "-R/config/config.yaml/-/from_yaml",
                context
            ) -> result
            label:    "Config"
            doc:      "Returns the default configuration parameters."
        };

        let tokens = sig.command_registration();

        let tokens_str = tokens.to_string();
        println!("{}",tokens_str)

    }

    #[test]
    fn test_command_parameter_with_label_argument_info() {
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
        assert!(info_string.contains(
            "argument_type : liquers_core :: command_metadata :: ArgumentType :: Integer"
        ));
    }
}
