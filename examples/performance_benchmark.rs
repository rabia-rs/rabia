//! # Performance Benchmark Example
//!
//! This example demonstrates the performance characteristics of the Rabia
//! consensus protocol under various conditions and workloads.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing::info;

use rabia_core::{
    batching::{BatchConfig, BatchProcessor},
    memory_pool::{MemoryPool, PoolConfig},
    serialization::{BinarySerializer, MessageSerializer},
    Command, CommandBatch,
};
use rabia_kvstore::{KVStore, KVStoreConfig};

/// Benchmark configuration
#[derive(Clone)]
struct BenchmarkConfig {
    /// Number of operations to perform
    pub operation_count: usize,
    /// Batch size for batched operations
    pub batch_size: usize,
    /// Number of concurrent workers
    pub concurrency: usize,
    /// Value size in bytes
    pub value_size: usize,
    /// Enable memory pooling
    pub use_memory_pool: bool,
    /// Enable binary serialization
    pub use_binary_serialization: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            operation_count: 10_000,
            batch_size: 100,
            concurrency: 10,
            value_size: 256,
            use_memory_pool: true,
            use_binary_serialization: true,
        }
    }
}

/// Benchmark results
#[derive(Debug)]
struct BenchmarkResults {
    pub test_name: String,
    pub total_operations: usize,
    pub duration: Duration,
    pub operations_per_second: f64,
    pub avg_latency_us: f64,
    pub memory_usage_mb: f64,
}

impl BenchmarkResults {
    fn new(test_name: String, total_operations: usize, duration: Duration) -> Self {
        let operations_per_second = total_operations as f64 / duration.as_secs_f64();
        let avg_latency_us = duration.as_micros() as f64 / total_operations as f64;

        Self {
            test_name,
            total_operations,
            duration,
            operations_per_second,
            avg_latency_us,
            memory_usage_mb: 0.0, // Will be updated separately
        }
    }

    fn print(&self) {
        println!("📊 {}", self.test_name);
        println!("   - Operations: {}", self.total_operations);
        println!("   - Duration: {:?}", self.duration);
        println!("   - Throughput: {:.2} ops/sec", self.operations_per_second);
        println!("   - Avg Latency: {:.2} μs", self.avg_latency_us);
        if self.memory_usage_mb > 0.0 {
            println!("   - Memory Usage: {:.2} MB", self.memory_usage_mb);
        }
        println!();
    }
}

/// Benchmark suite for Rabia performance testing
struct PerformanceBenchmark {
    config: BenchmarkConfig,
    memory_pool: Arc<MemoryPool>,
    serializer: Arc<BinarySerializer>,
}

