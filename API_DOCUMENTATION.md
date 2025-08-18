# Rabia API Documentation

This document provides comprehensive API documentation for the Rabia consensus protocol implementation in Rust.

## Table of Contents

1. [Core Types](#core-types)
2. [Error Handling](#error-handling)
3. [KVStore API](#kvstore-api)
4. [Notifications System](#notifications-system)
5. [Consensus Components](#consensus-components)
6. [Examples](#examples)

---

## Core Types

The `rabia-core` crate provides fundamental types used throughout the system:

### NodeId

```rust
use rabia_core::NodeId;

let node_id = NodeId::new();
println!("Node ID: {}", node_id);
```

- **Purpose**: Unique identifier for nodes in the consensus cluster
- **Generated**: Automatically using UUID v4
- **Used for**: Message routing, leader election, membership management

### PhaseId

```rust
use rabia_core::PhaseId;

let phase1 = PhaseId::new(1);
let phase2 = phase1.next();
assert!(phase2 > phase1);
```

- **Purpose**: Identifies consensus phases in the Rabia protocol
- **Monotonic**: Phase IDs are ordered and increasing
- **Used for**: Consensus round ordering and state transitions

### Command and CommandBatch

```rust
use rabia_core::{Command, CommandBatch};

// Single command
let cmd = Command::new("SET key value");

// Batch of commands
let batch = CommandBatch::new(vec![
    Command::new("SET key1 value1"),
    Command::new("GET key1"),
]);
```

- **Command**: Individual operation with unique ID and data
- **CommandBatch**: Group of commands for improved throughput
- **Features**: Automatic timestamping, checksum calculation

### StateValue

```rust
use rabia_core::StateValue;

let vote = StateValue::V1; // Accept
let reject = StateValue::V0; // Reject
let undecided = StateValue::VQuestion; // Randomization
```

- **Purpose**: Vote values in consensus protocol
- **Values**: V0 (reject), V1 (accept), VQuestion (undecided)
- **Used for**: Consensus voting and randomization

---

## Error Handling

Comprehensive error types with recovery information:

### RabiaError

```rust
use rabia_core::{RabiaError, Result};

// Create errors
let network_err = RabiaError::network("Connection timeout");
let storage_err = RabiaError::persistence("Disk full");

// Check if retryable
if network_err.is_retryable() {
    println!("Can retry this operation");
}

// Function returning Result
fn consensus_operation() -> Result<String> {
    Ok("Success".to_string())
}
```

#### Error Categories

- **Network Errors**: Communication failures (retryable)
- **Persistence Errors**: Storage failures
- **State Machine Errors**: Application-level failures
- **Consensus Errors**: Protocol violations
- **Resource Errors**: Missing nodes/phases/batches
- **Integrity Errors**: Checksum mismatches, corruption
- **Timeout Errors**: Operation timeouts (retryable)

#### Error Properties

- `is_retryable()`: Whether the error might resolve with retry
- Context information for debugging
- Structured error variants with specific data

---

## KVStore API

Production-grade key-value store with consensus integration:

### Basic Operations

```rust
use rabia_kvstore::{KVStore, KVStoreConfig};

// Create store
let config = KVStoreConfig::default();
let store = KVStore::new(config).await?;

// SET operation
store.set("key1", "value1").await?;

// GET operation
let value = store.get("key1").await?;
assert_eq!(value.unwrap(), "value1");

// DELETE operation
store.delete("key1").await?;

// EXISTS check
let exists = store.exists("key1").await?;
assert!(!exists);
```

### Metadata Operations

```rust
// Get value with metadata
let entry = store.get_with_metadata("key1").await?;
if let Some(entry) = entry {
    println!("Version: {}", entry.version);
    println!("Created: {}", entry.created_at);
    println!("Size: {} bytes", entry.size);
}

// List keys with prefix
let user_keys = store.keys(Some("user:")).await?;
println!("User keys: {:?}", user_keys);

// Get store statistics
let stats = store.get_stats().await;
println!("Total keys: {}", stats.total_keys);
println!("Memory usage: {} bytes", stats.memory_usage_bytes);
```

### Batch Operations

```rust
use rabia_kvstore::KVOperation;

let operations = vec![
    KVOperation::Set { 
        key: "key1".to_string(), 
        value: "value1".to_string() 
    },
    KVOperation::Get { 
        key: "key2".to_string() 
    },
    KVOperation::Delete { 
        key: "key3".to_string() 
    },
];

let results = store.apply_batch(operations).await?;
println!("Processed {} operations", results.len());
```

### Snapshots

```rust
// Create snapshot
let snapshot = store.create_snapshot().await?;
println!("Snapshot version: {}", snapshot.version);
println!("Keys: {}", snapshot.data.len());

// Restore from snapshot
store.restore_snapshot(snapshot).await?;
```

### Configuration

```rust
let config = KVStoreConfig {
    max_keys: 1_000_000,
    enable_notifications: true,
    snapshot_frequency: 10_000,
    max_value_size: 1024 * 1024, // 1MB
    enable_compression: false,
};
```

---

## Notifications System

Event-driven change notifications with filtering:

### Basic Subscription

```rust
use rabia_kvstore::notifications::{ChangeType, NotificationFilter};

let notification_bus = store.notification_bus();

// Subscribe to all changes
let mut all_changes = notification_bus.subscribe_all();

// Subscribe to specific key
let mut key_changes = notification_bus.subscribe_key("user:alice");

// Subscribe to prefix
let mut user_changes = notification_bus.subscribe_prefix("user:");

// Subscribe to specific change type
let mut deletions = notification_bus.subscribe_change_type(ChangeType::Deleted);
```

### Advanced Filtering

```rust
// Complex filters with AND/OR logic
let complex_filter = NotificationFilter::And(vec![
    NotificationFilter::KeyPrefix("user:".to_string()),
    NotificationFilter::ChangeType(ChangeType::Updated),
]);

let subscription = notification_bus.subscribe(complex_filter);
```

### Handling Notifications

```rust
// Listen for notifications
tokio::spawn(async move {
    while let Some(notification) = subscription.receiver.recv().await {
        println!("Change: {} {} -> {:?}", 
                 notification.change_type,
                 notification.key,
                 notification.new_value);
    }
});
```

### Notification Properties

```rust
pub struct ChangeNotification {
    pub key: String,
    pub change_type: ChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub version: u64,
    pub timestamp: u64,
}
```

### Statistics

```rust
let stats = notification_bus.get_stats();
println!("Notifications sent: {}", stats.total_notifications_sent);
println!("Active subscribers: {}", stats.total_subscribers);
println!("Dropped notifications: {}", stats.dropped_notifications);
```

---

## Consensus Components

### State Machine Interface

```rust
use rabia_core::state_machine::{StateMachine, InMemoryStateMachine};

let mut state_machine = InMemoryStateMachine::new();

// Apply command
let command = Command::new("SET key value");
let result = state_machine.apply_command(&command).await?;
```

### Network Layer

```rust
use rabia_core::network::ClusterConfig;
use rabia_network::InMemoryNetwork;

let cluster_config = ClusterConfig::new(node_id, node_set);
let network = InMemoryNetwork::new(node_id);
```

### Persistence Layer

```rust
use rabia_persistence::InMemoryPersistence;

let persistence = InMemoryPersistence::new();
```

---

## Examples

### Running Examples

The project includes comprehensive examples:

```bash
# Basic usage
cargo run --bin basic_usage

# KVStore demonstration
cargo run --bin kvstore_usage

# Multi-node consensus cluster
cargo run --bin consensus_cluster

# Performance benchmarking
cargo run --bin performance_benchmark
```

### KVStore Example Highlights

```rust
// High-performance operations
for i in 0..10_000 {
    store.set(&format!("key{}", i), "value").await?;
}

// Notification handling
let mut subscription = bus.subscribe_prefix("user:");
while let Some(notification) = subscription.receiver.recv().await {
    println!("User change: {}", notification.key);
}

// Batch processing
let operations = (0..1000)
    .map(|i| KVOperation::Set {
        key: format!("batch_key{}", i),
        value: "batch_value".to_string(),
    })
    .collect();
store.apply_batch(operations).await?;
```

### Performance Characteristics

- **High Throughput**: Optimized for concurrent batch processing
- **Low Latency**: Microsecond-scale operation processing
- **Concurrency**: Thread-safe operations with DashMap
- **Memory**: Efficient pooling and cleanup
- **Serialization**: Compact binary format for reduced overhead

---

## Best Practices

### Error Handling

```rust
match store.set("key", "value").await {
    Ok(result) => {
        if result.is_success() {
            println!("Operation successful");
        }
    }
    Err(e) => {
        if e.is_retryable() {
            // Retry logic
            tokio::time::sleep(Duration::from_millis(100)).await;
            // retry...
        } else {
            // Handle permanent failure
            eprintln!("Permanent error: {}", e);
        }
    }
}
```

### Resource Management

```rust
// Always clean up subscriptions
let subscription_id = subscription.id;
// When done:
notification_bus.unsubscribe(subscription_id);

// Periodic cleanup
notification_bus.cleanup_closed_subscribers();

// Graceful shutdown
store.shutdown().await?;
```

### Performance Optimization

```rust
// Use batching for bulk operations
let batch_size = 100;
for chunk in operations.chunks(batch_size) {
    store.apply_batch(chunk.to_vec()).await?;
}

// Configure appropriately
let config = KVStoreConfig {
    max_keys: expected_key_count * 2, // Allow growth
    enable_notifications: false, // Disable if not needed
    snapshot_frequency: 10_000, // Tune for workload
    ..Default::default()
};
```

---

## Documentation Generation

Generate complete API documentation:

```bash
# Generate docs for all crates
cargo doc --workspace --no-deps

# Open in browser
cargo doc --workspace --no-deps --open

# Generate docs for specific crate
cargo doc -p rabia-core --no-deps
cargo doc -p rabia-kvstore --no-deps
```

The generated documentation includes:

- Complete API reference
- Code examples
- Usage patterns
- Performance characteristics
- Error handling guides

---

## Additional Resources

- **Repository**: https://github.com/rabia-rs/rabia
- **Documentation**: https://docs.rs/rabia
- **Examples**: `examples/` directory
- **Benchmarks**: `benchmarks/` directory
- **License**: Apache 2.0

For questions or contributions, please refer to the project repository and contribution guidelines.