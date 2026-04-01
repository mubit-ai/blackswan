use std::path::{Path, PathBuf};

use crate::error::{MemoryError, Result};

/// Tracks the UUID of the last processed message, persisted to disk.
pub struct ExtractionCursor {
    cursor_path: PathBuf,
}

impl ExtractionCursor {
    pub fn new(memory_dir: &Path) -> Self {
        Self {
            cursor_path: memory_dir.join(".extraction-cursor"),
        }
    }

    /// Load the last processed message UUID. Returns None if no cursor exists.
    pub fn load(&self) -> Result<Option<String>> {
        match std::fs::read_to_string(&self.cursor_path) {
            Ok(s) => {
                let trimmed = s.trim().to_string();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(MemoryError::io(&self.cursor_path, e)),
        }
    }

    /// Save the cursor (last processed message UUID).
    pub fn save(&self, uuid: &str) -> Result<()> {
        std::fs::write(&self.cursor_path, uuid).map_err(|e| MemoryError::io(&self.cursor_path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn no_cursor_returns_none() {
        let dir = tempdir().unwrap();
        let cursor = ExtractionCursor::new(dir.path());
        assert_eq!(cursor.load().unwrap(), None);
    }

    #[test]
    fn save_and_load() {
        let dir = tempdir().unwrap();
        let cursor = ExtractionCursor::new(dir.path());
        cursor.save("abc-123").unwrap();
        assert_eq!(cursor.load().unwrap(), Some("abc-123".to_string()));
    }

    #[test]
    fn overwrite() {
        let dir = tempdir().unwrap();
        let cursor = ExtractionCursor::new(dir.path());
        cursor.save("first").unwrap();
        cursor.save("second").unwrap();
        assert_eq!(cursor.load().unwrap(), Some("second".to_string()));
    }
}
