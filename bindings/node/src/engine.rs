use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

use crate::provider::JsLlmProvider;
use crate::types::*;

type InnerEngine = blackswan::MemoryEngine<JsLlmProvider>;

fn to_napi_err(e: blackswan::MemoryError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

fn shutdown_err() -> napi::Error {
    napi::Error::from_reason("engine has been shut down")
}

/// The memory engine.
#[napi]
pub struct MemoryEngine {
    inner: Arc<TokioMutex<Option<InnerEngine>>>,
}

#[napi]
impl MemoryEngine {
    /// Create a new MemoryEngine.
    ///
    /// @param config - Configuration options
    /// @param provider - Async function (messages: string, system: string | null) => Promise<string>
    ///   The messages argument is a JSON string of Message[]. Parse it with JSON.parse().
    #[napi(factory)]
    pub async fn create(
        config: JsMemoryConfigOptions,
        #[napi(ts_arg_type = "(messages: string, system: string | null) => Promise<string>")]
        provider: ThreadsafeFunction<(String, Option<String>), String>,
    ) -> Result<Self> {
        let cfg = config.into_config()?;
        let llm = JsLlmProvider::new(provider);
        let engine = InnerEngine::new(cfg, llm)
            .await
            .map_err(to_napi_err)?;
        Ok(Self {
            inner: Arc::new(TokioMutex::new(Some(engine))),
        })
    }

    /// Recall relevant memories for a query.
    #[napi]
    pub async fn recall(
        &self,
        query: String,
        recently_used_tools: Option<Vec<String>>,
    ) -> Result<JsRecallResult> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        let tools = recently_used_tools.unwrap_or_default();
        let result = engine.recall(&query, &tools).await.map_err(to_napi_err)?;
        Ok(result.into())
    }

    /// Run extraction from conversation messages.
    #[napi]
    pub async fn extract(&self, messages: Vec<JsMessage>) -> Result<()> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        let msgs: Vec<blackswan::Message> = messages.iter().map(Into::into).collect();
        engine.extract(msgs).await.map_err(to_napi_err)
    }

    /// Spawn background extraction (non-blocking).
    #[napi]
    pub fn extract_background(&self, messages: Vec<JsMessage>) -> Result<()> {
        let inner = self.inner.clone();
        let msgs: Vec<blackswan::Message> = messages.iter().map(Into::into).collect();
        tokio::spawn(async move {
            let guard = inner.lock().await;
            if let Some(engine) = guard.as_ref() {
                engine.extract_background(msgs);
            }
        });
        Ok(())
    }

    /// Create a new memory.
    #[napi]
    pub async fn create_memory(&self, memory: JsMemory) -> Result<()> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        let mem: blackswan::Memory = (&memory).into();
        engine.create_memory(&mem).await.map_err(to_napi_err)
    }

    /// Update an existing memory.
    #[napi]
    pub async fn update_memory(&self, name: String, memory: JsMemory) -> Result<()> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        let mem: blackswan::Memory = (&memory).into();
        engine.update_memory(&name, &mem).await.map_err(to_napi_err)
    }

    /// Delete a memory.
    #[napi]
    pub async fn delete_memory(&self, name: String) -> Result<()> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        engine.delete_memory(&name).await.map_err(to_napi_err)
    }

    /// Return the MEMORY.md manifest.
    #[napi]
    pub fn manifest(&self) -> Result<JsMemoryManifest> {
        let inner = self.inner.blocking_lock();
        let engine = inner.as_ref().ok_or_else(shutdown_err)?;
        let m = engine.manifest().map_err(to_napi_err)?;
        Ok(m.into())
    }

    /// Read a single memory.
    #[napi]
    pub fn read_memory(&self, name: String) -> Result<JsMemory> {
        let inner = self.inner.blocking_lock();
        let engine = inner.as_ref().ok_or_else(shutdown_err)?;
        let m = engine.read_memory(&name).map_err(to_napi_err)?;
        Ok(m.into())
    }

    /// Check if the memory system is enabled.
    #[napi]
    pub fn is_enabled(&self) -> Result<bool> {
        let inner = self.inner.blocking_lock();
        let engine = inner.as_ref().ok_or_else(shutdown_err)?;
        Ok(engine.is_enabled())
    }

    /// Record session end.
    #[napi]
    pub async fn record_session_end(&self) -> Result<()> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        engine.record_session_end().await;
        Ok(())
    }

    /// Attempt consolidation.
    #[napi]
    pub async fn consolidate(&self) -> Result<bool> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        engine.consolidate().await.map_err(to_napi_err)
    }

    /// Spawn background consolidation.
    #[napi]
    pub async fn consolidate_background(&self) -> Result<()> {
        let guard = self.inner.lock().await;
        let engine = guard.as_ref().ok_or_else(shutdown_err)?;
        engine.consolidate_background().await;
        Ok(())
    }

    /// Shut down the engine gracefully.
    #[napi]
    pub async fn shutdown(&self) -> Result<()> {
        let engine = self.inner.lock().await.take().ok_or_else(shutdown_err)?;
        engine.shutdown().await;
        Ok(())
    }
}
