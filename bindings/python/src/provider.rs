use pyo3::prelude::*;
use blackswan::{LlmProvider, MemoryError, Message};
use std::future::Future;

/// Wraps a Python callable (sync or async) as a Rust LlmProvider.
pub struct PyLlmProvider {
    callable: PyObject,
}

// Safety: PyObject is Send. The GIL is acquired only in contained blocks.
unsafe impl Send for PyLlmProvider {}
unsafe impl Sync for PyLlmProvider {}

impl PyLlmProvider {
    pub fn new(callable: PyObject) -> Self {
        Self { callable }
    }
}

impl LlmProvider for PyLlmProvider {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String, MemoryError>> + Send + '_ {
        let callable = Python::with_gil(|py| self.callable.clone_ref(py));

        async move {
            // Convert messages to JSON-serializable format for Python
            let messages_json = serde_json::to_string(&messages).map_err(|e| {
                MemoryError::LlmError {
                    message: format!("failed to serialize messages: {e}"),
                }
            })?;

            // Call the Python function and check if it returns a coroutine
            let result_or_coro = Python::with_gil(|py| -> PyResult<PyObject> {
                let json_module = py.import("json")?;
                let py_messages = json_module.call_method1("loads", (&messages_json,))?;
                let py_system = system.as_deref().map(|s| s.to_string());
                callable.call1(py, (py_messages, py_system))
            })
            .map_err(|e| MemoryError::LlmError {
                message: format!("Python call failed: {e}"),
            })?;

            // Check if the result is a coroutine (async function)
            let is_coroutine = Python::with_gil(|py| -> PyResult<bool> {
                let inspect = py.import("inspect")?;
                let is_coro = inspect
                    .call_method1("iscoroutine", (result_or_coro.bind(py),))?
                    .is_truthy()?;
                Ok(is_coro)
            })
            .map_err(|e| MemoryError::LlmError {
                message: format!("coroutine check failed: {e}"),
            })?;

            if is_coroutine {
                // Await the coroutine
                let future = Python::with_gil(|py| {
                    pyo3_async_runtimes::tokio::into_future(result_or_coro.into_bound(py))
                })
                .map_err(|e| MemoryError::LlmError {
                    message: format!("coroutine conversion failed: {e}"),
                })?;

                let result = future.await.map_err(|e| MemoryError::LlmError {
                    message: format!("Python coroutine raised: {e}"),
                })?;

                Python::with_gil(|py| result.extract::<String>(py)).map_err(|e| {
                    MemoryError::LlmError {
                        message: format!("Python provider did not return str: {e}"),
                    }
                })
            } else {
                // Sync result — extract string directly
                Python::with_gil(|py| result_or_coro.extract::<String>(py)).map_err(|e| {
                    MemoryError::LlmError {
                        message: format!("Python provider did not return str: {e}"),
                    }
                })
            }
        }
    }
}
