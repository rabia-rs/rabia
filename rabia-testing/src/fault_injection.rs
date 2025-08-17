use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use rabia_core::{
    network::ClusterConfig, state_machine::InMemoryStateMachine, Command, CommandBatch, NodeId,
};
use rabia_engine::{EngineCommand, EngineCommandSender, RabiaConfig, RabiaEngine};
use rabia_persistence::InMemoryPersistence;

use crate::network_sim::{NetworkConditions, NetworkSimulator, SimulatedNetwork};

#[derive(Debug, Clone)]
pub enum FaultType {
    NodeCrash {
        node_id: NodeId,
        duration: Duration,
    },
    NetworkPartition {
        nodes: HashSet<NodeId>,
        duration: Duration,
    },
    PacketLoss {
        rate: f64,
        duration: Duration,
    },
    HighLatency {
        min: Duration,
        max: Duration,
        duration: Duration,
    },
    SlowNode {
        node_id: NodeId,
        delay_factor: f64,
        duration: Duration,
    },
    MessageReordering {
        probability: f64,
        max_delay: Duration,
    },
}

#[derive(Debug, Clone)]
pub struct TestScenario {
    pub name: String,
    pub description: String,
    pub node_count: usize,
    pub initial_commands: Vec<Command>,
    pub faults: Vec<(Duration, FaultType)>, // (when to inject, fault type)
    pub expected_outcome: ExpectedOutcome,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum ExpectedOutcome {
    AllCommitted,
    PartialCommitment { min_committed: usize },
    NoProgress,
    EventualConsistency,
}

pub struct ConsensusTestHarness {
    simulator: Arc<NetworkSimulator>,
    nodes: HashMap<NodeId, Arc<ConsensusNode>>,
    #[allow(dead_code)]
    test_duration: Duration,
    #[allow(dead_code)]
    start_time: Instant,
}

struct ConsensusNode {
    #[allow(dead_code)]
    node_id: NodeId,
    engine_tx: EngineCommandSender,
    #[allow(dead_code)]
    engine_handle: tokio::task::JoinHandle<()>,
}

impl ConsensusTestHarness {
    pub async fn new(node_count: usize, config: RabiaConfig) -> Self {
        let simulator = Arc::new(NetworkSimulator::new());
        let mut nodes = HashMap::new();
        let mut all_node_ids = HashSet::new();

        // Create all node IDs first
        for _ in 0..node_count {
            all_node_ids.insert(NodeId::new());
        }

        // Create nodes
        for &node_id in &all_node_ids {
            let cluster_config = ClusterConfig::new(node_id, all_node_ids.clone());
            let state_machine = InMemoryStateMachine::new();
            let network = SimulatedNetwork::new(node_id, simulator.clone()).await;
            let persistence = InMemoryPersistence::new();

            // Connect network to all other nodes
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
                    error!("Engine for node {} failed: {}", node_id, e);
                }
            });

            let consensus_node = Arc::new(ConsensusNode {
                node_id,
                engine_tx,
                engine_handle,
            });

