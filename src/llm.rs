use crate::error::MemoryError;
use crate::types::Message;
use std::future::Future;

/// User-provided LLM backend. Implement this trait to connect any model.
///
/// The library calls this for three purposes:
/// - **Extraction**: deciding what memories to create/update from conversation
/// - **Recall**: selecting relevant memories for a query
/// - **Consolidation**: dedup/reorg/cleanup decisions
///
/// All methods receive pre-built message lists and an optional system prompt.
/// The implementation forwards them to an LLM and returns the text response.
///
/// # Example
/// ```ignore
/// struct MyLlm { client: SomeHttpClient }
///
/// impl LlmProvider for MyLlm {
///     fn complete(
///         &self,
///         messages: Vec<Message>,
///         system: Option<String>,
///     ) -> impl Future<Output = Result<String, MemoryError>> + Send + '_ {
///         async move {
///             let resp = self.client.chat(messages, system).await
///                 .map_err(|e| MemoryError::LlmError { message: e.to_string() })?;
///             Ok(resp.text)
///         }
///     }
/// }
/// ```
pub trait LlmProvider: Send + Sync + 'static {
    /// Send a completion request to the LLM.
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String, MemoryError>> + Send + '_;
}
