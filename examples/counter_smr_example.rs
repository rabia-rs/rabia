//! # Counter SMR Example
//!
//! Demonstrates how to use the Counter SMR implementation with Rabia consensus.

use rabia_counter_example::{CounterCommand, CounterSMR};
use rabia_core::smr::StateMachine;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Counter SMR Example");

    // Create a new Counter SMR instance
    let mut counter = CounterSMR::new();

    info!(
        "Counter SMR created with initial value: {}",
        counter.value()
    );

    // Demonstrate basic operations
    let commands = vec![
        CounterCommand::Increment(10),
        CounterCommand::Increment(5),
        CounterCommand::Decrement(3),
        CounterCommand::Get,
        CounterCommand::Set(100),
        CounterCommand::Get,
        CounterCommand::Increment(25),
        CounterCommand::Reset,
        CounterCommand::Get,
    ];

    info!("Applying {} commands to Counter", commands.len());

    // Apply commands individually to show responses
    for (i, command) in commands.into_iter().enumerate() {
        let response = counter.apply_command(command.clone()).await;
        info!(
            "Command {}: {:?} -> Response: value={}, success={}",
            i + 1,
            command,
            response.value,
            response.success
        );

        if let Some(message) = &response.message {
            info!("  Message: {}", message);
        }
    }

    // Demonstrate overflow/underflow handling
    info!("Testing overflow and underflow handling...");

    let mut overflow_counter = CounterSMR::with_value(i64::MAX);
    let overflow_response = overflow_counter
        .apply_command(CounterCommand::Increment(1))
        .await;
    info!(
        "Overflow test: {:?} -> success={}, message={:?}",
        CounterCommand::Increment(1),
        overflow_response.success,
        overflow_response.message
    );

    let mut underflow_counter = CounterSMR::with_value(i64::MIN);
    let underflow_response = underflow_counter
        .apply_command(CounterCommand::Decrement(1))
        .await;
    info!(
        "Underflow test: {:?} -> success={}, message={:?}",
        CounterCommand::Decrement(1),
        underflow_response.success,
        underflow_response.message
    );

    // Demonstrate state serialization and restoration
    info!("Testing state serialization...");

    // Apply some operations to the counter
    counter.apply_command(CounterCommand::Set(42)).await;
    counter.apply_command(CounterCommand::Increment(8)).await;

    let serialized_state = counter.serialize_state();
    info!("State serialized to {} bytes", serialized_state.len());

    // Create a new instance and restore state
    let mut new_counter = CounterSMR::new();
    new_counter.deserialize_state(&serialized_state)?;
    info!("State restored to new Counter instance");

    // Verify state was restored correctly
    let state = new_counter.get_state();
    info!(
        "Restored state: value={}, operations={}",
        state.value, state.operation_count
    );

    let get_response = new_counter.apply_command(CounterCommand::Get).await;
    info!("Verification - Get: {:?}", get_response);

    // Test batch operations
    info!("Testing batch operations...");
    let batch_commands = vec![
        CounterCommand::Increment(5),
        CounterCommand::Increment(10),
        CounterCommand::Decrement(3),
        CounterCommand::Get,
        CounterCommand::Set(200),
    ];

    let batch_responses = new_counter.apply_commands(batch_commands).await;
    info!("Batch operation results:");
    for (i, response) in batch_responses.iter().enumerate() {
        info!(
            "  Response {}: value={}, success={}",
            i + 1,
            response.value,
            response.success
        );
    }

    // Show final state
    let final_state = new_counter.get_state();
    info!(
        "Final state: value={}, total_operations={}",
        final_state.value, final_state.operation_count
    );

    info!("Counter SMR Example completed successfully!");

    Ok(())
}
