# Rabia Consensus Protocol Guide

A comprehensive guide to understanding and implementing the Rabia consensus protocol.

## Table of Contents

1. [Protocol Overview](#protocol-overview)
2. [Core Concepts](#core-concepts)
3. [Algorithm Details](#algorithm-details)
4. [Implementation Architecture](#implementation-architecture)
5. [Safety and Liveness](#safety-and-liveness)
6. [Performance Characteristics](#performance-characteristics)
7. [Comparison with Other Protocols](#comparison-with-other-protocols)
8. [Troubleshooting](#troubleshooting)

---

## Protocol Overview

Rabia is a **randomized consensus protocol** designed to be simpler and more efficient than traditional protocols like Raft or PBFT. It was introduced in the SOSP 2021 paper ["Rabia: Simplifying State-Machine Replication Through Randomization"](https://www.cs.cornell.edu/~rvr/papers/rabia.pdf).

### Key Properties

- ✅ **No Fixed Leader**: Eliminates leader election overhead - any node can propose when it receives client requests
- ✅ **Randomized Agreement**: Uses randomization to break ties and ensure progress
- ✅ **Crash Fault Tolerant**: Handles node crashes and network partitions gracefully
- ✅ **Simple**: Fewer states and transitions than traditional protocols
- ✅ **High Throughput**: Optimized for batched operations

### When to Use Rabia

**Good For:**
- High-throughput applications needing consensus
- Systems where leader election overhead is problematic
- Applications that can tolerate crash faults (not Byzantine faults)
- Scenarios where simplicity and maintainability matter

**Not Ideal For:**
- Systems requiring Byzantine fault tolerance
- Applications needing strict ordering guarantees
- Very low-latency requirements (sub-millisecond)

---

## Core Concepts

### Nodes and Clusters

```rust
use rabia_core::NodeId;
use std::collections::HashSet;

// Create node identifiers
let node1 = NodeId::new();
let node2 = NodeId::new();
let node3 = NodeId::new();

// Form a cluster (minimum 3 nodes for fault tolerance)
let mut cluster_nodes = HashSet::new();
cluster_nodes.insert(node1);
cluster_nodes.insert(node2);
cluster_nodes.insert(node3);
```

### Phases and Values

```rust
use rabia_core::{PhaseId, StateValue};

// Each consensus round has a unique phase ID
let phase = PhaseId::new(1);

// Nodes vote on values during consensus
let accept = StateValue::V1;      // Accept the proposal
let reject = StateValue::V0;      // Reject the proposal  
let uncertain = StateValue::VQuestion; // Uncertain (used in randomization)
```

### Commands and Batches

```rust
use rabia_core::{Command, CommandBatch};

// Individual operations
let cmd1 = Command::new("SET user:alice age=25");
let cmd2 = Command::new("GET user:alice");

// Batched for efficiency
let batch = CommandBatch::new(vec![cmd1, cmd2]);
println!("Batch ID: {}, Commands: {}", batch.id, batch.commands.len());
```

---

## Algorithm Details

### High-Level Flow

```
    Any Node                All Nodes              All Nodes
       │                        │                      │
   ┌───▼───┐                ┌───▼───┐            ┌───▼───┐
   │Propose│───────────────▶│ Vote  │───────────▶│Vote R2│
   │ Batch │                │ R1    │            │  &    │
   └───────┘                └───────┘            │Decide │
                                                 └───────┘
    Phase 0                  Phase 1              Phase 2
 (Actual Data)         (Randomized Voting)  (Randomized Voting)
```

### Randomization Timing (CRITICAL)

**The Rabia protocol uses randomization ONLY during voting rounds, never at proposal time:**

1. **Proposal Phase**: Nodes propose actual client request batches with StateValue::V1 (commit)
2. **Vote Round 1**: Nodes use randomized voting to decide whether to support the proposed batch
3. **Vote Round 2**: If Round 1 is inconclusive, nodes use further randomization to break ties
4. **Decision**: Based on voting outcomes, decide whether to commit (V1) or forfeit (V0) the batch

### Detailed Algorithm Steps

#### 1. Propose Phase
When a node receives a client request, it initiates consensus by proposing a batch of commands:

```rust
// When a client submits commands, the receiving node becomes the proposer
async fn process_batch_request(request: CommandRequest) -> Result<()> {
    // Check if we have quorum before proposing
    if !has_quorum() {
        return Err(RabiaError::QuorumNotAvailable);
    }

    // Add batch to pending and start consensus
    let batch_id = add_pending_batch(request.batch.clone(), self.node_id);
    
    // Generate new phase and propose the actual batch data
    let phase_id = self.engine_state.advance_phase();
    let proposed_value = StateValue::V1; // V1 means "commit this batch"
    
    let proposal = ProposeMessage {
        phase_id,
        batch_id,
        value: proposed_value,
        batch: Some(request.batch),
    };
    
    // Broadcast to all other nodes (excluding self)
    broadcast_to_all_nodes(proposal);
}
```

#### 2. Vote Round 1
Each node receives the proposal and votes based on local state:

**CRITICAL**: Randomization happens during voting rounds, NOT at proposal time.

```rust
fn determine_round1_vote(proposal: &ProposeMessage) -> StateValue {
    match has_conflicting_proposal(proposal.phase_id) {
        Some(existing_value) => {
            if existing_value == proposal.value {
                proposal.value  // Agree with consistent proposal
            } else {
                StateValue::VQuestion  // Conflict detected
            }
        }
        None => {
            // First proposal for this phase - randomized vote (this is where randomization occurs!)
            randomized_vote(&proposal.value)
        }
    }
}

fn randomized_vote(proposed_value: &StateValue) -> StateValue {
    match proposed_value {
        StateValue::V0 => {
            if random() < 0.5 { StateValue::V0 } else { StateValue::VQuestion }
        }
        StateValue::V1 => {
            if random() < 0.6 { StateValue::V1 } else { StateValue::VQuestion }
        }
        StateValue::VQuestion => StateValue::VQuestion,
    }
}
```

#### 3. Vote Round 2
After collecting majority votes from Round 1:

```rust
fn determine_round2_vote(round1_result: StateValue, round1_votes: &HashMap<NodeId, StateValue>) -> StateValue {
    match round1_result {
        StateValue::V0 => StateValue::V0,  // Must vote V0 for safety
        StateValue::V1 => StateValue::V1,  // Must vote V1 for safety
        StateValue::VQuestion => {
            // Round 1 inconclusive - use sophisticated randomization
            let v0_count = count_votes(&round1_votes, StateValue::V0);
            let v1_count = count_votes(&round1_votes, StateValue::V1);
            
            match v1_count.cmp(&v0_count) {
                Ordering::Greater => {
                    if random() < 0.8 { StateValue::V1 } else { StateValue::V0 }
                }
                Ordering::Less => {
                    if random() < 0.7 { StateValue::V0 } else { StateValue::V1 }
                }
                Ordering::Equal => {
                    // Bias toward V1 for liveness
                    if random() < 0.6 { StateValue::V1 } else { StateValue::V0 }
                }
            }
        }
    }
}
```

#### 4. Decision Phase
Once majority votes are collected in Round 2:

```rust
if let Some(decision) = has_round2_majority() {
    if decision == StateValue::V1 {
        // Commit the batch to state machine
        apply_batch_to_state_machine(&batch).await?;
        broadcast_decision(Decision::Commit);
    } else {
        // Reject the batch
        broadcast_decision(Decision::Abort);
    }
}
```

---

## Implementation Architecture

### Core Traits

The implementation is built around key abstractions:

```rust
// State machine interface - your application logic
#[async_trait]
pub trait StateMachine: Send + Sync {
    type State: Clone + Send + Sync;
    
    async fn apply_command(&mut self, command: &Command) -> Result<Bytes>;
    async fn apply_commands(&mut self, commands: &[Command]) -> Result<Vec<Bytes>>;
    async fn create_snapshot(&self) -> Result<Snapshot>;
    async fn restore_snapshot(&mut self, snapshot: &Snapshot) -> Result<()>;
    async fn get_state(&self) -> Self::State;
}

// Network transport - how nodes communicate  
#[async_trait]
pub trait NetworkTransport: Send + Sync {
    async fn send_to(&self, target: NodeId, message: ProtocolMessage) -> Result<()>;
    async fn broadcast(&self, message: ProtocolMessage, exclude: Option<NodeId>) -> Result<()>;
    async fn receive(&mut self) -> Result<(NodeId, ProtocolMessage)>;
    async fn get_connected_nodes(&self) -> Result<HashSet<NodeId>>;
    async fn is_connected(&self, node_id: NodeId) -> Result<bool>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn reconnect(&mut self) -> Result<()>;
}

// Persistence layer - how state is stored
#[async_trait]
pub trait PersistenceLayer: Send + Sync {
    async fn save_state(&self, state: &PersistedState) -> Result<()>;
    async fn load_state(&self) -> Result<Option<PersistedState>>;
}
```

### Engine Architecture

```rust
use rabia_engine::RabiaEngine;

// The engine coordinates all consensus activities
pub struct RabiaEngine<SM, NT, PL> {
    node_id: NodeId,
    state_machine: Arc<Mutex<SM>>,     // Your application
    network: Arc<Mutex<NT>>,           // Communication layer  
    persistence: Arc<PL>,              // Storage layer
    engine_state: Arc<EngineState>,    // Consensus state
    // ...
}

// Main consensus loop
async fn run(mut self) -> Result<()> {
    loop {
        tokio::select! {
            // Process incoming commands from clients
            command = self.command_rx.recv() => {
                self.handle_command(command).await?;
            }
            
            // Process network messages from other nodes
            _ = self.receive_messages() => {
                // Handle consensus messages
            }
            
            // Periodic maintenance  
            _ = cleanup_interval.tick() => {
                self.cleanup_old_state().await;
            }
            
            // Send heartbeats
            _ = heartbeat_interval.tick() => {
                self.send_heartbeat().await?;
            }
        }
    }
}
```

---

## Safety and Liveness

### Safety Properties ✅

**Definition**: "Bad things never happen"

1. **Agreement**: All nodes that decide on a batch decide on the same value
2. **Validity**: If all nodes propose the same value, that value is decided
3. **Integrity**: A batch is decided at most once

**How Rabia Ensures Safety:**
- Round 1 voting prevents inconsistent decisions
- Round 2 voting enforces agreement when Round 1 reaches consensus
- Randomization in Round 2 only when Round 1 is inconclusive

### Liveness Properties ✅

**Definition**: "Good things eventually happen"

1. **Termination**: Every proposed batch is eventually decided
2. **Progress**: The system makes progress even under contention

**How Rabia Ensures Liveness:**
- Randomized voting breaks ties and prevents infinite loops
- Bias toward acceptance (V1) encourages progress
- No leader election delays or single points of failure

### Fault Tolerance

```rust
// Rabia tolerates up to f crash failures in a 2f+1 node cluster
let cluster_size = 5;  // Can tolerate 2 failures
let max_failures = (cluster_size - 1) / 2;  // = 2
let quorum_size = max_failures + 1;  // = 3

println!("Cluster: {} nodes, tolerates {} failures, needs {} for quorum", 
         cluster_size, max_failures, quorum_size);
```

---

## Performance Characteristics

### Latency Profile

```
Best Case (no conflicts):    2 network round-trips
Average Case (conflicts):    2-3 network round-trips  
Worst Case (heavy conflicts): 3-4 network round-trips
```

### Throughput Optimization

```rust
use rabia_core::batching::{CommandBatcher, BatchConfig};

// Configure for high throughput
let config = BatchConfig {
    max_batch_size: 1000,                        // Larger batches
    max_batch_delay: Duration::from_millis(5),   // Short delays
    adaptive: true,                              // Adapt to load
    memory_threshold: 64 * 1024 * 1024,         // 64MB memory limit
};

let mut batcher = CommandBatcher::new(config);

// Batch commands automatically
for command in command_stream {
    if let Some(batch) = batcher.add_command(command)? {
        submit_to_consensus(batch).await?;
    }
}
```

### Memory Usage

```rust
use rabia_core::memory_pool::get_pooled_buffer;

// Use memory pools for efficient allocation
let mut buffer = get_pooled_buffer(1024);
buffer.extend_from_slice(&message_data);

// Buffer is automatically returned to pool when dropped
```

---

## Comparison with Other Protocols

| Feature | Rabia | Raft | PBFT | Viewstamped Replication |
|---------|-------|------|------|------------------------|
| **Leader Required** | ❌ No | ✅ Yes | ✅ Yes | ✅ Yes |
| **Fault Model** | Crash | Crash | Byzantine | Crash |
| **Message Complexity** | O(n²) | O(n) | O(n²) | O(n) |
| **Randomization** | ✅ Yes | ❌ No | ❌ No | ❌ No |
| **Simplicity** | ✅ High | 🟡 Medium | ❌ Complex | 🟡 Medium |
| **Throughput** | ✅ High | 🟡 Medium | 🟡 Medium | 🟡 Medium |

### When to Choose Rabia

**Choose Rabia when:**
- You need high throughput with reasonable latency
- Leader election overhead is problematic
- You prefer algorithmic simplicity
- Crash fault tolerance is sufficient

**Choose Raft when:**
- You need strong consistency guarantees
- You have asymmetric workloads (read-heavy)
- You need well-understood behavior
- Leadership semantics are useful

**Choose PBFT when:**
- You need Byzantine fault tolerance
- Security against malicious actors is critical
- You can tolerate higher complexity

---

## Troubleshooting

### Common Issues

#### 1. Consensus Not Making Progress

**Symptoms:**
```
WARN: Phase 123 has been active for 30s without decision
WARN: Round 1 voting stuck with no majority
```

**Causes & Solutions:**
```rust
// Check cluster size and quorum
let cluster_size = cluster_config.all_nodes.len();
if cluster_size < 3 {
    eprintln!("❌ Need at least 3 nodes for consensus");
}

// Verify network connectivity
let connected = network.get_connected_nodes().await?;
if connected.len() < cluster_config.quorum_size {
    eprintln!("❌ Insufficient connected nodes: {} < {}", 
              connected.len(), cluster_config.quorum_size);
}

// Check for network partitions
if let Some(partition) = detect_network_partition() {
    eprintln!("❌ Network partition detected: {:?}", partition);
}
```

#### 2. High Memory Usage

**Symptoms:**
```
WARN: Phase cache size: 10000 entries
WARN: Pending batches: 5000 entries  
```

**Solutions:**
```rust
// Configure cleanup more aggressively
let config = RabiaConfig {
    max_phase_history: 100,        // Keep fewer phases
    cleanup_interval: Duration::from_secs(30),  // Clean more often
    max_pending_batches: 1000,     // Limit pending batches
    ..Default::default()
};

// Monitor memory usage
let stats = engine.get_statistics().await;
if stats.memory_usage_mb > 512 {
    println!("⚠️ High memory usage: {} MB", stats.memory_usage_mb);
    engine.trigger_cleanup().await?;
}
```

#### 3. Message Validation Failures

**Symptoms:**
```
ERROR: Invalid message from node_123: Checksum mismatch
ERROR: Message validation failed: Invalid phase transition
```

**Solutions:**
```rust
// Check message integrity
if let Err(e) = message.validate() {
    match e.error_type {
        ErrorType::Checksum => {
            warn!("Checksum error - possible network corruption");
            // Request retransmission
        }
        ErrorType::Protocol => {
            error!("Protocol violation - possible software bug");
            // Log detailed message info for debugging
        }
    }
}

// Verify node compatibility
let version_info = get_node_version_info();
if !is_compatible_version(&version_info) {
    error!("Version incompatibility detected");
}
```

#### 4. Performance Issues

**Symptoms:**
```
WARN: Average consensus latency: 500ms (expected: <100ms)
WARN: Throughput: 100 ops/sec (expected: >1000 ops/sec)
```

**Performance Tuning:**
```rust
// Optimize batching
let config = BatchConfig {
    max_batch_size: 500,                      // Increase batch size
    max_batch_delay: Duration::from_millis(2), // Reduce delay
    adaptive: true,                           // Enable adaptive batching
    ..Default::default()
};

// Tune network settings
let network_config = TcpNetworkConfig {
    connection_timeout: Duration::from_secs(5),
    max_message_size: 16 * 1024 * 1024,     // 16MB messages
    buffer_config: BufferConfig {
        read_buffer_size: 128 * 1024,        // 128KB buffers
        write_buffer_size: 128 * 1024,
        message_queue_size: 2000,            // Larger queues
    },
    ..Default::default()
};

// Use memory pools
let pool_config = MemoryPoolConfig {
    initial_pool_size: 1000,
    max_pool_size: 10000,
    buffer_sizes: vec![1024, 4096, 16384],  // Multiple buffer sizes
};
```

### Debugging Tools

#### Enable Detailed Logging
```rust
use tracing_subscriber::{FmtSubscriber, EnvFilter};

let subscriber = FmtSubscriber::builder()
    .with_env_filter(EnvFilter::from_default_env().add_directive("rabia=debug".parse()?))
    .with_thread_ids(true)
    .with_file(true)
    .with_line_number(true)
    .finish();

tracing::subscriber::set_global_default(subscriber)?;
```

#### Consensus State Inspection  
```rust
// Get detailed engine statistics
let stats = engine.get_statistics().await;
println!("Current phase: {}", stats.current_phase);
println!("Active phases: {}", stats.active_phases.len());
println!("Pending batches: {}", stats.pending_batches);
println!("Memory usage: {} MB", stats.memory_usage_mb);

// Dump state for debugging
if std::env::var("DEBUG_DUMP").is_ok() {
    let state_dump = engine.dump_debug_state().await?;
    std::fs::write("consensus_state.json", serde_json::to_string_pretty(&state_dump)?)?;
}
```

### Best Practices

1. **Monitoring**: Always monitor consensus progress and performance metrics
2. **Timeouts**: Configure appropriate timeouts for your network environment  
3. **Cleanup**: Enable regular cleanup of old phases and batches
4. **Testing**: Use fault injection testing to validate behavior under failures
5. **Logging**: Enable structured logging for production environments

---

## Further Reading

- **Original Paper**: [Rabia: Simplifying State-Machine Replication Through Randomization](https://www.cs.cornell.edu/~rvr/papers/rabia.pdf)
- **Implementation**: Browse the `examples/` directory for practical usage
- **API Docs**: Run `cargo doc --open` for detailed API documentation
- **Performance**: See `benchmarks/` directory for performance analysis tools

For questions or contributions, please visit the [project repository](https://github.com/rabia-rs/rabia).