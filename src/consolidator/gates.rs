use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use crate::config::MemoryConfig;
use crate::error::{MemoryError, Result};

use super::lock::PidLock;

/// Result of evaluating a consolidation gate.
#[derive(Debug)]
pub enum GateResult {
    Pass,
    Block { reason: String },
}

/// Evaluates the gate sequence to determine if consolidation should run.
pub struct ConsolidationGates {
    config: Arc<MemoryConfig>,
    state_path: PathBuf,
    lock: PidLock,
}

impl ConsolidationGates {
    pub fn new(config: Arc<MemoryConfig>) -> Self {
        let state_path = config.memory_dir.join(".consolidation-state");
        let lock = PidLock::new(&config.memory_dir);
        Self {
            config,
            state_path,
            lock,
        }
    }

    pub fn lock(&self) -> &PidLock {
        &self.lock
    }

    /// Evaluate all gates in order. Returns Pass only if all gates pass.
    pub fn evaluate(&self) -> GateResult {
        // 1. Time gate: at least N hours since last consolidation
        if let Some(last_time) = self.lock.last_consolidation_time() {
            let age = SystemTime::now()
                .duration_since(last_time)
                .unwrap_or_default();
            if age < self.config.consolidation_cooldown {
                return GateResult::Block {
                    reason: format!(
                        "time gate: last consolidation was {}s ago (cooldown: {}s)",
                        age.as_secs(),
                        self.config.consolidation_cooldown.as_secs()
                    ),
                };
            }
        }

        // 2. Scan throttle: don't re-scan session list too frequently
        if let Some(last_scan) = self.last_scan_time() {
            let age = SystemTime::now()
                .duration_since(last_scan)
                .unwrap_or_default();
            if age < self.config.consolidation_scan_throttle {
                return GateResult::Block {
                    reason: format!(
                        "scan throttle: last scan was {}s ago (throttle: {}s)",
                        age.as_secs(),
                        self.config.consolidation_scan_throttle.as_secs()
                    ),
                };
            }
        }

        // Update scan timestamp
        let _ = self.touch_scan_time();

        // 3. Session gate: at least N sessions since last consolidation
        let sessions = self.sessions_since_last().unwrap_or(0);
        if sessions < self.config.consolidation_session_gate {
            return GateResult::Block {
                reason: format!(
                    "session gate: {} sessions since last (need {})",
                    sessions, self.config.consolidation_session_gate
                ),
            };
        }

        GateResult::Pass
    }

    /// Record that a session has completed.
    pub fn record_session(&self) -> Result<()> {
        let state = self.load_state()?;
        let new_count = state.session_count + 1;
        self.save_state(&ConsolidationState {
            session_count: new_count,
            last_scan: state.last_scan,
        })
    }

    /// Reset session count (after successful consolidation).
    pub fn reset_sessions(&self) -> Result<()> {
        let state = self.load_state()?;
        self.save_state(&ConsolidationState {
            session_count: 0,
            last_scan: state.last_scan,
        })
    }

    fn sessions_since_last(&self) -> Result<usize> {
        Ok(self.load_state()?.session_count)
    }

    fn last_scan_time(&self) -> Option<SystemTime> {
        self.load_state()
            .ok()
            .and_then(|s| s.last_scan)
    }

    fn touch_scan_time(&self) -> Result<()> {
        let state = self.load_state()?;
        self.save_state(&ConsolidationState {
            session_count: state.session_count,
            last_scan: Some(SystemTime::now()),
        })
    }

    fn load_state(&self) -> Result<ConsolidationState> {
        match std::fs::read_to_string(&self.state_path) {
            Ok(content) => {
                serde_json::from_str(&content).map_err(|e| MemoryError::io(&self.state_path, std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(ConsolidationState::default())
            }
            Err(e) => Err(MemoryError::io(&self.state_path, e)),
        }
    }

    fn save_state(&self, state: &ConsolidationState) -> Result<()> {
        let content = serde_json::to_string(state).map_err(|e| MemoryError::io(&self.state_path, std::io::Error::other(e)))?;
        std::fs::write(&self.state_path, content).map_err(|e| MemoryError::io(&self.state_path, e))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct ConsolidationState {
    session_count: usize,
    #[serde(default)]
    last_scan: Option<SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::tempdir;

    fn test_config(dir: &Path) -> Arc<MemoryConfig> {
        Arc::new(
            MemoryConfig::builder(dir)
                .consolidation_cooldown(Duration::from_secs(0))
                .consolidation_scan_throttle(Duration::from_secs(0))
                .consolidation_session_gate(2)
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn passes_when_enough_sessions() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let gates = ConsolidationGates::new(config);

        gates.record_session().unwrap();
        gates.record_session().unwrap();

        match gates.evaluate() {
            GateResult::Pass => {}
            GateResult::Block { reason } => panic!("expected pass, got block: {reason}"),
        }
    }

    #[test]
    fn blocks_on_insufficient_sessions() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let gates = ConsolidationGates::new(config);

        gates.record_session().unwrap();

        match gates.evaluate() {
            GateResult::Block { .. } => {}
            GateResult::Pass => panic!("expected block"),
        }
    }

    #[test]
    fn reset_sessions_works() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let gates = ConsolidationGates::new(config);

        gates.record_session().unwrap();
        gates.record_session().unwrap();
        gates.reset_sessions().unwrap();

        match gates.evaluate() {
            GateResult::Block { .. } => {}
            GateResult::Pass => panic!("expected block after reset"),
        }
    }
}
