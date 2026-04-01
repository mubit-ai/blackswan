use pyo3::prelude::*;
use std::path::PathBuf;

/// Python wrapper for MemoryType.
#[pyclass(eq, eq_int, module = "blackswan")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyMemoryType {
    User = 0,
    Feedback = 1,
    Project = 2,
    Reference = 3,
}

impl From<blackswan::MemoryType> for PyMemoryType {
    fn from(t: blackswan::MemoryType) -> Self {
        match t {
            blackswan::MemoryType::User => Self::User,
            blackswan::MemoryType::Feedback => Self::Feedback,
            blackswan::MemoryType::Project => Self::Project,
            blackswan::MemoryType::Reference => Self::Reference,
        }
    }
}

impl From<PyMemoryType> for blackswan::MemoryType {
    fn from(t: PyMemoryType) -> Self {
        match t {
            PyMemoryType::User => Self::User,
            PyMemoryType::Feedback => Self::Feedback,
            PyMemoryType::Project => Self::Project,
            PyMemoryType::Reference => Self::Reference,
        }
    }
}

/// Python wrapper for MessageRole.
#[pyclass(eq, eq_int, module = "blackswan")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PyMessageRole {
    User = 0,
    Assistant = 1,
    System = 2,
}

impl From<blackswan::MessageRole> for PyMessageRole {
    fn from(r: blackswan::MessageRole) -> Self {
        match r {
            blackswan::MessageRole::User => Self::User,
            blackswan::MessageRole::Assistant => Self::Assistant,
            blackswan::MessageRole::System => Self::System,
        }
    }
}

impl From<PyMessageRole> for blackswan::MessageRole {
    fn from(r: PyMessageRole) -> Self {
        match r {
            PyMessageRole::User => Self::User,
            PyMessageRole::Assistant => Self::Assistant,
            PyMessageRole::System => Self::System,
        }
    }
}

/// A memory entry.
#[pyclass(module = "blackswan")]
#[derive(Clone)]
pub struct PyMemory {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub description: String,
    #[pyo3(get, set)]
    pub memory_type: PyMemoryType,
    #[pyo3(get, set)]
    pub content: String,
    #[pyo3(get)]
    pub path: String,
    #[pyo3(get)]
    pub modified: Option<f64>,
}

#[pymethods]
impl PyMemory {
    #[new]
    #[pyo3(signature = (name, description, memory_type, content, path=None, modified=None))]
    fn new(
        name: String,
        description: String,
        memory_type: PyMemoryType,
        content: String,
        path: Option<String>,
        modified: Option<f64>,
    ) -> Self {
        Self {
            name,
            description,
            memory_type,
            content,
            path: path.unwrap_or_default(),
            modified,
        }
    }

    fn __repr__(&self) -> String {
        format!("Memory(name={:?}, type={:?})", self.name, self.memory_type)
    }
}

impl From<blackswan::Memory> for PyMemory {
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

impl From<&PyMemory> for blackswan::Memory {
    fn from(m: &PyMemory) -> Self {
        Self {
            name: m.name.clone(),
            description: m.description.clone(),
            memory_type: m.memory_type.into(),
            content: m.content.clone(),
            path: PathBuf::new(),
            modified: None,
        }
    }
}

/// A conversation message.
#[pyclass(module = "blackswan")]
#[derive(Clone)]
pub struct PyMessage {
    #[pyo3(get, set)]
    pub uuid: String,
    #[pyo3(get, set)]
    pub role: PyMessageRole,
    #[pyo3(get, set)]
    pub content: String,
}

#[pymethods]
impl PyMessage {
    #[new]
    fn new(uuid: String, role: PyMessageRole, content: String) -> Self {
        Self { uuid, role, content }
    }
}

impl From<&PyMessage> for blackswan::Message {
    fn from(m: &PyMessage) -> Self {
        Self {
            uuid: m.uuid.clone(),
            role: m.role.into(),
            content: m.content.clone(),
        }
    }
}

/// A MEMORY.md index entry.
#[pyclass(module = "blackswan")]
#[derive(Clone)]
pub struct PyManifestEntry {
    #[pyo3(get)]
    pub title: String,
    #[pyo3(get)]
    pub filename: String,
    #[pyo3(get)]
    pub hook: String,
}

impl From<blackswan::ManifestEntry> for PyManifestEntry {
    fn from(e: blackswan::ManifestEntry) -> Self {
        Self {
            title: e.title,
            filename: e.filename,
            hook: e.hook,
        }
    }
}

/// The parsed MEMORY.md manifest.
#[pyclass(module = "blackswan")]
#[derive(Clone)]
pub struct PyMemoryManifest {
    #[pyo3(get)]
    pub entries: Vec<PyManifestEntry>,
    #[pyo3(get)]
    pub line_count: usize,
    #[pyo3(get)]
    pub byte_size: usize,
}

impl From<blackswan::MemoryManifest> for PyMemoryManifest {
    fn from(m: blackswan::MemoryManifest) -> Self {
        Self {
            entries: m.entries.into_iter().map(Into::into).collect(),
            line_count: m.line_count,
            byte_size: m.byte_size,
        }
    }
}

/// Result of a recall operation.
#[pyclass(module = "blackswan")]
#[derive(Clone)]
pub struct PyRecallResult {
    #[pyo3(get)]
    pub memories: Vec<PyMemory>,
    #[pyo3(get)]
    pub filtered: Vec<String>,
}

impl From<blackswan::RecallResult> for PyRecallResult {
    fn from(r: blackswan::RecallResult) -> Self {
        Self {
            memories: r.memories.into_iter().map(Into::into).collect(),
            filtered: r.filtered,
        }
    }
}

/// Engine configuration.
#[pyclass(module = "blackswan")]
#[derive(Clone)]
pub struct PyMemoryConfig {
    pub(crate) inner: blackswan::MemoryConfig,
}

#[pymethods]
impl PyMemoryConfig {
    #[new]
    #[pyo3(signature = (
        memory_dir,
        max_index_lines=200,
        max_index_bytes=25600,
        max_scan_files=200,
        max_recall=5,
        extraction_turn_interval=1,
        consolidation_session_gate=5,
        enabled=None,
        bare_mode=false,
        remote_mode=false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        memory_dir: String,
        max_index_lines: usize,
        max_index_bytes: usize,
        max_scan_files: usize,
        max_recall: usize,
        extraction_turn_interval: usize,
        consolidation_session_gate: usize,
        enabled: Option<bool>,
        bare_mode: bool,
        remote_mode: bool,
    ) -> PyResult<Self> {
        let config = blackswan::MemoryConfig::builder(memory_dir)
            .max_index_lines(max_index_lines)
            .max_index_bytes(max_index_bytes)
            .max_scan_files(max_scan_files)
            .max_recall(max_recall)
            .extraction_turn_interval(extraction_turn_interval)
            .consolidation_session_gate(consolidation_session_gate)
            .enabled_override(enabled)
            .bare_mode(bare_mode)
            .remote_mode(remote_mode)
            .build()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner: config })
    }
}