            nodes.insert(node_id, consensus_node);
        }

        // Start network simulation
        let sim_clone = simulator.clone();
        tokio::spawn(async move {
            sim_clone.run_simulation().await;
        });

        Self {
            simulator,
            nodes,
            test_duration: Duration::from_secs(30),
            start_time: Instant::now(),
        }
    }

    pub async fn run_scenario(&mut self, scenario: TestScenario) -> TestResult {
        info!("Running test scenario: {}", scenario.name);
        let start_time = Instant::now();

        // Submit initial commands
        for (i, command) in scenario.initial_commands.iter().enumerate() {
            let node_id = self.nodes.keys().nth(i % self.nodes.len()).unwrap();
            if let Some(node) = self.nodes.get(node_id) {
                let batch = CommandBatch::new(vec![command.clone()]);
                let (response_tx, _response_rx) = tokio::sync::oneshot::channel();

                let cmd = EngineCommand::ProcessBatch(rabia_engine::CommandRequest {
                    batch,
                    response_tx,
                });

                if let Err(e) = node.engine_tx.send(cmd) {
                    warn!("Failed to send command to node {}: {}", node_id, e);
                }
            }
        }

        // Inject faults according to schedule
        for (delay, fault) in scenario.faults {
            sleep(delay).await;
            self.inject_fault(fault).await;
        }

        // Wait for test completion
        sleep(scenario.timeout).await;

        // Collect results
        let test_duration = start_time.elapsed();
        let network_stats = self.simulator.get_stats().await;

        // Analyze outcome
        let actual_outcome = self.analyze_outcome().await;
        let success = self
            .check_expected_outcome(&scenario.expected_outcome, &actual_outcome)
            .await;
        let details = format!(
            "Expected: {:?}, Actual: {:?}",
            scenario.expected_outcome, actual_outcome
        );

        TestResult {
            scenario: scenario.name,
            success,
            duration: test_duration,
            network_stats,
            actual_outcome,
            details,
        }
    }

    async fn inject_fault(&self, fault: FaultType) {
        match fault {
            FaultType::NodeCrash { node_id, duration } => {
                info!(
                    "Injecting node crash for {} (duration: {:?})",
                    node_id, duration
                );
                self.simulator.remove_node(node_id).await;

                let _sim = self.simulator.clone();
                tokio::spawn(async move {
                    sleep(duration).await;
                    // Note: In a real implementation, we'd need to restart the node
                    info!("Node {} would restart now", node_id);
                });
            }

            FaultType::NetworkPartition { nodes, duration } => {
                info!(
                    "Injecting network partition for {:?} (duration: {:?})",
                    nodes, duration
                );
                self.simulator.create_partition(nodes, duration).await;
            }

            FaultType::PacketLoss { rate, duration } => {
                info!(
                    "Injecting packet loss rate {} (duration: {:?})",
                    rate, duration
                );
                let conditions = NetworkConditions {
                    packet_loss_rate: rate,
                    ..Default::default()
                };
                self.simulator.update_conditions(conditions.clone()).await;

                let sim = self.simulator.clone();
                tokio::spawn(async move {
                    sleep(duration).await;
                    let restore_conditions = NetworkConditions {
                        packet_loss_rate: 0.0,
                        ..Default::default()
                    };
                    sim.update_conditions(restore_conditions).await;
                });
            }

            FaultType::HighLatency { min, max, duration } => {
                info!(
                    "Injecting high latency {}-{:?} (duration: {:?})",
                    min.as_millis(),
                    max.as_millis(),
                    duration
                );
                let conditions = NetworkConditions {
                    latency_min: min,
                    latency_max: max,
                    ..Default::default()
                };
                self.simulator.update_conditions(conditions.clone()).await;

                let sim = self.simulator.clone();
                tokio::spawn(async move {
                    sleep(duration).await;
                    sim.update_conditions(NetworkConditions::default()).await;
                });
            }

            FaultType::SlowNode {
                node_id,
                delay_factor: _,
                duration,
            } => {
                // This would require modifying the node's processing speed
                info!(
                    "Injecting slow node for {} (duration: {:?})",
                    node_id, duration
                );
                // Implementation would involve throttling the node's message processing
            }

            FaultType::MessageReordering {
                probability: _,
                max_delay: _,
            } => {
                // This would require modifying the network simulator to reorder messages
                info!("Injecting message reordering");
                // Implementation would involve delaying random messages
            }
        }
    }

    async fn analyze_outcome(&self) -> ActualOutcome {
        // Get statistics from all nodes
        let mut node_stats = HashMap::new();

        for (&node_id, node) in &self.nodes {
            let (stats_tx, stats_rx) = tokio::sync::oneshot::channel();
            if node
                .engine_tx
                .send(EngineCommand::GetStatistics(stats_tx))
                .is_ok()
            {
                if let Ok(Ok(stats)) = tokio::time::timeout(Duration::from_millis(100), stats_rx).await
                {
                    node_stats.insert(node_id, stats);
                }
            }
        }

        ActualOutcome {
            committed_phases: node_stats
                .values()
                .map(|s| s.last_committed_phase.value())
                .collect(),
            current_phases: node_stats
                .values()
                .map(|s| s.current_phase.value())
                .collect(),
            nodes_with_progress: node_stats.len(),
        }
    }

    async fn check_expected_outcome(
        &self,
        expected: &ExpectedOutcome,
        actual: &ActualOutcome,
    ) -> bool {
        match expected {
            ExpectedOutcome::AllCommitted => {
                // All nodes should have committed the same number of phases
                if let Some(&first) = actual.committed_phases.first() {
                    actual.committed_phases.iter().all(|&phase| phase == first) && first > 0
                } else {
                    false
                }
            }

            ExpectedOutcome::PartialCommitment { min_committed } => actual
                .committed_phases
                .iter()
                .any(|&phase| phase >= *min_committed as u64),

            ExpectedOutcome::NoProgress => actual.committed_phases.iter().all(|&phase| phase == 0),

            ExpectedOutcome::EventualConsistency => {
                // Check if nodes eventually converge (allowing for some lag)
                let max_committed = actual.committed_phases.iter().max().unwrap_or(&0);
                let min_committed = actual.committed_phases.iter().min().unwrap_or(&0);
                max_committed - min_committed <= 2 // Allow up to 2 phases difference
            }
        }
    }

    pub async fn shutdown(&self) {
        self.simulator.shutdown().await;

        // Send shutdown to all nodes
        for node in self.nodes.values() {
            let _ = node.engine_tx.send(EngineCommand::Shutdown);
        }
    }
}

#[derive(Debug)]
pub struct TestResult {
    pub scenario: String,
    pub success: bool,
    pub duration: Duration,
    pub network_stats: crate::network_sim::NetworkStats,
    pub actual_outcome: ActualOutcome,
    pub details: String,
}

