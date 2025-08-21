use async_trait::async_trait;
use parking_lot::RwLock;
use rabia_core::{persistence::PersistenceLayer, Result};
use std::sync::Arc;

/// Simple in-memory persistence implementation.
///
/// This implementation stores a single state value in memory. It's suitable for
/// testing and non-persistent scenarios where state doesn't need to survive
/// process restarts.
#[derive(Debug, Clone)]
pub struct InMemoryPersistence {
    state: Arc<RwLock<Option<Vec<u8>>>>,
}

impl InMemoryPersistence {
    /// Create a new in-memory persistence instance.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
        }
    }
}

impl Default for InMemoryPersistence {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PersistenceLayer for InMemoryPersistence {
    async fn save_state(&self, state: &[u8]) -> Result<()> {
        let mut current_state = self.state.write();
        *current_state = Some(state.to_vec());
        Ok(())
    }

    async fn load_state(&self) -> Result<Option<Vec<u8>>> {
        let state = self.state.read();
        Ok(state.clone())
    }
}
