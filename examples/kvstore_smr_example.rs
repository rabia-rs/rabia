//! # KVStore SMR Example
//!
//! Demonstrates how to use the KVStore SMR implementation with Rabia consensus.

use kvstore_smr::{KVOperation, KVStoreSMR};
use rabia_core::smr::StateMachine;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting KVStore SMR Example");

    // Create a new KVStore SMR instance
    let mut kvstore = KVStoreSMR::new_default().await?;

    info!("KVStore SMR created successfully");

    // Demonstrate basic operations
    let commands = vec![
        KVOperation::Set {
            key: "user:1:name".to_string(),
            value: "Alice".to_string(),
        },
        KVOperation::Set {
            key: "user:1:email".to_string(),
            value: "alice@example.com".to_string(),
        },
        KVOperation::Set {
            key: "user:2:name".to_string(),
            value: "Bob".to_string(),
        },
        KVOperation::Get {
            key: "user:1:name".to_string(),
        },
        KVOperation::Exists {
            key: "user:2:name".to_string(),
        },
        KVOperation::Delete {
            key: "user:1:email".to_string(),
        },
        KVOperation::Get {
            key: "user:1:email".to_string(),
        },
    ];

    info!("Applying {} commands to KVStore", commands.len());

    // Apply commands individually to show responses
    for (i, command) in commands.into_iter().enumerate() {
        let response = kvstore.apply_command(command.clone()).await;
        info!(
            "Command {}: {:?} -> Response: {:?}",
            i + 1,
            command,
            response
        );
    }

    // Demonstrate state serialization and restoration
    info!("Testing state serialization...");
    let serialized_state = kvstore.serialize_state();
    info!("State serialized to {} bytes", serialized_state.len());

    // Create a new instance and restore state
    let mut new_kvstore = KVStoreSMR::new_default().await?;
    new_kvstore.deserialize_state(&serialized_state)?;
    info!("State restored to new KVStore instance");

    // Verify state was restored correctly
    let get_response = new_kvstore
        .apply_command(KVOperation::Get {
            key: "user:1:name".to_string(),
        })
        .await;
    info!("Verification - Get user:1:name: {:?}", get_response);

    let get_response = new_kvstore
        .apply_command(KVOperation::Get {
            key: "user:2:name".to_string(),
        })
        .await;
    info!("Verification - Get user:2:name: {:?}", get_response);

    // Test batch operations
    info!("Testing batch operations...");
    let batch_commands = vec![
        KVOperation::Set {
            key: "config:timeout".to_string(),
            value: "30s".to_string(),
        },
        KVOperation::Set {
            key: "config:retries".to_string(),
            value: "3".to_string(),
        },
        KVOperation::Set {
            key: "config:debug".to_string(),
            value: "true".to_string(),
        },
    ];

    let batch_responses = new_kvstore.apply_commands(batch_commands).await;
    info!("Batch operation results: {:?}", batch_responses);

    info!("KVStore SMR Example completed successfully!");

    Ok(())
}
