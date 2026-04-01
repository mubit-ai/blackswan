use std::path::{Path, PathBuf};

use crate::error::{MemoryError, Result};
use crate::types::{ManifestEntry, MemoryManifest};

/// Manages the MEMORY.md index file.
pub struct MemoryIndex {
    index_path: PathBuf,
    max_lines: usize,
    max_bytes: usize,
}

impl MemoryIndex {
    pub fn new(memory_dir: &Path, max_lines: usize, max_bytes: usize) -> Self {
        Self {
            index_path: memory_dir.join("MEMORY.md"),
            max_lines,
            max_bytes,
        }
    }

    pub fn path(&self) -> &Path {
        &self.index_path
    }

    /// Load and parse the MEMORY.md index.
    pub fn load(&self) -> Result<MemoryManifest> {
        let content = match std::fs::read_to_string(&self.index_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MemoryManifest::default());
            }
            Err(e) => return Err(MemoryError::io(&self.index_path, e)),
        };

        let byte_size = content.len();
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();

        let mut entries = Vec::new();
        for line in &lines {
            if let Some(entry) = parse_index_line(line) {
                entries.push(entry);
            }
        }

        Ok(MemoryManifest {
            entries,
            line_count,
            byte_size,
        })
    }

    /// Add an entry to the index. Enforces caps — truncates with warning if needed.
    pub fn add_entry(&self, entry: &ManifestEntry) -> Result<()> {
        let mut manifest = self.load()?;
        // Check for duplicate filename
        if manifest.entries.iter().any(|e| e.filename == entry.filename) {
            // Update existing entry
            return self.update_entry(&entry.filename, entry);
        }

        manifest.entries.push(entry.clone());
        self.write_manifest(&manifest.entries)
    }

    /// Remove an entry by filename.
    pub fn remove_entry(&self, filename: &str) -> Result<()> {
        let manifest = self.load()?;
        let entries: Vec<_> = manifest
            .entries
            .into_iter()
            .filter(|e| e.filename != filename)
            .collect();
        self.write_manifest(&entries)
    }

    /// Update an existing entry by filename.
    pub fn update_entry(&self, filename: &str, new_entry: &ManifestEntry) -> Result<()> {
        let manifest = self.load()?;
        let entries: Vec<_> = manifest
            .entries
            .into_iter()
            .map(|e| {
                if e.filename == filename {
                    new_entry.clone()
                } else {
                    e
                }
            })
            .collect();
        self.write_manifest(&entries)
    }

    /// Rewrite the entire index with the given entries.
    pub fn rewrite(&self, entries: &[ManifestEntry]) -> Result<()> {
        self.write_manifest(entries)
    }

    fn write_manifest(&self, entries: &[ManifestEntry]) -> Result<()> {
        let mut lines: Vec<String> = entries.iter().map(|e| e.to_string()).collect();

        // Enforce line cap
        let mut truncated = false;
        if lines.len() > self.max_lines {
            lines.truncate(self.max_lines);
            truncated = true;
        }

        // Enforce byte cap
        let mut content = lines.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
        if content.len() > self.max_bytes {
            // Truncate at last complete line within budget
            let budget = self.max_bytes;
            let mut kept = 0;
            for line in content.lines() {
                let line_bytes = line.len() + 1; // +1 for newline
                if kept + line_bytes > budget {
                    break;
                }
                kept += line_bytes;
            }
            content.truncate(kept);
            truncated = true;
        }

        if truncated {
            // Append warning at the boundary
            let warning = "\n<!-- Warning: index truncated at cap -->\n";
            content.push_str(warning);
        }

        std::fs::write(&self.index_path, &content)
            .map_err(|e| MemoryError::io(&self.index_path, e))
    }
}

