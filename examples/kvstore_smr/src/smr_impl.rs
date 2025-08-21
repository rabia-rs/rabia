//! # KVStore SMR Implementation
//!
//! This module implements the StateMachine trait for the KVStore,
//! making it compatible with the Rabia consensus protocol.

use crate::operations::{KVOperation, KVResult, StoreError};
use crate::store::{KVStore, KVStoreConfig, ValueEntry};
use async_trait::async_trait;
use rabia_core::smr::StateMachine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// KVStore state that can be serialized/deserialized
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KVStoreState {
    pub data: HashMap<String, ValueEntry>,
    pub version: u64,
}

impl Default for KVStoreState {
    fn default() -> Self {
        Self {
            data: HashMap::new(),
            version: 0,
        }
    }
}

/// SMR wrapper for KVStore that implements the StateMachine trait
pub struct KVStoreSMR {
    store: KVStore,
}

impl Clone for KVStoreSMR {
    fn clone(&self) -> Self {
        // For state machine replication, we need to create a new instance
        // with the same configuration and current state
        let config = self.store.config.clone();

        // Create a new store instance with the same configuration
        let new_store = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { KVStore::new(config).await })
        })
        .expect("Failed to create new KVStore instance");

        // Copy the current state
        let current_data = self.store.get_all_data();
        let current_version = self.store.current_version();

        new_store.set_all_data(current_data);
        new_store.set_version(current_version);

        Self { store: new_store }
    }
}

impl KVStoreSMR {
    /// Create a new KVStoreSMR instance
    pub async fn new(config: KVStoreConfig) -> Result<Self, StoreError> {
        let store = KVStore::new(config).await?;
        Ok(Self { store })
    }

    /// Create a new KVStoreSMR instance with default configuration
    pub async fn new_default() -> Result<Self, StoreError> {
        Self::new(KVStoreConfig::default()).await
    }

    /// Get access to the underlying store for advanced operations
    pub fn store(&self) -> &KVStore {
        &self.store
    }
}

#[async_trait]
impl StateMachine for KVStoreSMR {
    type Command = KVOperation;
    type Response = KVResult;
    type State = KVStoreState;

    async fn apply_command(&mut self, command: Self::Command) -> Self::Response {
        match command {
            KVOperation::Set { key, value } => match self.store.set(&key, &value).await {
                Ok(result) => result,
                Err(e) => KVResult::Error(e.to_string()),
            },
            KVOperation::Get { key } => match self.store.get(&key).await {
                Ok(Some(_)) => KVResult::Success,
                Ok(None) => KVResult::NotFound,
                Err(e) => KVResult::Error(e.to_string()),
            },
            KVOperation::Delete { key } => match self.store.delete(&key).await {
                Ok(result) => result,
                Err(e) => KVResult::Error(e.to_string()),
            },
            KVOperation::Exists { key } => match self.store.exists(&key).await {
                Ok(true) => KVResult::Success,
                Ok(false) => KVResult::NotFound,
                Err(e) => KVResult::Error(e.to_string()),
            },
        }
    }

    fn get_state(&self) -> Self::State {
        // Create a state snapshot from the current store data
        let data = self.store.get_all_data();
        let version = self.store.current_version();

        KVStoreState { data, version }
    }

    fn set_state(&mut self, state: Self::State) {
        // Clear current data and restore from state
        self.store.set_all_data(state.data);
        self.store.set_version(state.version);
    }

    fn serialize_state(&self) -> Vec<u8> {
        let state = self.get_state();
        bincode::serialize(&state).unwrap_or_default()
    }

    fn deserialize_state(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let state: KVStoreState = bincode::deserialize(data)?;
        self.set_state(state);
        Ok(())
    }

    async fn apply_commands(&mut self, commands: Vec<Self::Command>) -> Vec<Self::Response> {
        let mut responses = Vec::with_capacity(commands.len());
        for command in commands {
            responses.push(self.apply_command(command).await);
        }
        responses
    }

    fn is_deterministic(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kvstore_smr_basic_operations() {
        let mut smr = KVStoreSMR::new_default().await.unwrap();

        // Test SET command
        let set_cmd = KVOperation::Set {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        };
        let result = smr.apply_command(set_cmd).await;
        assert!(result.is_success());

        // Test GET command
        let get_cmd = KVOperation::Get {
            key: "test_key".to_string(),
        };
        let result = smr.apply_command(get_cmd).await;
        assert!(result.is_success());

        // Test EXISTS command
        let exists_cmd = KVOperation::Exists {
            key: "test_key".to_string(),
        };
        let result = smr.apply_command(exists_cmd).await;
        assert!(result.is_success());

        // Test DELETE command
        let delete_cmd = KVOperation::Delete {
            key: "test_key".to_string(),
        };
        let result = smr.apply_command(delete_cmd).await;
        assert!(result.is_success());

        // Test GET after DELETE
        let get_cmd = KVOperation::Get {
            key: "test_key".to_string(),
        };
        let result = smr.apply_command(get_cmd).await;
        assert!(result.is_not_found());
    }

    #[tokio::test]
    async fn test_kvstore_smr_state_serialization() {
        let mut smr = KVStoreSMR::new_default().await.unwrap();

        // Add some data
        let set_cmd1 = KVOperation::Set {
            key: "key1".to_string(),
            value: "value1".to_string(),
        };
        let set_cmd2 = KVOperation::Set {
            key: "key2".to_string(),
            value: "value2".to_string(),
        };
        smr.apply_command(set_cmd1).await;
        smr.apply_command(set_cmd2).await;

        // Serialize state
        let serialized = smr.serialize_state();
        assert!(!serialized.is_empty());

        // Create new SMR instance and deserialize
        let mut new_smr = KVStoreSMR::new_default().await.unwrap();
        new_smr.deserialize_state(&serialized).unwrap();

        // Verify state was restored
        let state = new_smr.get_state();
        assert_eq!(state.data.len(), 2);
        assert!(state.data.contains_key("key1"));
        assert!(state.data.contains_key("key2"));
        assert_eq!(state.data["key1"].value, "value1");
        assert_eq!(state.data["key2"].value, "value2");
    }

    #[tokio::test]
    async fn test_kvstore_smr_multiple_commands() {
        let mut smr = KVStoreSMR::new_default().await.unwrap();

        let commands = vec![
            KVOperation::Set {
                key: "key1".to_string(),
                value: "value1".to_string(),
            },
            KVOperation::Set {
                key: "key2".to_string(),
                value: "value2".to_string(),
            },
            KVOperation::Get {
                key: "key1".to_string(),
            },
            KVOperation::Delete {
                key: "key2".to_string(),
            },
            KVOperation::Get {
                key: "key2".to_string(),
            },
        ];

        let responses = smr.apply_commands(commands).await;
        assert_eq!(responses.len(), 5);
        assert!(responses[0].is_success()); // SET key1
        assert!(responses[1].is_success()); // SET key2
        assert!(responses[2].is_success()); // GET key1
        assert!(responses[3].is_success()); // DELETE key2
        assert!(responses[4].is_not_found()); // GET key2 (after delete)
    }
}
