//! KVStore integration tests
//!
//! These tests verify the key-value store functionality
//! and its integration with the consensus protocol.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use rabia_kvstore::{KVOperation, KVStore, KVStoreConfig};

/// Test basic KVStore operations
#[tokio::test]
async fn test_kvstore_basic_operations() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let config = KVStoreConfig {
        max_keys: 1000,
        enable_notifications: false,
        ..Default::default()
    };

    let store = KVStore::new(config)
        .await
        .expect("Failed to create KVStore");

    // Test SET operation
    let result = store.set("key1", "value1").await;
    assert!(result.is_ok(), "SET operation failed: {:?}", result.err());

    // Test GET operation
    let result = store.get("key1").await;
    assert!(result.is_ok(), "GET operation failed: {:?}", result.err());

    let value = result.unwrap();
    assert_eq!(
        value,
        Some("value1".to_string()),
        "Retrieved value doesn't match"
    );

    // Test GET non-existent key
    let result = store.get("nonexistent").await;
    assert!(
        result.is_ok(),
        "GET non-existent operation failed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), None, "Non-existent key should return None");

    // Test DELETE operation
    let result = store.delete("key1").await;
    assert!(
        result.is_ok(),
        "DELETE operation failed: {:?}",
        result.err()
    );

    // Verify key is deleted
    let result = store.get("key1").await;
    assert!(
        result.is_ok(),
        "GET after DELETE failed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), None, "Deleted key should return None");
}

/// Test KVStore batch operations
#[tokio::test]
async fn test_kvstore_batch_operations() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let config = KVStoreConfig {
        max_keys: 1000,
        enable_notifications: false,
        ..Default::default()
    };

    let store = KVStore::new(config)
        .await
        .expect("Failed to create KVStore");

    // Create batch operations
    let batch_ops = vec![
        KVOperation::Set {
            key: "batch_key1".to_string(),
            value: "batch_value1".to_string(),
        },
        KVOperation::Set {
            key: "batch_key2".to_string(),
            value: "batch_value2".to_string(),
        },
        KVOperation::Set {
            key: "batch_key3".to_string(),
            value: "batch_value3".to_string(),
        },
    ];

    // Apply batch
    let result = store.apply_batch(batch_ops).await;
    assert!(result.is_ok(), "Batch operation failed: {:?}", result.err());

    // Verify all keys were set
    for i in 1..=3 {
        let key = format!("batch_key{}", i);
        let expected_value = format!("batch_value{}", i);

        let result = store.get(&key).await;
        assert!(
            result.is_ok(),
            "GET batch key {} failed: {:?}",
            key,
            result.err()
        );
        assert_eq!(
            result.unwrap(),
            Some(expected_value),
            "Batch key {} value mismatch",
            key
        );
    }
}

/// Test KVStore with concurrent operations
#[tokio::test]
async fn test_kvstore_concurrent_operations() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let config = KVStoreConfig {
        max_keys: 1000,
        enable_notifications: false,
        ..Default::default()
    };

    let store = Arc::new(
        KVStore::new(config)
            .await
            .expect("Failed to create KVStore"),
    );

    let mut handles = Vec::new();

    // Spawn concurrent operations
    for i in 0..10 {
        let store = store.clone();
        let handle = tokio::spawn(async move {
            let key = format!("concurrent_key_{}", i);
            let value = format!("concurrent_value_{}", i);

            // SET operation
            let result = store.set(&key, &value).await;
            assert!(
                result.is_ok(),
                "Concurrent SET {} failed: {:?}",
                i,
                result.err()
            );

            // GET operation
            let result = store.get(&key).await;
            assert!(
                result.is_ok(),
                "Concurrent GET {} failed: {:?}",
                i,
                result.err()
            );
            assert_eq!(
                result.unwrap(),
                Some(value),
                "Concurrent operation {} value mismatch",
                i
            );
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let result = timeout(Duration::from_secs(5), handle).await;
        assert!(result.is_ok(), "Concurrent operation timed out");
        assert!(result.unwrap().is_ok(), "Concurrent operation failed");
    }

    // Verify all keys exist
    for i in 0..10 {
        let key = format!("concurrent_key_{}", i);
        let expected_value = format!("concurrent_value_{}", i);

        let result = store.get(&key).await;
        assert!(result.is_ok(), "Final GET {} failed: {:?}", i, result.err());
        assert_eq!(
            result.unwrap(),
            Some(expected_value),
            "Final value mismatch for key {}",
            i
        );
    }
}

/// Test KVStore performance under load
#[tokio::test]
async fn test_kvstore_performance() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let config = KVStoreConfig {
        max_keys: 10000,
        enable_notifications: false,
        ..Default::default()
    };

    let store = KVStore::new(config)
        .await
        .expect("Failed to create KVStore");

    let start_time = std::time::Instant::now();
    let operation_count = 1000;

    // Perform many operations
    for i in 0..operation_count {
        let key = format!("perf_key_{}", i);
        let value = format!("perf_value_{}", i);

        let result = store.set(&key, &value).await;
        assert!(
            result.is_ok(),
            "Performance SET {} failed: {:?}",
            i,
            result.err()
        );

        if i % 100 == 0 {
            // Occasional GET operations
            let result = store.get(&key).await;
            assert!(
                result.is_ok(),
                "Performance GET {} failed: {:?}",
                i,
                result.err()
            );
        }
    }

    let duration = start_time.elapsed();
    let ops_per_sec = operation_count as f64 / duration.as_secs_f64();

    println!(
        "KVStore performance: {} operations in {:?} ({:.2} ops/sec)",
        operation_count, duration, ops_per_sec
    );

    // Reasonable performance expectation (this may need adjustment based on hardware)
    assert!(
        ops_per_sec > 100.0,
        "Performance too low: {:.2} ops/sec",
        ops_per_sec
    );
}

