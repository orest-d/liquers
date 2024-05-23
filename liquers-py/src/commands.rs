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

