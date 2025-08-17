//! # Error Types
//!
//! Comprehensive error handling for the Rabia consensus protocol.

use crate::{BatchId, NodeId, PhaseId};
use thiserror::Error;

/// Error types that can occur during Rabia consensus operations.
///
/// This enum covers all possible error conditions that can arise
/// during consensus protocol execution, from network failures to
/// state corruption. Each error includes context information to
/// aid in debugging and recovery.
///
/// # Error Categories
///
/// - **Network Errors**: Communication failures between nodes
/// - **Persistence Errors**: Storage and retrieval failures
/// - **State Machine Errors**: Application-level execution failures
/// - **Consensus Errors**: Protocol-level violations or failures
/// - **Resource Errors**: Missing nodes, phases, or batches
/// - **Integrity Errors**: Checksum mismatches and corruption
/// - **Timeout Errors**: Operations that exceed time limits
///
/// # Examples
///
/// ```rust
/// use rabia_core::RabiaError;
///
/// let error = RabiaError::network("Connection refused");
/// if error.is_retryable() {
///     println!("This error can be retried");
/// }
/// ```
#[derive(Error, Debug)]
pub enum RabiaError {
    /// Network communication failure between nodes
    #[error("Network error: {message}")]
    Network { message: String },

    /// Persistent storage operation failure
    #[error("Persistence error: {message}")]
    Persistence { message: String },

    /// State machine execution failure
    #[error("State machine error: {message}")]
    StateMachine { message: String },

    /// Consensus protocol violation or failure
    #[error("Consensus error: {message}")]
    Consensus { message: String },

    /// Referenced node was not found in the cluster
    #[error("Node {node_id} not found")]
    NodeNotFound { node_id: NodeId },

    /// Referenced consensus phase was not found
    #[error("Phase {phase_id} not found")]
    PhaseNotFound { phase_id: PhaseId },

    /// Referenced command batch was not found
    #[error("Batch {batch_id} not found")]
    BatchNotFound { batch_id: BatchId },

    /// Invalid state machine transition attempted
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    /// Insufficient nodes available to form a quorum
    #[error("Quorum not available: {current}/{required} nodes")]
    QuorumNotAvailable { current: usize, required: usize },

    /// Data integrity check failed due to checksum mismatch
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    /// State corruption detected in persistent storage
    #[error("State corruption detected: {details}")]
    StateCorruption { details: String },

    /// Incomplete write operation detected
    #[error("Partial write detected: {details}")]
    PartialWrite { details: String },

    /// Operation exceeded its timeout limit
    #[error("Timeout occurred: {operation}")]
    Timeout { operation: String },

    /// JSON serialization/deserialization failure
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// File system or network I/O failure
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected internal error
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Type alias for Results in the Rabia consensus system.
///
/// This type alias provides a convenient way to return results that may
/// contain Rabia-specific errors without having to specify the error type
/// each time.
///
/// # Examples
///
/// ```rust
/// use rabia_core::{Result, RabiaError};
///
/// fn consensus_operation() -> Result<String> {
///     Ok("Success".to_string())
/// }
/// ```
pub type Result<T> = std::result::Result<T, RabiaError>;

impl RabiaError {
    /// Creates a new network error with the given message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let error = RabiaError::network("Connection timeout");
    /// ```
    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
        }
    }

    /// Creates a new persistence error with the given message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let error = RabiaError::persistence("Disk full");
    /// ```
    pub fn persistence(message: impl Into<String>) -> Self {
        Self::Persistence {
            message: message.into(),
        }
    }

    /// Creates a new state machine error with the given message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let error = RabiaError::state_machine("Invalid command");
    /// ```
    pub fn state_machine(message: impl Into<String>) -> Self {
        Self::StateMachine {
            message: message.into(),
        }
    }

    /// Creates a new consensus error with the given message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let error = RabiaError::consensus("Phase mismatch");
    /// ```
    pub fn consensus(message: impl Into<String>) -> Self {
        Self::Consensus {
            message: message.into(),
        }
    }

    /// Creates a new internal error with the given message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let error = RabiaError::internal("Unexpected condition");
    /// ```
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Creates a new serialization error with the given message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let error = RabiaError::serialization("Invalid JSON");
    /// ```
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Internal {
            message: format!("Serialization error: {}", message.into()),
        }
    }

    /// Determines if this error condition is potentially recoverable.
    ///
    /// Retryable errors are typically transient conditions that may
    /// resolve themselves with time or retry attempts. Non-retryable
    /// errors usually indicate permanent failures or programming errors.
    ///
    /// # Returns
    ///
    /// `true` if the error might be resolved by retrying the operation
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::RabiaError;
    ///
    /// let network_error = RabiaError::network("Connection timeout");
    /// assert!(network_error.is_retryable());
    ///
    /// let corruption_error = RabiaError::ChecksumMismatch {
    ///     expected: 123,
    ///     actual: 456,
    /// };
    /// assert!(!corruption_error.is_retryable());
    /// ```
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network { .. } | Self::Timeout { .. } | Self::QuorumNotAvailable { .. }
        )
    }
}
