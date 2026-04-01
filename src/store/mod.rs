pub mod fileops;
pub mod frontmatter;
pub mod index;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::MemoryConfig;
use crate::error::{MemoryError, Result};
use crate::types::{ManifestEntry, Memory, MemoryManifest};

use self::index::MemoryIndex;

/// Coordinates all memory file operations: CRUD, index maintenance, scanning.
pub struct MemoryStore {
    config: Arc<MemoryConfig>,
    index: MemoryIndex,
}

impl MemoryStore {
    pub fn new(config: Arc<MemoryConfig>) -> Self {
        let index = MemoryIndex::new(
            &config.memory_dir,
            config.max_index_lines,
            config.max_index_bytes,
        );
        Self { config, index }
    }

    pub fn memory_dir(&self) -> &Path {
        &self.config.memory_dir
    }

    /// Create a new memory file and add it to the index.
    pub fn create(&self, memory: &Memory) -> Result<()> {
        let path = self.memory_path(&memory.name);
        if path.exists() {
            return self.update(&memory.name, memory);
        }

        let content = frontmatter::serialize(memory);
        fileops::write_file(&path, &content)?;

        let entry = memory_to_entry(memory);
        self.index.add_entry(&entry)?;

        Ok(())
    }

    /// Read a single memory by filename (without extension) or full filename.
    pub fn read(&self, name: &str) -> Result<Memory> {
        let path = self.resolve_path(name)?;
        let content = fileops::read_file(&path)?;
        let mut memory = frontmatter::parse_memory(&content, &path)?;
        memory.path = path.clone();
        memory.modified = fileops::file_mtime(&path).ok();
        Ok(memory)
    }

    /// Update an existing memory's content and metadata.
    pub fn update(&self, name: &str, memory: &Memory) -> Result<()> {
        let path = self.resolve_path(name)?;
        let content = frontmatter::serialize(memory);
        fileops::write_file(&path, &content)?;

        let entry = memory_to_entry(memory);
        self.index.update_entry(
            path.file_name().unwrap().to_str().unwrap(),
            &entry,
        )?;

        Ok(())
    }

    /// Delete a memory file and remove it from the index.
    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.resolve_path(name)?;
        let filename = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        fileops::delete_file(&path)?;
        self.index.remove_entry(&filename)?;
        Ok(())
    }

    /// Return the parsed MEMORY.md manifest.
    pub fn manifest(&self) -> Result<MemoryManifest> {
        self.index.load()
    }

    /// Scan all memory files (up to config.max_scan_files), parse and return them.
    pub fn scan_all(&self) -> Result<Vec<Memory>> {
        let files = fileops::scan_memory_files(&self.config.memory_dir, self.config.max_scan_files)?;
        let mut memories = Vec::with_capacity(files.len());

        for path in files {
            // Check file size
            if let Some(size) = fileops::check_file_size(&path, self.config.large_file_warning_bytes)? {
                tracing::warn!(
                    path = %path.display(),
                    size_bytes = size,
                    "memory file exceeds size warning threshold"
                );
            }

            match fileops::read_file(&path) {
                Ok(content) => match frontmatter::parse_memory(&content, &path) {
                    Ok(mut memory) => {
                        memory.path = path.clone();
                        memory.modified = fileops::file_mtime(&path).ok();
                        memories.push(memory);
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skipping unparseable memory file");
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping unreadable memory file");
                }
            }
        }

        Ok(memories)
    }

    /// Rewrite the entire index.
    pub fn rewrite_index(&self, entries: &[ManifestEntry]) -> Result<()> {
        self.index.rewrite(entries)
    }

    /// Ensure the memory directory exists.
    pub fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config.memory_dir)
            .map_err(|e| MemoryError::io(&self.config.memory_dir, e))
    }

    /// Get the expected path for a memory with the given name.
    fn memory_path(&self, name: &str) -> PathBuf {
        let filename = name_to_filename(name);
        self.config.memory_dir.join(filename)
    }

    /// Resolve a name to an existing file path.
    fn resolve_path(&self, name: &str) -> Result<PathBuf> {
        // Try direct filename first
        let direct = self.config.memory_dir.join(name);
        if direct.exists() {
            return Ok(direct);
        }

        // Try with .md extension
        let with_ext = self.config.memory_dir.join(format!("{name}.md"));
        if with_ext.exists() {
            return Ok(with_ext);
        }

        // Try converting name to filename
        let converted = self.memory_path(name);
        if converted.exists() {
            return Ok(converted);
        }

        Err(MemoryError::NotFound {
            name: name.to_string(),
        })
    }
}

/// Convert a memory name to a filesystem-safe filename.
fn name_to_filename(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    // Collapse consecutive underscores
    let mut result = String::with_capacity(slug.len());
    let mut prev_underscore = false;
    for c in slug.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    let trimmed = result.trim_matches('_').to_string();
    format!("{trimmed}.md")
}

fn memory_to_entry(memory: &Memory) -> ManifestEntry {
    let filename = name_to_filename(&memory.name);
    ManifestEntry {
        title: memory.name.clone(),
        filename,
        hook: memory.description.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MemoryType;
    use tempfile::tempdir;

    fn test_config(dir: &Path) -> Arc<MemoryConfig> {
        Arc::new(
            MemoryConfig::builder(dir)
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn name_to_filename_basic() {
        assert_eq!(name_to_filename("User Role"), "user_role.md");
        assert_eq!(name_to_filename("API migration deadline"), "api_migration_deadline.md");
        assert_eq!(name_to_filename("  spaces  "), "spaces.md");
    }

    #[test]
    fn create_and_read() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::new(test_config(dir.path()));

        let memory = Memory {
            name: "test memory".into(),
            description: "a test".into(),
            memory_type: MemoryType::User,
            content: "Hello world".into(),
            path: PathBuf::new(),
            modified: None,
        };

        store.create(&memory).unwrap();
        let loaded = store.read("test_memory.md").unwrap();
        assert_eq!(loaded.name, "test memory");
        assert_eq!(loaded.content.trim(), "Hello world");
    }

    #[test]
    fn create_updates_index() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::new(test_config(dir.path()));

        let memory = Memory {
            name: "indexed".into(),
            description: "should appear in index".into(),
            memory_type: MemoryType::Feedback,
            content: "content".into(),
            path: PathBuf::new(),
            modified: None,
        };

        store.create(&memory).unwrap();
        let manifest = store.manifest().unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].filename, "indexed.md");
    }

    #[test]
    fn delete_removes_file_and_index() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::new(test_config(dir.path()));

        let memory = Memory {
            name: "to delete".into(),
            description: "will be removed".into(),
            memory_type: MemoryType::Project,
            content: "temporary".into(),
            path: PathBuf::new(),
            modified: None,
        };

        store.create(&memory).unwrap();
        store.delete("to_delete.md").unwrap();

        assert!(store.read("to_delete.md").is_err());
        let manifest = store.manifest().unwrap();
        assert_eq!(manifest.entries.len(), 0);
    }

    #[test]
    fn scan_all_returns_memories() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::new(test_config(dir.path()));

        for i in 0..3 {
            store.create(&Memory {
                name: format!("mem {i}"),
                description: format!("desc {i}"),
                memory_type: MemoryType::User,
                content: format!("content {i}"),
                path: PathBuf::new(),
                modified: None,
            }).unwrap();
        }

        let all = store.scan_all().unwrap();
        assert_eq!(all.len(), 3);
    }
}
