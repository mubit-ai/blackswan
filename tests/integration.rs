use std::sync::Mutex as StdMutex;

use blackswan::*;
use tempfile::tempdir;

/// A mock LLM provider that returns canned responses.
struct MockLlm {
    responses: StdMutex<Vec<String>>,
}

impl MockLlm {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: StdMutex::new(responses),
        }
    }

    fn with_single(response: &str) -> Self {
        Self::new(vec![response.to_string()])
    }
}

impl LlmProvider for MockLlm {
    fn complete(
        &self,
        _messages: Vec<Message>,
        _system: Option<String>,
    ) -> impl std::future::Future<Output = Result<String>> + Send + '_ {
        async {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(r#"{"actions": []}"#.to_string())
            } else {
                Ok(responses.remove(0))
            }
        }
    }
}

#[tokio::test]
async fn create_and_recall_memory() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();

    let recall_response = r#"{"selected_memories": ["user_preferences.md"]}"#;
    let llm = MockLlm::with_single(recall_response);

    let engine = MemoryEngine::new(config, llm).await.unwrap();

    // Create a memory directly
    let memory = Memory {
        name: "user preferences".into(),
        description: "user prefers concise responses".into(),
        memory_type: MemoryType::User,
        content: "The user prefers short, direct answers.".into(),
        path: std::path::PathBuf::new(),
        modified: None,
    };
    engine.create_memory(&memory).await.unwrap();

    // Verify it's in the manifest
    let manifest = engine.manifest().unwrap();
    assert_eq!(manifest.entries.len(), 1);
    assert_eq!(manifest.entries[0].title, "user preferences");

    // Recall
    let result = engine.recall("How should I respond?", &[]).await.unwrap();
    assert_eq!(result.memories.len(), 1);
    assert_eq!(result.memories[0].name, "user preferences");

    engine.shutdown().await;
}

#[tokio::test]
async fn extract_memories_from_conversation() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();

    let extraction_response = r#"{
        "actions": [
            {
                "action": "create",
                "name": "user is backend engineer",
                "description": "user works as a backend engineer specializing in Rust",
                "type": "user",
                "content": "The user is a backend engineer who primarily works in Rust."
            }
        ]
    }"#;

    let llm = MockLlm::with_single(extraction_response);
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    let messages = vec![
        Message {
            uuid: "msg-1".into(),
            role: MessageRole::User,
            content: "I'm a backend engineer and I mainly write Rust.".into(),
        },
        Message {
            uuid: "msg-2".into(),
            role: MessageRole::Assistant,
            content: "Great! I'll keep that in mind.".into(),
        },
    ];

    // Run extraction synchronously
    engine.extract(messages).await.unwrap();

    // Verify memory was created
    let manifest = engine.manifest().unwrap();
    assert_eq!(manifest.entries.len(), 1);

    let mem = engine.read_memory("user_is_backend_engineer.md").unwrap();
    assert_eq!(mem.memory_type, MemoryType::User);
    assert!(mem.content.contains("Rust"));

    engine.shutdown().await;
}

#[tokio::test]
async fn update_and_delete_memory() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();
    let llm = MockLlm::new(vec![]);
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    // Create
    let memory = Memory {
        name: "project deadline".into(),
        description: "API v3 ships by March 31".into(),
        memory_type: MemoryType::Project,
        content: "API v3 ships by 2026-03-31.".into(),
        path: std::path::PathBuf::new(),
        modified: None,
    };
    engine.create_memory(&memory).await.unwrap();

    // Update
    let updated = Memory {
        name: "project deadline".into(),
        description: "API v3 deadline extended to April 15".into(),
        memory_type: MemoryType::Project,
        content: "API v3 deadline extended to 2026-04-15.".into(),
        path: std::path::PathBuf::new(),
        modified: None,
    };
    engine
        .update_memory("project_deadline.md", &updated)
        .await
        .unwrap();

    let loaded = engine.read_memory("project_deadline.md").unwrap();
    assert!(loaded.content.contains("2026-04-15"));

    // Delete
    engine.delete_memory("project_deadline.md").await.unwrap();
    assert!(engine.read_memory("project_deadline.md").is_err());
    assert_eq!(engine.manifest().unwrap().entries.len(), 0);

    engine.shutdown().await;
}

