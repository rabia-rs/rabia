//! # Rabia Engine - SMR Consensus Coordinator
//!
//! The consensus engine that coordinates State Machine Replication using the Rabia protocol.
//!
//! This crate provides the core engine that ensures all SMR replicas apply operations
//! in the same order, providing strong consistency guarantees across distributed nodes.
//! The engine handles consensus phases, operation ordering, and coordination with
//! your state machine implementations.
//!
//! ## SMR Coordination Components
//!
//! - **RabiaEngine**: The main SMR coordinator that ensures operation ordering
//! - **RabiaConfig**: Configuration for SMR behavior and performance tuning
//! - **EngineState**: Internal state management for consensus coordination
//! - **Operation Submission**: Interface for submitting operations to the SMR system
//!
//! ## SMR Engine Usage
//!
//! ```rust,no_run
//! use rabia_engine::{RabiaEngine, RabiaConfig};
//! use rabia_core::{smr::StateMachine, network::ClusterConfig, NodeId, Operation};
//! use rabia_persistence::InMemoryPersistence;
//! use std::collections::HashSet;
//! use tokio::sync::mpsc;
//!
//! // Your custom state machine implementation
//! use your_app::YourStateMachine;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let node_id = NodeId::new();
//!     let mut node_ids = HashSet::new();
//!     node_ids.insert(node_id);
//!     
//!     // SMR configuration
//!     let config = RabiaConfig::default();
//!     let cluster_config = ClusterConfig::new(node_id, node_ids);
//!     
//!     // Your custom state machine
//!     let state_machine = YourStateMachine::new();
//!     let persistence = InMemoryPersistence::new();
//!     let (operation_tx, operation_rx) = mpsc::unbounded_channel();
//!
//!     // Create SMR replica coordinator
//!     let engine = RabiaEngine::new(
//!         node_id,
//!         config,
//!         cluster_config,
//!         state_machine,
//!         persistence,
//!         operation_rx,
//!     );
//!
//!     // Start SMR coordination
//!     let handle = tokio::spawn(async move {
//!         engine.run().await
//!     });
//!     
//!     // Submit operations to be ordered and applied consistently
//!     let operation = Operation::new(b"your_operation_data".to_vec());
//!     operation_tx.send(operation)?;
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