/// Parse a single MEMORY.md line: `- [Title](filename.md) — hook text`
fn parse_index_line(line: &str) -> Option<ManifestEntry> {
    let line = line.trim();
    if !line.starts_with("- [") {
        return None;
    }

    let after_bracket = &line[3..]; // skip "- ["
    let close_bracket = after_bracket.find(']')?;
    let title = after_bracket[..close_bracket].to_string();

    let after_title = &after_bracket[close_bracket + 1..];
    if !after_title.starts_with('(') {
        return None;
    }
    let close_paren = after_title.find(')')?;
    let filename = after_title[1..close_paren].to_string();

    let after_link = &after_title[close_paren + 1..];
    // Look for " — " or " - " separator
    let hook = if let Some(pos) = after_link.find(" — ") {
        after_link[pos + " — ".len()..].trim().to_string()
    } else if let Some(pos) = after_link.find(" - ") {
        after_link[pos + 3..].trim().to_string()
    } else {
        String::new()
    };

    Some(ManifestEntry {
        title,
        filename,
        hook,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_index_line_full() {
        let entry =
            parse_index_line("- [User prefers small PRs](feedback_prs.md) — corrected approach")
                .unwrap();
        assert_eq!(entry.title, "User prefers small PRs");
        assert_eq!(entry.filename, "feedback_prs.md");
        assert_eq!(entry.hook, "corrected approach");
    }

    #[test]
    fn parse_index_line_no_hook() {
        let entry = parse_index_line("- [Title](file.md)").unwrap();
        assert_eq!(entry.title, "Title");
        assert_eq!(entry.filename, "file.md");
        assert_eq!(entry.hook, "");
    }

    #[test]
    fn parse_index_line_invalid() {
        assert!(parse_index_line("not a valid line").is_none());
        assert!(parse_index_line("").is_none());
    }

    #[test]
    fn add_and_load() {
        let dir = tempdir().unwrap();
        let index = MemoryIndex::new(dir.path(), 200, 25_600);

        let entry = ManifestEntry {
            title: "Test".into(),
            filename: "test.md".into(),
            hook: "a test memory".into(),
        };
        index.add_entry(&entry).unwrap();

        let manifest = index.load().unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].filename, "test.md");
    }

    #[test]
    fn remove_entry() {
        let dir = tempdir().unwrap();
        let index = MemoryIndex::new(dir.path(), 200, 25_600);

        for i in 0..3 {
            index
                .add_entry(&ManifestEntry {
                    title: format!("Entry {i}"),
                    filename: format!("entry_{i}.md"),
                    hook: format!("hook {i}"),
                })
                .unwrap();
        }

        index.remove_entry("entry_1.md").unwrap();
        let manifest = index.load().unwrap();
        assert_eq!(manifest.entries.len(), 2);
        assert!(manifest.entries.iter().all(|e| e.filename != "entry_1.md"));
    }

    #[test]
    fn enforces_line_cap() {
        let dir = tempdir().unwrap();
        let index = MemoryIndex::new(dir.path(), 5, 25_600);

        for i in 0..10 {
            index
                .add_entry(&ManifestEntry {
                    title: format!("Entry {i}"),
                    filename: format!("entry_{i}.md"),
                    hook: format!("hook {i}"),
                })
                .unwrap();
        }

        let manifest = index.load().unwrap();
        assert!(manifest.entries.len() <= 5);
    }

    #[test]
    fn empty_index_returns_default() {
        let dir = tempdir().unwrap();
        let index = MemoryIndex::new(dir.path(), 200, 25_600);
        let manifest = index.load().unwrap();
        assert_eq!(manifest.entries.len(), 0);
        assert_eq!(manifest.line_count, 0);
    }

    #[test]
    fn round_trip_entry_format() {
        let entry = ManifestEntry {
            title: "My Title".into(),
            filename: "my_file.md".into(),
            hook: "some description".into(),
        };
        let line = entry.to_string();
        let parsed = parse_index_line(&line).unwrap();
        assert_eq!(parsed.title, entry.title);
        assert_eq!(parsed.filename, entry.filename);
        assert_eq!(parsed.hook, entry.hook);
    }
}