#[tokio::test]
async fn recall_graceful_degradation_on_llm_failure() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();

    // LLM returns invalid response
    let llm = MockLlm::with_single("this is not json at all");
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    // Create a memory so the manifest isn't empty
    engine
        .create_memory(&Memory {
            name: "test".into(),
            description: "test desc".into(),
            memory_type: MemoryType::User,
            content: "test".into(),
            path: std::path::PathBuf::new(),
            modified: None,
        })
        .await
        .unwrap();

    // Recall should return empty, not error
    let result = engine.recall("anything", &[]).await.unwrap();
    assert!(result.memories.is_empty());

    engine.shutdown().await;
}

#[tokio::test]
async fn disabled_engine_returns_defaults() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path())
        .enabled_override(Some(false))
        .build()
        .unwrap();

    let llm = MockLlm::new(vec![]);
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    assert!(!engine.is_enabled());
    let result = engine.recall("anything", &[]).await.unwrap();
    assert!(result.memories.is_empty());

    engine.shutdown().await;
}

#[tokio::test]
async fn extraction_cursor_advances() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();

    let response = r#"{"actions": [{"action": "create", "name": "first extraction", "description": "test", "type": "user", "content": "first"}]}"#;
    let response2 = r#"{"actions": [{"action": "create", "name": "second extraction", "description": "test2", "type": "user", "content": "second"}]}"#;

    let llm = MockLlm::new(vec![response.into(), response2.into()]);
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    // First extraction
    let msgs1 = vec![Message {
        uuid: "msg-1".into(),
        role: MessageRole::User,
        content: "hello".into(),
    }];
    engine.extract(msgs1).await.unwrap();

    // Second extraction with same + new messages — cursor should skip msg-1
    let msgs2 = vec![
        Message {
            uuid: "msg-1".into(),
            role: MessageRole::User,
            content: "hello".into(),
        },
        Message {
            uuid: "msg-2".into(),
            role: MessageRole::User,
            content: "world".into(),
        },
    ];
    engine.extract(msgs2).await.unwrap();

    let manifest = engine.manifest().unwrap();
    assert_eq!(manifest.entries.len(), 2);

    engine.shutdown().await;
}

#[tokio::test]
async fn coalescing_keeps_only_latest() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();

    let response = r#"{"actions": []}"#;
    let llm = MockLlm::with_single(response);
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    // Fire off multiple extract_background calls rapidly
    for i in 0..5 {
        engine.extract_background(vec![Message {
            uuid: format!("msg-{i}"),
            role: MessageRole::User,
            content: format!("message {i}"),
        }]);
    }

    // Give the background task time to process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // The coalescer should have kept only the latest batch
    // (We can't assert exactly which ran, but the engine shouldn't panic or deadlock)
    engine.shutdown().await;
}

#[tokio::test]
async fn multiple_memory_types() {
    let dir = tempdir().unwrap();
    let config = MemoryConfig::builder(dir.path()).build().unwrap();
    let llm = MockLlm::new(vec![]);
    let engine = MemoryEngine::new(config, llm).await.unwrap();

    let types = [
        ("user role", MemoryType::User),
        ("no mocks in tests", MemoryType::Feedback),
        ("api migration", MemoryType::Project),
        ("linear tracker", MemoryType::Reference),
    ];

    for (name, memory_type) in &types {
        engine
            .create_memory(&Memory {
                name: name.to_string(),
                description: format!("{name} description"),
                memory_type: *memory_type,
                content: format!("Content for {name}"),
                path: std::path::PathBuf::new(),
                modified: None,
            })
            .await
            .unwrap();
    }

    let manifest = engine.manifest().unwrap();
    assert_eq!(manifest.entries.len(), 4);

    // Read each one back
    for (name, expected_type) in &types {
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        let filename = format!("{}.md", slug.trim_matches('_'));
        let mem = engine.read_memory(&filename).unwrap();
        assert_eq!(mem.memory_type, *expected_type);
    }

    engine.shutdown().await;
}
