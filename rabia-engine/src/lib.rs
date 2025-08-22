//! # Rabia Engine - SMR Protocol Coordinator
//!
//! Implementation of the Rabia consensus protocol for State Machine Replication.
//!
//! This crate provides the core engine implementing the Rabia SMR protocol,
//! ensuring all replicas apply operations in the same order and providing
//! strong consistency guarantees across distributed nodes. The engine handles
//! consensus phases, operation ordering, and coordination with state machines.
//!
//! ## SMR Protocol Components
//!
//! - **RabiaEngine**: The main SMR protocol engine that ensures operation ordering
//! - **RabiaConfig**: Configuration for the SMR protocol behavior and performance
//! - **EngineState**: Internal state management for consensus coordination
//! - **Operation Submission**: Interface for submitting operations to the SMR system
//!
//! ## SMR Protocol Usage
//!
//! ```rust,no_run
//! use rabia_engine::{RabiaEngine, RabiaConfig, EngineCommand, CommandRequest};
//! use rabia_core::{state_machine::{StateMachine, Snapshot}, network::ClusterConfig, NodeId, Command, CommandBatch};
//! use rabia_persistence::InMemoryPersistence;
//! use std::collections::HashSet;
//! use tokio::sync::mpsc;
//! use bytes::Bytes;
//!
//! // Example state machine implementation
//! #[derive(Clone)]
//! struct ExampleStateMachine {
//!     counter: i64,
//! }
//!
//! #[async_trait::async_trait]
//! impl StateMachine for ExampleStateMachine {
//!     type State = i64;
//!     
//!     async fn apply_command(&mut self, _command: &Command) -> rabia_core::Result<Bytes> {
//!         self.counter += 1;
//!         Ok(Bytes::from(format!("Counter: {}", self.counter)))
//!     }
//!     
//!     async fn create_snapshot(&self) -> rabia_core::Result<Snapshot> {
//!         Ok(Snapshot::new(1, self.counter.to_be_bytes().to_vec()))
//!     }
//!     
//!     async fn restore_snapshot(&mut self, snapshot: &Snapshot) -> rabia_core::Result<()> {
//!         let bytes: [u8; 8] = snapshot.data.as_ref().try_into().unwrap_or([0; 8]);
//!         self.counter = i64::from_be_bytes(bytes);
//!         Ok(())
//!     }
//!     
//!     async fn get_state(&self) -> Self::State {
//!         self.counter
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let node_id = NodeId::new();
//!     let mut node_ids = HashSet::new();
//!     node_ids.insert(node_id);
//!     
//!     // Protocol configuration
//!     let config = RabiaConfig::default();
//!     let cluster_config = ClusterConfig::new(node_id, node_ids);
//!     
//!     // State machine and persistence
//!     let state_machine = ExampleStateMachine { counter: 0 };
//!     let persistence = InMemoryPersistence::new();
//!     
//!     // Create command channel for engine commands
//!     let (command_tx, command_rx) = mpsc::unbounded_channel();
//!
//!     // Create Rabia protocol engine with TCP networking
//!     let engine = RabiaEngine::new_with_tcp(
//!         node_id,
//!         config,
//!         cluster_config,
//!         state_machine,
//!         persistence,
//!         command_rx,
//!     ).await?;
//!
//!     // Start Rabia protocol engine
//!     let handle = tokio::spawn(async move {
//!         engine.run().await
//!     });
//!     
//!     // Submit batch for consensus
//!     let command = Command::new(b"your_command_data".to_vec());
//!     let batch = CommandBatch::new(vec![command]);
//!     let (response_tx, response_rx) = tokio::sync::oneshot::channel();
//!     let request = CommandRequest { batch, response_tx };
//!     command_tx.send(EngineCommand::ProcessBatch(request))?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! The engine ensures that your state machine's operations are applied in the same
//! order across all healthy replicas, providing strong consistency for your
//! distributed application.

pub mod config;
pub mod engine;
pub mod leader;
pub mod network;
pub mod state;

pub use config::*;
pub use engine::*;
pub use leader::*;
pub use network::*;
pub use state::*;
