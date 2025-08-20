//! # Simple Custom State Machine Example
//!
//! This example demonstrates how to implement a basic custom state machine
//! that works with the actual Rabia API (simplified from the original complex example).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tracing::info;

use bytes::Bytes;
use rabia_core::{
    network::ClusterConfig,
    state_machine::{Snapshot, StateMachine},
    Command, NodeId, RabiaError, Result,
};
use rabia_engine::{RabiaConfig, RabiaEngine};
use rabia_network::InMemoryNetwork;
use rabia_persistence::InMemoryPersistence;

/// Simple key-value state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleKVStateMachine {
    data: HashMap<String, String>,
    operation_count: u64,
}

impl Default for SimpleKVStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleKVStateMachine {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            operation_count: 0,
        }
    }

    fn execute_command(&mut self, command: &str) -> Result<String> {
        self.operation_count += 1;

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(RabiaError::state_machine("Empty command"));
        }

        match parts[0].to_uppercase().as_str() {
            "SET" => {
                if parts.len() != 3 {
                    return Err(RabiaError::state_machine("SET requires key and value"));
                }
                let key = parts[1].to_string();
                let value = parts[2].to_string();
                self.data.insert(key.clone(), value.clone());
                Ok(format!("SET {} = {}", key, value))
            }
            "GET" => {
                if parts.len() != 2 {
                    return Err(RabiaError::state_machine("GET requires key"));
                }
                let key = parts[1];
                match self.data.get(key) {
                    Some(value) => Ok(value.clone()),
                    None => Ok("NOT_FOUND".to_string()),
                }
            }
            "DELETE" => {
                if parts.len() != 2 {
                    return Err(RabiaError::state_machine("DELETE requires key"));
                }
                let key = parts[1];
                match self.data.remove(key) {
                    Some(value) => Ok(format!("DELETED {} (was: {})", key, value)),
                    None => Ok("NOT_FOUND".to_string()),
                }
            }
            "COUNT" => Ok(self.data.len().to_string()),
            "LIST" => {
                let keys: Vec<String> = self.data.keys().cloned().collect();
                Ok(format!("KEYS: [{}]", keys.join(", ")))
            }
            _ => Err(RabiaError::state_machine(format!(
                "Unknown command: {}",
                parts[0]
            ))),
        }
    }
}

#[async_trait]
impl StateMachine for SimpleKVStateMachine {
    type State = HashMap<String, String>;

    async fn apply_command(&mut self, command: &Command) -> Result<Bytes> {
        let command_str = std::str::from_utf8(&command.data)
            .map_err(|_| RabiaError::state_machine("Invalid UTF-8 in command"))?;
        let result = self.execute_command(command_str)?;
        Ok(Bytes::from(result))
    }

    async fn create_snapshot(&self) -> Result<Snapshot> {
        let serialized = bincode::serialize(self).map_err(|e| {
            RabiaError::state_machine(format!("Snapshot serialization failed: {}", e))
        })?;

        Ok(Snapshot::new(self.operation_count, serialized))
    }

    async fn restore_snapshot(&mut self, snapshot: &Snapshot) -> Result<()> {
        let restored: SimpleKVStateMachine = bincode::deserialize(&snapshot.data).map_err(|e| {
            RabiaError::state_machine(format!("Snapshot deserialization failed: {}", e))
        })?;

        *self = restored;
        info!(
            "Restored state machine from snapshot (operations: {})",
            self.operation_count
        );
        Ok(())
    }

    async fn get_state(&self) -> Self::State {
        self.data.clone()
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🔧 Simple Custom State Machine Example");
    println!("=====================================\n");

    // Create a 3-node cluster
    let node1_id = NodeId::new();
    let node2_id = NodeId::new();
    let node3_id = NodeId::new();

    let mut all_nodes = HashSet::new();
    all_nodes.insert(node1_id);
    all_nodes.insert(node2_id);
    all_nodes.insert(node3_id);

    let cluster_config = ClusterConfig::new(node1_id, all_nodes);

    println!("🌐 Cluster initialized:");
    println!("   - Primary node: {}", node1_id);
    println!("   - Cluster size: 3 nodes");
    println!("   - Quorum size: {}", cluster_config.quorum_size);

    // Create the custom state machine
    let state_machine = SimpleKVStateMachine::new();
    let network = InMemoryNetwork::new(node1_id);
    let persistence = InMemoryPersistence::new();
    let config = RabiaConfig::default();

    // Create command channel
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    // Create the consensus engine with our custom state machine
    let _engine = RabiaEngine::new(
        node1_id,
        config,
        cluster_config,
        state_machine,
        network,
        persistence,
        cmd_rx,
    );

    println!("✅ Consensus engine with custom state machine created successfully\n");

    println!("💡 This example demonstrates:");
    println!("   ✅ Custom StateMachine trait implementation");
    println!("   ✅ Command parsing and execution");
    println!("   ✅ Error handling in state machine");
    println!("   ✅ State snapshots and recovery");
    println!("   ✅ Integration with consensus engine");

    println!("\n🎯 Key Implementation Points:");
    println!("   • StateMachine trait requires: apply_command, create_snapshot, restore_snapshot");
    println!("   • Commands return Bytes (use Bytes::from(string) for text)");
    println!("   • Snapshots use bincode serialization with version/checksum");
    println!("   • State must be deterministic across all nodes");
    println!("   • Error handling uses RabiaError types");

    println!("\n📝 Supported Commands (in real usage):");
    println!("   • SET key value  - Store key-value pair");
    println!("   • GET key        - Retrieve value for key");
    println!("   • DELETE key     - Remove key-value pair");
    println!("   • COUNT          - Get number of stored items");
    println!("   • LIST           - List all keys");

    Ok(())
}
