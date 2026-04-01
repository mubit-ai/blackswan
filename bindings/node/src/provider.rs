use napi::threadsafe_function::ThreadsafeFunction;
use blackswan::{LlmProvider, MemoryError, Message};
use std::future::Future;

/// Wraps a JS async function as a Rust LlmProvider.
pub struct JsLlmProvider {
    tsfn: ThreadsafeFunction<(String, Option<String>), String>,
}

unsafe impl Send for JsLlmProvider {}
unsafe impl Sync for JsLlmProvider {}

impl JsLlmProvider {
    pub fn new(tsfn: ThreadsafeFunction<(String, Option<String>), String>) -> Self {
        Self { tsfn }
    }
}

impl LlmProvider for JsLlmProvider {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String, MemoryError>> + Send + '_ {
        async move {
            let messages_json = serde_json::to_string(&messages).map_err(|e| {
                MemoryError::LlmError {
                    message: format!("failed to serialize messages: {e}"),
                }
            })?;

            let result = self
                .tsfn
                .call_async(Ok((messages_json, system)))
                .await
                .map_err(|e| MemoryError::LlmError {
                    message: format!("JS LLM call failed: {e}"),
                })?;

            Ok(result)
        }
    }
}
