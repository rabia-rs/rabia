//! # Rabia Core - State Machine Replication Framework
//!
//! Core components for building State Machine Replication (SMR) applications with the Rabia consensus protocol.
//!
//! This crate provides the fundamental building blocks for implementing
//! fault-tolerant distributed applications using the SMR pattern:
//!
//! ## SMR Framework Components
//!
//! - **StateMachine Trait**: Interface for implementing deterministic state machines
//! - **Operation Types**: Core types for SMR operations, batching, and results
//! - **Consensus Messages**: Protocol messages for coordinating operation ordering
//! - **Node Management**: Types like NodeId, BatchId, PhaseId for cluster coordination
//! - **Error Handling**: Comprehensive error types and recovery mechanisms
//! - **Serialization**: High-performance binary serialization for SMR operations
//! - **Memory Management**: Optimized memory pools for reduced allocations
//! - **Validation**: Operation and state validation utilities
//!
//! ## Building Your SMR Application
//!
//! ```rust
//! use rabia_core::smr::{StateMachine, Operation, OperationResult};
//! use async_trait::async_trait;
//!
//! // Define your application state
//! pub struct CounterSMR {
//!     value: i64,
//! }
//!
//! // Implement the StateMachine trait
//! #[async_trait]
//! impl StateMachine for CounterSMR {
//!     async fn apply_operation(&mut self, op: &Operation) -> OperationResult {
//!         // Your deterministic operation logic here
//!         // This will execute identically on all replicas
//!         match op.operation_type.as_str() {
//!             "increment" => {
//!                 self.value += 1;
//!                 Ok(bincode::serialize(&self.value)?)
//!             }
//!             _ => Err("Unknown operation".into())
//!         }
//!     }
//!     
//!     async fn snapshot(&self) -> OperationResult {
//!         Ok(bincode::serialize(&self.value)?)
//!     }
//!     
//!     async fn restore_from_snapshot(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         self.value = bincode::deserialize(data)?;
//!         Ok(())
//!     }
//! }
//! ```
//!
//! This framework handles consensus, networking, and persistence,
//! letting you focus on your application's business logic.

pub mod batching;
pub mod error;
pub mod memory_pool;
pub mod messages;
pub mod network;
pub mod persistence;
pub mod serialization;
pub mod smr;
pub mod state_machine;
pub mod types;
pub mod validation;

// Re-export commonly used types for convenience
pub use error::*;
pub use types::*;
pub use validation::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{ProposeMessage, ProtocolMessage};
    use crate::state_machine::{InMemoryStateMachine, StateMachine};

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
