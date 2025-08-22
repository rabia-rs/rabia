# Rabia - State Machine Replication Protocol

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/rabia-core.svg)](https://crates.io/crates/rabia-core)
[![Documentation](https://docs.rs/rabia-core/badge.svg)](https://docs.rs/rabia-core)
[![CI](https://github.com/rabia-rs/rabia/actions/workflows/ci.yml/badge.svg)](https://github.com/rabia-rs/rabia/actions/workflows/ci.yml)
[![Security](https://github.com/rabia-rs/rabia/actions/workflows/security.yml/badge.svg)](https://github.com/rabia-rs/rabia/actions/workflows/security.yml)
[![codecov](https://codecov.io/gh/rabia-rs/rabia/branch/main/graph/badge.svg)](https://codecov.io/gh/rabia-rs/rabia)

A high-performance implementation of the Rabia consensus (**State Machine Replication (SMR)**)protocol. Rabia-rs enables developers to build fault-tolerant distributed applications by implementing custom state machines that are replicated across multiple nodes with strong consistency guarantees.

## What is State Machine Replication?

State Machine Replication (SMR) is a fundamental approach to building fault-tolerant distributed systems. In SMR:

- **Deterministic State Machines**: Each replica runs the same deterministic state machine
- **Consensus on Operations**: Nodes agree on the order of operations to apply
- **Identical State**: All healthy replicas maintain identical state by applying operations in the same order
- **Fault Tolerance**: The system continues operating correctly as long as a majority of nodes are healthy

Rabia-rs provides a clean SMR protocol implementation where you implement the `StateMachine` trait to define your application logic, and the Rabia consensus protocol ensures all replicas apply operations in the same order.

## 🚀 SMR Protocol Features

- **Simple SMR Interface**: Implement the `StateMachine` trait to build fault-tolerant applications
- **Multiple SMR Examples**: Counter, key-value store, and banking system implementations included
- **Deterministic Execution**: Ensures identical state across all replicas
- **High Performance**: Intelligent batching and optimized serialization for maximum throughput
- **Production Ready**: Comprehensive error handling, recovery mechanisms, and edge case handling  
- **Memory Efficient**: Advanced memory pooling and zero-allocation serialization paths
- **Async/Await**: Built on Tokio for scalable concurrent processing
- **Type Safe**: Leverages Rust's type system for correctness guarantees
- **Well Tested**: Comprehensive test suite including network simulation and fault injection

## 🎯 Performance Characteristics

Rabia-rs is designed for high-performance consensus with:

- **Efficient Serialization**: Compact binary message format for reduced network overhead
- **Adaptive Batching**: Automatically groups commands for optimal throughput
- **Memory Optimization**: Advanced pooling reduces allocation overhead
- **Async Architecture**: Non-blocking I/O for concurrent processing
- **Zero-Copy Paths**: Minimizes data copying in hot code paths

## 🏗️ SMR Architecture

Rabia-rs provides a clean SMR protocol implementation organized into focused crates:

```
rabia-rs/
├── rabia-core/         # SMR traits, consensus types, and algorithms
├── rabia-engine/       # Consensus engine coordinating SMR replicas
├── rabia-persistence/  # Simple persistence for SMR state
├── rabia-kvstore/      # Example SMR application (key-value store)
├── rabia-testing/      # Testing utilities and network simulation
├── examples/           # SMR implementations (counter, banking, kvstore)
└── benchmarks/         # Performance benchmarks
```

### SMR Protocol Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   RabiaEngine   │────│   Networking    │────│  Persistence    │
│                 │    │                 │    │                 │
│ • SMR Coord     │    │ • Message Bus   │    │ • Operation Log │
│ • Consensus     │    │ • TCP/Memory    │    │ • Snapshots     │
│ • Phase Mgmt    │    │ • Node Discovery│    │ • Recovery      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │
         ▼
┌─────────────────┐    Your SMR Applications:
│  StateMachine   │    ┌─────────────────┐    ┌─────────────────┐
│     Trait       │────│  Counter SMR    │    │  Banking SMR    │
│                 │    │                 │    │                 │
│ • apply_op()    │    │ • Increment     │    │ • Accounts      │
│ • deterministic │    │ • Decrement     │    │ • Transfers     │
│ • serializable  │    │ • Get Value     │    │ • Balances      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## 🚀 Quick Start - Implementing State Machines with Rabia

Add to your `Cargo.toml`:

```toml
[dependencies]
rabia-core = "0.3.0"
rabia-engine = "0.3.0" 
tokio = { version = "1.0", features = ["full"] }
```

### Step 1: Implement Your State Machine

```rust
use rabia_core::smr::{StateMachine, Operation, OperationResult};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CounterOp {
    Increment,
    Decrement, 
    Get,
}

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
        let counter_op: CounterOp = bincode::deserialize(&op.data)?;
        
        match counter_op {
            CounterOp::Increment => {
                self.value += 1;
                Ok(bincode::serialize(&self.value)?)
            }
            CounterOp::Decrement => {
                self.value -= 1;  
                Ok(bincode::serialize(&self.value)?)
            }
            CounterOp::Get => {
                Ok(bincode::serialize(&self.value)?)
            }
        }
    }
    
    async fn snapshot(&self) -> OperationResult {
        Ok(bincode::serialize(&self.value)?)
    }
    
    async fn restore_from_snapshot(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.value = bincode::deserialize(data)?;
        Ok(())
    }
}
```

### Step 2: Set Up Rabia Protocol Cluster

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
    
    // Create your state machine
    let state_machine = CounterSMR::new();
    let persistence = InMemoryPersistence::new();
    let config = RabiaConfig::default();
    let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    
    // Start the Rabia protocol replica
    let engine = RabiaEngine::new(
        cluster_config.node_id,
        config,
        cluster_config,
        state_machine,
        persistence,
        cmd_rx,
    );
    
    println!("✅ Counter protocol replica started!");
    println!("   Your distributed counter is ready for operations");
    
    Ok(())
}
```

### Step 3: Submit Operations

```rust
// Increment the counter across all replicas
let increment_op = Operation::new(bincode::serialize(&CounterOp::Increment)?);
engine.submit_operation(increment_op).await?;

// Get the current value (same across all healthy replicas)
let get_op = Operation::new(bincode::serialize(&CounterOp::Get)?);
let result = engine.submit_operation(get_op).await?;
let counter_value: i64 = bincode::deserialize(&result)?;
println!("Counter value: {}", counter_value);
```

### Why SMR with Rabia?

**No Leader, No Single Point of Failure**: Unlike Raft or PBFT, Rabia has no leader election delays or single points of failure

**Transparent Node Management**: Adding/removing nodes is virtually transparent to SMR operation

**Simple State Machine Interface**: Just implement `apply_operation()`, `snapshot()`, and `restore_from_snapshot()`

**Deterministic Execution**: The Rabia protocol ensures your state machine operations are applied in the same order across all replicas

## 🔧 Advanced Features

### Network Simulation & Testing

```rust
use rabia_testing::{NetworkSimulator, NetworkConditions, FaultType};

// Create network simulator with realistic conditions
let simulator = NetworkSimulator::new();
simulator.update_conditions(NetworkConditions {
    latency_min: Duration::from_millis(10),
    latency_max: Duration::from_millis(50), 
    packet_loss_rate: 0.01, // 1% packet loss
    ..Default::default()
}).await;

// Inject faults for testing
simulator.inject_fault(FaultType::NodeCrash {
    node_id,
    duration: Duration::from_secs(30),
}).await;
```

### Memory Pooling

```rust
use rabia_core::memory_pool::{get_pooled_buffer, MemoryPool};

// Use memory pools for zero-allocation paths
let mut buffer = get_pooled_buffer(1024);
buffer.buffer_mut().extend_from_slice(data);
let bytes = buffer.take_bytes(); // Zero-copy conversion
```

## 📚 Protocol Examples

The `examples/` directory contains comprehensive state machine implementations using the Rabia protocol:

- **[Counter SMR](examples/counter_smr_example.rs)** - Simple distributed counter with increment/decrement operations
- **[Key-Value Store SMR](examples/kvstore_smr_example.rs)** - Production-grade distributed key-value store with transactions
- **[Banking SMR](examples/banking_smr_example.rs)** - Banking system with accounts, transfers, and balance management
- **[Custom State Machine](examples/custom_state_machine.rs)** - Template for building your own SMR application
- **[Basic Usage](examples/basic_usage.rs)** - Simple consensus setup and SMR basics
- **[Consensus Cluster](examples/consensus_cluster.rs)** - Multi-node SMR cluster with fault injection
- **[TCP Networking](examples/tcp_networking.rs)** - SMR over real network communication
- **[Performance Benchmark](examples/performance_benchmark.rs)** - SMR performance testing

Each example demonstrates different aspects of building fault-tolerant applications with the Rabia SMR protocol.

## 🐳 Docker Support

Run Rabia examples using Docker:

```bash
# Build the Docker image
docker build -t rabia-rs/rabia .

# Run the KVStore example
docker run --rm rabia-rs/rabia kvstore_usage

# Run the consensus cluster example
docker run --rm rabia-rs/rabia consensus_cluster

# Run performance benchmarks
docker run --rm rabia-rs/rabia performance_benchmark

# Interactive shell with all examples available
docker run --rm -it rabia-rs/rabia bash
```


## 🧪 Testing

Run the comprehensive test suite:

```bash
# Run all tests
cargo test --all

# Run with network simulation
cargo test --all --features network-sim

# Run performance benchmarks  
cargo bench

# Run fault injection tests
cargo test fault_injection
```

## 📈 Benchmarking

Measure performance on your system:

```bash
# Core performance benchmarks
cargo bench --bench baseline_performance

# Serialization comparison  
cargo bench --bench serialization_comparison

# Memory efficiency
cargo bench --bench memory_pool_comparison

# End-to-end optimization
cargo bench --bench comprehensive_optimization

# Peak throughput
cargo bench --bench peak_performance
```

## 🔬 Rabia Protocol Details

The Rabia consensus protocol implementation enables robust State Machine Replication:

- **Protocol Coordination**: Ensures all replicas apply operations in the same order
- **No Leader, No Single Point of Failure**: Unlike Raft or PBFT, Rabia has no leader election delays
- **Deterministic State Machines**: Your application logic runs identically across all replicas
- **Transparent Node Management**: Adding/removing nodes is virtually transparent to protocol operation
- **Randomized Agreement**: Uses randomization to achieve consensus on operation ordering efficiently
- **Crash Fault Tolerance**: Handles node crashes and network partitions while maintaining SMR consistency
- **Low Latency**: Typically 2-3 communication rounds to agree on operation order
- **High Throughput**: Optimized for batching operations to maximize protocol performance
- **Simplicity**: Easier to understand and implement than traditional SMR protocol implementations

### Protocol Operation Flow

1. **Operation Submission**: Clients submit operations to any replica
2. **Consensus on Order**: Nodes use Rabia protocol to agree on operation ordering:
   - **Propose Phase**: Replicas propose operation batches
   - **Vote Round 1**: Nodes vote with randomization
   - **Vote Round 2**: Final voting based on Round 1 results  
   - **Decision**: Commit the agreed operation order
3. **Deterministic Execution**: All replicas apply operations in the agreed order
4. **Response**: Clients receive consistent responses from any healthy replica

## 💾 State Management Implementation

### In-Memory State Structures

The implementation uses concurrent data structures for thread-safe state management:

```rust
pub struct EngineState {
    pub current_phase: Arc<AtomicU64>,           // Current consensus phase
    pub last_committed_phase: Arc<AtomicU64>,    // Last committed phase
    pub pending_batches: Arc<DashMap<BatchId, PendingBatch>>,  // Pending commands
    pub phases: Arc<DashMap<PhaseId, PhaseData>>,             // Phase tracking
    pub active_nodes: Arc<RwLock<HashSet<NodeId>>>,           // Network topology
    // ... additional state
}
```

### Persistence and Recovery

- **Write-Ahead Logging**: All state changes are logged before application
- **Atomic Operations**: State updates use compare-and-swap operations  
- **Checksum Verification**: Data integrity checks on read/write operations
- **Corruption Recovery**: Automatic detection and repair of corrupted state
- **Quorum-based Sync**: State synchronization using majority consensus

### Edge Case Handling

1. **Partial Writes**: Detection through checksums and rollback capability
2. **Network Partitions**: Quorum tracking with graceful degradation  
3. **State Corruption**: Automatic detection and recovery from backups
4. **Node Failures**: Heartbeat monitoring and cluster reconfiguration
5. **Phase Cleanup**: Garbage collection of old consensus phases

## 🛠️ Development

### Building from Source

```bash
git clone https://github.com/rabia-rs/rabia
cd rabia
cargo build --release
```

### Running Tests

```bash
# Unit tests
cargo test --all

# Integration tests with fault injection
cargo test --test integration_tests

# Network simulation tests
cargo test --features network-sim network_tests
```

### Performance Profiling

```bash
# Profile with perf
cargo build --release
perf record --call-graph=dwarf target/release/examples/cluster
perf report

# Memory profiling with valgrind
cargo build
valgrind --tool=massif target/debug/examples/cluster
```

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

1. Install Rust (1.70+ required)
2. Install development dependencies:
   ```bash
   cargo install cargo-audit cargo-deny cargo-outdated
   ```
3. Run tests: `cargo test --all`
4. Check formatting: `cargo fmt --check`
5. Run lints: `cargo clippy --all-targets`

### Performance Contributions

When contributing performance improvements:

1. Include before/after benchmarks
2. Explain the optimization technique
3. Verify correctness with tests
4. Document any trade-offs

## 📋 Implementation Status

### ✅ Completed
- [x] Core trait abstractions (StateMachine, Network, Persistence)
- [x] Message types and serialization with serde
- [x] In-memory state management with concurrent data structures  
- [x] Async/await based RabiaEngine with tokio
- [x] State persistence interface with atomic operations
- [x] Comprehensive testing suite with network simulation
- [x] Performance optimizations (binary serialization, batching, memory pooling)
- [x] Fault injection testing framework
- [x] Production-grade error handling and validation
- [x] Production-grade KV Store with notification system
- [x] Leader Manager implementation
- [x] Topology change notifications
- [x] Consensus appearance/disappearance notifications

### 📋 Roadmap

- [x] **v0.2.0**: Production KV Store with notifications and leader management
- [x] **v0.3.0**: TCP networking and production deployment features  
- [ ] **v1.0.0**: Production stability and long-term guarantees

## 🐛 Known Limitations

- Currently implements crash fault tolerance (not Byzantine fault tolerance)
- In-memory persistence only (suitable for many use cases, external persistence can be implemented via traits)
- Network layer provides both in-memory simulation and TCP networking

## 📄 License

Licensed under the [Apache License, Version 2.0](LICENSE).

## 🙏 Acknowledgments

- Original Rabia protocol research: [Rabia: Simplifying State-Machine Replication Through Randomization (SOSP 2021)](https://www.cs.cornell.edu/~rvr/papers/rabia.pdf)
- [Original Java Implementation](https://github.com/siy/pragmatica-lite/tree/main/cluster)
- Rust async ecosystem (Tokio, Serde, etc.)
- Performance optimization techniques from the Rust community

## 📖 Documentation

- **[SMR Developer Guide](docs/SMR_GUIDE.md)** - Comprehensive guide to building SMR applications with the Rabia protocol
- **[Protocol Guide](PROTOCOL_GUIDE.md)** - Deep dive into the Rabia consensus algorithm
- **[Migration Guide](MIGRATION_GUIDE.md)** - Guide for migrating to the new SMR-focused architecture
- **[Troubleshooting Guide](TROUBLESHOOTING.md)** - Common issues and solutions
- **[Generated Docs](https://docs.rs/rabia-core)** - API documentation on docs.rs

## 📞 Support

- **Documentation**: Complete guides and API docs in this repository
- **Issues**: https://github.com/rabia-rs/rabia/issues  
- **Discussions**: https://github.com/rabia-rs/rabia/discussions

---

**Made with ❤️ and ⚡ in Rust**