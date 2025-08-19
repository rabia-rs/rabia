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
//! ```rust
//! use rabia_engine::{RabiaEngine, RabiaConfig};
//! use rabia_core::state_machine::InMemoryStateMachine;
//! use rabia_network::InMemoryNetwork;
//! use rabia_persistence::InMemoryPersistence;
//! use rabia_core::NodeId;
//!
//! # tokio_test::block_on(async {
//! let node_id = NodeId::new();
//! let config = RabiaConfig::default();
//! let state_machine = InMemoryStateMachine::new();
//! let network = InMemoryNetwork::new(node_id);
//! let persistence = InMemoryPersistence::new();
//!
//! let engine = RabiaEngine::new(
//!     node_id,
//!     config,
//!     state_machine,
//!     network,
//!     persistence,
//! ).await.unwrap();
//! # });
//! ```

pub mod config;
pub mod engine;
pub mod state;

pub use config::*;
pub use engine::*;
pub use state::*;
