//! # Rabia Leader Manager
//!
//! Leader management and coordination for Rabia consensus clusters.
//!
//! This crate provides functionality for:
//! - Leader election and management
//! - Cluster topology monitoring
//! - Node health tracking
//! - Leadership transitions
//! - Consensus state coordination
//!
//! The Leader Manager is separate from the KVStore and focuses purely on
//! cluster coordination and leadership concerns.

pub mod errors;
pub mod health;
pub mod leader;
pub mod notifications;
pub mod topology;

pub use errors::{LeaderError, LeaderResult};
pub use health::{HealthMonitor, HealthStatus, NodeHealth};
pub use leader::{LeaderConfig, LeaderManager, LeaderState};
pub use notifications::{
    ConsensusChange, LeaderNotification, LeaderNotificationBus, LeadershipChange,
    NotificationFilter, TopologyChange,
};
pub use topology::{ClusterTopology, NodeInfo, NodeRole, TopologyManager};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_leader_manager_creation() {
        let config = LeaderConfig::default();
        let manager = LeaderManager::new(config).await;
        assert!(manager.is_ok());
    }
}
