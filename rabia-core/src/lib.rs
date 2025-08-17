//! # Rabia Core
//!
//! Core components and types for the Rabia consensus protocol implementation.
//!
//! This crate provides the fundamental building blocks for implementing
//! the Rabia consensus algorithm, including:
//!
//! ## Key Components
//!
//! - **Messages**: Protocol messages for communication between nodes
//! - **State Machine**: Interface and implementation for deterministic state transitions
//! - **Types**: Core types like NodeId, BatchId, PhaseId, and Commands
//! - **Error Handling**: Comprehensive error types and recovery mechanisms
//! - **Validation**: Message and state validation utilities
//! - **Serialization**: High-performance binary serialization
//! - **Memory Management**: Optimized memory pools for reduced allocations
//! - **Batching**: Command batching for improved throughput
//!
//! ## Example Usage
//!
//! ```rust
//! use rabia_core::{Command, CommandBatch, NodeId, PhaseId};
//!
//! // Create commands
//! let cmd1 = Command::new("SET key1 value1");
//! let cmd2 = Command::new("GET key1");
//!
//! // Create a batch
//! let batch = CommandBatch::new(vec![cmd1, cmd2]);
//!
//! // Create node and phase identifiers
//! let node_id = NodeId::new();
//! let phase_id = PhaseId::new(1);
//! ```

pub mod error;
pub mod messages;
pub mod state_machine;
pub mod network;
pub mod persistence;
pub mod types;
pub mod validation;
pub mod serialization;
pub mod memory_pool;
pub mod batching;

// Re-export commonly used types for convenience
pub use error::*;
pub use types::*;
pub use validation::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::{InMemoryStateMachine, StateMachine};
    use crate::messages::{ProtocolMessage, ProposeMessage};

    #[tokio::test]
    async fn test_state_machine_basic_operations() {
        let mut sm = InMemoryStateMachine::new();
        
        // Test SET command
        let set_cmd = Command::new("SET key1 value1");
        let result = sm.apply_command(&set_cmd).await.unwrap();
        assert_eq!(result, bytes::Bytes::from("OK"));
        
        // Test GET command
        let get_cmd = Command::new("GET key1");
        let result = sm.apply_command(&get_cmd).await.unwrap();
        assert_eq!(result, bytes::Bytes::from("value1"));
        
        // Test GET non-existent key
        let get_cmd = Command::new("GET nonexistent");
        let result = sm.apply_command(&get_cmd).await.unwrap();
        assert_eq!(result, bytes::Bytes::from("NOT_FOUND"));
    }

    #[test]
    fn test_command_batch_creation() {
        let commands = vec![
            Command::new("SET key1 value1"),
            Command::new("SET key2 value2"),
        ];
        
        let batch = CommandBatch::new(commands.clone());
        assert_eq!(batch.commands.len(), 2);
        assert_eq!(batch.commands, commands);
        
        // Test checksum calculation
        let checksum = batch.checksum();
        assert!(checksum > 0);
    }

    #[test]
    fn test_phase_id_operations() {
        let phase1 = PhaseId::new(1);
        let phase2 = phase1.next();
        
        assert_eq!(phase1.value(), 1);
        assert_eq!(phase2.value(), 2);
        assert!(phase2 > phase1);
    }

    #[test]
    fn test_message_validation() {
        let node_id = NodeId::new();
        let batch_id = BatchId::new();
        let phase_id = PhaseId::new(1);
        
        let propose = ProposeMessage {
            phase_id,
            batch_id,
            value: StateValue::V1,
            batch: None,
        };
        
        let message = ProtocolMessage::propose(node_id, propose);
        
        // Basic message should validate successfully
        assert!(message.validate().is_ok());
    }

    #[test]
    fn test_error_types() {
        let error = RabiaError::network("test error");
        assert!(error.is_retryable());
        
        let error = RabiaError::ChecksumMismatch {
            expected: 123,
            actual: 456,
        };
        assert!(!error.is_retryable());
    }
}