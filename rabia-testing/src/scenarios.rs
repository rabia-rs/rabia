use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{info, warn};

use rabia_core::{
    network::ClusterConfig, state_machine::InMemoryStateMachine, Command, CommandBatch, NodeId,
};
use rabia_engine::{EngineCommand, EngineCommandSender, RabiaConfig, RabiaEngine};
use rabia_persistence::InMemoryPersistence;

use crate::network_sim::{NetworkConditions, NetworkSimulator, SimulatedNetwork};

#[derive(Debug, Clone)]
pub struct PerformanceTest {
    pub name: String,
    pub description: String,
    pub node_count: usize,
    pub total_operations: usize,
    pub operations_per_second: usize,
    pub batch_size: usize,
    pub test_duration: Duration,
    pub network_conditions: NetworkConditions,
}

#[derive(Debug)]
pub struct PerformanceResult {
    pub test_name: String,
    pub total_operations: usize,
    pub successful_operations: usize,
    pub failed_operations: usize,
    pub test_duration: Duration,
    pub throughput_ops_per_sec: f64,
    pub average_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub network_stats: crate::network_sim::NetworkStats,
    pub memory_usage_mb: f64,
}

pub struct PerformanceBenchmark {
    simulator: Arc<NetworkSimulator>,
    nodes: HashMap<NodeId, Arc<BenchmarkNode>>,
}

struct BenchmarkNode {
    node_id: NodeId,
    engine_tx: EngineCommandSender,
    #[allow(dead_code)]
    engine_handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug)]
struct OperationResult {
    latency: Duration,
    success: bool,
    timestamp: Instant,
}

impl PerformanceBenchmark {
    pub async fn new(node_count: usize, config: RabiaConfig) -> Self {
        let simulator = Arc::new(NetworkSimulator::new());
        let mut nodes = HashMap::new();
        let mut all_node_ids = std::collections::HashSet::new();

        // Create all node IDs first
        for _ in 0..node_count {
            all_node_ids.insert(NodeId::new());
        }

        // Create nodes with optimized configuration
        for &node_id in &all_node_ids {
            let cluster_config = ClusterConfig::new(node_id, all_node_ids.clone());
            let state_machine = InMemoryStateMachine::new();
            let network = SimulatedNetwork::new(node_id, simulator.clone()).await;
            let persistence = InMemoryPersistence::new();

            network.connect_to_nodes(all_node_ids.clone()).await;

            let (engine_tx, engine_rx) = mpsc::unbounded_channel();

            let engine = RabiaEngine::new(
                node_id,
                config.clone(),
                cluster_config,
                state_machine,
                network,
                persistence,
                engine_rx,
            );

            let engine_handle = tokio::spawn(async move {
                if let Err(e) = engine.run().await {
                    warn!("Engine for node {} failed: {}", node_id, e);
                }
            });

            let benchmark_node = Arc::new(BenchmarkNode {
                node_id,
                engine_tx,
                engine_handle,
            });

            nodes.insert(node_id, benchmark_node);
        }

        // Start network simulation
        let sim_clone = simulator.clone();
        tokio::spawn(async move {
            sim_clone.run_simulation().await;
        });

        Self { simulator, nodes }
    }