impl PerformanceBenchmark {
    fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            memory_pool: Arc::new(MemoryPool::new(PoolConfig::default())),
            serializer: Arc::new(BinarySerializer),
        }
    }

    /// Benchmark basic KVStore operations
    async fn benchmark_kvstore_basic(&self) -> BenchmarkResults {
        let store_config = KVStoreConfig {
            max_keys: self.config.operation_count * 2,
            enable_notifications: false, // Disable for pure performance
            ..Default::default()
        };

        let store = KVStore::new(store_config).await.unwrap();
        let value = "x".repeat(self.config.value_size);

        let start = Instant::now();

        // Perform SET operations
        for i in 0..self.config.operation_count {
            let key = format!("key_{}", i);
            store.set(&key, &value).await.unwrap();
        }

        let duration = start.elapsed();
        BenchmarkResults::new(
            "KVStore Basic Operations".to_string(),
            self.config.operation_count,
            duration,
        )
    }

    /// Benchmark batched KVStore operations
    async fn benchmark_kvstore_batched(&self) -> BenchmarkResults {
        let store_config = KVStoreConfig {
            max_keys: self.config.operation_count * 2,
            enable_notifications: false,
            ..Default::default()
        };

        let store = KVStore::new(store_config).await.unwrap();
        let value = "x".repeat(self.config.value_size);

        let start = Instant::now();

        // Create batches
        let mut batch_ops = Vec::new();
        for i in 0..self.config.operation_count {
            let key = format!("batch_key_{}", i);
            batch_ops.push(rabia_kvstore::KVOperation::Set {
                key,
                value: value.clone(),
            });

            // Process batch when it reaches the configured size
            if batch_ops.len() >= self.config.batch_size {
                store.apply_batch(batch_ops.clone()).await.unwrap();
                batch_ops.clear();
            }
        }

        // Process remaining operations
        if !batch_ops.is_empty() {
            store.apply_batch(batch_ops).await.unwrap();
        }

        let duration = start.elapsed();
        BenchmarkResults::new(
            "KVStore Batched Operations".to_string(),
            self.config.operation_count,
            duration,
        )
    }

    /// Benchmark concurrent KVStore operations
    async fn benchmark_kvstore_concurrent(&self) -> BenchmarkResults {
        let store_config = KVStoreConfig {
            max_keys: self.config.operation_count * 2,
            enable_notifications: false,
            ..Default::default()
        };

        let store = Arc::new(KVStore::new(store_config).await.unwrap());
        let value = "x".repeat(self.config.value_size);
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));

        let start = Instant::now();

        let mut handles = Vec::new();
        let ops_per_worker = self.config.operation_count / self.config.concurrency;

        for worker_id in 0..self.config.concurrency {
            let store = store.clone();
            let value = value.clone();
            let semaphore = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                for i in 0..ops_per_worker {
                    let key = format!("concurrent_{}_{}", worker_id, i);
                    store.set(&key, &value).await.unwrap();
                }
            });

            handles.push(handle);
        }

        // Wait for all workers to complete
        for handle in handles {
            handle.await.unwrap();
        }

        let duration = start.elapsed();
        BenchmarkResults::new(
            "KVStore Concurrent Operations".to_string(),
            self.config.operation_count,
            duration,
        )
    }

    /// Benchmark command batch creation and processing
    async fn benchmark_command_batching(&self) -> BenchmarkResults {
        let _batch_config = BatchConfig {
            max_batch_size: self.config.batch_size,
            max_batch_delay: Duration::from_millis(10),
            buffer_capacity: 1000,
            adaptive: false,
        };

        let _batch_processor = BatchProcessor::new();
        let value = "x".repeat(self.config.value_size);

        let start = Instant::now();

        // Create individual commands
        let mut commands = Vec::new();
        for i in 0..self.config.operation_count {
            let cmd_data = format!("SET key_{} {}", i, value);
            commands.push(Command::new(cmd_data));
        }

        // Process commands in batches
        let mut processed = 0;
        for chunk in commands.chunks(self.config.batch_size) {
            let batch = CommandBatch::new(chunk.to_vec());

            // Simulate batch processing
            let _checksum = batch.checksum();
            processed += chunk.len();
        }

        let duration = start.elapsed();
        BenchmarkResults::new("Command Batching".to_string(), processed, duration)
    }

    /// Benchmark memory pool performance
    async fn benchmark_memory_pool(&self) -> BenchmarkResults {
        let pool = &self.memory_pool;
        let value_size = self.config.value_size;

        let start = Instant::now();

        let mut allocations = Vec::new();

        // Allocate from pool
        for _ in 0..self.config.operation_count {
            if self.config.use_memory_pool {
                let pooled = pool.get_buffer(value_size);
                allocations.push(pooled);
            } else {
                // Standard allocation for comparison
                let _vec = vec![0u8; value_size];
                let pooled = pool.get_buffer(value_size);
                allocations.push(pooled);
            }
        }

        // Clear allocations to trigger pool recycling
        allocations.clear();

        let duration = start.elapsed();
        BenchmarkResults::new(
            "Memory Pool Operations".to_string(),
            self.config.operation_count,
            duration,
        )
    }

    /// Benchmark serialization performance
    async fn benchmark_serialization(&self) -> BenchmarkResults {
        let commands = (0..self.config.operation_count)
            .map(|i| {
                let data = format!("SET key_{} {}", i, "x".repeat(self.config.value_size));
                Command::new(data)
            })
            .collect::<Vec<_>>();

        let batch = CommandBatch::new(commands);

        let start = Instant::now();

        if self.config.use_binary_serialization {
            // Binary serialization
            for _ in 0..100 {
                // Repeat to get meaningful timing
                let _serialized = self.serializer.serialize(&batch).unwrap();
            }
        } else {
            // JSON serialization for comparison
            for _ in 0..100 {
                let _serialized = serde_json::to_vec(&batch).unwrap();
            }
        }

        let duration = start.elapsed();
        let effective_operations = self.config.operation_count * 100;

        BenchmarkResults::new(
            format!(
                "Serialization ({})",
                if self.config.use_binary_serialization {
                    "Binary"
                } else {
                    "JSON"
                }
            ),
            effective_operations,
            duration,
        )
    }

    /// Run all benchmarks
    async fn run_all_benchmarks(&self) -> Vec<BenchmarkResults> {
        let mut results = Vec::new();

        info!("Starting performance benchmarks...");

        results.push(self.benchmark_kvstore_basic().await);
        sleep(Duration::from_millis(100)).await;

        results.push(self.benchmark_kvstore_batched().await);
        sleep(Duration::from_millis(100)).await;

        results.push(self.benchmark_kvstore_concurrent().await);
        sleep(Duration::from_millis(100)).await;

        results.push(self.benchmark_command_batching().await);
        sleep(Duration::from_millis(100)).await;

        results.push(self.benchmark_memory_pool().await);
        sleep(Duration::from_millis(100)).await;

        results.push(self.benchmark_serialization().await);

        results
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 Rabia Performance Benchmark Suite");
    println!("====================================\n");

    // Configuration
    let config = BenchmarkConfig {
        operation_count: 50_000,
        batch_size: 100,
        concurrency: 20,
        value_size: 256,
        use_memory_pool: true,
        use_binary_serialization: true,
    };

    println!("📋 Benchmark Configuration:");
    println!("   - Operations: {}", config.operation_count);
    println!("   - Batch Size: {}", config.batch_size);
    println!("   - Concurrency: {}", config.concurrency);
    println!("   - Value Size: {} bytes", config.value_size);
    println!("   - Memory Pool: {}", config.use_memory_pool);
    println!(
        "   - Binary Serialization: {}",
        config.use_binary_serialization
    );
    println!();

    // Run benchmarks
    let benchmark = PerformanceBenchmark::new(config.clone());
    let results = benchmark.run_all_benchmarks().await;

    // Display results
    println!("📊 Benchmark Results:");
    println!("====================\n");

    for result in &results {
        result.print();
    }

    // Summary analysis
    println!("📈 Performance Analysis:");
    println!("-----------------------");

    let kvstore_basic = &results[0];
    let kvstore_batched = &results[1];
    let kvstore_concurrent = &results[2];

    let batching_improvement =
        kvstore_batched.operations_per_second / kvstore_basic.operations_per_second;
    let concurrency_improvement =
        kvstore_concurrent.operations_per_second / kvstore_basic.operations_per_second;

    println!(
        "✅ Batching provides {:.2}x throughput improvement",
        batching_improvement
    );
    println!(
        "✅ Concurrency provides {:.2}x throughput improvement",
        concurrency_improvement
    );

    // Memory usage estimation
    let estimated_memory_mb = (config.operation_count * config.value_size) as f64 / 1024.0 / 1024.0;
    println!("✅ Estimated memory usage: {:.2} MB", estimated_memory_mb);

    // Performance targets
    println!("\n🎯 Performance Targets:");
    println!("----------------------");

    let target_ops_per_sec = 100_000.0;
    let best_throughput = results
        .iter()
        .map(|r| r.operations_per_second)
        .fold(0.0, f64::max);

    if best_throughput >= target_ops_per_sec {
        println!(
            "✅ Target throughput achieved: {:.0} ops/sec >= {:.0} ops/sec",
            best_throughput, target_ops_per_sec
        );
    } else {
        println!(
            "⚠️ Target throughput not reached: {:.0} ops/sec < {:.0} ops/sec",
            best_throughput, target_ops_per_sec
        );
    }

    // Comparison with different configurations
    println!("\n🔄 Configuration Comparison:");
    println!("---------------------------");

    // Test with different batch sizes
    for &batch_size in &[10, 50, 100, 500, 1000] {
        let test_config = BenchmarkConfig {
            operation_count: 10_000,
            batch_size,
            ..config
        };

        let test_benchmark = PerformanceBenchmark::new(test_config);
        let result = test_benchmark.benchmark_kvstore_batched().await;

        println!(
            "   Batch size {}: {:.0} ops/sec",
            batch_size, result.operations_per_second
        );
        sleep(Duration::from_millis(50)).await;
    }

    println!("\n✅ Performance benchmark suite completed successfully!");

    Ok(())
}
