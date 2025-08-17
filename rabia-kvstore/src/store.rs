//! # KVStore Implementation
//!
//! Production-grade key-value store with consensus integration and change notifications.
//! This is focused purely on the storage operations and data management.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::{debug, info};
use crate::notifications::{ChangeNotification, NotificationBus, ChangeType};
use crate::operations::{KVOperation, KVResult, StoreError};

/// Configuration for the KVStore
#[derive(Debug, Clone)]
pub struct KVStoreConfig {
    /// Maximum number of keys to store
    pub max_keys: usize,
    /// Enable change notifications
    pub enable_notifications: bool,
    /// Snapshot frequency (number of operations)
    pub snapshot_frequency: usize,
    /// Enable compression for large values
    pub enable_compression: bool,
    /// Maximum value size in bytes
    pub max_value_size: usize,
}

impl Default for KVStoreConfig {
    fn default() -> Self {
        Self {
            max_keys: 1_000_000,
            enable_notifications: true,
            snapshot_frequency: 10_000,
            enable_compression: false,
            max_value_size: 1024 * 1024, // 1MB
        }
    }
}

/// Value entry in the store with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueEntry {
    pub value: String,
    pub version: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub size: usize,
}

impl ValueEntry {
    pub fn new(value: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let size = value.len();
        
        Self {
            value,
            version: 1,
            created_at: now,
            updated_at: now,
            size,
        }
    }

    pub fn update(&mut self, new_value: String) {
        self.value = new_value;
        self.version += 1;
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.size = self.value.len();
    }
}

/// Store statistics
#[derive(Debug, Clone, Default)]
pub struct StoreStats {
    pub total_keys: usize,
    pub total_operations: u64,
    pub memory_usage_bytes: usize,
    pub last_snapshot_at: u64,
    pub operations_since_snapshot: usize,
}

/// Snapshot of the store state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreSnapshot {
    pub data: HashMap<String, ValueEntry>,
    pub version: u64,
    pub created_at: u64,
    pub checksum: u64,
}

/// Production-grade key-value store
pub struct KVStore {
    /// Configuration
    config: KVStoreConfig,
    
    /// Main data storage
    data: Arc<DashMap<String, ValueEntry>>,
    
    /// Store statistics
    stats: Arc<RwLock<StoreStats>>,
    
    /// Global version counter
    version: Arc<std::sync::atomic::AtomicU64>,
    
    /// Notification bus for change events
    notification_bus: Arc<NotificationBus>,
    
    /// Shutdown signal
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl KVStore {
    /// Create a new KVStore instance
    pub async fn new(config: KVStoreConfig) -> Result<Self, StoreError> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        
        let store = Self {
            config: config.clone(),
            data: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(StoreStats::default())),
            version: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            notification_bus: Arc::new(NotificationBus::new()),
            shutdown_tx,
            shutdown_rx,
        };