/// Test KVStore error handling
#[tokio::test]
async fn test_kvstore_error_handling() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let config = KVStoreConfig {
        max_keys: 2, // Very small limit to test capacity limits
        enable_notifications: false,
        ..Default::default()
    };

    let store = KVStore::new(config)
        .await
        .expect("Failed to create KVStore");

    // Fill up the store
    for i in 0..2 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        let result = store.set(&key, &value).await;
        assert!(
            result.is_ok(),
            "SET {} within capacity failed: {:?}",
            i,
            result.err()
        );
    }

    // Try to exceed capacity
    let result = store.set("overflow_key", "overflow_value").await;
    // Note: Depending on implementation, this might succeed or fail
    // For this test, we just verify it doesn't panic
    println!("Overflow operation result: {:?}", result);

    // Test operations with empty keys (if they should fail)
    let result = store.set("", "empty_key_value").await;
    println!("Empty key operation result: {:?}", result);

    // Test operations with very long keys
    let long_key = "x".repeat(1000);
    let result = store.set(&long_key, "long_key_value").await;
    println!("Long key operation result: {:?}", result);
}

/// Test KVStore with mixed operation types
#[tokio::test]
async fn test_kvstore_mixed_operations() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let config = KVStoreConfig {
        max_keys: 1000,
        enable_notifications: false,
        ..Default::default()
    };

    let store = KVStore::new(config)
        .await
        .expect("Failed to create KVStore");

    // Set initial values
    for i in 0..10 {
        let key = format!("mixed_key_{}", i);
        let value = format!("initial_value_{}", i);
        let result = store.set(&key, &value).await;
        assert!(
            result.is_ok(),
            "Initial SET {} failed: {:?}",
            i,
            result.err()
        );
    }

    // Mix of operations: update some, delete some, read some
    for i in 0..10 {
        match i % 3 {
            0 => {
                // Update
                let key = format!("mixed_key_{}", i);
                let value = format!("updated_value_{}", i);
                let result = store.set(&key, &value).await;
                assert!(
                    result.is_ok(),
                    "Update SET {} failed: {:?}",
                    i,
                    result.err()
                );
            }
            1 => {
                // Delete
                let key = format!("mixed_key_{}", i);
                let result = store.delete(&key).await;
                assert!(result.is_ok(), "DELETE {} failed: {:?}", i, result.err());
            }
            2 => {
                // Read
                let key = format!("mixed_key_{}", i);
                let result = store.get(&key).await;
                assert!(result.is_ok(), "GET {} failed: {:?}", i, result.err());
                // Value should still be the initial value
                assert_eq!(result.unwrap(), Some(format!("initial_value_{}", i)));
            }
            _ => unreachable!(),
        }
    }

    // Verify final state
    for i in 0..10 {
        let key = format!("mixed_key_{}", i);
        let result = store.get(&key).await;
        assert!(result.is_ok(), "Final GET {} failed: {:?}", i, result.err());

        match i % 3 {
            0 => {
                // Should be updated value
                assert_eq!(result.unwrap(), Some(format!("updated_value_{}", i)));
            }
            1 => {
                // Should be deleted (None)
                assert_eq!(result.unwrap(), None);
            }
            2 => {
                // Should be original value
                assert_eq!(result.unwrap(), Some(format!("initial_value_{}", i)));
            }
            _ => unreachable!(),
        }
    }
}
