use crate::state_machine::Snapshot;
use crate::{PhaseId, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Simple state structure for Rabia engine persistence.
///
/// This contains only the essential state information needed for Rabia consensus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    pub current_phase: PhaseId,
    pub last_committed_phase: PhaseId,
    pub snapshot: Option<Snapshot>,
}

impl EngineState {
    pub fn new(
        current_phase: PhaseId,
        last_committed_phase: PhaseId,
        snapshot: Option<Snapshot>,
    ) -> Self {
        Self {
            current_phase,
            last_committed_phase,
            snapshot,
        }
    }

    /// Serialize the engine state to bytes for persistence.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| {
            crate::RabiaError::serialization(format!("Failed to serialize engine state: {}", e))
        })
    }

    /// Deserialize engine state from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        serde_json::from_slice(data).map_err(|e| {
            crate::RabiaError::serialization(format!("Failed to deserialize engine state: {}", e))
        })
    }
}

/// Simplified persistence layer for Rabia consensus protocol.
///
/// Rabia only needs to persist a single state value - the current SMR (State Machine Replication) state.
/// This simplified interface removes the complexity of WAL, checkpoints, and versioning that was
/// unnecessary for Rabia's consensus requirements.
#[async_trait]
pub trait PersistenceLayer: Send + Sync {
    /// Save the current state to persistent storage.
    ///
    /// # Arguments
    /// * `state` - The state data as bytes to persist
    ///
    /// # Returns
    /// * `Ok(())` if the state was successfully saved
    /// * `Err(RabiaError)` if the save operation failed
    async fn save_state(&self, state: &[u8]) -> Result<()>;

    /// Load the current state from persistent storage.
    ///
    /// # Returns
    /// * `Ok(Some(state))` if state was found and loaded successfully  
    /// * `Ok(None)` if no state exists (first startup)
    /// * `Err(RabiaError)` if the load operation failed
    async fn load_state(&self) -> Result<Option<Vec<u8>>>;
}
