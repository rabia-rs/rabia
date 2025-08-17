//! # KVStore Operations and Error Types
//!
//! Defines the operations that can be performed on the KVStore and their error types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Operations that can be performed on the KVStore
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KVOperation {
    /// Set a key-value pair
    Set { key: String, value: String },
    /// Get a value by key
    Get { key: String },
    /// Delete a key
    Delete { key: String },
    /// Check if a key exists
    Exists { key: String },
}

impl KVOperation {
    /// Get the key being operated on
    pub fn key(&self) -> &str {
        match self {
            KVOperation::Set { key, .. } => key,
            KVOperation::Get { key } => key,
            KVOperation::Delete { key } => key,
            KVOperation::Exists { key } => key,
        }
    }

    /// Get the operation type as a string
    pub fn operation_type(&self) -> &'static str {
        match self {
            KVOperation::Set { .. } => "SET",
            KVOperation::Get { .. } => "GET",
            KVOperation::Delete { .. } => "DELETE",
            KVOperation::Exists { .. } => "EXISTS",
        }
    }

    /// Check if this operation modifies the store
    pub fn is_write_operation(&self) -> bool {
        matches!(self, KVOperation::Set { .. } | KVOperation::Delete { .. })
    }

    /// Check if this operation only reads from the store
    pub fn is_read_operation(&self) -> bool {
        !self.is_write_operation()
    }
}

/// Result of a KVStore operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KVResult {
    /// Operation completed successfully
    Success,
    /// Key was not found
    NotFound,
    /// Operation failed with an error
    Error(String),
}

impl KVResult {
    /// Check if the result indicates success
    pub fn is_success(&self) -> bool {
        matches!(self, KVResult::Success)
    }

    /// Check if the result indicates a not found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, KVResult::NotFound)
    }

    /// Check if the result indicates an error
    pub fn is_error(&self) -> bool {
        matches!(self, KVResult::Error(_))
    }

    /// Get the error message if this is an error result
    pub fn error_message(&self) -> Option<&str> {
        match self {
            KVResult::Error(msg) => Some(msg),
            _ => None,
        }
    }
}

impl From<StoreError> for KVResult {
    fn from(error: StoreError) -> Self {
        KVResult::Error(error.to_string())
    }
}

/// Errors that can occur during KVStore operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum StoreError {
    /// Invalid key provided
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    /// Value is too large to store
    #[error("Value too large")]
    ValueTooLarge,

    /// Store has reached maximum capacity
    #[error("Store is full")]
    StoreFull,

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Invalid snapshot
    #[error("Invalid snapshot")]
    InvalidSnapshot,

    /// IO error
    #[error("IO error: {0}")]
    IoError(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Consensus error
    #[error("Consensus error: {0}")]
    ConsensusError(String),

    /// Timeout error
    #[error("Operation timed out")]
    Timeout,

    /// Store is shutting down
    #[error("Store is shutting down")]
    ShuttingDown,

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl StoreError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            StoreError::NetworkError(_) | StoreError::Timeout | StoreError::ConsensusError(_)
        )
    }

    /// Check if this error indicates a client error (4xx equivalent)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            StoreError::InvalidKey(_) | StoreError::ValueTooLarge | StoreError::StoreFull
        )
    }

    /// Check if this error indicates a server error (5xx equivalent)
    pub fn is_server_error(&self) -> bool {
        matches!(
            self,
            StoreError::IoError(_) | StoreError::Internal(_) | StoreError::ShuttingDown
        )
    }
}

/// Batch of operations to be executed atomically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationBatch {
    /// The operations in this batch
    pub operations: Vec<KVOperation>,
    /// Unique identifier for this batch
    pub batch_id: String,
    /// Timestamp when the batch was created
    pub created_at: u64,
}

impl OperationBatch {
    /// Create a new operation batch
    pub fn new(operations: Vec<KVOperation>) -> Self {
        Self {
            operations,
            batch_id: uuid::Uuid::new_v4().to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }

    /// Get the number of operations in this batch
    pub fn size(&self) -> usize {
        self.operations.len()
    }

    /// Check if this batch contains any write operations
    pub fn has_write_operations(&self) -> bool {
        self.operations.iter().any(|op| op.is_write_operation())
    }

    /// Check if this batch contains only read operations
    pub fn is_read_only(&self) -> bool {
        self.operations.iter().all(|op| op.is_read_operation())
    }

    /// Get all keys that will be affected by this batch
    pub fn affected_keys(&self) -> Vec<&str> {
        self.operations.iter().map(|op| op.key()).collect()
    }
}

/// Result of executing an operation batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    /// The batch that was executed
    pub batch_id: String,
    /// Results for each operation in the batch
    pub results: Vec<KVResult>,
    /// Number of successful operations
    pub success_count: usize,
    /// Number of failed operations
    pub failure_count: usize,
    /// Time taken to execute the batch (in milliseconds)
    pub execution_time_ms: u64,
}

