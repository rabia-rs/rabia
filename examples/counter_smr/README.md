# Counter SMR Example

This example demonstrates how to build a simple distributed counter using State Machine Replication (SMR) with the Rabia consensus protocol.

## What This Example Shows

The Counter SMR demonstrates the fundamental SMR concepts:

1. **Deterministic State Machine**: All operations on the counter produce predictable, reproducible results
2. **Operation Ordering**: Rabia ensures all replicas apply increment/decrement operations in the same order
3. **Fault Tolerance**: The counter remains available as long as a majority of replicas are healthy
4. **Strong Consistency**: All replicas maintain identical counter values

## State Machine Implementation

The counter implements these operations:

- `Increment(value)` - Add a value to the counter
- `Decrement(value)` - Subtract a value from the counter  
- `Set(value)` - Set counter to a specific value
- `Get` - Read the current counter value
- `Reset` - Reset counter to zero

All operations include overflow/underflow protection and operation counting.

## Key SMR Features Demonstrated

### Deterministic Operations
```rust
match command {
    CounterCommand::Increment(value) => {
        // Deterministic: same input always produces same output
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
    // ... other operations
}
```

### State Serialization for Snapshots
```rust
fn serialize_state(&self) -> Vec<u8> {
    bincode::serialize(&self.state).unwrap_or_default()
}

fn deserialize_state(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    self.state = bincode::deserialize(data)?;
    Ok(())
}
```

### Error Handling
The counter demonstrates proper error handling for edge cases like integer overflow/underflow, ensuring the state machine never panics or produces undefined behavior.

## Running the Example

```bash
# Run the counter SMR example
cargo run --bin counter_smr_example

# Run tests to see SMR behavior
cargo test -p counter_smr

# Run with multiple replicas (if implemented)
cargo run --bin counter_smr_cluster
```

## Use Cases

This pattern is useful for:

- **Distributed Counters**: Website hit counters, API rate limiting
- **Resource Allocation**: Database connection pools, queue sizes
- **Metrics Collection**: Aggregating statistics across services
- **Game Scores**: Leaderboards, player statistics

## Extending This Example

You can extend the counter SMR to demonstrate more advanced SMR concepts:

1. **Batch Operations**: Apply multiple increments/decrements atomically
2. **Conditional Operations**: Increment only if value is below threshold
3. **Named Counters**: Maintain multiple named counters in one state machine
4. **Time-based Operations**: Add timestamps to track operation history

## Implementation Notes

### Why This Works Well for SMR

1. **Small State**: The counter state is minimal (just an integer), making snapshots efficient
2. **Fast Operations**: Arithmetic operations are fast and deterministic
3. **No External Dependencies**: Operations don't depend on external systems or current time
4. **Easy to Test**: Simple operations make it easy to verify correctness

### Performance Characteristics

- **Memory Usage**: Minimal (single integer + operation counter)
- **Operation Latency**: Very low (simple arithmetic)
- **Throughput**: High (operations can be batched efficiently)
- **Snapshot Size**: Tiny (serialized integer)

This makes the counter an ideal first example for learning SMR concepts with Rabia.

## Next Steps

After understanding the counter example, explore:
- [Key-Value Store SMR](../kvstore_smr/) - More complex state with multiple operations
- [Banking SMR](../banking_smr/) - Business logic with validation and transactions
- [Custom State Machine](../custom_state_machine.rs) - Template for your own SMR applications