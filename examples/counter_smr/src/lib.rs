//! # Counter SMR Example
//!
//! A simple counter implementation that demonstrates how to create a State Machine
//! Replication (SMR) application using the Rabia consensus protocol.
//!
//! This example shows the minimal implementation needed to create an SMR application:
//! - Command types for operations
//! - Response types for results
//! - State type for the machine state
//! - StateMachine trait implementation
//!
//! ## Example Usage
//!
//! ```rust
//! use counter_smr::{CounterSMR, CounterCommand};
//! use rabia_core::smr::StateMachine;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut counter = CounterSMR::new();
//!     
//!     let command = CounterCommand::Increment(5);
//!     let response = counter.apply_command(command).await;
//!     println!("Counter value: {}", response.value);
//!     
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use rabia_core::smr::StateMachine;
use serde::{Deserialize, Serialize};

/// Commands that can be applied to the counter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CounterCommand {
    /// Increment the counter by the given value
    Increment(i64),
    /// Decrement the counter by the given value
    Decrement(i64),
    /// Set the counter to a specific value
    Set(i64),
    /// Get the current value (read-only operation)
    Get,
    /// Reset the counter to zero
    Reset,
}

/// Response from counter operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CounterResponse {
    /// The current value after the operation
    pub value: i64,
    /// Whether the operation was successful
    pub success: bool,
    /// Optional message (e.g., for errors)
    pub message: Option<String>,
}

impl CounterResponse {
    pub fn success(value: i64) -> Self {
        Self {
            value,
            success: true,
            message: None,
        }
    }

    pub fn error(value: i64, message: String) -> Self {
        Self {
            value,
            success: false,
            message: Some(message),
        }
    }
}

/// State of the counter state machine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CounterState {
    /// The current counter value
    pub value: i64,
    /// Total number of operations performed
    pub operation_count: u64,
}

/// Simple counter state machine implementation
#[derive(Debug, Clone)]
pub struct CounterSMR {
    state: CounterState,
}

impl CounterSMR {
    /// Create a new counter state machine
    pub fn new() -> Self {
        Self {
            state: CounterState::default(),
        }
    }

    /// Create a new counter with an initial value
    pub fn with_value(initial_value: i64) -> Self {
        Self {
            state: CounterState {
                value: initial_value,
                operation_count: 0,
            },
        }
    }

    /// Get the current counter value
    pub fn value(&self) -> i64 {
        self.state.value
    }

    /// Get the total number of operations performed
    pub fn operation_count(&self) -> u64 {
        self.state.operation_count
    }
}

impl Default for CounterSMR {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateMachine for CounterSMR {
    type Command = CounterCommand;
    type Response = CounterResponse;
    type State = CounterState;

    async fn apply_command(&mut self, command: Self::Command) -> Self::Response {
        self.state.operation_count += 1;

        match command {
            CounterCommand::Increment(value) => {
                // Check for overflow
                match self.state.value.checked_add(value) {
                    Some(new_value) => {
                        self.state.value = new_value;
                        CounterResponse::success(self.state.value)
                    }
                    None => CounterResponse::error(
                        self.state.value,
                        "Overflow: cannot increment counter".to_string(),
                    ),
                }
            }
            CounterCommand::Decrement(value) => {
                // Check for underflow
                match self.state.value.checked_sub(value) {
                    Some(new_value) => {
                        self.state.value = new_value;
                        CounterResponse::success(self.state.value)
                    }
                    None => CounterResponse::error(
                        self.state.value,
                        "Underflow: cannot decrement counter".to_string(),
                    ),
                }
            }
            CounterCommand::Set(value) => {
                self.state.value = value;
                CounterResponse::success(self.state.value)
            }
            CounterCommand::Get => {
                // Read-only operation, don't change state
                CounterResponse::success(self.state.value)
            }
            CounterCommand::Reset => {
                self.state.value = 0;
                CounterResponse::success(self.state.value)
            }
        }
    }

    fn get_state(&self) -> Self::State {
        self.state.clone()
    }

    fn set_state(&mut self, state: Self::State) {
        self.state = state;
    }

