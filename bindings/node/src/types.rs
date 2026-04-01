use napi_derive::napi;

/// Memory type enum (passed as string from JS).
#[napi(string_enum)]
#[derive(Clone, Copy)]
pub enum JsMemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

impl From<blackswan::MemoryType> for JsMemoryType {
    fn from(t: blackswan::MemoryType) -> Self {
        match t {
            blackswan::MemoryType::User => Self::User,
            blackswan::MemoryType::Feedback => Self::Feedback,
            blackswan::MemoryType::Project => Self::Project,
            blackswan::MemoryType::Reference => Self::Reference,
        }
    }
}

impl From<JsMemoryType> for blackswan::MemoryType {
    fn from(t: JsMemoryType) -> Self {
        match t {
            JsMemoryType::User => Self::User,
            JsMemoryType::Feedback => Self::Feedback,
            JsMemoryType::Project => Self::Project,
            JsMemoryType::Reference => Self::Reference,
        }
    }
}

/// Message role enum.
#[napi(string_enum)]
#[derive(Clone, Copy)]
pub enum JsMessageRole {
    User,
    Assistant,
    System,
}

impl From<JsMessageRole> for blackswan::MessageRole {
    fn from(r: JsMessageRole) -> Self {
        match r {
            JsMessageRole::User => Self::User,
            JsMessageRole::Assistant => Self::Assistant,
            JsMessageRole::System => Self::System,
        }
    }
}

impl From<blackswan::MessageRole> for JsMessageRole {
    fn from(r: blackswan::MessageRole) -> Self {
        match r {
            blackswan::MessageRole::User => Self::User,
            blackswan::MessageRole::Assistant => Self::Assistant,
            blackswan::MessageRole::System => Self::System,
        }
    }
}

/// A conversation message.
#[napi(object)]
#[derive(Clone)]
pub struct JsMessage {
    pub uuid: String,
    pub role: JsMessageRole,
    pub content: String,
}

impl From<&JsMessage> for blackswan::Message {
    fn from(m: &JsMessage) -> Self {
        Self {
            uuid: m.uuid.clone(),
            role: m.role.into(),
            content: m.content.clone(),
        }
    }
}

/// A memory entry.
#[napi(object)]
#[derive(Clone)]
pub struct JsMemory {
    pub name: String,
    pub description: String,
    pub memory_type: JsMemoryType,
    pub content: String,
    pub path: String,
    pub modified: Option<f64>,
}

impl From<blackswan::Memory> for JsMemory {
    fn from(m: blackswan::Memory) -> Self {
        let modified = m.modified.map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
        });
        Self {
            name: m.name,
            description: m.description,
            memory_type: m.memory_type.into(),
            content: m.content,
            path: m.path.to_string_lossy().to_string(),
            modified,
        }
    }
}

impl From<&JsMemory> for blackswan::Memory {
    fn from(m: &JsMemory) -> Self {
        Self {
            name: m.name.clone(),
            description: m.description.clone(),
            memory_type: m.memory_type.into(),
            content: m.content.clone(),
            path: std::path::PathBuf::new(),
            modified: None,
        }
    }
}

/// A MEMORY.md index entry.
#[napi(object)]
#[derive(Clone)]
pub struct JsManifestEntry {
    pub title: String,
    pub filename: String,
    pub hook: String,
}

impl From<blackswan::ManifestEntry> for JsManifestEntry {
    fn from(e: blackswan::ManifestEntry) -> Self {
        Self {
            title: e.title,
            filename: e.filename,
            hook: e.hook,
        }
    }
}

/// The parsed MEMORY.md manifest.
#[napi(object)]
#[derive(Clone)]
pub struct JsMemoryManifest {
    pub entries: Vec<JsManifestEntry>,
    pub line_count: u32,
    pub byte_size: u32,
}

impl From<blackswan::MemoryManifest> for JsMemoryManifest {
    fn from(m: blackswan::MemoryManifest) -> Self {
        Self {
            entries: m.entries.into_iter().map(Into::into).collect(),
            line_count: m.line_count as u32,
            byte_size: m.byte_size as u32,
        }
    }
}

/// Result of a recall operation.
#[napi(object)]
#[derive(Clone)]
pub struct JsRecallResult {
    pub memories: Vec<JsMemory>,
    pub filtered: Vec<String>,
}

impl From<blackswan::RecallResult> for JsRecallResult {
    fn from(r: blackswan::RecallResult) -> Self {
        Self {
            memories: r.memories.into_iter().map(Into::into).collect(),
            filtered: r.filtered,
        }
    }
}

/// Engine configuration.
#[napi(object)]
#[derive(Clone)]
pub struct JsMemoryConfigOptions {
    pub memory_dir: String,
    pub max_index_lines: Option<u32>,
    pub max_index_bytes: Option<u32>,
    pub max_scan_files: Option<u32>,
    pub max_recall: Option<u32>,
    pub extraction_turn_interval: Option<u32>,
    pub consolidation_session_gate: Option<u32>,
    pub enabled: Option<bool>,
    pub bare_mode: Option<bool>,
    pub remote_mode: Option<bool>,
}

impl JsMemoryConfigOptions {
    pub fn into_config(self) -> napi::Result<blackswan::MemoryConfig> {
        let mut builder = blackswan::MemoryConfig::builder(&self.memory_dir);
        if let Some(v) = self.max_index_lines {
            builder = builder.max_index_lines(v as usize);
        }
        if let Some(v) = self.max_index_bytes {
            builder = builder.max_index_bytes(v as usize);
        }
        if let Some(v) = self.max_scan_files {
            builder = builder.max_scan_files(v as usize);
        }
        if let Some(v) = self.max_recall {
            builder = builder.max_recall(v as usize);
        }
        if let Some(v) = self.extraction_turn_interval {
            builder = builder.extraction_turn_interval(v as usize);
        }
        if let Some(v) = self.consolidation_session_gate {
            builder = builder.consolidation_session_gate(v as usize);
        }
        if let Some(v) = self.enabled {
            builder = builder.enabled_override(Some(v));
        }
        if let Some(v) = self.bare_mode {
            builder = builder.bare_mode(v);
        }
        if let Some(v) = self.remote_mode {
            builder = builder.remote_mode(v);
        }
        builder
            .build()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}
