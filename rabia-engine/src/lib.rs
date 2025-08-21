//! # Rabia Engine
//!
//! The core consensus engine implementation for the Rabia protocol.
//!
//! This crate provides the main consensus engine that coordinates between
//! different components to implement the Rabia consensus algorithm. It handles
//! message processing, state transitions, and coordination with the network,
//! state machine, and persistence layers.
//!
//! ## Key Components
//!
//! - **RabiaEngine**: The main consensus engine that orchestrates the protocol
//! - **RabiaConfig**: Configuration for the consensus engine behavior
//! - **EngineState**: Internal state management for the consensus protocol
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use rabia_engine::{RabiaEngine, RabiaConfig};
//! use rabia_core::{state_machine::InMemoryStateMachine, network::ClusterConfig, NodeId};
//! use rabia_testing::InMemoryNetwork;
//! use rabia_persistence::InMemoryPersistence;
//! use std::collections::HashSet;
//! use tokio::sync::mpsc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let node_id = NodeId::new();
//!     let mut node_ids = HashSet::new();
//!     node_ids.insert(node_id);
//!     
//!     let config = RabiaConfig::default();
//!     let cluster_config = ClusterConfig::new(node_id, node_ids);
//!     let state_machine = InMemoryStateMachine::new();
//!     let network = InMemoryNetwork::new(node_id);
//!     let persistence = InMemoryPersistence::new();
//!     let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
//!
//!     let engine = RabiaEngine::new(
//!         node_id,
//!         config,
//!         cluster_config,
//!         state_machine,
//!         network,
//!         persistence,
//!         cmd_rx,
//!     );
//!
//!     // Start the engine
//!     let handle = tokio::spawn(async move {
//!         engine.run().await
//!     });
//!     
//!     // Use cmd_tx to send commands to the engine
//!     // handle.await.unwrap();
//! }
//! ```

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
