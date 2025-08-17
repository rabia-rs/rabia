use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use crate::{Result, NodeId, PhaseId};
use crate::state_machine::Snapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub node_id: NodeId,
    pub current_phase: PhaseId,
    pub last_committed_phase: PhaseId,
    pub snapshot: Option<Snapshot>,
    pub pending_data: Bytes,
    pub version: u64,
    pub timestamp: u64,
    pub checksum: u32,
}

impl PersistedState {
    pub fn new(
        node_id: NodeId,
        current_phase: PhaseId,
        last_committed_phase: PhaseId,
        snapshot: Option<Snapshot>,
        pending_data: Bytes,
    ) -> Self {
        let version = 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut state = Self {
            node_id,
            current_phase,
            last_committed_phase,
            snapshot,
            pending_data,
            version,
            timestamp,
            checksum: 0,
        };

        state.checksum = state.calculate_checksum();
        state
    }

    pub fn calculate_checksum(&self) -> u32 {
        let mut temp_state = self.clone();
        temp_state.checksum = 0;
        
        let serialized = serde_json::to_vec(&temp_state).unwrap_or_default();
        crc32fast::hash(&serialized)
    }

    pub fn verify_checksum(&self) -> bool {
        self.calculate_checksum() == self.checksum
    }

    pub fn update_version(&mut self) {
        self.version += 1;
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.checksum = self.calculate_checksum();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteAheadLogEntry {
    pub id: uuid::Uuid,
    pub operation: WALOperation,
    pub timestamp: u64,
    pub checksum: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALOperation {
    StateUpdate {
        old_state: PersistedState,
        new_state: PersistedState,
    },
    PhaseAdvance {
        from: PhaseId,
        to: PhaseId,
    },
    SnapshotUpdate {
        snapshot: Snapshot,
    },
}

impl WriteAheadLogEntry {
    pub fn new(operation: WALOperation) -> Self {
        let mut entry = Self {
            id: uuid::Uuid::new_v4(),
            operation,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            checksum: 0,
        };

        entry.checksum = entry.calculate_checksum();
        entry
    }

    pub fn calculate_checksum(&self) -> u32 {
        let mut temp_entry = self.clone();
        temp_entry.checksum = 0;
        
        let serialized = serde_json::to_vec(&temp_entry).unwrap_or_default();
        crc32fast::hash(&serialized)
    }

    pub fn verify_checksum(&self) -> bool {
        self.calculate_checksum() == self.checksum
    }
}

#[async_trait]
pub trait PersistenceLayer: Send + Sync {
    async fn save_state(&self, state: &PersistedState) -> Result<()>;

    async fn load_state(&self) -> Result<Option<PersistedState>>;

    async fn append_wal_entry(&self, entry: &WriteAheadLogEntry) -> Result<()>;

    async fn read_wal_entries(&self, from_timestamp: u64) -> Result<Vec<WriteAheadLogEntry>>;

    async fn compact_wal(&self, up_to_timestamp: u64) -> Result<()>;

    async fn atomic_update<F>(&self, update_fn: F) -> Result<()>
    where
        F: FnOnce(Option<PersistedState>) -> Result<PersistedState> + Send;

    async fn create_checkpoint(&self) -> Result<String>;

    async fn restore_from_checkpoint(&self, checkpoint_id: &str) -> Result<Option<PersistedState>>;

    async fn cleanup_old_checkpoints(&self, keep_count: usize) -> Result<()>;

    async fn verify_integrity(&self) -> Result<bool>;

    async fn repair_corruption(&self) -> Result<bool>;
}

#[derive(Debug)]
pub struct PersistenceConfig {
    pub wal_segment_size: usize,
    pub checkpoint_interval: u64,
    pub max_checkpoints: usize,
    pub fsync_on_write: bool,
    pub compression_enabled: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            wal_segment_size: 64 * 1024 * 1024, // 64MB
            checkpoint_interval: 3600, // 1 hour in seconds
            max_checkpoints: 10,
            fsync_on_write: true,
            compression_enabled: true,
        }
    }
}