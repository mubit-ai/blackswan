use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{MemoryError, Result};

/// PID-based filesystem lock for consolidation.
pub struct PidLock {
    lock_path: PathBuf,
}

impl PidLock {
    pub fn new(memory_dir: &Path) -> Self {
        Self {
            lock_path: memory_dir.join(".consolidate-lock"),
        }
    }

    /// The mtime of the lock file represents the last successful consolidation time.
    pub fn last_consolidation_time(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.lock_path)
            .ok()
            .and_then(|m| m.modified().ok())
    }

    /// Try to acquire the lock. Returns a guard that releases on drop.
    ///
    /// Automatically cleans stale locks (mtime > timeout AND dead PID).
    pub fn try_acquire(&self, timeout: Duration) -> Result<PidLockGuard> {
        // Check if lock file exists
        if self.lock_path.exists() {
            self.try_reclaim_stale(timeout)?;
        }

        // Write our PID
        let pid = std::process::id();
        std::fs::write(&self.lock_path, pid.to_string())
            .map_err(|e| MemoryError::io(&self.lock_path, e))?;

        // Re-read to verify (last-writer-wins race resolution)
        let content = std::fs::read_to_string(&self.lock_path)
            .map_err(|e| MemoryError::io(&self.lock_path, e))?;
        let read_pid: u32 = content.trim().parse().map_err(|_| MemoryError::LockFailed {
            detail: format!("lock file contains non-PID content: {content}"),
        })?;

        if read_pid != pid {
            return Err(MemoryError::ConsolidationLocked { pid: read_pid });
        }

        Ok(PidLockGuard {
            lock_path: self.lock_path.clone(),
        })
    }

    /// Update the lock file's mtime to record a successful consolidation.
    pub fn touch(&self) -> Result<()> {
        // Write the current PID again to update mtime
        let pid = std::process::id();
        std::fs::write(&self.lock_path, pid.to_string())
            .map_err(|e| MemoryError::io(&self.lock_path, e))
    }

    fn try_reclaim_stale(&self, timeout: Duration) -> Result<()> {
        let metadata =
            std::fs::metadata(&self.lock_path).map_err(|e| MemoryError::io(&self.lock_path, e))?;

        let mtime = metadata
            .modified()
            .map_err(|e| MemoryError::io(&self.lock_path, e))?;

        let age = SystemTime::now()
            .duration_since(mtime)
            .unwrap_or_default();

        if age <= timeout {
            // Lock is recent — check if holder is alive
            let content = std::fs::read_to_string(&self.lock_path)
                .map_err(|e| MemoryError::io(&self.lock_path, e))?;
            let holder_pid: u32 = content.trim().parse().map_err(|_| MemoryError::LockFailed {
                detail: "lock file contains invalid PID".into(),
            })?;

            if is_pid_alive(holder_pid) {
                return Err(MemoryError::ConsolidationLocked { pid: holder_pid });
            }
        }

        // Lock is stale (old + dead PID, or old enough): reclaim
        tracing::info!(
            age_secs = age.as_secs(),
            "reclaiming stale consolidation lock"
        );
        std::fs::remove_file(&self.lock_path)
            .map_err(|e| MemoryError::io(&self.lock_path, e))?;
        Ok(())
    }
}

/// RAII guard that removes the lock file on drop.
pub struct PidLockGuard {
    lock_path: PathBuf,
}

impl Drop for PidLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// Check if a process with the given PID is alive.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    // kill(pid, 0) checks if the process exists without sending a signal
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    // On non-Unix, conservatively assume the process is alive
    // to avoid reclaiming locks that might be held
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_and_release() {
        let dir = tempdir().unwrap();
        let lock = PidLock::new(dir.path());

        {
            let _guard = lock.try_acquire(Duration::from_secs(3600)).unwrap();
            assert!(dir.path().join(".consolidate-lock").exists());
        }

        // Guard dropped — lock file removed
        assert!(!dir.path().join(".consolidate-lock").exists());
    }

    #[test]
    fn reclaims_stale_lock() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join(".consolidate-lock");

        // Write a lock with a non-existent PID
        std::fs::write(&lock_path, "999999999").unwrap();

        let lock = PidLock::new(dir.path());
        // With a zero timeout, any lock is considered stale age-wise
        let result = lock.try_acquire(Duration::from_secs(0));
        assert!(result.is_ok());
    }

    #[test]
    fn last_consolidation_time_none_when_no_lock() {
        let dir = tempdir().unwrap();
        let lock = PidLock::new(dir.path());
        assert!(lock.last_consolidation_time().is_none());
    }
}
