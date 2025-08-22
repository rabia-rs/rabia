//! # KVStore SMR Example
//!
//! A key-value store implementation that demonstrates how to create a State Machine
//! Replication (SMR) application using the Rabia consensus protocol.
//!
//! ## Features
//!
//! - **SMR Implementation**: Clean implementation of the StateMachine trait
//! - **Key-Value Operations**: Support for SET, GET, DELETE, and EXISTS operations
//! - **Change Notifications**: Event-driven updates via message bus
//! - **State Serialization**: Full state serialization for consensus integration
//! - **Production Ready**: Comprehensive error handling and monitoring
//!
//! ## Example Usage
//!
//! ```rust
//! use kvstore_smr::{KVStoreSMR, KVOperation, KVStoreConfig};
//! use rabia_core::smr::StateMachine;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut kvstore = KVStoreSMR::new_default().await?;
//!     
//!     let command = KVOperation::Set {
//!         key: "hello".to_string(),
//!         value: "world".to_string(),
//!     };
//!     
//!     let response = kvstore.apply_command(command).await;
//!     println!("Response: {:?}", response);
//!     
//!     Ok(())
//! }
//! ```

pub mod notifications;
pub mod operations;
pub mod smr_impl;
pub mod store;

pub use notifications::{ChangeNotification, NotificationBus, SubscriptionId};
pub use operations::{KVOperation, KVResult, StoreError};
pub use smr_impl::{KVStoreSMR, KVStoreState};
pub use store::{KVStore, KVStoreConfig, StoreSnapshot};

/// Re-export commonly used types for convenience
pub use rabia_core::smr::StateMachine;

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