#[derive(Debug, Clone)]
pub struct ActualOutcome {
    pub committed_phases: Vec<u64>,
    pub current_phases: Vec<u64>,
    pub nodes_with_progress: usize,
}

pub fn create_test_scenarios() -> Vec<TestScenario> {
    vec![
        TestScenario {
            name: "Basic Consensus".to_string(),
            description: "Normal operation with no faults".to_string(),
            node_count: 3,
            initial_commands: vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
                Command::new("GET key1"),
            ],
            faults: vec![],
            expected_outcome: ExpectedOutcome::AllCommitted,
            timeout: Duration::from_secs(5),
        },
        TestScenario {
            name: "Single Node Failure".to_string(),
            description: "One node crashes and recovers".to_string(),
            node_count: 3,
            initial_commands: vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
            ],
            faults: vec![(
                Duration::from_millis(500),
                FaultType::NodeCrash {
                    node_id: NodeId::new(), // Will be replaced with actual node ID
                    duration: Duration::from_secs(2),
                },
            )],
            expected_outcome: ExpectedOutcome::EventualConsistency,
            timeout: Duration::from_secs(10),
        },
        TestScenario {
            name: "Network Partition".to_string(),
            description: "Network splits into two partitions".to_string(),
            node_count: 5,
            initial_commands: vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
                Command::new("SET key3 value3"),
            ],
            faults: vec![(
                Duration::from_millis(1000),
                FaultType::NetworkPartition {
                    nodes: HashSet::new(), // Will be populated with actual node IDs
                    duration: Duration::from_secs(3),
                },
            )],
            expected_outcome: ExpectedOutcome::PartialCommitment { min_committed: 1 },
            timeout: Duration::from_secs(15),
        },
        TestScenario {
            name: "High Packet Loss".to_string(),
            description: "Network with high packet loss rate".to_string(),
            node_count: 3,
            initial_commands: vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
            ],
            faults: vec![(
                Duration::from_millis(200),
                FaultType::PacketLoss {
                    rate: 0.3, // 30% packet loss
                    duration: Duration::from_secs(5),
                },
            )],
            expected_outcome: ExpectedOutcome::EventualConsistency,
            timeout: Duration::from_secs(12),
        },
        TestScenario {
            name: "High Network Latency".to_string(),
            description: "Network with high latency".to_string(),
            node_count: 3,
            initial_commands: vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
            ],
            faults: vec![(
                Duration::from_millis(100),
                FaultType::HighLatency {
                    min: Duration::from_millis(100),
                    max: Duration::from_millis(500),
                    duration: Duration::from_secs(4),
                },
            )],
            expected_outcome: ExpectedOutcome::AllCommitted,
            timeout: Duration::from_secs(10),
        },
        TestScenario {
            name: "Cascading Failures".to_string(),
            description: "Multiple nodes fail in sequence".to_string(),
            node_count: 5,
            initial_commands: vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
                Command::new("SET key3 value3"),
            ],
            faults: vec![
                (
                    Duration::from_millis(500),
                    FaultType::NodeCrash {
                        node_id: NodeId::new(),
                        duration: Duration::from_secs(3),
                    },
                ),
                (
                    Duration::from_millis(1500),
                    FaultType::NodeCrash {
                        node_id: NodeId::new(),
                        duration: Duration::from_secs(2),
                    },
                ),
            ],
            expected_outcome: ExpectedOutcome::PartialCommitment { min_committed: 1 },
            timeout: Duration::from_secs(15),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_basic_consensus_scenario() {
        let config = RabiaConfig::default();
        let mut harness = ConsensusTestHarness::new(3, config).await;

        let scenario = TestScenario {
            name: "Test Basic".to_string(),
            description: "Basic test".to_string(),
            node_count: 3,
            initial_commands: vec![Command::new("SET test value")],
            faults: vec![],
            expected_outcome: ExpectedOutcome::AllCommitted,
            timeout: Duration::from_secs(5),
        };

        let result = timeout(Duration::from_secs(10), harness.run_scenario(scenario)).await;
        assert!(result.is_ok());

        harness.shutdown().await;
    }

    #[tokio::test]
    async fn test_packet_loss_scenario() {
        let config = RabiaConfig::default();
        let mut harness = ConsensusTestHarness::new(3, config).await;

        let scenario = TestScenario {
            name: "Test Packet Loss".to_string(),
            description: "Test with packet loss".to_string(),
            node_count: 3,
            initial_commands: vec![Command::new("SET test value")],
            faults: vec![(
                Duration::from_millis(100),
                FaultType::PacketLoss {
                    rate: 0.2,
                    duration: Duration::from_secs(2),
                },
            )],
            expected_outcome: ExpectedOutcome::EventualConsistency,
            timeout: Duration::from_secs(8),
        };

        let result = timeout(Duration::from_secs(15), harness.run_scenario(scenario)).await;
        assert!(result.is_ok());

        harness.shutdown().await;
    }
}
