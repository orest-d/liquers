use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use pyo3::{exceptions::PyException, prelude::*, types::PyList};

// TODO: Implement EnumArgumentType
// TODO: Implement EnumArgumentAlternative
// TODO: Implement EnumArgument
// TODO: Implement ArgumentType
// TODO: Implement ArgumentGUIInfo
// TODO: Implement CommandParameterValue
// TODO: Implement ParameterPreset
// TODO: Implement CommandPreset
// TODO: Implement CommandDefinition

/// The value type of an enum argument's alternatives: string, integer, float, boolean, their
/// optional forms, or any.
///
/// A narrower set than `ArgumentType`, used by `EnumArgument`.
#[pyclass(from_py_object, eq, eq_int, module = "liquers")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum EnumArgumentType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "int")]
    Integer,
    #[serde(rename = "int_opt")]
    IntegerOption,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "float_opt")]
    FloatOption,
    #[serde(rename = "bool")]
    Boolean,
    #[serde(rename = "bool_opt")]
    BooleanOption,
    #[serde(rename = "any")]
    Any,
}

/// An argument whose value is one of a fixed set of named alternatives.
///
/// `EnumArgument(name, typ)` makes one named `name` whose alternatives have the value type `typ`
/// (an `EnumArgumentType`).
#[pyclass(from_py_object, module = "liquers")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EnumArgument(pub liquers_core::command_metadata::EnumArgument);

#[pymethods]
impl EnumArgument {
    #[new]
    pub fn new(name: &str, typ: EnumArgumentType) -> Self {
        Self(liquers_core::command_metadata::EnumArgument::new(name).with_value_type(typ.into()))
    }

    /// The enum argument serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads an enum argument from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `EnumArgument`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let value =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(value))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

/// The type of a command argument, as recorded in command metadata.
///
/// `ArgumentInfo.argument_type` gives the same information as a string.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ArgumentType(pub liquers_core::command_metadata::ArgumentType);

#[pymethods]
impl ArgumentType {
    /// The argument type that accepts a value of any type.
    #[staticmethod]
    pub fn any() -> Self {
        Self(liquers_core::command_metadata::ArgumentType::Any)
    }

    /// The argument type serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads an argument type from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `ArgumentType`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let value =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(value))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

impl From<EnumArgumentType> for liquers_core::command_metadata::EnumArgumentType {
    fn from(e: EnumArgumentType) -> Self {
        match e {
            EnumArgumentType::String => liquers_core::command_metadata::EnumArgumentType::String,
            EnumArgumentType::Integer => liquers_core::command_metadata::EnumArgumentType::Integer,
            EnumArgumentType::IntegerOption => {
                liquers_core::command_metadata::EnumArgumentType::IntegerOption
            }
            EnumArgumentType::Float => liquers_core::command_metadata::EnumArgumentType::Float,
            EnumArgumentType::FloatOption => {
                liquers_core::command_metadata::EnumArgumentType::FloatOption
            }
            EnumArgumentType::Boolean => liquers_core::command_metadata::EnumArgumentType::Boolean,
            EnumArgumentType::BooleanOption => {
                liquers_core::command_metadata::EnumArgumentType::BooleanOption
            }
            EnumArgumentType::Any => liquers_core::command_metadata::EnumArgumentType::Any,
        }
    }
}

impl From<liquers_core::command_metadata::EnumArgumentType> for EnumArgumentType {
    fn from(e: liquers_core::command_metadata::EnumArgumentType) -> Self {
        match e {
            liquers_core::command_metadata::EnumArgumentType::String => EnumArgumentType::String,
            liquers_core::command_metadata::EnumArgumentType::Integer => EnumArgumentType::Integer,
            liquers_core::command_metadata::EnumArgumentType::IntegerOption => {
                EnumArgumentType::IntegerOption
            }
            liquers_core::command_metadata::EnumArgumentType::Float => EnumArgumentType::Float,
            liquers_core::command_metadata::EnumArgumentType::FloatOption => {
                EnumArgumentType::FloatOption
            }
            liquers_core::command_metadata::EnumArgumentType::Boolean => EnumArgumentType::Boolean,
            liquers_core::command_metadata::EnumArgumentType::BooleanOption => {
                EnumArgumentType::BooleanOption
            }
            liquers_core::command_metadata::EnumArgumentType::Any => EnumArgumentType::Any,
        }
    }
}

/// The full name of a command: realm, namespace and name.
///
/// `CommandKey(realm, namespace, name)`; an empty realm or namespace is the default one. Keys
/// compare and hash by value.
#[pyclass(module = "liquers")]
pub struct CommandKey(pub liquers_core::command_metadata::CommandKey);

#[pymethods]
impl CommandKey {
    #[new]
    fn new(realm: &str, namespace: &str, name: &str) -> Self {
        CommandKey(liquers_core::command_metadata::CommandKey::new(
            realm, namespace, name,
        ))
    }