impl BatchResult {
    /// Create a new batch result
    pub fn new(batch_id: String, results: Vec<KVResult>, execution_time_ms: u64) -> Self {
        let success_count = results.iter().filter(|r| r.is_success()).count();
        let failure_count = results.len() - success_count;

        Self {
            batch_id,
            results,
            success_count,
            failure_count,
            execution_time_ms,
        }
    }

    /// Check if all operations in the batch succeeded
    pub fn all_succeeded(&self) -> bool {
        self.failure_count == 0
    }

    /// Check if any operations in the batch failed
    pub fn has_failures(&self) -> bool {
        self.failure_count > 0
    }

    /// Get the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.results.is_empty() {
            0.0
        } else {
            (self.success_count as f64 / self.results.len() as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_operation_properties() {
        let set_op = KVOperation::Set {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        };
        let get_op = KVOperation::Get {
            key: "test_key".to_string(),
        };
        let delete_op = KVOperation::Delete {
            key: "test_key".to_string(),
        };
        let exists_op = KVOperation::Exists {
            key: "test_key".to_string(),
        };

        // Test key extraction
        assert_eq!(set_op.key(), "test_key");
        assert_eq!(get_op.key(), "test_key");
        assert_eq!(delete_op.key(), "test_key");
        assert_eq!(exists_op.key(), "test_key");

        // Test operation types
        assert_eq!(set_op.operation_type(), "SET");
        assert_eq!(get_op.operation_type(), "GET");
        assert_eq!(delete_op.operation_type(), "DELETE");
        assert_eq!(exists_op.operation_type(), "EXISTS");

        // Test write/read classification
        assert!(set_op.is_write_operation());
        assert!(!get_op.is_write_operation());
        assert!(delete_op.is_write_operation());
        assert!(!exists_op.is_write_operation());

        assert!(!set_op.is_read_operation());
        assert!(get_op.is_read_operation());
        assert!(!delete_op.is_read_operation());
        assert!(exists_op.is_read_operation());
    }

    #[test]
    fn test_kv_result_properties() {
        let success = KVResult::Success;
        let not_found = KVResult::NotFound;
        let error = KVResult::Error("Test error".to_string());

        assert!(success.is_success());
        assert!(!success.is_not_found());
        assert!(!success.is_error());

        assert!(!not_found.is_success());
        assert!(not_found.is_not_found());
        assert!(!not_found.is_error());

        assert!(!error.is_success());
        assert!(!error.is_not_found());
        assert!(error.is_error());
        assert_eq!(error.error_message().unwrap(), "Test error");
    }

    #[test]
    fn test_store_error_classification() {
        let client_error = StoreError::InvalidKey("test".to_string());
        let server_error = StoreError::Internal("test".to_string());
        let recoverable_error = StoreError::NetworkError("test".to_string());

        assert!(client_error.is_client_error());
        assert!(!client_error.is_server_error());
        assert!(!client_error.is_recoverable());

        assert!(!server_error.is_client_error());
        assert!(server_error.is_server_error());
        assert!(!server_error.is_recoverable());

        assert!(!recoverable_error.is_client_error());
        assert!(!recoverable_error.is_server_error());
        assert!(recoverable_error.is_recoverable());
    }

    #[test]
    fn test_operation_batch() {
        let operations = vec![
            KVOperation::Set {
                key: "key1".to_string(),
                value: "value1".to_string(),
            },
            KVOperation::Get {
                key: "key2".to_string(),
            },
            KVOperation::Delete {
                key: "key3".to_string(),
            },
        ];

        let batch = OperationBatch::new(operations);

        assert_eq!(batch.size(), 3);
        assert!(batch.has_write_operations());
        assert!(!batch.is_read_only());

        let affected_keys = batch.affected_keys();
        assert_eq!(affected_keys, vec!["key1", "key2", "key3"]);
    }

    #[test]
    fn test_batch_result() {
        let results = vec![
            KVResult::Success,
            KVResult::NotFound,
            KVResult::Error("test".to_string()),
        ];

        let batch_result = BatchResult::new("test_batch".to_string(), results, 100);

        assert_eq!(batch_result.success_count, 1);
        assert_eq!(batch_result.failure_count, 2);
        assert!(!batch_result.all_succeeded());
        assert!(batch_result.has_failures());
        assert!((batch_result.success_rate() - 33.333333333333336).abs() < 0.0001);
    }
}