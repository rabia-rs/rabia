//! Error types for leader management operations.

use thiserror::Error;

/// Result type for leader management operations
pub type LeaderResult<T> = Result<T, LeaderError>;

/// Errors that can occur during leader management operations
#[derive(Error, Debug)]
pub enum LeaderError {
    /// Leadership election failed
    #[error("Leadership election failed: {reason}")]
    ElectionFailed { reason: String },

    /// Leader validation failed
    #[error("Leader validation failed: {reason}")]
    ValidationFailed { reason: String },

    /// Node is not eligible for leadership
    #[error("Node {node_id} is not eligible for leadership: {reason}")]
    NotEligible { node_id: String, reason: String },

    /// Leadership transition failed
    #[error("Leadership transition failed from {from} to {to}: {reason}")]
    TransitionFailed {
        from: String,
        to: String,
        reason: String,
    },

    /// Cluster topology error
    #[error("Cluster topology error: {reason}")]
    TopologyError { reason: String },

    /// Node health check failed
    #[error("Node health check failed for {node_id}: {reason}")]
    HealthCheckFailed { node_id: String, reason: String },

    /// Communication error with cluster nodes
    #[error("Communication error with node {node_id}: {reason}")]
    CommunicationError { node_id: String, reason: String },

    /// Configuration error
    #[error("Configuration error: {reason}")]
    ConfigError { reason: String },

    /// Consensus coordination error
    #[error("Consensus coordination error: {reason}")]
    ConsensusError { reason: String },

    /// Internal system error
    #[error("Internal system error: {reason}")]
    Internal { reason: String },
}

impl From<anyhow::Error> for LeaderError {
    fn from(err: anyhow::Error) -> Self {
        LeaderError::Internal {
            reason: err.to_string(),
        }
    }
}
