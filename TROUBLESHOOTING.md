# Troubleshooting Guide

Comprehensive troubleshooting guide for the Rabia consensus library.

## Table of Contents

1. [Common Issues](#common-issues)
2. [Performance Problems](#performance-problems)
3. [Network Issues](#network-issues)
4. [State Machine Problems](#state-machine-problems)
5. [Memory and Resource Issues](#memory-and-resource-issues)
6. [Testing and Development Issues](#testing-and-development-issues)
7. [Debugging Tools](#debugging-tools)
8. [FAQ](#faq)

---

## Common Issues

### Issue: Consensus Not Making Progress

**Symptoms:**
```
WARN: Phase 123 has been active for 30s without decision
WARN: Round 1 voting stuck with no majority
INFO: Pending batches: 15 (oldest: 45s ago)
```

**Root Causes & Solutions:**

#### 1. Insufficient Cluster Size
```rust
// ❌ Problem: Too few nodes
let cluster_size = 2;  // Can't tolerate any failures

// ✅ Solution: Use at least 3 nodes  
let cluster_size = 3;  // Tolerates 1 failure
let cluster_size = 5;  // Tolerates 2 failures
let cluster_size = 7;  // Tolerates 3 failures

// Rule: 2f + 1 nodes tolerate f failures
```

#### 2. Network Connectivity Issues
```rust
// Check cluster connectivity
let connected = network.get_connected_nodes().await?;
let required_quorum = cluster_config.quorum_size;

if connected.len() < required_quorum {
    error!("❌ Insufficient connectivity: {}/{} nodes connected", 
           connected.len(), cluster_config.all_nodes.len());
    
    // Diagnostic steps:
    for node_id in &cluster_config.all_nodes {
        if !connected.contains(node_id) {
            error!("   Missing connection to node: {}", node_id);
        }
    }
}
```

#### 3. Clock Skew Issues
```rust
use std::time::{SystemTime, Duration};

// Check for clock skew between nodes
let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
let max_allowed_skew = 30; // 30 seconds

// In your message validation:
if message.timestamp.abs_diff(current_time) > max_allowed_skew {
    warn!("⚠️ Large clock skew detected: {}s difference", 
          message.timestamp.abs_diff(current_time));
}
```

### Issue: High Memory Usage

**Symptoms:**
```
WARN: Phase cache size: 10000 entries  
WARN: Pending batches: 5000 entries
WARN: Memory usage: 2.5GB (expected: <500MB)
```

**Solutions:**

#### 1. Configure Aggressive Cleanup
```rust
let config = RabiaConfig {
    max_phase_history: 50,         // Keep fewer phases
    cleanup_interval: Duration::from_secs(10),  // Clean more often
    max_pending_batches: 500,      // Limit pending operations
    max_message_queue_size: 1000,  // Prevent message buildup
    ..Default::default()
};
```

#### 2. Monitor and Force Cleanup
```rust
// Monitor memory usage
let stats = engine.get_statistics().await;
if stats.memory_usage_mb > 1000 {
    warn!("⚠️ High memory usage: {} MB", stats.memory_usage_mb);
    
    // Force immediate cleanup
    engine.send_command(EngineCommand::ForceCleanup).await?;
    
    // Trigger garbage collection if available
    #[cfg(feature = "gc")]
    std::hint::black_box(std::ptr::null::<()>());
}
```

#### 3. Use Memory Pools
```rust
use rabia_core::memory_pool::MemoryPoolConfig;

let pool_config = MemoryPoolConfig {
    initial_pool_size: 100,
    max_pool_size: 1000,
    buffer_sizes: vec![1024, 4096, 16384], // Common sizes
    enable_monitoring: true,
};

// Monitor pool efficiency
let pool_stats = memory_pool.get_statistics();
if pool_stats.hit_rate < 0.8 {
    warn!("⚠️ Low memory pool hit rate: {:.1}%", pool_stats.hit_rate * 100.0);
}
```

### Issue: Message Validation Failures

**Symptoms:**
```
ERROR: Invalid message from node_123: Checksum mismatch
ERROR: Message validation failed: Invalid phase transition  
ERROR: Dropping malformed message: Serialization error
```

**Debugging Steps:**

#### 1. Check Message Integrity
```rust
fn debug_message_validation(message: &ProtocolMessage) -> Result<()> {
    // Check basic structure
    if message.from.is_nil() {
        return Err(RabiaError::validation("Empty sender node ID"));
    }

    // Verify checksum
    let expected_checksum = message.calculate_checksum();
    if message.checksum != expected_checksum {
        error!("❌ Checksum mismatch: got {}, expected {}", 
               message.checksum, expected_checksum);
        
        // Log message details for debugging
        debug!("Message details: {:?}", message);
        return Err(RabiaError::validation("Checksum validation failed"));
    }

    // Check phase transitions
    match &message.message_type {
        MessageType::Propose(prop) => {
            if prop.phase_id.value() == 0 {
                return Err(RabiaError::validation("Invalid phase ID: 0"));
            }
        }
        // ... other message types
    }

    Ok(())
}
```

#### 2. Network Layer Debugging  
```rust
// Enable detailed network logging
use tracing::{Level, debug, error};

// In your network implementation:
async fn send_message(&self, target: NodeId, message: &[u8]) -> Result<()> {
    debug!("Sending {} bytes to {}", message.len(), target);
    
    if message.len() > self.max_message_size {
        error!("❌ Message too large: {} > {}", message.len(), self.max_message_size);
        return Err(RabiaError::network("Message size limit exceeded"));
    }

    // Log first few bytes for debugging
    debug!("Message header: {:02x?}", &message[..16.min(message.len())]);
    
    self.transport.send(target, message).await
}
```

---

## Performance Problems

### Issue: High Consensus Latency

**Symptoms:**
```
WARN: Average consensus latency: 500ms (expected: <100ms)
INFO: Phase duration breakdown: propose=50ms, vote1=200ms, vote2=180ms, decide=70ms
```

**Performance Tuning:**

#### 1. Optimize Batching
```rust
use rabia_core::batching::BatchConfig;

let config = BatchConfig {
    max_batch_size: 1000,              // Larger batches
    max_batch_delay: Duration::from_millis(2),  // Lower latency
    adaptive: true,                    // Adapt to load
    size_threshold: 50 * 1024,         // 50KB trigger size
    memory_pressure_threshold: 0.8,    // Back off when memory high
};

// Monitor batching efficiency
let batcher_stats = batcher.get_statistics();
if batcher_stats.average_batch_size < 10.0 {
    warn!("⚠️ Low batching efficiency: avg batch size {:.1}", 
          batcher_stats.average_batch_size);
}
```

#### 2. Network Optimization
```rust
use rabia_network::TcpNetworkConfig;

let network_config = TcpNetworkConfig {
    connection_timeout: Duration::from_secs(3),   // Faster timeouts
    max_message_size: 16 * 1024 * 1024,         // 16MB messages  
    buffer_config: BufferConfig {
        read_buffer_size: 256 * 1024,            // 256KB buffers
        write_buffer_size: 256 * 1024,
        message_queue_size: 5000,                // Larger queues
    },
    retry_config: RetryConfig {
        max_attempts: 3,                         // Fewer retries
        base_delay: Duration::from_millis(10),   // Shorter delays
        max_delay: Duration::from_secs(1),       // Cap max delay
        backoff_multiplier: 1.5,                 // Gentler backoff
    },
    ..Default::default()
};
```

#### 3. CPU and Threading Optimization
```rust
// Use dedicated thread pool for CPU-intensive operations
use tokio::runtime::Builder;

let rt = Builder::new_multi_thread()
    .worker_threads(num_cpus::get())
    .max_blocking_threads(512)
    .enable_all()
    .build()?;

// Parallelize batch processing where safe
use rayon::prelude::*;

let results: Vec<CommandResult> = commands
    .par_iter()  // Parallel iterator
    .map(|cmd| process_command_readonly(cmd))
    .collect();
```

### Issue: Low Throughput

**Symptoms:**
```
INFO: Throughput: 100 ops/sec (target: 1000+ ops/sec)
WARN: Queue depth: avg=50, max=500 (indicates backpressure)
```

**Optimization Strategies:**

#### 1. Increase Parallelism
```rust
// Process multiple phases concurrently where safe
use tokio::task::JoinSet;

let mut join_set = JoinSet::new();

for phase_id in ready_phases {
    let engine = engine.clone();
    join_set.spawn(async move {
        engine.process_phase_voting(phase_id).await
    });
}

// Wait for all phases to complete  
while let Some(result) = join_set.join_next().await {
    match result {
        Ok(Ok(_)) => info!("Phase completed successfully"),
        Ok(Err(e)) => error!("Phase processing failed: {}", e),
        Err(e) => error!("Task join failed: {}", e),
    }
}
```

#### 2. Pipeline Operations
```rust
// Pipeline consensus phases
struct PipelinedEngine {
    propose_queue: mpsc::Receiver<ProposeRequest>,
    vote_queue: mpsc::Receiver<VoteRequest>, 
    decision_queue: mpsc::Receiver<DecisionRequest>,
}

impl PipelinedEngine {
    async fn run_pipeline(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(propose) = self.propose_queue.recv() => {
                    // Process proposal in parallel
                    tokio::spawn(self.handle_propose(propose));
                }
                
                Some(vote) = self.vote_queue.recv() => {
                    // Process voting in parallel
                    tokio::spawn(self.handle_vote(vote));
                }
                
                Some(decision) = self.decision_queue.recv() => {
                    // Process decision in parallel
                    tokio::spawn(self.handle_decision(decision));
                }
            }
        }
    }
}
```

---

## Network Issues

### Issue: Connection Timeouts

**Symptoms:**
```
ERROR: Connection to node_456 timed out after 10s
WARN: Retry attempt 3/5 for node_789 failed
INFO: Node node_123 marked as unreachable
```

**Solutions:**

#### 1. Adaptive Timeouts
```rust
use std::time::Duration;

struct AdaptiveTimeout {
    base_timeout: Duration,
    current_timeout: Duration,
    success_count: u32,
    failure_count: u32,
}

impl AdaptiveTimeout {
    fn adjust_for_success(&mut self) {
        self.success_count += 1;
        if self.success_count > 10 {
            // Reduce timeout on consistent success
            self.current_timeout = Duration::max(
                self.base_timeout,
                self.current_timeout.mul_f32(0.9)
            );
            self.success_count = 0;
        }
    }

    fn adjust_for_failure(&mut self) {
        self.failure_count += 1;
        if self.failure_count > 3 {
            // Increase timeout on failures
            self.current_timeout = Duration::min(
                Duration::from_secs(60),
                self.current_timeout.mul_f32(1.5)
            );
            self.failure_count = 0;
        }
    }
}
```

#### 2. Connection Health Monitoring
```rust
use std::collections::HashMap;
use tokio::time::Instant;

struct ConnectionHealth {
    last_successful_send: Instant,
    consecutive_failures: u32,
    total_sent: u64,
    total_failed: u64,
    average_latency: Duration,
}

impl ConnectionHealth {
    fn is_healthy(&self) -> bool {
        let failure_rate = self.total_failed as f64 / self.total_sent.max(1) as f64;
        let recent_success = self.last_successful_send.elapsed() < Duration::from_secs(30);
        
        failure_rate < 0.1 && recent_success && self.consecutive_failures < 5
    }

    fn mark_success(&mut self, latency: Duration) {
        self.last_successful_send = Instant::now();
        self.consecutive_failures = 0;
        self.total_sent += 1;
        
        // Update rolling average latency
        self.average_latency = Duration::from_nanos(
            (self.average_latency.as_nanos() as f64 * 0.9 + 
             latency.as_nanos() as f64 * 0.1) as u64
        );
    }

    fn mark_failure(&mut self) {
        self.consecutive_failures += 1;
        self.total_sent += 1;
        self.total_failed += 1;
    }
}
```

### Issue: Network Partitions

**Symptoms:**
```
ERROR: Lost connection to 3/5 cluster nodes
WARN: Quorum lost: only 2/5 nodes reachable  
INFO: Entering read-only mode due to network partition
```

**Partition Detection & Handling:**

```rust
use std::collections::HashSet;

struct PartitionDetector {
    cluster_size: usize,
    connected_nodes: HashSet<NodeId>,
    partition_threshold: f64, // Fraction of nodes that must be unreachable
}

impl PartitionDetector {
    fn check_partition(&self) -> PartitionStatus {
        let connectivity_ratio = self.connected_nodes.len() as f64 / self.cluster_size as f64;
        
        if connectivity_ratio < 0.5 {
            PartitionStatus::MajorityLost
        } else if connectivity_ratio < (1.0 - self.partition_threshold) {
            PartitionStatus::PartialPartition
        } else {
            PartitionStatus::Healthy
        }
    }

    async fn handle_partition(&mut self, status: PartitionStatus) -> Result<()> {
        match status {
            PartitionStatus::MajorityLost => {
                warn!("🚨 Majority partition detected - entering read-only mode");
                // Stop accepting writes, serve only reads from local state
                self.enter_readonly_mode().await?;
            }
            
            PartitionStatus::PartialPartition => {
                warn!("⚠️ Partial partition detected - monitoring closely");
                // Continue operation but with increased monitoring
                self.increase_health_check_frequency().await?;
            }
            
            PartitionStatus::Healthy => {
                info!("✅ Network connectivity restored");
                self.exit_readonly_mode().await?;
            }
        }
        Ok(())
    }
}

enum PartitionStatus {
    Healthy,
    PartialPartition,
    MajorityLost,
}
```

---

## State Machine Problems

### Issue: Command Execution Failures

**Symptoms:**
```
ERROR: State machine rejected command: Invalid state transition
WARN: Command batch partially failed: 8/10 commands successful
ERROR: State machine panic: index out of bounds
```

**Debugging State Machine Issues:**

#### 1. Add Comprehensive Validation
```rust
use rabia_core::{Command, CommandResult, RabiaError};

#[async_trait]
impl StateMachine for MyStateMachine {
    async fn apply_commands(&mut self, commands: &[Command]) -> Result<Vec<CommandResult>> {
        let mut results = Vec::with_capacity(commands.len());
        
        for (i, command) in commands.iter().enumerate() {
            // Validate command before execution
            if let Err(e) = self.validate_command(command) {
                error!("Command {} validation failed: {}", i, e);
                results.push(CommandResult::error(format!("Validation failed: {}", e)));
                continue;
            }

            // Execute with error recovery
            match self.execute_command_safe(command).await {
                Ok(result) => {
                    results.push(result);
                    debug!("Command {} executed successfully", i);
                }
                Err(e) => {
                    error!("Command {} execution failed: {}", i, e);
                    results.push(CommandResult::error(format!("Execution failed: {}", e)));
                    
                    // Attempt state recovery if needed
                    if e.is_state_corruption() {
                        warn!("State corruption detected, attempting recovery");
                        self.attempt_state_recovery().await?;
                    }
                }
            }
        }
        
        Ok(results)
    }

    fn validate_command(&self, command: &Command) -> Result<()> {
        // Check command format
        if command.data.is_empty() {
            return Err(RabiaError::state_machine("Empty command data"));
        }

        // Check command size
        if command.data.len() > 1024 * 1024 {
            return Err(RabiaError::state_machine("Command too large"));
        }

        // Validate against current state
        if !self.is_command_valid_for_current_state(command) {
            return Err(RabiaError::state_machine("Command invalid for current state"));
        }

        Ok(())
    }
}
```

#### 2. State Consistency Checks
```rust
impl MyStateMachine {
    async fn verify_state_consistency(&self) -> Result<()> {
        // Check internal invariants
        let checksum = self.calculate_state_checksum();
        if checksum != self.expected_checksum {
            return Err(RabiaError::integrity(
                "State checksum mismatch - possible corruption"
            ));
        }

        // Verify data structures
        if self.primary_data.len() != self.index_data.len() {
            return Err(RabiaError::integrity(
                "Primary and index data size mismatch"
            ));
        }

        // Check business logic invariants
        let total_balance = self.accounts.values().map(|a| a.balance).sum::<i64>();
        if total_balance != self.expected_total_balance {
            return Err(RabiaError::integrity(
                "Total balance invariant violated"
            ));
        }

        info!("✅ State consistency check passed");
        Ok(())
    }

    async fn attempt_state_recovery(&mut self) -> Result<()> {
        warn!("🔧 Attempting automatic state recovery");

        // Try to rebuild indexes
        self.rebuild_indexes()?;

        // Recalculate derived data
        self.recalculate_derived_state()?;

        // Verify recovery was successful
        self.verify_state_consistency().await?;

        info!("✅ State recovery completed successfully");
        Ok(())
    }
}
```

---

## Memory and Resource Issues

### Issue: Memory Leaks

**Symptoms:**
```
WARN: Memory usage steadily increasing: 1.2GB -> 1.5GB -> 1.8GB
ERROR: Failed to allocate memory: Out of memory
INFO: GC collections: 0 (no garbage collection occurring)
```

**Memory Leak Detection:**

#### 1. Resource Tracking
```rust
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct ResourceTracker {
    active_phases: AtomicUsize,
    active_batches: AtomicUsize,
    active_connections: AtomicUsize,
    allocated_buffers: AtomicUsize,
}

impl ResourceTracker {
    fn track_phase_created(&self) {
        self.active_phases.fetch_add(1, Ordering::Relaxed);
    }

    fn track_phase_destroyed(&self) {
        self.active_phases.fetch_sub(1, Ordering::Relaxed);
    }

    fn report_stats(&self) {
        info!("📊 Resource Stats:");
        info!("   Active phases: {}", self.active_phases.load(Ordering::Relaxed));
        info!("   Active batches: {}", self.active_batches.load(Ordering::Relaxed));
        info!("   Active connections: {}", self.active_connections.load(Ordering::Relaxed));
        info!("   Allocated buffers: {}", self.allocated_buffers.load(Ordering::Relaxed));
    }
}

// Use RAII guards to track resource lifetimes
struct PhaseGuard {
    tracker: Arc<ResourceTracker>,
}

impl PhaseGuard {
    fn new(tracker: Arc<ResourceTracker>) -> Self {
        tracker.track_phase_created();
        Self { tracker }
    }
}

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        self.tracker.track_phase_destroyed();
    }
}
```

#### 2. Periodic Memory Auditing
```rust
use std::time::{Duration, Instant};

struct MemoryAuditor {
    last_audit: Instant,
    audit_interval: Duration,
    baseline_memory: usize,
    memory_samples: Vec<usize>,
}

impl MemoryAuditor {
    async fn periodic_audit(&mut self) -> Result<()> {
        if self.last_audit.elapsed() < self.audit_interval {
            return Ok(());
        }

        let current_memory = self.get_memory_usage();
        self.memory_samples.push(current_memory);

        // Keep only recent samples
        if self.memory_samples.len() > 100 {
            self.memory_samples.drain(0..50);
        }

        // Check for memory growth trend
        if self.memory_samples.len() >= 10 {
            let recent_avg = self.memory_samples.iter().rev().take(5).sum::<usize>() / 5;
            let older_avg = self.memory_samples.iter().rev().skip(5).take(5).sum::<usize>() / 5;
            
            let growth_rate = (recent_avg as f64 - older_avg as f64) / older_avg as f64;
            
            if growth_rate > 0.1 {
                warn!("🚨 Memory usage growing rapidly: {:.1}% increase", growth_rate * 100.0);
                self.trigger_cleanup().await?;
            }
        }

        self.last_audit = Instant::now();
        Ok(())
    }

    fn get_memory_usage(&self) -> usize {
        // Platform-specific memory usage detection
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|content| {
                    content.lines()
                        .find(|line| line.starts_with("VmRSS:"))
                        .and_then(|line| line.split_whitespace().nth(1))
                        .and_then(|kb| kb.parse::<usize>().ok())
                        .map(|kb| kb * 1024)
                })
                .unwrap_or(0)
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Fallback - use a rough estimate
            0
        }
    }
}
```

---

## Testing and Development Issues

### Issue: Flaky Tests

**Symptoms:**
```
FAILED test_consensus_basic - sometimes passes, sometimes fails
ERROR: test timeout after 30s (usually completes in 5s)
WARN: Race condition detected in test setup
```

**Test Stabilization:**

#### 1. Deterministic Test Setup
```rust
use tokio::time::{Duration, sleep};
use std::sync::atomic::{AtomicU64, Ordering};

// Use deterministic node IDs for reproducible tests
static TEST_NODE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn create_test_node_id() -> NodeId {
    let id = TEST_NODE_COUNTER.fetch_add(1, Ordering::Relaxed);
    NodeId::from_u64(id)  // Deterministic ID
}

// Add proper synchronization to test setup
async fn setup_test_cluster(size: usize) -> Result<Vec<TestNode>> {
    let mut nodes = Vec::new();
    
    // Create all nodes first
    for i in 0..size {
        let node = TestNode::new(create_test_node_id()).await?;
        nodes.push(node);
    }
    
    // Connect all nodes
    for i in 0..nodes.len() {
        for j in 0..nodes.len() {
            if i != j {
                nodes[i].connect_to(&nodes[j]).await?;
            }
        }
    }
    
    // Wait for all connections to stabilize
    sleep(Duration::from_millis(100)).await;
    
    // Verify all nodes are connected
    for (i, node) in nodes.iter().enumerate() {
        let connected = node.get_connected_nodes().await?;
        assert_eq!(connected.len(), size - 1, "Node {} not fully connected", i);
    }
    
    Ok(nodes)
}
```

#### 2. Robust Test Assertions
```rust
use tokio::time::timeout;

// Use timeouts for all async operations in tests
async fn wait_for_consensus(nodes: &[TestNode], expected_result: &str) -> Result<()> {
    let start = Instant::now();
    let max_wait = Duration::from_secs(10);
    
    loop {
        let mut consensus_reached = 0;
        
        for node in nodes {
            if let Ok(result) = timeout(Duration::from_millis(100), node.get_last_result()).await {
                if result.as_deref() == Some(expected_result) {
                    consensus_reached += 1;
                }
            }
        }
        
        if consensus_reached >= nodes.len() / 2 + 1 {
            return Ok(());
        }
        
        if start.elapsed() > max_wait {
            return Err(RabiaError::timeout(format!(
                "Consensus not reached after {:?}. Got {} agreements out of {} nodes",
                max_wait, consensus_reached, nodes.len()
            )));
        }
        
        sleep(Duration::from_millis(10)).await;
    }
}

// Retry flaky operations
async fn retry_operation<F, Fut, T>(mut operation: F, max_retries: u32) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_error = None;
    
    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt == max_retries {
                    return Err(e);
                }
                
                warn!("Operation failed on attempt {}: {}", attempt + 1, e);
                last_error = Some(e);
                
                // Exponential backoff
                let delay = Duration::from_millis(100 * (1 << attempt));
                sleep(delay).await;
            }
        }
    }
    
    Err(last_error.unwrap())
}
```

---

## Debugging Tools

### Enable Comprehensive Logging

```rust
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

fn setup_debugging_logs() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            // Default logging levels for debugging
            EnvFilter::new("rabia=debug,rabia_engine=trace,rabia_network=debug,warn")
        });

    tracing_subscriber::registry()
        .with(fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_file(true)
            .with_line_number(true)
            .with_level(true)
            .compact()
        )
        .with(env_filter)
        .init();

    Ok(())
}
```

### State Inspection Tools

```rust
use serde_json;

#[derive(Debug, Serialize)]
struct DebugState {
    current_phase: PhaseId,
    active_phases: Vec<PhaseId>,
    pending_batches: usize,
    connected_nodes: Vec<NodeId>,
    memory_usage_mb: usize,
    operation_stats: OperationStats,
}

impl RabiaEngine {
    pub async fn dump_debug_state(&self) -> Result<DebugState> {
        let stats = self.get_statistics().await;
        
        Ok(DebugState {
            current_phase: self.engine_state.current_phase(),
            active_phases: self.engine_state.get_active_phase_ids(),
            pending_batches: self.engine_state.pending_batches.len(),
            connected_nodes: self.network.get_connected_nodes().await?,
            memory_usage_mb: stats.memory_usage_mb,
            operation_stats: stats.operations,
        })
    }

    pub async fn export_debug_info(&self, path: &str) -> Result<()> {
        let state = self.dump_debug_state().await?;
        let json = serde_json::to_string_pretty(&state)?;
        
        tokio::fs::write(path, json).await?;
        info!("Debug state exported to {}", path);
        
        Ok(())
    }
}
```

### Performance Profiling

```rust
use std::time::{Duration, Instant};

struct PerformanceProfiler {
    operation_times: HashMap<String, Vec<Duration>>,
}

impl PerformanceProfiler {
    fn time_operation<F, R>(&mut self, name: &str, operation: F) -> R 
    where 
        F: FnOnce() -> R 
    {
        let start = Instant::now();
        let result = operation();
        let duration = start.elapsed();
        
        self.operation_times.entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(duration);
            
        result
    }

    fn report_statistics(&self) {
        info!("📊 Performance Profile:");
        
        for (name, times) in &self.operation_times {
            if !times.is_empty() {
                let total: Duration = times.iter().sum();
                let avg = total / times.len() as u32;
                let min = *times.iter().min().unwrap();
                let max = *times.iter().max().unwrap();
                
                info!("   {}: avg={:?}, min={:?}, max={:?}, count={}", 
                      name, avg, min, max, times.len());
            }
        }
    }
}

// Usage:
let mut profiler = PerformanceProfiler::new();

let result = profiler.time_operation("consensus_round", || {
    // Consensus operation
    execute_consensus_round()
});
```

---

## FAQ

### Q: Why is my consensus cluster not making progress?

**A:** Check these common issues:
1. **Cluster size**: Need at least 3 nodes (2f+1 for f failures)
2. **Network connectivity**: All nodes must be able to communicate
3. **Quorum**: Must have majority of nodes connected
4. **Clock synchronization**: Large clock skew can cause issues

### Q: How do I handle network partitions?

**A:** Rabia automatically handles partitions:
- **Majority partition**: Continue consensus operations
- **Minority partition**: Stop accepting writes, serve reads only
- **Partition healing**: Automatically resume normal operations

### Q: What's the maximum throughput I can expect?

**A:** Throughput depends on several factors:
- **Network latency**: Lower latency = higher throughput
- **Batch size**: Larger batches = better throughput 
- **Hardware**: CPU and network capacity matter
- **State machine complexity**: Simpler operations = higher throughput

Typical ranges: 1,000-10,000 operations/second in good conditions.

### Q: How do I monitor a production Rabia cluster?

**A:** Key metrics to monitor:
- **Consensus latency**: Time from propose to decision
- **Throughput**: Operations per second
- **Node connectivity**: Number of connected nodes
- **Phase progression**: Ensure phases complete normally
- **Memory usage**: Watch for memory leaks
- **Error rates**: Network and validation errors

### Q: Can I add nodes to a running cluster?

**A:** Yes, but with careful planning:
1. Start new node with current cluster configuration
2. Connect new node to existing cluster
3. Wait for state synchronization
4. Update cluster configuration on all nodes
5. Verify new node is participating in consensus

### Q: How do I backup and restore cluster state?

**A:** Use the state machine snapshot feature:
```rust
// Create backup
let snapshot = state_machine.create_snapshot().await?;
save_snapshot_to_storage(&snapshot).await?;

// Restore from backup
let snapshot = load_snapshot_from_storage().await?;
state_machine.restore_snapshot(&snapshot).await?;
```

### Q: What should I do if I suspect data corruption?

**A:**
1. **Stop writes immediately** to prevent further corruption
2. **Check logs** for integrity errors or checksum mismatches
3. **Verify state consistency** using built-in checks
4. **Restore from snapshot** if corruption is confirmed
5. **Investigate root cause** (hardware, network, software bug)

### Q: How do I tune performance for my workload?

**A:** Performance tuning checklist:
1. **Profile your workload** to identify bottlenecks
2. **Adjust batch sizes** based on operation characteristics
3. **Tune network settings** for your environment
4. **Optimize state machine** implementation
5. **Use memory pools** for frequently allocated objects
6. **Monitor and iterate** based on real-world metrics

---

For additional support:
- **Documentation**: https://docs.rs/rabia-core  
- **Issues**: Report bugs and issues on the project repository
- **Community**: Join discussions for usage questions and best practices