    /// The realm the command belongs to; empty for the default realm.
    #[getter]
    fn realm(&self) -> String {
        self.0.realm.clone()
    }

    /// The namespace the command belongs to; empty for the root namespace.
    #[getter]
    fn namespace(&self) -> String {
        self.0.namespace.clone()
    }

    /// The command name, as written in a query.
    #[getter]
    fn name(&self) -> String {
        self.0.name.clone()
    }

    fn __str__(&self) -> String {
        format!("{}", self.0)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }
}

/// The metadata of one command argument: name, label, type, default and whether it is variadic.
///
/// `ArgumentInfo(name)` describes an argument of any type with no default. Read it from
/// `CommandMetadata.arguments`.
#[pyclass(module = "liquers")]
pub struct ArgumentInfo(liquers_core::command_metadata::ArgumentInfo);

#[pymethods]
impl ArgumentInfo {
    #[new]
    fn new(name: &str) -> Self {
        ArgumentInfo(liquers_core::command_metadata::ArgumentInfo::any_argument(
            name,
        ))
    }

    /// The argument metadata serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// The argument metadata serialized as YAML.
    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads argument metadata from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `ArgumentInfo`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::ArgumentInfo =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(ArgumentInfo(a))
    }

    /// Reads argument metadata from YAML text, as `to_yaml()` writes it.
    ///
    /// Raises an exception when the text is not valid YAML for `ArgumentInfo`.
    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::ArgumentInfo =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(ArgumentInfo(a))
    }

    /// The argument name.
    #[getter]
    fn name(&self) -> String {
        self.0.name.clone()
    }

    /// A short human-readable label for the argument.
    #[getter]
    fn label(&self) -> String {
        self.0.label.clone()
    }

    /// The default as a Python value: `None` when there is none, the query's text for a default
    /// link.
    #[getter]
    fn default(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        use liquers_core::command_metadata::CommandParameterValue;
        match &self.0.default {
            CommandParameterValue::None => Ok(py.None()),
            CommandParameterValue::Query(query) => {
                Ok(pyo3::types::PyString::new(py, &query.encode()).into_any().unbind())
            }
            CommandParameterValue::Value(value) => {
                let text = serde_json::to_string(value)
                    .map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
                Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
            }
        }
    }

    /// The argument type as its serialized name.
    ///
    /// One of `"string"`, `"int"`, `"float"`, `"bool"`, `"int_opt"`, `"float_opt"`, `"bool_opt"`,
    /// ..., or `"any"`.
    #[getter]
    fn argument_type(&self) -> PyResult<String> {
        match serde_json::to_value(&self.0.argument_type) {
            Ok(serde_json::Value::String(name)) => Ok(name),
            Ok(other) => Ok(other.to_string()),
            Err(e) => Err(PyErr::new::<PyException, _>(e.to_string())),
        }
    }