    pub async fn run_performance_test(&self, test: PerformanceTest) -> PerformanceResult {
        info!("Running performance test: {}", test.name);

        // Configure network conditions
        self.simulator
            .update_conditions(test.network_conditions.clone())
            .await;

        let start_time = Instant::now();
        let mut operation_results = Vec::new();
        let mut successful_operations = 0;
        let mut failed_operations = 0;

        // Calculate timing
        let operations_interval =
            Duration::from_nanos((1_000_000_000 / test.operations_per_second as u64).max(1));

        let mut operation_count = 0;
        let node_ids: Vec<NodeId> = self.nodes.keys().copied().collect();

        // Run performance test
        while operation_count < test.total_operations && start_time.elapsed() < test.test_duration {
            let batch_start = Instant::now();

            // Create batch of operations
            let mut batch_commands = Vec::new();
            for i in 0..test.batch_size.min(test.total_operations - operation_count) {
                let command = self.generate_command(operation_count + i);
                batch_commands.push(command);
            }

            if !batch_commands.is_empty() {
                let batch = CommandBatch::new(batch_commands);

                // Select node to submit to (round-robin)
                let node_id = node_ids[operation_count % node_ids.len()];

                if let Some(node) = self.nodes.get(&node_id) {
                    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

                    let cmd = EngineCommand::ProcessBatch(rabia_engine::CommandRequest {
                        batch,
                        response_tx,
                    });

                    let submit_time = Instant::now();

                    if node.engine_tx.send(cmd).is_ok() {
                        // Wait for response with timeout
                        match tokio::time::timeout(Duration::from_secs(5), response_rx).await {
                            Ok(Ok(_results)) => {
                                let latency = submit_time.elapsed();
                                operation_results.push(OperationResult {
                                    latency,
                                    success: true,
                                    timestamp: submit_time,
                                });
                                successful_operations += test.batch_size;
                            }
                            Ok(Err(_)) | Err(_) => {
                                let latency = submit_time.elapsed();
                                operation_results.push(OperationResult {
                                    latency,
                                    success: false,
                                    timestamp: submit_time,
                                });
                                failed_operations += test.batch_size;
                            }
                        }
                    } else {
                        failed_operations += test.batch_size;
                    }
                }

                operation_count += test.batch_size;
            }

            // Rate limiting
            let elapsed = batch_start.elapsed();
            if elapsed < operations_interval {
                sleep(operations_interval - elapsed).await;
            }
        }

        let total_duration = start_time.elapsed();
        let network_stats = self.simulator.get_stats().await;

        // Calculate performance metrics
        let successful_latencies: Vec<Duration> = operation_results
            .iter()
            .filter(|r| r.success)
            .map(|r| r.latency)
            .collect();

        let average_latency = if !successful_latencies.is_empty() {
            successful_latencies.iter().sum::<Duration>() / successful_latencies.len() as u32
        } else {
            Duration::ZERO
        };

        let mut sorted_latencies = successful_latencies.clone();
        sorted_latencies.sort();

        let p95_latency = if !sorted_latencies.is_empty() {
            let p95_index = (sorted_latencies.len() as f64 * 0.95) as usize;
            sorted_latencies[p95_index.min(sorted_latencies.len() - 1)]
        } else {
            Duration::ZERO
        };

        let p99_latency = if !sorted_latencies.is_empty() {
            let p99_index = (sorted_latencies.len() as f64 * 0.99) as usize;
            sorted_latencies[p99_index.min(sorted_latencies.len() - 1)]
        } else {
            Duration::ZERO
        };

        let throughput_ops_per_sec = if total_duration.as_secs_f64() > 0.0 {
            successful_operations as f64 / total_duration.as_secs_f64()
        } else {
            0.0
        };

        PerformanceResult {
            test_name: test.name,
            total_operations: operation_count,
            successful_operations,
            failed_operations,
            test_duration: total_duration,
            throughput_ops_per_sec,
            average_latency,
            p95_latency,
            p99_latency,
            network_stats,
            memory_usage_mb: self.estimate_memory_usage().await,
        }
    }

    fn generate_command(&self, operation_id: usize) -> Command {
        // Generate different types of commands for realistic workload
        match operation_id % 4 {
            0 => Command::new(format!("SET key{} value{}", operation_id, operation_id)),
            1 => Command::new(format!("GET key{}", operation_id / 4)),
            2 => Command::new(format!("SET shared_key value{}", operation_id)),
            3 => Command::new("GET shared_key"),
            _ => unreachable!(),
        }
    }

    async fn estimate_memory_usage(&self) -> f64 {
        // Rough estimation of memory usage
        // In a real implementation, we'd use system metrics
        let base_memory_per_node = 10.0; // MB
        let network_memory = 5.0; // MB for network simulation

        (self.nodes.len() as f64 * base_memory_per_node) + network_memory
    }

    pub async fn shutdown(&self) {
        self.simulator.shutdown().await;

        for node in self.nodes.values() {
            let _ = node.engine_tx.send(EngineCommand::Shutdown);
        }
    }
}

pub fn create_performance_tests() -> Vec<PerformanceTest> {
    vec![
        PerformanceTest {
            name: "Baseline Throughput".to_string(),
            description: "Maximum throughput with ideal network conditions".to_string(),
            node_count: 3,
            total_operations: 1000,
            operations_per_second: 100,
            batch_size: 10,
            test_duration: Duration::from_secs(30),
            network_conditions: NetworkConditions::default(),
        },
        PerformanceTest {
            name: "High Load".to_string(),
            description: "High throughput test with larger batches".to_string(),
            node_count: 5,
            total_operations: 5000,
            operations_per_second: 500,
            batch_size: 50,
            test_duration: Duration::from_secs(60),
            network_conditions: NetworkConditions::default(),
        },
        PerformanceTest {
            name: "Network Latency Impact".to_string(),
            description: "Performance with realistic network latency".to_string(),
            node_count: 3,
            total_operations: 1000,
            operations_per_second: 50,
            batch_size: 10,
            test_duration: Duration::from_secs(45),
            network_conditions: NetworkConditions {
                latency_min: Duration::from_millis(10),
                latency_max: Duration::from_millis(50),
                packet_loss_rate: 0.0,
                partition_probability: 0.0,
                bandwidth_limit: None,
            },
        },
        PerformanceTest {
            name: "Packet Loss Resilience".to_string(),
            description: "Performance under moderate packet loss".to_string(),
            node_count: 3,
            total_operations: 500,
            operations_per_second: 25,
            batch_size: 5,
            test_duration: Duration::from_secs(60),
            network_conditions: NetworkConditions {
                latency_min: Duration::from_millis(5),
                latency_max: Duration::from_millis(20),
                packet_loss_rate: 0.05, // 5% packet loss
                partition_probability: 0.0,
                bandwidth_limit: None,
            },
        },
        PerformanceTest {
            name: "Large Cluster".to_string(),
            description: "Scalability test with larger cluster".to_string(),
            node_count: 7,
            total_operations: 2000,
            operations_per_second: 100,
            batch_size: 20,
            test_duration: Duration::from_secs(45),
            network_conditions: NetworkConditions {
                latency_min: Duration::from_millis(5),
                latency_max: Duration::from_millis(25),
                packet_loss_rate: 0.01, // 1% packet loss
                partition_probability: 0.0,
                bandwidth_limit: None,
            },
        },
        PerformanceTest {
            name: "Small Batches".to_string(),
            description: "Performance with small batch sizes".to_string(),
            node_count: 3,
            total_operations: 1000,
            operations_per_second: 200,
            batch_size: 1,
            test_duration: Duration::from_secs(30),
            network_conditions: NetworkConditions::default(),
        },
    ]
}

