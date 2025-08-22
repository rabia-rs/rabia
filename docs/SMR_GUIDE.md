# State Machine Replication Developer Guide

This guide provides a comprehensive introduction to building fault-tolerant distributed applications using State Machine Replication (SMR) with the Rabia framework.

## Table of Contents

1. [Understanding State Machine Replication](#understanding-state-machine-replication)
2. [The StateMachine Trait](#the-statemachine-trait)
3. [Building Your First SMR Application](#building-your-first-smr-application)
4. [Advanced SMR Patterns](#advanced-smr-patterns)
5. [Performance Considerations](#performance-considerations)
6. [Testing SMR Applications](#testing-smr-applications)
7. [Production Deployment](#production-deployment)

## Understanding State Machine Replication

### What is SMR?

State Machine Replication (SMR) is a fundamental technique for building fault-tolerant distributed systems. The core idea is simple:

- **Deterministic State Machine**: Your application logic is implemented as a deterministic state machine
- **Operation Ordering**: A consensus protocol ensures all replicas apply operations in the same order
- **Identical State**: All healthy replicas maintain identical state by applying the same operations in the same order
- **Fault Tolerance**: The system remains available as long as a majority of replicas are healthy

### Why SMR?

SMR provides several key benefits:

- **Strong Consistency**: All clients see the same state across replicas
- **Fault Tolerance**: Automatic failover when nodes crash
- **Scalability**: Read operations can be served from any replica
- **Simplicity**: Focus on business logic, not distributed systems complexity

### SMR vs Other Patterns

| Pattern | Consistency | Complexity | Use Case |
|---------|-------------|------------|----------|
| SMR | Strong | Low | Critical data, financial systems |
| Leader-Follower | Strong | Medium | Databases, message queues |
| Eventually Consistent | Weak | Low | Caching, social media |
| CRDT | Eventual | Medium | Collaborative editing |

## The StateMachine Trait

The `StateMachine` trait is the heart of Rabia's SMR framework. It defines three key methods:

```rust
use async_trait::async_trait;
use rabia_core::smr::{Operation, OperationResult};

#[async_trait]
pub trait StateMachine: Send + Sync {
    /// Apply an operation deterministically
    async fn apply_operation(&mut self, operation: &Operation) -> OperationResult;
    
    /// Create a snapshot of current state
    async fn snapshot(&self) -> OperationResult;
    
    /// Restore state from a snapshot
    async fn restore_from_snapshot(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

### Key Principles

1. **Determinism**: `apply_operation` must be deterministic - same operation + same state = same result
2. **Idempotency**: Operations should handle duplicate application gracefully
3. **Serializability**: State must be serializable for snapshots and recovery

## Building Your First SMR Application

Let's build a distributed counter as our first SMR application.

### Step 1: Define Operations

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CounterOperation {
    Increment,
    Decrement,
    Get,
    Set(i64),
}
```

### Step 2: Implement the State Machine

```rust
use rabia_core::smr::{StateMachine, Operation, OperationResult};
use async_trait::async_trait;

pub struct CounterSMR {
    value: i64,
}

impl CounterSMR {
    pub fn new() -> Self {
        Self { value: 0 }
    }
}

#[async_trait]
impl StateMachine for CounterSMR {
    async fn apply_operation(&mut self, op: &Operation) -> OperationResult {
        // Deserialize the operation
        let counter_op: CounterOperation = bincode::deserialize(&op.data)
            .map_err(|e| format!("Failed to deserialize operation: {}", e))?;
        
        // Apply operation deterministically
        let result = match counter_op {
            CounterOperation::Increment => {
                self.value += 1;
                self.value
            }
            CounterOperation::Decrement => {
                self.value -= 1;
                self.value
            }
            CounterOperation::Get => self.value,
            CounterOperation::Set(new_value) => {
                self.value = new_value;
                self.value
            }
        };
        
        // Return serialized result
        Ok(bincode::serialize(&result)
            .map_err(|e| format!("Failed to serialize result: {}", e))?)
    }
    
    async fn snapshot(&self) -> OperationResult {
        Ok(bincode::serialize(&self.value)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?)
    }
    
    async fn restore_from_snapshot(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.value = bincode::deserialize(data)?;
        Ok(())
    }
}
```

### Step 3: Set Up the SMR Cluster

```rust
use rabia_core::{NodeId, ClusterConfig, Operation};
use rabia_engine::{RabiaEngine, RabiaConfig};
use rabia_persistence::InMemoryPersistence;
use std::collections::HashSet;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a 3-node cluster (minimum for fault tolerance)
    let nodes: Vec<NodeId> = (0..3).map(|_| NodeId::new()).collect();
    let cluster_config = ClusterConfig::new(nodes[0], nodes.into_iter().collect());
    
    // Create state machine and dependencies
    let state_machine = CounterSMR::new();
    let persistence = InMemoryPersistence::new();
    let config = RabiaConfig::default();
    let (operation_tx, operation_rx) = mpsc::unbounded_channel();
    
    // Create SMR replica
    let engine = RabiaEngine::new(
        cluster_config.node_id,
        config,
        cluster_config,
        state_machine,
        persistence,
        operation_rx,
    );
    
    // Start the replica in background
    let engine_handle = tokio::spawn(async move {
        engine.run().await
    });
    
    // Submit operations
    let increment_op = Operation::new(bincode::serialize(&CounterOperation::Increment)?);
    operation_tx.send(increment_op)?;
    
    let get_op = Operation::new(bincode::serialize(&CounterOperation::Get)?);
    operation_tx.send(get_op)?;
    
    println!("Counter SMR is running! Operations submitted.");
    
    // In a real application, you'd handle the engine result
    // engine_handle.await??;
    
    Ok(())
}
```

## Advanced SMR Patterns

### Banking System Example

A more complex example showing transactions and validation:

```rust
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BankingOperation {
    CreateAccount { account_id: String, initial_balance: u64 },
    Deposit { account_id: String, amount: u64 },
    Withdraw { account_id: String, amount: u64 },
    Transfer { from: String, to: String, amount: u64 },
    GetBalance { account_id: String },
    GetAllAccounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BankingResult {
    Success(String),
    Balance(u64),
    AllAccounts(HashMap<String, u64>),
    Error(String),
}

pub struct BankingSMR {
    accounts: HashMap<String, u64>,
}

impl BankingSMR {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }
}

#[async_trait]
impl StateMachine for BankingSMR {
    async fn apply_operation(&mut self, op: &Operation) -> OperationResult {
        let banking_op: BankingOperation = bincode::deserialize(&op.data)
            .map_err(|e| format!("Failed to deserialize operation: {}", e))?;
        
        let result = match banking_op {
            BankingOperation::CreateAccount { account_id, initial_balance } => {
                if self.accounts.contains_key(&account_id) {
                    BankingResult::Error("Account already exists".to_string())
                } else {
                    self.accounts.insert(account_id.clone(), initial_balance);
                    BankingResult::Success(format!("Account {} created", account_id))
                }
            }
            
            BankingOperation::Deposit { account_id, amount } => {
                match self.accounts.get_mut(&account_id) {
                    Some(balance) => {
                        *balance += amount;
                        BankingResult::Success(format!("Deposited {} to {}", amount, account_id))
                    }
                    None => BankingResult::Error("Account not found".to_string()),
                }
            }
            
            BankingOperation::Withdraw { account_id, amount } => {
                match self.accounts.get_mut(&account_id) {
                    Some(balance) => {
                        if *balance >= amount {
                            *balance -= amount;
                            BankingResult::Success(format!("Withdrew {} from {}", amount, account_id))
                        } else {
                            BankingResult::Error("Insufficient funds".to_string())
                        }
                    }
                    None => BankingResult::Error("Account not found".to_string()),
                }
            }
            
            BankingOperation::Transfer { from, to, amount } => {
                // Check source account
                let source_balance = match self.accounts.get(&from) {
                    Some(balance) if *balance >= amount => *balance,
                    Some(_) => return Ok(bincode::serialize(&BankingResult::Error("Insufficient funds".to_string()))?),
                    None => return Ok(bincode::serialize(&BankingResult::Error("Source account not found".to_string()))?),
                };
                
                // Check destination account
                if !self.accounts.contains_key(&to) {
                    return Ok(bincode::serialize(&BankingResult::Error("Destination account not found".to_string()))?);
                }
                
                // Perform transfer
                *self.accounts.get_mut(&from).unwrap() = source_balance - amount;
                *self.accounts.get_mut(&to).unwrap() += amount;
                
                BankingResult::Success(format!("Transferred {} from {} to {}", amount, from, to))
            }
            
            BankingOperation::GetBalance { account_id } => {
                match self.accounts.get(&account_id) {
                    Some(balance) => BankingResult::Balance(*balance),
                    None => BankingResult::Error("Account not found".to_string()),
                }
            }
            
            BankingOperation::GetAllAccounts => {
                BankingResult::AllAccounts(self.accounts.clone())
            }
        };
        
        Ok(bincode::serialize(&result)
            .map_err(|e| format!("Failed to serialize result: {}", e))?)
    }
    
    async fn snapshot(&self) -> OperationResult {
        Ok(bincode::serialize(&self.accounts)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?)
    }
    
    async fn restore_from_snapshot(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.accounts = bincode::deserialize(data)?;
        Ok(())
    }
}
```

### Key-Value Store with Transactions

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KVOperation {
    Get { key: String },
    Set { key: String, value: Vec<u8> },
    Delete { key: String },
    Transaction { ops: Vec<KVOperation> },
    ListKeys,
}

pub struct KVStoreSMR {
    data: HashMap<String, Vec<u8>>,
}

#[async_trait]
impl StateMachine for KVStoreSMR {
    async fn apply_operation(&mut self, op: &Operation) -> OperationResult {
        let kv_op: KVOperation = bincode::deserialize(&op.data)?;
        
        let result = match kv_op {
            KVOperation::Get { key } => {
                self.data.get(&key).cloned().unwrap_or_default()
            }
            
            KVOperation::Set { key, value } => {
                self.data.insert(key, value.clone());
                b"OK".to_vec()
            }
            
            KVOperation::Delete { key } => {
                self.data.remove(&key);
                b"OK".to_vec()
            }
            
            KVOperation::Transaction { ops } => {
                // Apply all operations atomically
                let mut temp_state = self.data.clone();
                let mut results = Vec::new();
                
                for tx_op in ops {
                    match tx_op {
                        KVOperation::Set { key, value } => {
                            temp_state.insert(key, value);
                            results.push(b"OK".to_vec());
                        }
                        KVOperation::Delete { key } => {
                            temp_state.remove(&key);
                            results.push(b"OK".to_vec());
                        }
                        // Transactions of transactions not supported
                        _ => return Ok(b"Invalid transaction operation".to_vec()),
                    }
                }
                
                // Commit transaction
                self.data = temp_state;
                bincode::serialize(&results)?
            }
            
            KVOperation::ListKeys => {
                let keys: Vec<String> = self.data.keys().cloned().collect();
                bincode::serialize(&keys)?
            }
        };
        
        Ok(result)
    }
    
    async fn snapshot(&self) -> OperationResult {
        Ok(bincode::serialize(&self.data)?)
    }
    
    async fn restore_from_snapshot(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.data = bincode::deserialize(data)?;
        Ok(())
    }
}
```

## Performance Considerations

### Operation Batching

Rabia automatically batches operations for better performance:

```rust
use rabia_engine::RabiaConfig;
use std::time::Duration;

let config = RabiaConfig {
    max_batch_size: 100,           // Maximum operations per batch
    batch_timeout: Duration::from_millis(10), // Maximum batching delay
    ..Default::default()
};
```

### Memory Management

For high-throughput applications, consider:

1. **Efficient Serialization**: Use `bincode` for compact binary encoding
2. **Memory Pooling**: Reuse operation buffers where possible
3. **Snapshot Frequency**: Balance consistency recovery with performance

```rust
// Efficient operation creation
let operation_data = bincode::serialize(&your_operation)?;
let operation = Operation::new(operation_data);
```

### State Size Management

For large state machines:

1. **Implement Incremental Snapshots**: Only serialize changed data
2. **Use Compression**: Compress snapshots for storage efficiency
3. **Consider State Partitioning**: Split large state across multiple SMR instances

## Testing SMR Applications

### Unit Testing State Machines

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rabia_core::smr::Operation;
    
    #[tokio::test]
    async fn test_counter_operations() {
        let mut counter = CounterSMR::new();
        
        // Test increment
        let increment_op = Operation::new(bincode::serialize(&CounterOperation::Increment).unwrap());
        let result = counter.apply_operation(&increment_op).await.unwrap();
        let value: i64 = bincode::deserialize(&result).unwrap();
        assert_eq!(value, 1);
        
        // Test decrement
        let decrement_op = Operation::new(bincode::serialize(&CounterOperation::Decrement).unwrap());
        let result = counter.apply_operation(&decrement_op).await.unwrap();
        let value: i64 = bincode::deserialize(&result).unwrap();
        assert_eq!(value, 0);
        
        // Test determinism - same operation should produce same result
        let get_op = Operation::new(bincode::serialize(&CounterOperation::Get).unwrap());
        let result1 = counter.apply_operation(&get_op).await.unwrap();
        let result2 = counter.apply_operation(&get_op).await.unwrap();
        assert_eq!(result1, result2);
    }
    
    #[tokio::test]
    async fn test_snapshot_restore() {
        let mut counter1 = CounterSMR::new();
        counter1.value = 42;
        
        // Create snapshot
        let snapshot = counter1.snapshot().await.unwrap();
        
        // Restore to new instance
        let mut counter2 = CounterSMR::new();
        counter2.restore_from_snapshot(&snapshot).await.unwrap();
        
        assert_eq!(counter1.value, counter2.value);
    }
}
```

### Integration Testing with Fault Injection

```rust
use rabia_testing::{NetworkSimulator, FaultType};
use std::time::Duration;

#[tokio::test]
async fn test_counter_with_node_failure() {
    // Set up 5-node cluster
    let mut nodes = Vec::new();
    for _ in 0..5 {
        let node_id = NodeId::new();
        // Set up each node...
        nodes.push(node_id);
    }
    
    // Simulate network partition
    let simulator = NetworkSimulator::new();
    simulator.inject_fault(FaultType::NodeCrash {
        node_id: nodes[0],
        duration: Duration::from_secs(10),
    }).await;
    
    // Verify system continues operating with 4 nodes
    // Submit operations and verify consistency
}
```

## Production Deployment

### Cluster Configuration

For production deployments:

```rust
use rabia_engine::RabiaConfig;
use std::time::Duration;

let production_config = RabiaConfig {
    // Performance tuning
    max_batch_size: 1000,
    batch_timeout: Duration::from_millis(5),
    
    // Reliability settings  
    heartbeat_interval: Duration::from_millis(100),
    election_timeout: Duration::from_millis(1000),
    
    // Recovery settings
    snapshot_interval: Duration::from_secs(300), // 5 minutes
    max_log_size: 10_000_000, // 10MB
    
    ..Default::default()
};
```

### Monitoring and Observability

Implement monitoring for:

1. **Operation Throughput**: Operations per second across replicas
2. **Consensus Latency**: Time from operation submission to application
3. **Node Health**: Heartbeat status and connectivity
4. **State Size**: Memory usage and snapshot sizes

```rust
use tracing::{info, warn, error};

#[async_trait]
impl StateMachine for YourSMR {
    async fn apply_operation(&mut self, op: &Operation) -> OperationResult {
        let start = std::time::Instant::now();
        
        let result = self.internal_apply(op).await;
        
        let duration = start.elapsed();
        info!("Operation applied in {:?}", duration);
        
        if duration > Duration::from_millis(100) {
            warn!("Slow operation detected: {:?}", duration);
        }
        
        result
    }
}
```

### Deployment Architecture

```
Internet
    │
    ▼
[Load Balancer]
    │
    ├─── [SMR Replica 1] ── [Persistent Storage]
    │         │
    ├─── [SMR Replica 2] ── [Persistent Storage]  
    │         │
    └─── [SMR Replica 3] ── [Persistent Storage]
              │
      [Cross-Replica Network]
```

### Operational Considerations

1. **Minimum Cluster Size**: Use odd numbers (3, 5, 7) for optimal fault tolerance
2. **Geographic Distribution**: Deploy replicas across availability zones
3. **Network Connectivity**: Ensure low-latency, high-bandwidth connections between replicas
4. **Backup Strategy**: Regular snapshots with off-site storage
5. **Capacity Planning**: Monitor state growth and operation throughput

### Security Considerations

1. **Network Encryption**: Use TLS for inter-replica communication
2. **Operation Validation**: Validate operations before application
3. **Access Control**: Implement authentication and authorization
4. **Audit Logging**: Log all state changes for compliance

## Best Practices

1. **Keep Operations Small**: Large operations increase consensus latency
2. **Design for Determinism**: Avoid non-deterministic operations (random numbers, timestamps)
3. **Handle Errors Gracefully**: Return errors as operation results, don't panic
4. **Test Thoroughly**: Unit test state machines, integration test with failures
5. **Monitor Performance**: Track operation latency and throughput
6. **Plan for Growth**: Design state machines to handle increasing load

## Next Steps

- Explore the [examples/](../examples/) directory for complete implementations
- Read the [Protocol Guide](PROTOCOL_GUIDE.md) for consensus algorithm details
- Check the [Troubleshooting Guide](TROUBLESHOOTING.md) for common issues
- Join the community discussions for questions and best practices

---

This guide provides the foundation for building robust, fault-tolerant applications with State Machine Replication using the Rabia framework. The SMR pattern, combined with Rabia's efficient consensus protocol, enables you to focus on your application logic while ensuring strong consistency and fault tolerance.