    /// True when the argument takes every remaining query parameter, like `*args`.
    #[getter]
    fn multiple(&self) -> bool {
        self.0.multiple
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

/// How a command is implemented: registered code, or an alias of another command.
///
/// Part of `CommandMetadata`.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDefinition(pub liquers_core::command_metadata::CommandDefinition);

#[pymethods]
impl CommandDefinition {
    /// The definition of a command implemented by a registered function.
    #[staticmethod]
    pub fn registered() -> Self {
        Self(liquers_core::command_metadata::CommandDefinition::Registered)
    }

    /// The command definition serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads a command definition from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `CommandDefinition`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let value =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(value))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

/// A ready-made action offered for a command, with a label and a description.
///
/// `CommandPreset(action, label, description)`; `action` is query text and raises an exception when
/// it does not parse.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone, PartialEq)]
pub struct CommandPreset(pub liquers_core::command_metadata::CommandPreset);

#[pymethods]
impl CommandPreset {
    #[new]
    pub fn new(action: &str, label: &str, description: &str) -> PyResult<Self> {
        liquers_core::command_metadata::CommandPreset::new(action, label, description)
            .map(Self)
            .map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// The preset serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads a preset from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `CommandPreset`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let value =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(value))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }
    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

/// The metadata of a registered command: its name, namespace, realm, arguments and flags.
///
/// `Environment.command_metadata(name)` returns the metadata of a registered command. See
/// `PYTHON_GUIDE.md` §5, "Metadata".
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct CommandMetadata(pub liquers_core::command_metadata::CommandMetadata);

#[pymethods]
impl CommandMetadata {
    #[new]
    fn new(name: &str) -> Self {
        CommandMetadata(liquers_core::command_metadata::CommandMetadata::new(name))
    }

    /// The command metadata serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// The command metadata serialized as YAML.
    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads command metadata from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `CommandMetadata`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadata =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadata(a))
    }

    /// Reads command metadata from YAML text, as `to_yaml()` writes it.
    ///
    /// Raises an exception when the text is not valid YAML for `CommandMetadata`.
    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadata =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadata(a))
    }

    /// The command name, as written in a query.
    #[getter]
    fn name(&self) -> String {
        self.0.name.clone()
    }

    /// The namespace the command is registered in; empty for the root namespace.
    #[getter]
    fn namespace(&self) -> String {
        self.0.namespace.clone()
    }

    /// The realm the command is registered in; empty for the default realm.
    #[getter]
    fn realm(&self) -> String {
        self.0.realm.clone()
    }

    /// A short human-readable label for the command.
    #[getter]
    fn label(&self) -> String {
        self.0.label.clone()
    }

    /// The command's documentation text; for a Python command, its function's docstring unless one
    /// was given.
    #[getter]
    fn doc(&self) -> String {
        self.0.doc.clone()
    }

    /// The default filename of the command's result (for example `text.txt`), or empty.
    #[getter]
    fn filename(&self) -> String {
        self.0.filename.clone()
    }

    /// Where the command was registered from: `"python"` for a command registered from Python.
    #[getter]
    fn module(&self) -> String {
        self.0.module.clone()
    }

    /// True when the command's output is kept for reuse.
    #[getter]
    fn cached(&self) -> bool {
        self.0.cached()
    }

    /// The query arguments, in order, as `ArgumentInfo` objects; the input state and `context` are
    /// not among them.
    #[getter]
    fn arguments(&self) -> Vec<ArgumentInfo> {
        self.0.arguments.iter().cloned().map(ArgumentInfo).collect()
    }

    /// True for an async command.
    #[getter]
    fn is_async(&self) -> bool {
        self.0.is_async
    }

    /// True when the command's result is never reused: it is recomputed on every evaluation.
    #[getter]
    fn volatile(&self) -> bool {
        self.0.volatile
    }

    /// Whether the command needs an evaluation payload: `"none"` or `"required"`.
    ///
    /// A string rather than an enum, since there are only two states. A command that requires a
    /// payload is also `volatile`.
    #[getter]
    fn payload_required(&self) -> String {
        self.0.payload_required.to_string()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

/// A collection of command metadata, without the implementations.
///
/// Enough to build a plan (`build_plan`) without running anything.
#[pyclass(module = "liquers")]
pub struct CommandMetadataRegistry(pub liquers_core::command_metadata::CommandMetadataRegistry);

#[pymethods]
impl CommandMetadataRegistry {
    #[new]
    fn new() -> Self {
        CommandMetadataRegistry(liquers_core::command_metadata::CommandMetadataRegistry::new())
    }

    /// Adds the metadata of one command.
    fn add_command(&mut self, command: &CommandMetadata) {
        self.0.add_command(&command.0);
    }
    /// Adds metadata for a Python function declared as an alias of a `pycall` command.
    ///
    /// No environment registers `pycall`, so a command declared this way cannot run. Register
    /// Python commands with `Environment.command` instead (`PYTHON_GUIDE.md` §5).
    pub fn add_python_command(
        &mut self,
        namespace: &str,
        command_name: &str,
        module: &str,
        function: &str,
        pass_state: &str,
        arguments: &Bound<'_, PyList>,
        multiple: bool,
    ) -> PyResult<()> {
        let mut cmd = liquers_core::command_metadata::CommandMetadata::new(command_name);
        cmd.with_namespace(namespace);
        let mut args = Vec::new();
        for item in arguments.iter() {
            let pystr = item.str()?;
            let aname = pystr.to_str()?;
            let a = if aname == "context" {
                liquers_core::command_metadata::ArgumentInfo::argument(aname).set_injected()
            } else {
                liquers_core::command_metadata::ArgumentInfo::any_argument(aname)
            };
            args.push(a);
        }
        cmd.arguments = args;
        if multiple {
            if let Some(last) = cmd.arguments.last_mut() {
                last.multiple = true;
            }
        }
        cmd.definition = liquers_core::command_metadata::CommandDefinition::Alias {
            command: liquers_core::command_metadata::CommandKey::new("", "", "pycall"),
            head_parameters: vec![
                liquers_core::command_metadata::CommandParameterValue::Value(module.into()),
                liquers_core::command_metadata::CommandParameterValue::Value(function.into()),
                liquers_core::command_metadata::CommandParameterValue::Value(pass_state.into()),
            ],
        };

        self.0.commands.push(cmd);
        Ok(())
    }

    /// The registry serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// The registry serialized as YAML.
    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads a registry from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `CommandMetadataRegistry`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadataRegistry =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadataRegistry(a))
    }

    /// Reads a registry from YAML text, as `to_yaml()` writes it.
    ///
    /// Raises an exception when the text is not valid YAML for `CommandMetadataRegistry`.
    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadataRegistry =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadataRegistry(a))
    }
}
