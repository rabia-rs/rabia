//! # Rabia Network
//!
//! Network transport implementations for the Rabia consensus protocol.
//!
//! This crate provides different networking backends for nodes in a Rabia cluster
//! to communicate with each other. It includes both production-ready implementations
//! and testing utilities.
//!
//! ## Available Implementations
//!
//! - **TCP Network**: Production-ready TCP/IP networking with connection pooling,
//!   automatic reconnection, and comprehensive error handling
//! - **In-Memory Network**: High-performance in-memory networking for testing and
//!   single-process clusters
//!
//! ## Example Usage
//!
//! ### TCP Networking
//!
//! ```rust
//! use rabia_network::{TcpNetwork, TcpNetworkConfig};
//! use rabia_core::{NodeId, network::NetworkTransport};
//!
//! # tokio_test::block_on(async {
//! let node_id = NodeId::new();
//! let config = TcpNetworkConfig {
//!     bind_addr: "127.0.0.1:8000".parse().unwrap(),
//!     ..Default::default()
//! };
//!
//! let mut network = TcpNetwork::new(node_id, config).await.unwrap();
//!
//! // Connect to a peer
//! let peer_id = NodeId::new();
//! let peer_addr = "127.0.0.1:8001".parse().unwrap();
//! network.connect_to_peer(peer_id, peer_addr).await.unwrap();
//! # });
//! ```
//!
//! ### In-Memory Networking
//!
//! ```rust
//! use rabia_network::InMemoryNetwork;
//! use rabia_core::NodeId;
//!
//! let node_id = NodeId::new();
//! let network = InMemoryNetwork::new(node_id);
//! ```

pub mod in_memory;
pub mod tcp;

pub use in_memory::*;
pub use tcp::{BufferConfig, RetryConfig, TcpNetwork, TcpNetworkConfig};
