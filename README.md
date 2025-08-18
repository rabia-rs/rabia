# Rabia Consensus Protocol - Rust Implementation

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/rabia-core.svg)](https://crates.io/crates/rabia-core)
[![Documentation](https://docs.rs/rabia-core/badge.svg)](https://docs.rs/rabia-core)
[![CI](https://github.com/rabia-rs/rabia/actions/workflows/ci.yml/badge.svg)](https://github.com/rabia-rs/rabia/actions/workflows/ci.yml)
[![Security](https://github.com/rabia-rs/rabia/actions/workflows/security.yml/badge.svg)](https://github.com/rabia-rs/rabia/actions/workflows/security.yml)
[![codecov](https://codecov.io/gh/rabia-rs/rabia/branch/main/graph/badge.svg)](https://codecov.io/gh/rabia-rs/rabia)

A high-performance, production-ready Rust implementation of the **Rabia consensus protocol** - a randomized Byzantine-resilient consensus algorithm optimized for crash-fault tolerant systems.

## 🚀 Key Features

- **High Performance**: Intelligent batching and optimized serialization for maximum throughput
- **Production Ready**: Comprehensive error handling, recovery mechanisms, and edge case handling  
- **Memory Efficient**: Advanced memory pooling and zero-allocation serialization paths
- **Binary Serialization**: Compact binary format for efficient network communication
- **Adaptive Batching**: Intelligent command grouping that adapts to load patterns
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

## 🏗️ Architecture

Rabia-rs is organized into focused crates for modularity and reusability:

```
rabia-rs/
├── rabia-core/         # Core types, traits, and algorithms
├── rabia-engine/       # Consensus engine implementation  
├── rabia-network/      # Network transport abstractions
├── rabia-persistence/  # Persistence layer implementations
├── rabia-kvstore/      # Production-grade key-value store
├── rabia-leader/       # Leader management and cluster coordination
├── rabia-testing/      # Testing utilities and network simulation
├── examples/           # Usage examples and tutorials
└── benchmarks/         # Performance benchmarks
```

### Core Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   RabiaEngine   │────│  NetworkLayer   │────│  Persistence    │
│                 │    │                 │    │                 │
│ • Phase Mgmt    │    │ • Message Bus   │    │ • WAL           │
│ • Voting Logic  │    │ • Node Discovery│    │ • Snapshots     │
│ • State Sync    │    │ • Fault Detect  │    │ • Recovery      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                        │
         ▼                        ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  State Machine  │    │ Leader Manager  │────│    KV Store     │
│                 │    │                 │    │                 │
│ • Command Exec  │    │ • Cluster Coord │    │ • Concurrent    │
│ • Deterministic │    │ • Health Mon    │    │ • Notifications │
│ • Snapshotting  │    │ • Topology      │    │ • Snapshots     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## 🚀 Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rabia-core = "0.2"
rabia-engine = "0.2" 
rabia-kvstore = "0.2"  # Optional: for key-value storage
rabia-leader = "0.2"   # Optional: for leader management
tokio = { version = "1.0", features = ["full"] }
```

### Basic Usage

```rust
use rabia_core::{Command, NodeId, state_machine::InMemoryStateMachine};
use rabia_engine::{RabiaEngine, RabiaConfig};
use rabia_persistence::InMemoryPersistence;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a simple 3-node cluster
    let node_id = NodeId::new();
    let config = RabiaConfig::default();
    
    // Set up components
    let state_machine = InMemoryStateMachine::new();
    let persistence = InMemoryPersistence::new();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    
    // Create and run consensus engine
    let engine = RabiaEngine::new(
        node_id,
        config,
        cluster_config,
        state_machine,
        network,
        persistence,
        cmd_rx,
    );
    
    // Submit commands
    cmd_tx.send(Command::new("SET key1 value1"))?;
    cmd_tx.send(Command::new("GET key1"))?;
    
    // Run consensus
    engine.run().await?;
    
    Ok(())
}
```

### High-Performance Batching

```rust
use rabia_core::batching::{CommandBatcher, BatchConfig};
use std::time::Duration;

// Configure adaptive batching for optimal performance
let config = BatchConfig {
    max_batch_size: 100,
    max_batch_delay: Duration::from_millis(10),
    adaptive: true,
    ..Default::default()
};

let mut batcher = CommandBatcher::new(config);

// Add commands - automatically batches for efficiency
for i in 0..1000 {
    let cmd = Command::new(format!("SET key{} value{}", i, i));
    if let Some(batch) = batcher.add_command(cmd)? {
        // Process committed batch
        process_batch(batch).await?;
    }
}
```

### Binary Serialization

```rust
use rabia_core::serialization::Serializer;

// Use high-performance binary serialization
let serializer = Serializer::binary();

// High-performance binary serialization
let message = create_consensus_message();
let serialized = serializer.serialize_message(&message)?;
let deserialized = serializer.deserialize_message(&serialized)?;
```

### Production KV Store

```rust
use rabia_kvstore::{KVStore, KVStoreConfig, NotificationFilter};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure high-performance KV store
    let config = KVStoreConfig {
        max_entries: 1_000_000,
        max_memory_mb: 1024,
        enable_notifications: true,
        snapshot_interval: Duration::from_secs(300),
        ..Default::default()
    };
    
    let store = KVStore::new(config).await?;
    
    // Subscribe to change notifications
    let (sub_id, mut notifications) = store
        .subscribe(NotificationFilter::KeyPrefix("user:".to_string()))
        .await?;
    
    // Concurrent operations
    tokio::spawn(async move {
        while let Some(notification) = notifications.recv().await {
            println!("Key changed: {:?}", notification);
        }
    });
    
    // High-performance operations
    store.set("user:1".to_string(), "data".into()).await?;
    let value = store.get("user:1").await?;
    
    // Atomic transactions
    store.transaction(|tx| async move {
        tx.set("key1".to_string(), "value1".into()).await?;
        tx.set("key2".to_string(), "value2".into()).await?;
        Ok(())
    }).await?;
    
    Ok(())
}
```

### Leader Management & Cluster Coordination

```rust
use rabia_leader::{
    LeaderManager, LeaderConfig, 
    LeaderNotificationBus, NotificationFilter
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure leader management
    let config = LeaderConfig {
        heartbeat_interval: Duration::from_millis(1000),
        election_timeout: Duration::from_millis(5000),
        min_healthy_nodes: 3,
        auto_failover: true,
        leadership_priority: 100,
    };
    
    let leader_manager = std::sync::Arc::new(LeaderManager::new(config).await?);
    let notification_bus = LeaderNotificationBus::new();
    
    // Subscribe to leadership events
    let (sub_id, mut leadership_events) = notification_bus
        .subscribe(NotificationFilter::Leadership)
        .await?;
    
    // Handle leadership changes
    tokio::spawn(async move {
        while let Some(event) = leadership_events.recv().await {
            match event {
                LeaderNotification::Leadership(LeadershipChange::LeaderElected { node_id, term, .. }) => {
                    println!("New leader elected: {} (term {})", node_id, term);
                }
                LeaderNotification::Topology(TopologyChange::QuorumChanged { has_quorum, .. }) => {
                    println!("Cluster quorum: {}", has_quorum);
                }
                _ => {}
            }
        }
    });
    
    // Start leader management
    leader_manager.clone().start().await?;
    
    // Check leadership status
    if leader_manager.is_leader().await {
        println!("This node is the cluster leader");
    }
    
    Ok(())
}
```

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

## 📚 Examples

The `examples/` directory contains comprehensive examples:

- **[Basic Usage](examples/basic_usage.rs)** - Simple consensus setup
- **[Multi-Node Cluster](examples/cluster.rs)** - Full cluster implementation  
- **[Performance Tuning](examples/performance.rs)** - Optimization techniques
- **[Fault Tolerance](examples/fault_tolerance.rs)** - Handling failures
- **[Custom State Machine](examples/custom_state_machine.rs)** - Implementing your own state machine

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

### Pre-built Images

Pull pre-built images from Docker Hub:

```bash
# Latest release
docker pull rabiars/rabia:latest

# Specific version
docker pull rabiars/rabia:v0.2.0
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

## 🔬 Protocol Details

The Rabia consensus protocol provides:

- **No Leader, No Single Point of Failure**: Unlike Raft or PBFT, Rabia has no leader election delays or single points of failure
- **Transparent Node Management**: Adding/removing nodes is virtually transparent to cluster operation
- **Randomized Agreement**: Uses randomization to achieve consensus efficiently
- **Crash Fault Tolerance**: Handles node crashes and network partitions
- **Low Latency**: Typically 2-3 communication rounds for decision
- **High Throughput**: Optimized for batch processing scenarios
- **Simplicity**: Easier to understand and implement than traditional consensus protocols

### Consensus Phases

1. **Propose Phase**: Any node can propose a value
2. **Vote Round 1**: Nodes vote with randomization
3. **Vote Round 2**: Final voting based on Round 1 results  
4. **Decision**: Commit the agreed value

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
- [ ] **v0.3.0**: TCP networking and production deployment features  
- [ ] **v1.0.0**: Production stability and long-term guarantees

## 🐛 Known Limitations

- Currently implements crash fault tolerance (not Byzantine fault tolerance)
- In-memory persistence only (suitable for many use cases, external persistence can be implemented via traits)
- Network layer uses in-memory simulation (TCP networking planned for v0.3.0)

## 📄 License

Licensed under the [Apache License, Version 2.0](LICENSE).

## 🙏 Acknowledgments

- Original Rabia protocol research: [Rabia: Simplifying State-Machine Replication Through Randomization (SOSP 2021)](https://www.cs.cornell.edu/~rvr/papers/rabia.pdf)
- [Original Java Implementation](https://github.com/siy/pragmatica-lite/tree/main/cluster)
- Rust async ecosystem (Tokio, Serde, etc.)
- Performance optimization techniques from the Rust community

## 📞 Support

- **Documentation**: https://docs.rs/rabia-core
- **Issues**: https://github.com/rabia-rs/rabia/issues  
- **Discussions**: https://github.com/rabia-rs/rabia/discussions

---

**Made with ❤️ and ⚡ in Rust**