    fn serialize_state(&self) -> Vec<u8> {
        bincode::serialize(&self.state).unwrap_or_default()
    }

    fn deserialize_state(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.state = bincode::deserialize(data)?;
        Ok(())
    }

    async fn apply_commands(&mut self, commands: Vec<Self::Command>) -> Vec<Self::Response> {
        let mut responses = Vec::with_capacity(commands.len());
        for command in commands {
            responses.push(self.apply_command(command).await);
        }
        responses
    }

    fn is_deterministic(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counter_basic_operations() {
        let mut counter = CounterSMR::new();

        // Test increment
        let response = counter.apply_command(CounterCommand::Increment(5)).await;
        assert!(response.success);
        assert_eq!(response.value, 5);
        assert_eq!(counter.value(), 5);

        // Test decrement
        let response = counter.apply_command(CounterCommand::Decrement(2)).await;
        assert!(response.success);
        assert_eq!(response.value, 3);
        assert_eq!(counter.value(), 3);

        // Test set
        let response = counter.apply_command(CounterCommand::Set(10)).await;
        assert!(response.success);
        assert_eq!(response.value, 10);
        assert_eq!(counter.value(), 10);

        // Test get
        let response = counter.apply_command(CounterCommand::Get).await;
        assert!(response.success);
        assert_eq!(response.value, 10);

        // Test reset
        let response = counter.apply_command(CounterCommand::Reset).await;
        assert!(response.success);
        assert_eq!(response.value, 0);
        assert_eq!(counter.value(), 0);
    }

    #[tokio::test]
    async fn test_counter_overflow_underflow() {
        let mut counter = CounterSMR::with_value(i64::MAX);

        // Test overflow
        let response = counter.apply_command(CounterCommand::Increment(1)).await;
        assert!(!response.success);
        assert_eq!(response.value, i64::MAX);
        assert!(response.message.as_ref().unwrap().contains("Overflow"));

        // Reset to minimum value
        counter = CounterSMR::with_value(i64::MIN);

        // Test underflow
        let response = counter.apply_command(CounterCommand::Decrement(1)).await;
        assert!(!response.success);
        assert_eq!(response.value, i64::MIN);
        assert!(response.message.as_ref().unwrap().contains("Underflow"));
    }

    #[tokio::test]
    async fn test_counter_state_serialization() {
        let mut counter = CounterSMR::new();

        // Apply some operations
        counter.apply_command(CounterCommand::Increment(42)).await;
        counter.apply_command(CounterCommand::Decrement(10)).await;

        // Serialize state
        let serialized = counter.serialize_state();
        assert!(!serialized.is_empty());

        // Create new counter and deserialize
        let mut new_counter = CounterSMR::new();
        new_counter.deserialize_state(&serialized).unwrap();

        // Verify state was restored
        assert_eq!(new_counter.value(), 32);
        assert_eq!(new_counter.operation_count(), 2);
        assert_eq!(new_counter.get_state(), counter.get_state());
    }

    #[tokio::test]
    async fn test_counter_multiple_commands() {
        let mut counter = CounterSMR::new();

        let commands = vec![
            CounterCommand::Increment(10),
            CounterCommand::Increment(5),
            CounterCommand::Decrement(3),
            CounterCommand::Set(100),
            CounterCommand::Get,
        ];

        let responses = counter.apply_commands(commands).await;
        assert_eq!(responses.len(), 5);

        // All operations should succeed
        assert!(responses.iter().all(|r| r.success));

        // Check final value
        assert_eq!(counter.value(), 100);
        assert_eq!(counter.operation_count(), 5);

        // Check individual responses
        assert_eq!(responses[0].value, 10); // Increment 10
        assert_eq!(responses[1].value, 15); // Increment 5
        assert_eq!(responses[2].value, 12); // Decrement 3
        assert_eq!(responses[3].value, 100); // Set 100
        assert_eq!(responses[4].value, 100); // Get
    }

    #[test]
    fn test_counter_deterministic() {
        let counter = CounterSMR::new();
        assert!(counter.is_deterministic());
    }
}
