use std::path::{Path, PathBuf};

use crate::error::{MemoryError, Result};

/// Scan a directory for `.md` memory files, sorted by mtime descending, capped at max.
/// Excludes MEMORY.md (the index file).
pub fn scan_memory_files(dir: &Path, max: usize) -> Result<Vec<PathBuf>> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| MemoryError::io(dir, e))?;

    let mut files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| MemoryError::io(dir, e))?;
        let path = entry.path();

        // Only .md files, exclude MEMORY.md
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str()) == Some("MEMORY.md")
        {
            continue;
        }

        let metadata = std::fs::metadata(&path).map_err(|e| MemoryError::io(&path, e))?;
        let mtime = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        files.push((path, mtime));
    }

    // Sort by mtime descending (most recent first)
    files.sort_by(|a, b| b.1.cmp(&a.1));

    // Cap at max
    files.truncate(max);

    Ok(files.into_iter().map(|(p, _)| p).collect())
}

/// Check if a file exceeds the large file warning threshold.
pub fn check_file_size(path: &Path, warning_bytes: u64) -> Result<Option<u64>> {
    let metadata = std::fs::metadata(path).map_err(|e| MemoryError::io(path, e))?;
    let size = metadata.len();
    if size > warning_bytes {
        Ok(Some(size))
    } else {
        Ok(None)
    }
}

/// Read a memory file from disk.
pub fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| MemoryError::io(path, e))
}

/// Write a memory file to disk.
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content).map_err(|e| MemoryError::io(path, e))
}

/// Delete a memory file from disk.
pub fn delete_file(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|e| MemoryError::io(path, e))
}

/// Get the modified time of a file.
pub fn file_mtime(path: &Path) -> Result<std::time::SystemTime> {
    let metadata = std::fs::metadata(path).map_err(|e| MemoryError::io(path, e))?;
    metadata
        .modified()
        .map_err(|e| MemoryError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_finds_md_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "content").unwrap();
        fs::write(dir.path().join("b.md"), "content").unwrap();
        fs::write(dir.path().join("c.txt"), "content").unwrap(); // not .md
        fs::write(dir.path().join("MEMORY.md"), "index").unwrap(); // excluded

        let files = scan_memory_files(dir.path(), 200).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|p| p.extension().unwrap() == "md"));
        assert!(files.iter().all(|p| p.file_name().unwrap() != "MEMORY.md"));
    }

    #[test]
    fn scan_respects_cap() {
        let dir = tempdir().unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("mem_{i}.md")), "x").unwrap();
        }
        let files = scan_memory_files(dir.path(), 3).unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn scan_sorted_by_mtime() {
        let dir = tempdir().unwrap();

        // Create files with slight time gaps
        let p1 = dir.path().join("old.md");
        fs::write(&p1, "old").unwrap();

        // Touch a newer file
        std::thread::sleep(std::time::Duration::from_millis(50));
        let p2 = dir.path().join("new.md");
        fs::write(&p2, "new").unwrap();

        let files = scan_memory_files(dir.path(), 200).unwrap();
        assert_eq!(files.len(), 2);
        // Most recent first
        assert_eq!(files[0].file_name().unwrap(), "new.md");
    }

    #[test]
    fn check_file_size_warns() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("big.md");
        fs::write(&path, "x".repeat(100)).unwrap();

        assert!(check_file_size(&path, 50).unwrap().is_some());
        assert!(check_file_size(&path, 200).unwrap().is_none());
    }
}