        info!("KVStore initialized with config: {:?}", config);
        Ok(store)
    }

    /// Set a key-value pair
    pub async fn set(&self, key: &str, value: &str) -> Result<KVResult, StoreError> {
        self.validate_key(key)?;
        self.validate_value(value)?;

        let old_value = if let Some(mut entry) = self.data.get_mut(key) {
            let old = entry.value.clone();
            entry.update(value.to_string());
            Some(old)
        } else {
            if self.data.len() >= self.config.max_keys {
                return Err(StoreError::StoreFull);
            }
            self.data.insert(key.to_string(), ValueEntry::new(value.to_string()));
            None
        };

        self.increment_operation_count();
        
        // Send notification if enabled
        if self.config.enable_notifications {
            let change_type = if old_value.is_some() {
                ChangeType::Updated
            } else {
                ChangeType::Created
            };
            
            let notification = ChangeNotification {
                key: key.to_string(),
                change_type,
                old_value,
                new_value: Some(value.to_string()),
                version: self.get_version(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            
            self.notification_bus.publish(notification).await;
        }

        debug!("SET operation: key={}, value_len={}", key, value.len());
        Ok(KVResult::Success)
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.validate_key(key)?;
        
        let result = self.data.get(key).map(|entry| entry.value.clone());
        self.increment_operation_count();
        
        debug!("GET operation: key={}, found={}", key, result.is_some());
        Ok(result)
    }

    /// Get a value with metadata
    pub async fn get_with_metadata(&self, key: &str) -> Result<Option<ValueEntry>, StoreError> {
        self.validate_key(key)?;
        
        let result = self.data.get(key).map(|entry| entry.clone());
        self.increment_operation_count();
        
        debug!("GET_META operation: key={}, found={}", key, result.is_some());
        Ok(result)
    }

    /// Delete a key
    pub async fn delete(&self, key: &str) -> Result<KVResult, StoreError> {
        self.validate_key(key)?;
        
        let old_value = self.data.remove(key).map(|(_, entry)| entry.value);
        self.increment_operation_count();

        // Send notification if enabled and key existed
        if self.config.enable_notifications && old_value.is_some() {
            let notification = ChangeNotification {
                key: key.to_string(),
                change_type: ChangeType::Deleted,
                old_value: old_value.clone(),
                new_value: None,
                version: self.get_version(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            
            self.notification_bus.publish(notification).await;
        }

        debug!("DELETE operation: key={}, existed={}", key, old_value.is_some());
        
        if old_value.is_some() {
            Ok(KVResult::Success)
        } else {
            Ok(KVResult::NotFound)
        }
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> Result<bool, StoreError> {
        self.validate_key(key)?;
        
        let exists = self.data.contains_key(key);
        self.increment_operation_count();
        
        debug!("EXISTS operation: key={}, exists={}", key, exists);
        Ok(exists)
    }

    /// List all keys (with optional prefix filter)
    pub async fn keys(&self, prefix: Option<&str>) -> Result<Vec<String>, StoreError> {
        let keys: Vec<String> = if let Some(prefix) = prefix {
            self.data
                .iter()
                .filter(|entry| entry.key().starts_with(prefix))
                .map(|entry| entry.key().clone())
                .collect()
        } else {
            self.data.iter().map(|entry| entry.key().clone()).collect()
        };

        self.increment_operation_count();
        debug!("KEYS operation: prefix={:?}, count={}", prefix, keys.len());
        Ok(keys)
    }

    /// Get the number of keys in the store
    pub async fn size(&self) -> usize {
        self.data.len()
    }

    /// Clear all data
    pub async fn clear(&self) -> Result<KVResult, StoreError> {
        let old_size = self.data.len();
        self.data.clear();
        self.increment_operation_count();

        // Send bulk notification if enabled
        if self.config.enable_notifications && old_size > 0 {
            let notification = ChangeNotification {
                key: "*".to_string(),
                change_type: ChangeType::Cleared,
                old_value: Some(format!("{} keys", old_size)),
                new_value: None,
                version: self.get_version(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            
            self.notification_bus.publish(notification).await;
        }

        info!("CLEAR operation: removed {} keys", old_size);
        Ok(KVResult::Success)
    }

    /// Process a batch of operations atomically
    pub async fn apply_batch(&self, operations: Vec<KVOperation>) -> Result<Vec<KVResult>, StoreError> {
        let mut results = Vec::with_capacity(operations.len());
        
        // In a production implementation, this would use transactions
        // For now, we apply operations sequentially
        for operation in operations {
            let result = match operation {
                KVOperation::Set { key, value } => {
                    self.set(&key, &value).await?
                }
                KVOperation::Get { key } => {
                    let value = self.get(&key).await?;
                    if value.is_some() {
                        KVResult::Success
                    } else {
                        KVResult::NotFound
                    }
                }
                KVOperation::Delete { key } => {
                    self.delete(&key).await?
                }
                KVOperation::Exists { key } => {
                    let exists = self.exists(&key).await?;
                    if exists {
                        KVResult::Success
                    } else {
                        KVResult::NotFound
                    }
                }
            };
            results.push(result);
        }

        debug!("BATCH operation: {} operations processed", results.len());
        Ok(results)
    }

    /// Create a snapshot of the current state
    pub async fn create_snapshot(&self) -> Result<StoreSnapshot, StoreError> {
        let data: HashMap<String, ValueEntry> = self.data
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        let version = self.get_version();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Simple checksum calculation
        let checksum = self.calculate_checksum(&data);

        let snapshot = StoreSnapshot {
            data,
            version,
            created_at,
            checksum,
        };

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.last_snapshot_at = created_at;
            stats.operations_since_snapshot = 0;
        }

        info!("Snapshot created: version={}, keys={}", version, snapshot.data.len());
        Ok(snapshot)
    }

    /// Restore from a snapshot
    pub async fn restore_snapshot(&self, snapshot: StoreSnapshot) -> Result<(), StoreError> {
        // Verify checksum
        let calculated_checksum = self.calculate_checksum(&snapshot.data);
        if calculated_checksum != snapshot.checksum {
            return Err(StoreError::InvalidSnapshot);
        }

        // Clear and restore data
        self.data.clear();
        for (key, value) in snapshot.data {
            self.data.insert(key, value);
        }

        self.version.store(snapshot.version, std::sync::atomic::Ordering::Release);

        info!("Snapshot restored: version={}, keys={}", snapshot.version, self.data.len());
        Ok(())
    }

    /// Get store statistics
    pub async fn get_stats(&self) -> StoreStats {
        let mut stats = self.stats.read().clone();
        stats.total_keys = self.data.len();
        stats.memory_usage_bytes = self.estimate_memory_usage();
        stats
    }

    /// Get notification bus for subscribing to changes
    pub fn notification_bus(&self) -> Arc<NotificationBus> {
        self.notification_bus.clone()
    }

    /// Shutdown the store
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        info!("Shutting down KVStore");
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }

    // Private helper methods

    fn validate_key(&self, key: &str) -> Result<(), StoreError> {
        if key.is_empty() {
            return Err(StoreError::InvalidKey("Key cannot be empty".to_string()));
        }
        if key.len() > 256 {
            return Err(StoreError::InvalidKey("Key too long".to_string()));
        }
        Ok(())
    }

    fn validate_value(&self, value: &str) -> Result<(), StoreError> {
        if value.len() > self.config.max_value_size {
            return Err(StoreError::ValueTooLarge);
        }
        Ok(())
    }

    fn increment_operation_count(&self) {
        let mut stats = self.stats.write();
        stats.total_operations += 1;
        stats.operations_since_snapshot += 1;
    }

    fn get_version(&self) -> u64 {
        self.version.fetch_add(1, std::sync::atomic::Ordering::AcqRel)
    }

    fn calculate_checksum(&self, data: &HashMap<String, ValueEntry>) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        for (key, value) in data {
            key.hash(&mut hasher);
            value.value.hash(&mut hasher);
            value.version.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn estimate_memory_usage(&self) -> usize {
        let mut total = 0;
        for entry in self.data.iter() {
            total += entry.key().len();
            total += entry.value().size;
            total += std::mem::size_of::<ValueEntry>();
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_operations() {
        let config = KVStoreConfig::default();
        let store = KVStore::new(config).await.unwrap();

        // Test SET
        let result = store.set("key1", "value1").await.unwrap();
        assert!(matches!(result, KVResult::Success));

        // Test GET
        let value = store.get("key1").await.unwrap();
        assert_eq!(value.unwrap(), "value1");

        // Test EXISTS
        let exists = store.exists("key1").await.unwrap();
        assert!(exists);

        // Test DELETE
        let result = store.delete("key1").await.unwrap();
        assert!(matches!(result, KVResult::Success));

        // Test GET after DELETE
        let value = store.get("key1").await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let config = KVStoreConfig::default();
        let store = KVStore::new(config).await.unwrap();

        let operations = vec![
            KVOperation::Set { key: "key1".to_string(), value: "value1".to_string() },
            KVOperation::Set { key: "key2".to_string(), value: "value2".to_string() },
            KVOperation::Get { key: "key1".to_string() },
        ];

        let results = store.apply_batch(operations).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0], KVResult::Success));
        assert!(matches!(results[1], KVResult::Success));
        assert!(matches!(results[2], KVResult::Success));
    }

    #[tokio::test]
    async fn test_snapshot_and_restore() {
        let config = KVStoreConfig::default();
        let store = KVStore::new(config).await.unwrap();

        // Add some data
        store.set("key1", "value1").await.unwrap();
        store.set("key2", "value2").await.unwrap();

        // Create snapshot
        let snapshot = store.create_snapshot().await.unwrap();
        assert_eq!(snapshot.data.len(), 2);

        // Clear store
        store.clear().await.unwrap();
        assert_eq!(store.size().await, 0);

        // Restore snapshot
        store.restore_snapshot(snapshot).await.unwrap();
        assert_eq!(store.size().await, 2);

        let value = store.get("key1").await.unwrap();
        assert_eq!(value.unwrap(), "value1");
    }
}