use crate::config::MemoryConfig;

/// Evaluate the enable/disable chain. First match wins.
///
/// Priority:
/// 1. `AGENT_MEMORY_ENABLED=0` env var -> OFF
/// 2. bare_mode flag -> OFF
/// 3. remote_mode without persistent storage -> OFF
/// 4. enabled_override in config -> respect setting
/// 5. Default -> ON
pub fn is_enabled(config: &MemoryConfig) -> bool {
    // 1. Environment variable override
    if let Ok(val) = std::env::var("AGENT_MEMORY_ENABLED") {
        return val != "0";
    }

    // 2. Bare mode disables memory
    if config.bare_mode {
        return false;
    }

    // 3. Remote mode without persistent storage
    if config.remote_mode {
        return false;
    }

    // 4. Explicit config override
    if let Some(enabled) = config.enabled_override {
        return enabled;
    }

    // 5. Default: enabled
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn test_config() -> MemoryConfig {
        MemoryConfig {
            memory_dir: PathBuf::from("/tmp/test"),
            max_index_lines: 200,
            max_index_bytes: 25_600,
            max_scan_files: 200,
            large_file_warning_bytes: 40_960,
            extraction_turn_interval: 1,
            extraction_max_turns: 5,
            consolidation_cooldown: Duration::from_secs(86400),
            consolidation_scan_throttle: Duration::from_secs(600),
            consolidation_session_gate: 5,
            consolidation_lock_timeout: Duration::from_secs(3600),
            consolidation_max_turns: 30,
            max_recall: 5,
            enabled_override: None,
            bare_mode: false,
            remote_mode: false,
        }
    }

    #[test]
    fn default_is_enabled() {
        let config = test_config();
        // Clear env to not interfere
        std::env::remove_var("AGENT_MEMORY_ENABLED");
        assert!(is_enabled(&config));
    }

    #[test]
    fn bare_mode_disables() {
        std::env::remove_var("AGENT_MEMORY_ENABLED");
        let mut config = test_config();
        config.bare_mode = true;
        assert!(!is_enabled(&config));
    }

    #[test]
    fn remote_mode_disables() {
        std::env::remove_var("AGENT_MEMORY_ENABLED");
        let mut config = test_config();
        config.remote_mode = true;
        assert!(!is_enabled(&config));
    }

    #[test]
    fn config_override_respected() {
        std::env::remove_var("AGENT_MEMORY_ENABLED");
        let mut config = test_config();
        config.enabled_override = Some(false);
        assert!(!is_enabled(&config));

        config.enabled_override = Some(true);
        assert!(is_enabled(&config));
    }
}
