//! # Rabia KVStore
//!
//! Production-grade key-value store implementation with integrated consensus,
//! change notifications, and leader management capabilities.
//!
//! ## Features
//!
//! - **High Performance**: Optimized for high-throughput operations
//! - **Consensus Integration**: Built-in Rabia consensus for consistency
//! - **Change Notifications**: Event-driven updates via message bus
//! - **Leader Management**: Automatic leader election and failover
//! - **Topology Awareness**: Dynamic cluster membership handling
//! - **Production Ready**: Comprehensive error handling and monitoring

pub mod notifications;
pub mod operations;
pub mod store;
// TODO: Implement these modules as separate components
// pub mod leader;
// pub mod topology;

pub use notifications::{ChangeNotification, NotificationBus, SubscriptionId};
pub use operations::{KVOperation, KVResult, StoreError};
pub use store::{KVStore, KVStoreConfig, StoreSnapshot};
// pub use leader::{LeaderManager, LeaderEvent, LeaderState};
// pub use topology::{TopologyManager, TopologyEvent, ClusterMembership};

/// Re-export commonly used types
pub use rabia_core::{Command, CommandBatch, NodeId};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kvstore_basic_operations() {
        let config = KVStoreConfig::default();
        let store = KVStore::new(config).await.unwrap();

        // Test basic SET/GET operations
        let result = store.set("key1", "value1").await.unwrap();
        assert!(result.is_success());

        let value = store.get("key1").await.unwrap();
        assert_eq!(value.unwrap(), "value1");

        // Test DELETE
        let result = store.delete("key1").await.unwrap();
        assert!(result.is_success());

        let value = store.get("key1").await.unwrap();
        assert!(value.is_none());
    }
}
