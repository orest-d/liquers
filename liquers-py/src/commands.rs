use pyo3::{exceptions::PyException, prelude::*};

#[pyclass]
pub struct CommandArguments(liquers_core::commands::CommandArguments);

#[pymethods]
impl CommandArguments {
    #[new]
    fn new() -> Self {
        let ca = liquers_core::commands::CommandArguments::new(
            liquers_core::plan::ResolvedParameterValues::new(),
        );
        CommandArguments(ca)
    }
}

#[pyclass]
pub struct CommandRegistry(
    pub  liquers_core::commands::CommandRegistry<
        crate::context::EnvRef,
        crate::context::Environment,
        crate::value::Value,
    >,
);

#[pymethods]
impl CommandRegistry {
    #[new]
    fn new() -> Self {
        let cr = 
        liquers_core::commands::CommandRegistry::new();
        CommandRegistry(cr)
    }
}
