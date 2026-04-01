use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

use crate::config::MemoryConfig;
use crate::consolidator::MemoryConsolidator;
use crate::enable;
use crate::error::{MemoryError, Result};
use crate::extractor::coalesce::ExtractionCoalescer;
use crate::extractor::MemoryExtractor;
use crate::llm::LlmProvider;
use crate::recall::MemoryRecall;
use crate::store::MemoryStore;
use crate::types::*;

/// The main memory engine. Generic over the LLM provider.
///
/// # Example
/// ```ignore
/// use blackswan::{MemoryEngine, MemoryConfig};
///
/// let config = MemoryConfig::builder("./memories").build().unwrap();
/// let engine = MemoryEngine::new(config, my_llm).await;
///
/// // Recall relevant memories for a query
/// let result = engine.recall("What was the decision on the API design?", &[]).await?;
///
/// // After an agent turn, extract memories from conversation
/// engine.extract_background(messages);
/// ```
pub struct MemoryEngine<L: LlmProvider> {
    config: Arc<MemoryConfig>,
    store: Arc<MemoryStore>,
    provider: Arc<L>,
    recall: MemoryRecall<L>,
    consolidator: Arc<TokioMutex<MemoryConsolidator<L>>>,
    /// Write mutex: coordinates extraction background task vs direct CRUD.
    write_mutex: Arc<TokioMutex<()>>,
    /// Coalescer for background extraction requests.
    coalescer: Arc<ExtractionCoalescer>,
    /// Handle to the background extraction loop.
    extraction_handle: TokioMutex<Option<JoinHandle<()>>>,
    /// Handle to a running consolidation task, if any.
    consolidation_handle: TokioMutex<Option<JoinHandle<()>>>,
    /// Tracks which memories have been surfaced in this session.
    surfaced: TokioMutex<HashSet<String>>,
}

impl<L: LlmProvider> MemoryEngine<L> {
    /// Create a new memory engine. Ensures the memory directory exists and
    /// spawns the background extraction loop.
    pub async fn new(config: MemoryConfig, provider: L) -> Result<Self> {
        let config = Arc::new(config);
        let provider = Arc::new(provider);
        let store = Arc::new(MemoryStore::new(config.clone()));
        store.ensure_dir()?;

        let recall = MemoryRecall::new(store.clone(), provider.clone(), config.max_recall);
        let consolidator = Arc::new(TokioMutex::new(MemoryConsolidator::new(
            store.clone(),
            provider.clone(),
            config.clone(),
        )));

        let write_mutex = Arc::new(TokioMutex::new(()));
        let coalescer = Arc::new(ExtractionCoalescer::new());

        let engine = Self {
            config: config.clone(),
            store: store.clone(),
            provider: provider.clone(),
            recall,
            consolidator,
            write_mutex: write_mutex.clone(),
            coalescer: coalescer.clone(),
            extraction_handle: TokioMutex::new(None),
            consolidation_handle: TokioMutex::new(None),
            surfaced: TokioMutex::new(HashSet::new()),
        };

        // Spawn the long-lived background extraction loop
        let extractor = MemoryExtractor::new(
            store.clone(),
            provider.clone(),
            config.extraction_max_turns,
        );
        let wm = write_mutex.clone();
        let coal = coalescer.clone();

        let handle = tokio::spawn(async move {
            loop {
                coal.notified().await;
                let messages = coal.take();
                if let Some(msgs) = messages {
                    let _guard = wm.lock().await;
                    if let Err(e) = extractor.run(msgs).await {
                        tracing::warn!(error = %e, "background extraction failed");
                    }
                }
            }
        });

        *engine.extraction_handle.lock().await = Some(handle);
        Ok(engine)
    }

    // ── Recall ──────────────────────────────────────────────────

