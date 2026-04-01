use pyo3::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

use crate::provider::PyLlmProvider;
use crate::types::*;

type InnerEngine = blackswan::MemoryEngine<PyLlmProvider>;

/// The memory engine. Created via `MemoryEngine.create()`.
#[pyclass(module = "blackswan")]
pub struct PyMemoryEngine {
    inner: Arc<TokioMutex<Option<InnerEngine>>>,
}

fn to_py_err(e: blackswan::MemoryError) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
}

fn shutdown_err() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("engine has been shut down")
}

#[pymethods]
impl PyMemoryEngine {
    /// Create a new MemoryEngine. This is an async classmethod.
    ///
    /// Usage: `engine = await MemoryEngine.create(config, llm_callable)`
    #[staticmethod]
    #[pyo3(name = "create")]
    fn create<'py>(
        py: Python<'py>,
        config: &PyMemoryConfig,
        provider: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        let cfg = config.inner.clone();
        let llm = PyLlmProvider::new(provider);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let engine = InnerEngine::new(cfg, llm).await.map_err(to_py_err)?;
            Ok(PyMemoryEngine {
                inner: Arc::new(TokioMutex::new(Some(engine))),
            })
        })
    }

    /// Recall relevant memories for a query.
    #[pyo3(signature = (query, recently_used_tools=vec![]))]
    fn recall<'py>(
        &self,
        py: Python<'py>,
        query: String,
        recently_used_tools: Vec<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            let result = engine
                .recall(&query, &recently_used_tools)
                .await
                .map_err(to_py_err)?;
            Ok(PyRecallResult::from(result))
        })
    }

    /// Run extraction synchronously from conversation messages.
    fn extract<'py>(
        &self,
        py: Python<'py>,
        messages: Vec<PyMessage>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let msgs: Vec<blackswan::Message> = messages.iter().map(Into::into).collect();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            engine.extract(msgs).await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Spawn background extraction (non-blocking, coalesces rapid-fire calls).
    fn extract_background(&self, messages: Vec<PyMessage>) -> PyResult<()> {
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let inner = self.inner.clone();
        let msgs: Vec<blackswan::Message> = messages.iter().map(Into::into).collect();
        rt.spawn(async move {
            let guard = inner.lock().await;
            if let Some(engine) = guard.as_ref() {
                engine.extract_background(msgs);
            }
        });
        Ok(())
    }

    /// Create a new memory.
    fn create_memory<'py>(
        &self,
        py: Python<'py>,
        memory: &PyMemory,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let mem: blackswan::Memory = memory.into();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            engine.create_memory(&mem).await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Update an existing memory.
    fn update_memory<'py>(
        &self,
        py: Python<'py>,
        name: String,
        memory: &PyMemory,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let mem: blackswan::Memory = memory.into();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            engine.update_memory(&name, &mem).await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Delete a memory by name/filename.
    fn delete_memory<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            engine.delete_memory(&name).await.map_err(to_py_err)?;
            Ok(())
        })
    }

    /// Return the MEMORY.md manifest.
    fn manifest(&self) -> PyResult<PyMemoryManifest> {
        let inner = self.inner.blocking_lock();
        let engine = inner.as_ref().ok_or_else(shutdown_err)?;
        let m = engine.manifest().map_err(to_py_err)?;
        Ok(m.into())
    }

    /// Read a single memory by name/filename.
    fn read_memory(&self, name: String) -> PyResult<PyMemory> {
        let inner = self.inner.blocking_lock();
        let engine = inner.as_ref().ok_or_else(shutdown_err)?;
        let m = engine.read_memory(&name).map_err(to_py_err)?;
        Ok(m.into())
    }

    /// Check if the memory system is enabled.
    fn is_enabled(&self) -> PyResult<bool> {
        let inner = self.inner.blocking_lock();
        let engine = inner.as_ref().ok_or_else(shutdown_err)?;
        Ok(engine.is_enabled())
    }

    /// Record end of a session (drives consolidation gating).
    fn record_session_end<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            engine.record_session_end().await;
            Ok(())
        })
    }

    /// Attempt consolidation. Returns True if it ran, False if gated.
    fn consolidate<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            let ran = engine.consolidate().await.map_err(to_py_err)?;
            Ok(ran)
        })
    }

    /// Spawn background consolidation.
    fn consolidate_background<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.lock().await;
            let engine = guard.as_ref().ok_or_else(shutdown_err)?;
            engine.consolidate_background().await;
            Ok(())
        })
    }

    /// Shut down the engine gracefully.
    fn shutdown<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let engine = inner.lock().await.take().ok_or_else(shutdown_err)?;
            engine.shutdown().await;
            Ok(())
        })
    }
}
