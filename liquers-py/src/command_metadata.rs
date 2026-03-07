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

/// Type of an enum argument, see EnumArgument
/// This is a restricted version of ArgumentType to prevent circular type definition

#[pyclass]
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
    #[serde(rename = "any")]
    Any,
}

#[pyclass]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EnumArgument(pub liquers_core::command_metadata::EnumArgument);

#[pymethods]
impl EnumArgument {
    #[new]
    pub fn new(name: &str, typ: EnumArgumentType) -> Self {
        Self(liquers_core::command_metadata::EnumArgument::new(
            name,
            typ.into(),
        ))
    }

    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

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

#[pyclass]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ArgumentType(pub liquers_core::command_metadata::ArgumentType);

#[pymethods]
impl ArgumentType {
    #[staticmethod]
    pub fn any() -> Self {
        Self(liquers_core::command_metadata::ArgumentType::Any)
    }

    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

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
            liquers_core::command_metadata::EnumArgumentType::Any => EnumArgumentType::Any,
        }
    }
}

#[pyclass]
pub struct CommandKey(pub liquers_core::command_metadata::CommandKey);

#[pymethods]
impl CommandKey {
    #[new]
    fn new(realm: &str, namespace: &str, name: &str) -> Self {
        CommandKey(liquers_core::command_metadata::CommandKey::new(
            realm, namespace, name,
        ))
    }

    #[getter]
    fn realm(&self) -> String {
        self.0.realm.clone()
    }

    #[getter]
    fn namespace(&self) -> String {
        self.0.namespace.clone()
    }

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

#[pyclass]
pub struct ArgumentInfo(liquers_core::command_metadata::ArgumentInfo);

#[pymethods]
impl ArgumentInfo {
    #[new]
    fn new(name: &str) -> Self {
        ArgumentInfo(liquers_core::command_metadata::ArgumentInfo::any_argument(
            name,
        ))
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::ArgumentInfo =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(ArgumentInfo(a))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::ArgumentInfo =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(ArgumentInfo(a))
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

#[pyclass]
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDefinition(pub liquers_core::command_metadata::CommandDefinition);

#[pymethods]
impl CommandDefinition {
    #[staticmethod]
    pub fn registered() -> Self {
        Self(liquers_core::command_metadata::CommandDefinition::Registered)
    }

    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

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

#[pyclass]
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

    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

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

#[pyclass]
#[derive(Debug, Clone)]
pub struct CommandMetadata(pub liquers_core::command_metadata::CommandMetadata);

#[pymethods]
impl CommandMetadata {
    #[new]
    fn new(name: &str) -> Self {
        CommandMetadata(liquers_core::command_metadata::CommandMetadata::new(name))
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadata =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadata(a))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadata =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadata(a))
    }

    #[getter]
    fn is_async(&self) -> bool {
        self.0.is_async
    }

    #[getter]
    fn cache(&self) -> bool {
        self.0.cache
    }

    #[getter]
    fn volatile(&self) -> bool {
        self.0.volatile
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

#[pyclass]
pub struct CommandMetadataRegistry(pub liquers_core::command_metadata::CommandMetadataRegistry);

#[pymethods]
impl CommandMetadataRegistry {
    #[new]
    fn new() -> Self {
        CommandMetadataRegistry(liquers_core::command_metadata::CommandMetadataRegistry::new())
    }

    fn add_command(&mut self, command: &CommandMetadata) {
        self.0.add_command(&command.0);
    }
    pub fn add_python_command(
        &mut self,
        namespace: &str,
        command_name: &str,
        module: &str,
        function: &str,
        pass_state: &str,
        arguments: Py<PyList>,
        multiple: bool,
    ) -> PyResult<()> {
        let mut cmd = liquers_core::command_metadata::CommandMetadata::new(command_name);
        cmd.with_namespace(namespace);
        let x = Python::with_gil(|py| {
            let list = arguments.bind(py);
            let mut args = Vec::new();
            for i in 0..list.len() {
                let item = list.get_item(i).unwrap();
                let pystr = item.str().unwrap();
                let aname = pystr.to_str().unwrap();
                let a = if aname == "context" {
                    liquers_core::command_metadata::ArgumentInfo::argument(aname).set_injected()
                } else {
                    liquers_core::command_metadata::ArgumentInfo::any_argument(aname)
                };
                args.push(a);
            }
            cmd.arguments = args;
        });
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

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadataRegistry =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadataRegistry(a))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a: liquers_core::command_metadata::CommandMetadataRegistry =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadataRegistry(a))
    }
}
