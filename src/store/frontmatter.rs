use std::path::Path;

use crate::error::{MemoryError, Result};
use crate::types::{Memory, MemoryFrontmatter, MemoryType};

/// Parse a markdown file with YAML frontmatter into a Memory.
///
/// Expected format:
/// ```text
/// ---
/// name: some name
/// description: one-line description
/// type: user
/// ---
///
/// Markdown content here...
/// ```
pub(crate) fn parse(content: &str, path: &Path) -> Result<(MemoryFrontmatter, String)> {
    let content = content.trim_start_matches('\u{feff}'); // strip BOM

    if !content.starts_with("---") {
        return Err(MemoryError::FrontmatterParse {
            path: path.to_path_buf(),
            message: "file does not start with ---".into(),
        });
    }

    let after_first = &content[3..];
    let end = after_first.find("\n---").ok_or_else(|| MemoryError::FrontmatterParse {
        path: path.to_path_buf(),
        message: "no closing --- found".into(),
    })?;

    let yaml_str = &after_first[..end];
    let body_start = end + 4; // skip past \n---
    let body = if body_start < after_first.len() {
        after_first[body_start..].trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    let frontmatter: MemoryFrontmatter =
        serde_yaml::from_str(yaml_str).map_err(|e| MemoryError::FrontmatterParse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    Ok((frontmatter, body))
}

/// Parse file content into a full Memory struct.
pub fn parse_memory(content: &str, path: &Path) -> Result<Memory> {
    let (fm, body) = parse(content, path)?;
    Ok(Memory {
        name: fm.name,
        description: fm.description,
        memory_type: fm.memory_type,
        content: body,
        path: path.to_path_buf(),
        modified: None,
    })
}

/// Serialize a Memory into markdown with YAML frontmatter.
pub fn serialize(memory: &Memory) -> String {
    let fm = MemoryFrontmatter {
        name: memory.name.clone(),
        description: memory.description.clone(),
        memory_type: memory.memory_type,
    };
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    // serde_yaml includes a trailing newline; frontmatter delimiters surround it
    format!("---\n{}---\n\n{}\n", yaml, memory.content)
}

/// Serialize just the frontmatter portion (for manifest building).
pub fn serialize_frontmatter(name: &str, description: &str, memory_type: MemoryType) -> String {
    let fm = MemoryFrontmatter {
        name: name.to_string(),
        description: description.to_string(),
        memory_type,
    };
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    format!("---\n{yaml}---")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn round_trip() {
        let memory = Memory {
            name: "test memory".into(),
            description: "a test description".into(),
            memory_type: MemoryType::User,
            content: "Some markdown content here.".into(),
            path: PathBuf::from("test.md"),
            modified: None,
        };

        let serialized = serialize(&memory);
        let parsed = parse_memory(&serialized, &PathBuf::from("test.md")).unwrap();

        assert_eq!(parsed.name, memory.name);
        assert_eq!(parsed.description, memory.description);
        assert_eq!(parsed.memory_type, memory.memory_type);
        assert_eq!(parsed.content.trim(), memory.content.trim());
    }

    #[test]
    fn parse_error_no_frontmatter() {
        let result = parse("no frontmatter here", Path::new("bad.md"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_error_no_closing() {
        let result = parse("---\nname: test\n", Path::new("bad.md"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_all_types() {
        for (type_str, expected) in [
            ("user", MemoryType::User),
            ("feedback", MemoryType::Feedback),
            ("project", MemoryType::Project),
            ("reference", MemoryType::Reference),
        ] {
            let content = format!(
                "---\nname: test\ndescription: desc\ntype: {}\n---\nbody",
                type_str
            );
            let (fm, _) = parse(&content, Path::new("t.md")).unwrap();
            assert_eq!(fm.memory_type, expected);
        }
    }
}
