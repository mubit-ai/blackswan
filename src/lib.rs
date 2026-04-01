//! # blackswan
//!
//! A persistent memory system for AI agents. Flat-file storage (markdown + YAML
//! frontmatter), background extraction, semantic recall, and automatic consolidation.
//!
//! **Bring your own LLM**: implement [`LlmProvider`] to connect any model.
//!
//! # Quick Start
//!
//! ```ignore
//! use blackswan::{MemoryEngine, MemoryConfig, LlmProvider};
//!
//! let config = MemoryConfig::builder("./memories").build()?;
//! let engine = MemoryEngine::new(config, my_llm);
//!
//! // Recall relevant memories
//! let result = engine.recall("What was the API decision?", &[]).await?;
//!
//! // Extract memories from conversation
//! engine.extract_background(messages);
//! ```

pub mod config;
pub mod engine;
pub mod error;
pub mod llm;
pub mod types;

mod enable;
mod staleness;

pub mod store;
pub mod extractor;
pub mod recall;
pub mod consolidator;

// Flat re-exports for ergonomic use.
pub use config::{MemoryConfig, MemoryConfigBuilder};
pub use engine::MemoryEngine;
pub use error::{MemoryError, Result};
pub use llm::LlmProvider;
pub use types::{
    ManifestEntry, Memory, MemoryManifest, MemoryType, Message, MessageRole, RecallResult,
    Staleness,
};
