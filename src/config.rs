use std::path::PathBuf;
use std::time::Duration;

use crate::error::{MemoryError, Result};

/// Configuration for the memory engine.
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Root directory for memory files.
    pub memory_dir: PathBuf,
    /// Maximum lines in MEMORY.md index (default: 200).
    pub max_index_lines: usize,
    /// Maximum bytes in MEMORY.md index (default: 25KB).
    pub max_index_bytes: usize,
    /// Maximum number of memory files to scan, mtime descending (default: 200).
    pub max_scan_files: usize,
    /// Warn on files exceeding this size (default: 40KB).
    pub large_file_warning_bytes: u64,
    /// Extraction: process every N turns (default: 1).
    pub extraction_turn_interval: usize,
    /// Extraction: maximum LLM turns per run (default: 5).
    pub extraction_max_turns: usize,
    /// Consolidation: minimum time between runs (default: 24h).
    pub consolidation_cooldown: Duration,
    /// Consolidation: scan throttle interval (default: 10min).
    pub consolidation_scan_throttle: Duration,
    /// Consolidation: minimum sessions since last run (default: 5).
    pub consolidation_session_gate: usize,
    /// Consolidation: stale lock timeout (default: 60min).
    pub consolidation_lock_timeout: Duration,
    /// Consolidation: max LLM turns per run (default: 30).
    pub consolidation_max_turns: usize,
    /// Maximum memories to recall per query (default: 5).
    pub max_recall: usize,
    /// Override enable/disable (None = use chain logic).
    pub enabled_override: Option<bool>,
    /// Whether the agent is in "bare" mode (disables memory).
    pub bare_mode: bool,
    /// Whether the agent is in "remote" mode without persistent storage.
    pub remote_mode: bool,
}

impl MemoryConfig {
    pub fn builder(memory_dir: impl Into<PathBuf>) -> MemoryConfigBuilder {
        MemoryConfigBuilder {
            config: MemoryConfig {
                memory_dir: memory_dir.into(),
                max_index_lines: 200,
                max_index_bytes: 25_600,
                max_scan_files: 200,
                large_file_warning_bytes: 40_960,
                extraction_turn_interval: 1,
                extraction_max_turns: 5,
                consolidation_cooldown: Duration::from_secs(24 * 3600),
                consolidation_scan_throttle: Duration::from_secs(600),
                consolidation_session_gate: 5,
                consolidation_lock_timeout: Duration::from_secs(3600),
                consolidation_max_turns: 30,
                max_recall: 5,
                enabled_override: None,
                bare_mode: false,
                remote_mode: false,
            },
        }
    }
}

pub struct MemoryConfigBuilder {
    config: MemoryConfig,
}

impl MemoryConfigBuilder {
    pub fn max_index_lines(mut self, n: usize) -> Self {
        self.config.max_index_lines = n;
        self
    }
    pub fn max_index_bytes(mut self, n: usize) -> Self {
        self.config.max_index_bytes = n;
        self
    }
    pub fn max_scan_files(mut self, n: usize) -> Self {
        self.config.max_scan_files = n;
        self
    }
    pub fn large_file_warning_bytes(mut self, n: u64) -> Self {
        self.config.large_file_warning_bytes = n;
        self
    }
    pub fn extraction_turn_interval(mut self, n: usize) -> Self {
        self.config.extraction_turn_interval = n;
        self
    }
    pub fn extraction_max_turns(mut self, n: usize) -> Self {
        self.config.extraction_max_turns = n;
        self
    }
    pub fn consolidation_cooldown(mut self, d: Duration) -> Self {
        self.config.consolidation_cooldown = d;
        self
    }
    pub fn consolidation_scan_throttle(mut self, d: Duration) -> Self {
        self.config.consolidation_scan_throttle = d;
        self
    }
    pub fn consolidation_session_gate(mut self, n: usize) -> Self {
        self.config.consolidation_session_gate = n;
        self
    }
    pub fn consolidation_lock_timeout(mut self, d: Duration) -> Self {
        self.config.consolidation_lock_timeout = d;
        self
    }
    pub fn consolidation_max_turns(mut self, n: usize) -> Self {
        self.config.consolidation_max_turns = n;
        self
    }
    pub fn max_recall(mut self, n: usize) -> Self {
        self.config.max_recall = n;
        self
    }
    pub fn enabled_override(mut self, v: Option<bool>) -> Self {
        self.config.enabled_override = v;
        self
    }
    pub fn bare_mode(mut self, v: bool) -> Self {
        self.config.bare_mode = v;
        self
    }
    pub fn remote_mode(mut self, v: bool) -> Self {
        self.config.remote_mode = v;
        self
    }

    pub fn build(self) -> Result<MemoryConfig> {
        if self.config.max_index_lines == 0 {
            return Err(MemoryError::Config {
                message: "max_index_lines must be > 0".into(),
            });
        }
        if self.config.max_scan_files == 0 {
            return Err(MemoryError::Config {
                message: "max_scan_files must be > 0".into(),
            });
        }
        Ok(self.config)
    }
}
