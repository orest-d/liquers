use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

#[pyclass]
pub struct Cache(Arc<Mutex<Box<dyn liquers_core::cache::BinCache + Send>>>);

#[pyfunction]
pub fn memory_cache() -> Cache {
    Cache(Arc::new(Mutex::new(Box::new(
        liquers_core::cache::MemoryBinCache::new(),
    ))))
}

#[pymethods]
impl Cache {
    #[new]
    fn new() -> Self {
        memory_cache()
    }

    fn clear(&mut self) {
        self.0.lock().unwrap().clear();
    }

    fn get_metadata(&self, query: &crate::parse::Query) -> Option<crate::metadata::Metadata> {
        self.0
            .lock()
            .unwrap()
            .get_metadata(&query.0)
            .map(|m| crate::metadata::Metadata(m.as_ref().clone()))
    }

    fn set_metadata(&mut self, metadata: &crate::metadata::Metadata) -> PyResult<()> {
        self.0.lock().unwrap().set_metadata(&metadata.0).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyException, _>(format!(
                "Error setting metadata: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn set_binary(&mut self, data: &[u8], metadata: &crate::metadata::Metadata) -> PyResult<()> {
        self.0
            .lock()
            .unwrap()
            .set_binary(data, &metadata.0)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyException, _>(format!(
                    "Error setting binary: {}",
                    e
                ))
            })?;
        Ok(())
    }

    fn remove(&mut self, query: &crate::parse::Query) -> PyResult<()> {
        self.0.lock().unwrap().remove(&query.0).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyException, _>(format!(
                "Error removing from cache: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn contains(&self, query: &crate::parse::Query) -> bool {
        self.0.lock().unwrap().contains(&query.0)
    }

    fn keys(&self) -> Vec<crate::parse::Query> {
        self.0
            .lock()
            .unwrap()
            .keys()
            .into_iter()
            .map(|k| crate::parse::Query(k))
            .collect()
    }

    fn get_binary(&self, query: &crate::parse::Query) -> Option<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .get_binary(&query.0)
            .map(|b| b.to_vec())
    }
}
