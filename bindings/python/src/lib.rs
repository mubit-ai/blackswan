use pyo3::prelude::*;

mod engine;
mod provider;
mod types;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<types::PyMemoryConfig>()?;
    m.add_class::<types::PyMemoryType>()?;
    m.add_class::<types::PyMemory>()?;
    m.add_class::<types::PyMessage>()?;
    m.add_class::<types::PyMessageRole>()?;
    m.add_class::<types::PyRecallResult>()?;
    m.add_class::<types::PyManifestEntry>()?;
    m.add_class::<types::PyMemoryManifest>()?;
    m.add_class::<engine::PyMemoryEngine>()?;
    Ok(())
}
