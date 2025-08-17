use async_trait::async_trait;
use parking_lot::RwLock;
use rabia_core::{
    persistence::{PersistedState, PersistenceLayer, WALOperation, WriteAheadLogEntry},
    RabiaError, Result,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct InMemoryPersistence {
    state: Arc<RwLock<Option<PersistedState>>>,
    wal_entries: Arc<RwLock<Vec<WriteAheadLogEntry>>>,
    checkpoints: Arc<RwLock<HashMap<String, PersistedState>>>,
}

impl InMemoryPersistence {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
            wal_entries: Arc::new(RwLock::new(Vec::new())),
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
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
    async fn save_state(&self, state: &PersistedState) -> Result<()> {
        if !state.verify_checksum() {
            return Err(RabiaError::ChecksumMismatch {
                expected: state.checksum,
                actual: state.calculate_checksum(),
            });
        }

        let mut current_state = self.state.write();
        *current_state = Some(state.clone());
        Ok(())
    }

    async fn load_state(&self) -> Result<Option<PersistedState>> {
        let state = self.state.read();
        Ok(state.clone())
    }

    async fn append_wal_entry(&self, entry: &WriteAheadLogEntry) -> Result<()> {
        if !entry.verify_checksum() {
            return Err(RabiaError::ChecksumMismatch {
                expected: entry.checksum,
                actual: entry.calculate_checksum(),
            });
        }

        let mut wal = self.wal_entries.write();
        wal.push(entry.clone());
        Ok(())
    }

    async fn read_wal_entries(&self, from_timestamp: u64) -> Result<Vec<WriteAheadLogEntry>> {
        let wal = self.wal_entries.read();
        Ok(wal
            .iter()
            .filter(|entry| entry.timestamp >= from_timestamp)
            .cloned()
            .collect())
    }

    async fn compact_wal(&self, up_to_timestamp: u64) -> Result<()> {
        let mut wal = self.wal_entries.write();
        wal.retain(|entry| entry.timestamp > up_to_timestamp);
        Ok(())
    }

    async fn atomic_update<F>(&self, update_fn: F) -> Result<()>
    where
        F: FnOnce(Option<PersistedState>) -> Result<PersistedState> + Send,
    {
        let current_state = {
            let state = self.state.read();
            state.clone()
        };

        let new_state = update_fn(current_state)?;

        // Create WAL entry
        let wal_operation = match self.state.read().clone() {
            Some(old_state) => WALOperation::StateUpdate {
                old_state,
                new_state: new_state.clone(),
            },
            None => WALOperation::StateUpdate {
                old_state: new_state.clone(), // Use new_state as placeholder
                new_state: new_state.clone(),
            },
        };

        let wal_entry = WriteAheadLogEntry::new(wal_operation);
        self.append_wal_entry(&wal_entry).await?;

        // Update state
        self.save_state(&new_state).await?;

        Ok(())
    }

    async fn create_checkpoint(&self) -> Result<String> {
        let state = self.state.read();
        if let Some(state) = state.clone() {
            let checkpoint_id = uuid::Uuid::new_v4().to_string();
            let mut checkpoints = self.checkpoints.write();
            checkpoints.insert(checkpoint_id.clone(), state);
            Ok(checkpoint_id)
        } else {
            Err(RabiaError::internal("No state to checkpoint"))
        }
    }

    async fn restore_from_checkpoint(&self, checkpoint_id: &str) -> Result<Option<PersistedState>> {
        let checkpoints = self.checkpoints.read();
        Ok(checkpoints.get(checkpoint_id).cloned())
    }

    async fn cleanup_old_checkpoints(&self, keep_count: usize) -> Result<()> {
        let mut checkpoints = self.checkpoints.write();
        if checkpoints.len() > keep_count {
            // Keep only the most recent checkpoints (simplified - would need timestamps in real impl)
            let to_remove = checkpoints.len() - keep_count;
            let keys_to_remove: Vec<_> = checkpoints.keys().take(to_remove).cloned().collect();
            for key in keys_to_remove {
                checkpoints.remove(&key);
            }
        }
        Ok(())
    }

    async fn verify_integrity(&self) -> Result<bool> {
        // Verify state checksum
        let state = self.state.read();
        if let Some(state) = state.as_ref() {
            if !state.verify_checksum() {
                return Ok(false);
            }
        }

        // Verify all WAL entries
        let wal = self.wal_entries.read();
        for entry in wal.iter() {
            if !entry.verify_checksum() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn repair_corruption(&self) -> Result<bool> {
        // For in-memory implementation, we can't really repair corruption
        // In a real file-based implementation, this would try to restore from backups
        // or use redundancy to fix corrupted data
        let is_valid = self.verify_integrity().await?;
        if !is_valid {
            // Reset to empty state as a fallback
            let mut state = self.state.write();
            *state = None;
            let mut wal = self.wal_entries.write();
            wal.clear();
            Ok(true) // Successfully "repaired" by resetting
        } else {
            Ok(false) // No repair needed
        }
    }
}
