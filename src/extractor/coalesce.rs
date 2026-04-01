use std::sync::Mutex as StdMutex;
use tokio::sync::Notify;

use crate::types::Message;

/// Single-slot stash for coalescing rapid-fire extraction requests.
///
/// Only the latest batch of messages is kept. When a new batch arrives
/// while extraction is in-progress, it replaces the previous pending batch.
pub struct ExtractionCoalescer {
    /// Single-slot stash: only the latest batch is kept.
    stash: StdMutex<Option<Vec<Message>>>,
    /// Notify the background loop that new work arrived.
    notify: Notify,
}

impl ExtractionCoalescer {
    pub fn new() -> Self {
        Self {
            stash: StdMutex::new(None),
            notify: Notify::new(),
        }
    }

    /// Push messages into the stash, replacing any pending batch.
    pub fn push(&self, messages: Vec<Message>) {
        *self.stash.lock().unwrap() = Some(messages);
        self.notify.notify_one();
    }

    /// Take the current stash contents (leaving None).
    pub fn take(&self) -> Option<Vec<Message>> {
        self.stash.lock().unwrap().take()
    }

    /// Wait for a notification that new work arrived.
    pub async fn notified(&self) {
        self.notify.notified().await;
    }
}

impl Default for ExtractionCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageRole;

    fn msg(content: &str) -> Message {
        Message {
            uuid: content.to_string(),
            role: MessageRole::User,
            content: content.to_string(),
        }
    }

    #[test]
    fn coalesces_to_latest() {
        let c = ExtractionCoalescer::new();
        c.push(vec![msg("first")]);
        c.push(vec![msg("second")]);
        c.push(vec![msg("third")]);

        let taken = c.take().unwrap();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].content, "third");
    }

    #[test]
    fn take_empties_stash() {
        let c = ExtractionCoalescer::new();
        c.push(vec![msg("data")]);
        c.take();
        assert!(c.take().is_none());
    }
}