pub async fn run_all_performance_tests(config: RabiaConfig) -> Vec<PerformanceResult> {
    let tests = create_performance_tests();
    let mut results = Vec::new();

    for test in tests {
        info!("Starting performance test: {}", test.name);

        // Create fresh benchmark instance for each test
        let benchmark = PerformanceBenchmark::new(test.node_count, config.clone()).await;

        // Wait for nodes to initialize
        sleep(Duration::from_millis(500)).await;

        let result = benchmark.run_performance_test(test).await;

        info!(
            "Test '{}' completed: {:.2} ops/sec, {:.2}ms avg latency",
            result.test_name,
            result.throughput_ops_per_sec,
            result.average_latency.as_millis()
        );

        results.push(result);

        benchmark.shutdown().await;

        // Brief pause between tests
        sleep(Duration::from_millis(1000)).await;
    }

    results
}

pub fn print_performance_summary(results: &[PerformanceResult]) {
    println!("\n=== PERFORMANCE TEST SUMMARY ===");
    println!(
        "{:<25} {:<12} {:<15} {:<15} {:<15} {:<15}",
        "Test Name", "Ops/Sec", "Avg Latency", "P95 Latency", "P99 Latency", "Success Rate"
    );
    println!("{}", "-".repeat(100));

    for result in results {
        let success_rate = if result.total_operations > 0 {
            (result.successful_operations as f64 / result.total_operations as f64) * 100.0
        } else {
            0.0
        };

        println!(
            "{:<25} {:<12.1} {:<15.2} {:<15.2} {:<15.2} {:<15.1}%",
            result.test_name,
            result.throughput_ops_per_sec,
            result.average_latency.as_millis(),
            result.p95_latency.as_millis(),
            result.p99_latency.as_millis(),
            success_rate
        );
    }

    println!("\n=== NETWORK STATISTICS ===");
    if let Some(result) = results.first() {
        let stats = &result.network_stats;
        println!("Total Messages Sent: {}", stats.messages_sent);
        println!("Total Messages Delivered: {}", stats.messages_delivered);
        println!("Total Messages Dropped: {}", stats.messages_dropped);
        println!(
            "Average Network Latency: {:.2}ms",
            stats.average_latency().as_millis()
        );
        println!(
            "Network Throughput: {:.2} Mbps",
            stats.throughput_mbps(result.test_duration)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_baseline_performance() {
        let config = RabiaConfig::default();
        let benchmark = PerformanceBenchmark::new(3, config).await;

        let test = PerformanceTest {
            name: "Test Baseline".to_string(),
            description: "Basic performance test".to_string(),
            node_count: 3,
            total_operations: 50,
            operations_per_second: 25,
            batch_size: 5,
            test_duration: Duration::from_secs(10),
            network_conditions: NetworkConditions::default(),
        };

        let result = timeout(
            Duration::from_secs(15),
            benchmark.run_performance_test(test),
        )
        .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.throughput_ops_per_sec > 0.0);
        assert!(result.successful_operations > 0);

        benchmark.shutdown().await;
    }

    #[tokio::test]
    async fn test_performance_with_latency() {
        let config = RabiaConfig::default();
        let benchmark = PerformanceBenchmark::new(3, config).await;

        let test = PerformanceTest {
            name: "Test Latency".to_string(),
            description: "Performance test with network latency".to_string(),
            node_count: 3,
            total_operations: 20,
            operations_per_second: 10,
            batch_size: 2,
            test_duration: Duration::from_secs(8),
            network_conditions: NetworkConditions {
                latency_min: Duration::from_millis(20),
                latency_max: Duration::from_millis(50),
                packet_loss_rate: 0.0,
                partition_probability: 0.0,
                bandwidth_limit: None,
            },
        };

        let result = timeout(
            Duration::from_secs(15),
            benchmark.run_performance_test(test),
        )
        .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // With added latency, we expect some successful operations but lower throughput
        assert!(result.successful_operations > 0);

        benchmark.shutdown().await;
    }
}