    /// Select and return relevant memories for the given query.
    ///
    /// `recently_used_tools` helps the selector avoid surfacing redundant reference docs.
    pub async fn recall(
        &self,
        query: &str,
        recently_used_tools: &[String],
    ) -> Result<RecallResult> {
        if !self.is_enabled() {
            return Ok(RecallResult::default());
        }

        let surfaced = self.surfaced.lock().await;
        let result = self
            .recall
            .recall(query, &surfaced, recently_used_tools)
            .await?;

        drop(surfaced);

        // Track newly surfaced memories
        let mut surfaced = self.surfaced.lock().await;
        for mem in &result.memories {
            if let Some(filename) = mem.path.file_name().and_then(|n| n.to_str()) {
                surfaced.insert(filename.to_string());
            }
        }

        Ok(result)
    }

    /// Return the full memory manifest (index contents).
    pub fn manifest(&self) -> Result<MemoryManifest> {
        self.store.manifest()
    }

    /// Read a single memory by name.
    pub fn read_memory(&self, name: &str) -> Result<Memory> {
        self.store.read(name)
    }

    // ── Extraction ──────────────────────────────────────────────

    /// Spawn background memory extraction from conversation messages.
    /// Coalesces rapid-fire calls (single-slot stash).
    pub fn extract_background(&self, messages: Vec<Message>) {
        if !self.is_enabled() || messages.is_empty() {
            return;
        }
        self.coalescer.push(messages);
    }

    /// Run extraction synchronously (for testing or blocking contexts).
    pub async fn extract(&self, messages: Vec<Message>) -> Result<()> {
        if !self.is_enabled() {
            return Err(MemoryError::Disabled {
                reason: "memory system is disabled".into(),
            });
        }

        let extractor = MemoryExtractor::new(
            self.store.clone(),
            self.provider.clone(),
            self.config.extraction_max_turns,
        );

        let _guard = self.write_mutex.lock().await;
        extractor.run(messages).await
    }

    // ── Direct CRUD ─────────────────────────────────────────────

    /// Create a new memory file and add it to the index.
    pub async fn create_memory(&self, memory: &Memory) -> Result<()> {
        let _guard = self.write_mutex.lock().await;
        self.store.create(memory)
    }

    /// Update an existing memory's content and/or metadata.
    pub async fn update_memory(&self, name: &str, memory: &Memory) -> Result<()> {
        let _guard = self.write_mutex.lock().await;
        self.store.update(name, memory)
    }

    /// Delete a memory file and remove it from the index.
    pub async fn delete_memory(&self, name: &str) -> Result<()> {
        let _guard = self.write_mutex.lock().await;
        self.store.delete(name)
    }

    // ── Consolidation ───────────────────────────────────────────

    /// Attempt to run consolidation. Respects all gates.
    /// Returns Ok(true) if consolidation ran, Ok(false) if gated.
    pub async fn consolidate(&self) -> Result<bool> {
        if !self.is_enabled() {
            return Ok(false);
        }
        let consolidator = self.consolidator.lock().await;
        consolidator.run().await
    }

    /// Spawn consolidation as a background task if gates pass.
    pub async fn consolidate_background(&self) {
        if !self.is_enabled() {
            return;
        }

        let consolidator = self.consolidator.clone();
        let handle = tokio::spawn(async move {
            let c = consolidator.lock().await;
            if let Err(e) = c.run().await {
                tracing::warn!(error = %e, "background consolidation failed");
            }
        });

        *self.consolidation_handle.lock().await = Some(handle);
    }

    /// Record that a session has completed (increments session counter for gating).
    pub async fn record_session_end(&self) {
        let consolidator = self.consolidator.lock().await;
        if let Err(e) = consolidator.gates().record_session() {
            tracing::warn!(error = %e, "failed to record session end");
        }
        // Clear surfaced set for new session
        self.surfaced.lock().await.clear();
    }

    // ── Lifecycle ───────────────────────────────────────────────

    /// Check whether the memory system is enabled (evaluates the enable chain).
    pub fn is_enabled(&self) -> bool {
        enable::is_enabled(&self.config)
    }

    /// Gracefully shut down background tasks, waiting for completion.
    pub async fn shutdown(self) {
        // Abort the extraction background loop
        if let Some(handle) = self.extraction_handle.lock().await.take() {
            handle.abort();
            let _ = handle.await;
        }
        // Wait for consolidation to finish (don't abort — it holds a PID lock)
        if let Some(handle) = self.consolidation_handle.lock().await.take() {
            let _ = handle.await;
        }
    }
}
