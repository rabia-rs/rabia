# Key-Value Store SMR Example

This example demonstrates how to build a production-grade distributed key-value store using State Machine Replication (SMR) with the Rabia consensus protocol.

## What This Example Shows

The KV Store SMR demonstrates advanced SMR concepts:

1. **Complex State Management**: Managing a dictionary of key-value pairs across replicas
2. **Change Notifications**: Event-driven architecture with publish-subscribe patterns
3. **Efficient Serialization**: Optimized state serialization for large datasets
4. **Production Features**: Comprehensive error handling, monitoring, and observability

## State Machine Implementation

The KV store implements these operations:

- `Set { key, value }` - Store a key-value pair
- `Get { key }` - Retrieve value for a key
- `Delete { key }` - Remove a key-value pair
- `Exists { key }` - Check if a key exists
- `ListKeys` - Get all keys (for debugging/monitoring)
- `Clear` - Remove all key-value pairs
- `Size` - Get current number of stored keys

## Key SMR Features Demonstrated

### Complex State Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KVStoreState {
    pub data: HashMap<String, String>,
    pub operation_count: u64,
    pub created_at: SystemTime,
    pub last_modified: SystemTime,
}
```

### Change Notifications (Event-Driven SMR)
```rust
// The KV store publishes change notifications
pub enum ChangeNotification {
    KeySet { key: String, value: String, old_value: Option<String> },
    KeyDeleted { key: String, old_value: String },
    StoreCleared { key_count: usize },
}

// Clients can subscribe to changes
let subscription_id = store.subscribe_to_changes(|notification| {
    println!("Key changed: {:?}", notification);
}).await;
```

### Efficient State Snapshots
```rust
// Implements efficient serialization for large state
fn serialize_state(&self) -> Vec<u8> {
    // Uses bincode for compact binary serialization
    bincode::serialize(&self.state).unwrap_or_default()
}

// Supports incremental state updates
async fn apply_command(&mut self, command: Self::Command) -> Self::Response {
    let old_value = self.state.data.get(&key).cloned();
    // ... apply operation ...
    
    // Emit change notification for subscribers
    if let Some(notification) = self.create_notification(&command, &old_value) {
        self.notification_bus.publish(notification).await;
    }
    
    KVResult::success(response_data)
}
```

## Architecture Components

### Store Layer (`store.rs`)
High-level interface for KV operations with:
- Connection pooling and load balancing
- Caching and performance optimization
- Metrics collection and monitoring
- Configuration management

### Operations Layer (`operations.rs`)
Defines KV operations and results:
- Operation types and serialization
- Error handling and validation
- Result types and status codes

### SMR Implementation (`smr_impl.rs`)
Core StateMachine trait implementation:
- Deterministic operation application
- State serialization/deserialization
- Change notification generation

### Notifications (`notifications.rs`)
Event-driven change notifications:
- Publication/subscription patterns
- Change event types and routing
- Asynchronous notification delivery

## Running the Example

```bash
# Run the KV store SMR example
cargo run --bin kvstore_smr_example

# Run with multiple replicas
cargo run --bin kvstore_smr_cluster

# Run tests to see SMR behavior
cargo test -p kvstore_smr

# Run benchmarks
cargo bench --bench kvstore_performance
```

## Use Cases

This pattern is ideal for:

- **Configuration Stores**: Application configuration management
- **Session Storage**: User session data across web servers
- **Caching**: Distributed caching with strong consistency
- **Service Discovery**: Registry of available services and endpoints
- **Feature Flags**: Centralized feature flag management
- **Metadata Storage**: Database metadata, schema information

## Advanced Features

### Change Notifications
```rust
use kvstore_smr::{KVStoreSMR, NotificationBus};

let mut kvstore = KVStoreSMR::new_with_notifications().await?;

// Subscribe to all changes
let subscription = kvstore.subscribe_to_changes().await;

// Subscribe to specific key patterns
let user_subscription = kvstore.subscribe_to_prefix("user:").await;

// Apply operations and receive notifications
kvstore.set("user:123", "john_doe").await?;
// Notification: KeySet { key: "user:123", value: "john_doe", old_value: None }
```

### Batch Operations
```rust
use kvstore_smr::KVOperation;

let batch_ops = vec![
    KVOperation::Set { key: "key1".to_string(), value: "value1".to_string() },
    KVOperation::Set { key: "key2".to_string(), value: "value2".to_string() },
    KVOperation::Delete { key: "old_key".to_string() },
];

let results = kvstore.apply_commands(batch_ops).await;
// All operations applied atomically across replicas
```

### State Snapshots and Recovery
```rust
// Create snapshot
let snapshot = kvstore.serialize_state();

// Restore from snapshot
let mut new_kvstore = KVStoreSMR::new().await?;
new_kvstore.deserialize_state(&snapshot)?;

// Verify restored state
assert_eq!(new_kvstore.size().await, original_size);
```

## Performance Characteristics

### Memory Usage
- Efficient HashMap storage with string interning
- Configurable memory limits and eviction policies
- Compressed snapshots for large datasets

### Throughput
- Optimized for high-frequency SET/GET operations
- Batch operation support for bulk updates
- Asynchronous notification delivery

### Consistency
- Strong consistency across all replicas
- Linearizable read operations
- Atomic batch operations

## Configuration Options

```rust
use kvstore_smr::KVStoreConfig;

let config = KVStoreConfig {
    max_entries: 1_000_000,           // Maximum number of keys
    max_memory_bytes: 1_024_000_000,  // 1GB memory limit
    enable_notifications: true,        // Enable change notifications
    notification_buffer_size: 1000,   // Notification queue size
    snapshot_compression: true,        // Compress snapshots
    key_expiration_enabled: false,    // TTL support (optional)
    metrics_enabled: true,            // Performance metrics
};

let kvstore = KVStoreSMR::new(config).await?;
```

## Implementation Notes

### Why This Works Well for SMR

1. **Deterministic Operations**: Hash map operations are deterministic and reproducible
2. **Efficient Serialization**: Binary serialization minimizes snapshot overhead
3. **Event-Driven Architecture**: Change notifications enable reactive applications
4. **Scalable State**: Can handle large numbers of keys with efficient memory usage

### SMR Considerations

1. **Memory Usage**: Large key-value stores require careful memory management
2. **Snapshot Frequency**: Balance between recovery time and performance overhead
3. **Notification Ordering**: Events are delivered in operation order for consistency
4. **Error Handling**: Robust error handling prevents state machine corruption

## Next Steps

After understanding the KV store example, explore:
- [Banking SMR](../banking_smr/) - Business logic with complex validation
- [Custom State Machine](../custom_state_machine.rs) - Template for your own SMR applications
- [Performance Benchmarks](../../benchmarks/) - SMR performance optimization techniques