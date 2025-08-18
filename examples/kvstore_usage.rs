//! # KVStore Usage Example
//!
//! This example demonstrates comprehensive usage of the Rabia KVStore,
//! including basic operations, batch processing, notifications, and snapshots.

use rabia_kvstore::{
    notifications::{ChangeType, NotificationFilter},
    KVOperation, KVStore, KVStoreConfig,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🏪 Rabia KVStore Usage Example");
    println!("================================\n");

    // 1. Create and configure the KVStore
    let config = KVStoreConfig {
        max_keys: 1000,
        enable_notifications: true,
        snapshot_frequency: 100,
        enable_compression: false,
        max_value_size: 1024 * 1024, // 1MB
    };

    let store = KVStore::new(config).await?;
    println!("✅ KVStore created with notifications enabled\n");

    // 2. Set up notification listeners
    let notification_bus = store.notification_bus();

    // Listen for all changes
    let mut all_changes = notification_bus.subscribe_all();

    // Listen for changes to keys with "user:" prefix
    let mut user_changes = notification_bus.subscribe_prefix("user:");

    // Listen only for deletions
    let mut deletion_changes = notification_bus.subscribe_change_type(ChangeType::Deleted);

    // Start background task to handle notifications
    tokio::spawn(async move {
        println!("🔔 Notification listeners started");

        // Handle all changes
        tokio::spawn(async move {
            while let Some(notification) = all_changes.receiver.recv().await {
                println!(
                    "📢 All changes: {} {} -> {:?}",
                    notification.change_type, notification.key, notification.new_value
                );
            }
        });

        // Handle user prefix changes
        tokio::spawn(async move {
            while let Some(notification) = user_changes.receiver.recv().await {
                println!(
                    "👤 User change: {} {}",
                    notification.change_type, notification.key
                );
            }
        });

        // Handle deletions
        tokio::spawn(async move {
            while let Some(notification) = deletion_changes.receiver.recv().await {
                println!(
                    "🗑️  Deletion: {} (was: {:?})",
                    notification.key, notification.old_value
                );
            }
        });
    });

    // Give notification listeners time to start
    sleep(Duration::from_millis(100)).await;

    // 3. Basic operations
    println!("🔧 Basic Operations:");
    println!("-------------------");

    // SET operations
    store.set("app:name", "Rabia Consensus").await?;
    store.set("app:version", "1.0.0").await?;
    store.set("user:alice", "Alice Smith").await?;
    store.set("user:bob", "Bob Johnson").await?;

    // GET operations
    if let Some(name) = store.get("app:name").await? {
        println!("✅ App name: {}", name);
    }

    // EXISTS operation
    let exists = store.exists("user:alice").await?;
    println!("✅ User alice exists: {}", exists);

    // List keys with prefix
    let user_keys = store.keys(Some("user:")).await?;
    println!("✅ User keys: {:?}", user_keys);

    sleep(Duration::from_millis(200)).await;

    // 4. Batch operations
    println!("\n📦 Batch Operations:");
    println!("--------------------");

    let batch_ops = vec![
        KVOperation::Set {
            key: "session:123".to_string(),
            value: "active".to_string(),
        },
        KVOperation::Set {
            key: "session:456".to_string(),
            value: "inactive".to_string(),
        },
        KVOperation::Get {
            key: "user:alice".to_string(),
        },
        KVOperation::Delete {
            key: "user:bob".to_string(),
        },
    ];

    let results = store.apply_batch(batch_ops).await?;
    println!("✅ Batch processed: {} operations completed", results.len());

    sleep(Duration::from_millis(200)).await;

    // 5. Metadata operations
    println!("\n📊 Metadata and Statistics:");
    println!("---------------------------");

    // Get value with metadata
    if let Some(entry) = store.get_with_metadata("app:name").await? {
        println!("✅ app:name metadata:");
        println!("   - Version: {}", entry.version);
        println!("   - Created: {}", entry.created_at);
        println!("   - Updated: {}", entry.updated_at);
        println!("   - Size: {} bytes", entry.size);
    }

    // Get store statistics
    let stats = store.get_stats().await;
    println!("✅ Store statistics:");
    println!("   - Total keys: {}", stats.total_keys);
    println!("   - Total operations: {}", stats.total_operations);
    println!("   - Memory usage: {} bytes", stats.memory_usage_bytes);

    // Get notification statistics
    let notification_stats = notification_bus.get_stats();
    println!("✅ Notification statistics:");
    println!(
        "   - Total notifications sent: {}",
        notification_stats.total_notifications_sent
    );
    println!(
        "   - Active subscribers: {}",
        notification_stats.total_subscribers
    );
    println!(
        "   - Dropped notifications: {}",
        notification_stats.dropped_notifications
    );

    sleep(Duration::from_millis(200)).await;

    // 6. Snapshot operations
    println!("\n📸 Snapshot Operations:");
    println!("----------------------");

    // Create a snapshot
    let snapshot = store.create_snapshot().await?;
    println!("✅ Snapshot created:");
    println!("   - Version: {}", snapshot.version);
    println!("   - Keys: {}", snapshot.data.len());
    println!("   - Checksum: {}", snapshot.checksum);

    // Modify some data
    store.set("temp:data", "temporary").await?;
    store.delete("session:456").await?;
    println!("✅ Modified store after snapshot");

    let current_size = store.size().await;
    println!("   - Current size: {}", current_size);

    // Restore from snapshot
    store.restore_snapshot(snapshot).await?;
    println!("✅ Restored from snapshot");

    let restored_size = store.size().await;
    println!("   - Restored size: {}", restored_size);

    sleep(Duration::from_millis(200)).await;

    // 7. Advanced filtering example
    println!("\n🔍 Advanced Notification Filtering:");
    println!("-----------------------------------");

    // Create a complex filter: user prefix AND update operations
    let complex_filter = NotificationFilter::And(vec![
        NotificationFilter::KeyPrefix("user:".to_string()),
        NotificationFilter::ChangeType(ChangeType::Updated),
    ]);

    let mut filtered_subscription = notification_bus.subscribe(complex_filter);

    tokio::spawn(async move {
        while let Some(notification) = filtered_subscription.receiver.recv().await {
            println!(
                "🎯 Filtered notification: {} {}",
                notification.change_type, notification.key
            );
        }
    });

    // Trigger various operations to test filtering
    store.set("user:charlie", "Charlie Brown").await?; // Won't match (Create, not Update)
    store.set("user:alice", "Alice Johnson").await?; // Will match (Update + user: prefix)
    store.set("app:debug", "true").await?; // Won't match (wrong prefix)

    sleep(Duration::from_millis(300)).await;

    // 8. Error handling example
    println!("\n⚠️  Error Handling:");
    println!("-------------------");

    // Try to set a value that's too large
    let large_value = "x".repeat(2 * 1024 * 1024); // 2MB
    match store.set("large_key", &large_value).await {
        Ok(_) => println!("❌ Large value should have failed"),
        Err(e) => println!("✅ Expected error for large value: {}", e),
    }

    // Try to use invalid key
    match store.set("", "empty_key").await {
        Ok(_) => println!("❌ Empty key should have failed"),
        Err(e) => println!("✅ Expected error for empty key: {}", e),
    }

    sleep(Duration::from_millis(200)).await;

    // 9. Performance demonstration
    println!("\n🚀 Performance Demonstration:");
    println!("-----------------------------");

    let start = std::time::Instant::now();

    // Perform 100 operations
    for i in 0..100 {
        store
            .set(&format!("perf:key{}", i), &format!("value{}", i))
            .await?;
    }

    let duration = start.elapsed();
    println!("✅ 100 SET operations completed in {:?}", duration);
    println!("   - Average: {:?} per operation", duration / 100);

    // Clean up performance test data
    for i in 0..100 {
        store.delete(&format!("perf:key{}", i)).await?;
    }

    sleep(Duration::from_millis(200)).await;

    // 10. Final statistics
    let final_stats = store.get_stats().await;
    let final_notification_stats = notification_bus.get_stats();

    println!("\n📈 Final Statistics:");
    println!("--------------------");
    println!("✅ Store operations: {}", final_stats.total_operations);
    println!("✅ Final key count: {}", final_stats.total_keys);
    println!(
        "✅ Notifications sent: {}",
        final_notification_stats.total_notifications_sent
    );

    // Graceful shutdown
    println!("\n🛑 Shutting down...");
    store.shutdown().await?;
    println!("✅ KVStore shut down successfully");

    Ok(())
}
