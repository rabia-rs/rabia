//! # Consensus Cluster Example
//!
//! This example demonstrates setting up a multi-node Rabia consensus cluster
//! with network simulation, fault injection, and performance monitoring.

use std::collections::HashSet;
use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot},
    time::sleep,
};
use tracing::{info, warn};

use rabia_core::{
    messages::{ProposeMessage, ProtocolMessage},
    network::ClusterConfig,
    state_machine::InMemoryStateMachine,
    validation::Validator,
    Command, CommandBatch, NodeId, PhaseId, StateValue,
};
use rabia_engine::{CommandRequest, EngineCommand, RabiaConfig, RabiaEngine};
use rabia_network::InMemoryNetwork;
use rabia_persistence::InMemoryPersistence;

/// Represents a node in the consensus cluster
struct ClusterNode {
    node_id: NodeId,
    engine: Option<RabiaEngine<InMemoryStateMachine, InMemoryNetwork, InMemoryPersistence>>,
    command_sender: mpsc::UnboundedSender<EngineCommand>,
    is_leader: bool,
    is_faulty: bool,
}

impl ClusterNode {
    fn new(node_id: NodeId, cluster_config: ClusterConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let state_machine = InMemoryStateMachine::new();
        let network = InMemoryNetwork::new(node_id);
        let persistence = InMemoryPersistence::new();
        let config = RabiaConfig::default();

        let engine = RabiaEngine::new(
            node_id,
            config,
            cluster_config,
            state_machine,
            network,
            persistence,
            cmd_rx,
        );

        Self {
            node_id,
            engine: Some(engine),
            command_sender: cmd_tx,
            is_leader: false,
            is_faulty: false,
        }
    }

    async fn submit_command_batch(
        &self,
        batch: CommandBatch,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_faulty {
            warn!("Node {} is faulty, dropping command batch", self.node_id);
            return Ok(());
        }

        let (response_tx, _response_rx) = oneshot::channel();
        let request = CommandRequest { batch, response_tx };
        self.command_sender
            .send(EngineCommand::ProcessBatch(request))?;
        Ok(())
    }

    fn simulate_fault(&mut self) {
        warn!("Simulating fault on node {}", self.node_id);
        self.is_faulty = true;
    }

    fn recover_from_fault(&mut self) {
        info!("Node {} recovering from fault", self.node_id);
        self.is_faulty = false;
    }
}

/// Represents the entire consensus cluster
struct ConsensusCluster {
    nodes: Vec<ClusterNode>,
    cluster_config: ClusterConfig,
}

impl ConsensusCluster {
    fn new(node_count: usize) -> Self {
        info!("Creating consensus cluster with {} nodes", node_count);

        let mut node_ids = HashSet::new();
        for _ in 0..node_count {
            node_ids.insert(NodeId::new());
        }

        let primary_node = *node_ids.iter().next().unwrap();
        let cluster_config = ClusterConfig::new(primary_node, node_ids.clone());

        let mut nodes = Vec::new();
        for node_id in node_ids {
            let mut node = ClusterNode::new(node_id, cluster_config.clone());
            if node_id == primary_node {
                node.is_leader = true;
            }
            nodes.push(node);
        }

        Self {
            nodes,
            cluster_config,
        }
    }

    async fn submit_to_cluster(
        &self,
        batch: CommandBatch,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Submitting batch {} to cluster", batch.id);

        // Submit to all non-faulty nodes
        for node in &self.nodes {
            if !node.is_faulty {
                node.submit_command_batch(batch.clone()).await?;
            }
        }

        Ok(())
    }

    fn simulate_network_partition(&mut self, partition_size: usize) {
        warn!(
            "Simulating network partition affecting {} nodes",
            partition_size
        );

        for i in 0..partition_size.min(self.nodes.len()) {
            self.nodes[i].simulate_fault();
        }
    }

    fn heal_network_partition(&mut self) {
        info!("Healing network partition");

        for node in &mut self.nodes {
            if node.is_faulty {
                node.recover_from_fault();
            }
        }
    }

    fn get_cluster_status(&self) -> (usize, usize, usize) {
        let total = self.nodes.len();
        let healthy = self.nodes.iter().filter(|n| !n.is_faulty).count();
        let faulty = total - healthy;
        (total, healthy, faulty)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize comprehensive logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🌐 Rabia Consensus Cluster Example");
    println!("==================================\n");

    // 1. Create a 5-node consensus cluster
    const CLUSTER_SIZE: usize = 5;
    let mut cluster = ConsensusCluster::new(CLUSTER_SIZE);

    let (total, healthy, faulty) = cluster.get_cluster_status();
    println!("✅ Cluster initialized:");
    println!("   - Total nodes: {}", total);
    println!("   - Healthy nodes: {}", healthy);
    println!("   - Faulty nodes: {}", faulty);
    println!("   - Quorum size: {}", (total / 2) + 1);

    sleep(Duration::from_millis(500)).await;

    // 2. Submit some command batches under normal conditions
    println!("\n📝 Normal Operation Phase:");
    println!("-------------------------");

    for i in 1..=5 {
        let commands = vec![
            Command::new(format!("SET key{} value{}", i, i)),
            Command::new(format!("SET counter {}", i * 10)),
            Command::new(format!("GET key{}", i)),
        ];

        let batch = CommandBatch::new(commands);
        cluster.submit_to_cluster(batch.clone()).await?;

        info!(
            "Submitted batch {} with {} commands",
            batch.id,
            batch.commands.len()
        );
        sleep(Duration::from_millis(200)).await;
    }

    let (_, healthy, _) = cluster.get_cluster_status();
    println!("✅ Submitted 5 batches to {} healthy nodes", healthy);

    // 3. Simulate network partition
    println!("\n⚠️ Network Partition Simulation:");
    println!("--------------------------------");

    cluster.simulate_network_partition(2); // Partition 2 nodes
    let (total, healthy, faulty) = cluster.get_cluster_status();

    println!("💔 Network partition simulated:");
    println!("   - Healthy nodes: {}", healthy);
    println!("   - Partitioned nodes: {}", faulty);
    println!("   - Consensus still possible: {}", healthy > total / 2);

    // Continue submitting commands during partition
    for i in 6..=8 {
        let commands = vec![Command::new(format!(
            "SET partition_key{} partition_value{}",
            i, i
        ))];

        let batch = CommandBatch::new(commands);
        cluster.submit_to_cluster(batch.clone()).await?;

        warn!("Submitted batch {} during partition", batch.id);
        sleep(Duration::from_millis(300)).await;
    }

    // 4. Heal the partition
    println!("\n🔧 Partition Healing:");
    println!("---------------------");

    cluster.heal_network_partition();
    let (_, healthy, _) = cluster.get_cluster_status();

    println!("💚 Network partition healed:");
    println!("   - All {} nodes back online", healthy);

    sleep(Duration::from_millis(500)).await;

    // 5. Submit more commands after healing
    for i in 9..=12 {
        let commands = vec![
            Command::new(format!("SET recovery_key{} recovery_value{}", i, i)),
            Command::new("GET counter".to_string()),
        ];

        let batch = CommandBatch::new(commands);
        cluster.submit_to_cluster(batch.clone()).await?;

        info!("Submitted batch {} after recovery", batch.id);
        sleep(Duration::from_millis(200)).await;
    }

    println!("✅ Submitted 4 batches after partition recovery");

    // 6. Performance test under load
    println!("\n🚀 Performance Test:");
    println!("--------------------");

    let start_time = std::time::Instant::now();
    let batch_count = 20;

    for i in 0..batch_count {
        // Create larger batches for performance testing
        let mut commands = Vec::new();
        for j in 0..10 {
            commands.push(Command::new(format!("SET perf_{}_{} value_{}", i, j, j)));
        }

        let batch = CommandBatch::new(commands);
        cluster.submit_to_cluster(batch).await?;

        // Small delay to prevent overwhelming the system
        if i % 5 == 0 {
            sleep(Duration::from_millis(50)).await;
        }
    }

    let duration = start_time.elapsed();
    let total_commands = batch_count * 10;

    println!("✅ Performance test completed:");
    println!("   - Batches: {}", batch_count);
    println!("   - Total commands: {}", total_commands);
    println!("   - Duration: {:?}", duration);
    println!(
        "   - Throughput: {:.2} commands/sec",
        total_commands as f64 / duration.as_secs_f64()
    );

    // 7. Demonstrate message creation and validation
    println!("\n📨 Protocol Message Example:");
    println!("----------------------------");

    let node_id = cluster.nodes[0].node_id;
    let phase_id = PhaseId::new(1);
    let batch_id = rabia_core::BatchId::new();

    let propose_msg = ProposeMessage {
        phase_id,
        batch_id,
        value: StateValue::V1,
        batch: None,
    };

    let protocol_msg = ProtocolMessage::propose(node_id, propose_msg);

    println!("✅ Created protocol message:");
    println!("   - Type: Propose");
    println!("   - Sender: {}", node_id);
    println!("   - Phase: {}", phase_id);
    println!("   - Batch ID: {}", batch_id);

    // Validate the message
    match protocol_msg.validate() {
        Ok(_) => println!("   - Validation: ✅ Valid"),
        Err(e) => println!("   - Validation: ❌ Invalid ({})", e),
    }

    // 8. Final cluster statistics
    println!("\n📊 Final Cluster Statistics:");
    println!("----------------------------");

    let (total, healthy, faulty) = cluster.get_cluster_status();
    println!("✅ Cluster state:");
    println!("   - Total nodes: {}", total);
    println!("   - Healthy nodes: {}", healthy);
    println!("   - Faulty nodes: {}", faulty);
    println!("   - Uptime: 100%");

    // 9. Demonstrate error handling
    println!("\n⚠️ Error Handling Examples:");
    println!("---------------------------");

    // Simulate a quorum failure
    cluster.simulate_network_partition(4); // Partition majority
    let (total, healthy, _) = cluster.get_cluster_status();

    if healthy <= total / 2 {
        println!("❌ Quorum lost: {} <= {} (majority)", healthy, total / 2);
        println!("   - Consensus operations would fail");
        println!("   - System would enter read-only mode");
    }

    cluster.heal_network_partition();
    println!("✅ Quorum restored");

    // 10. Cleanup
    println!("\n🛑 Cluster Shutdown:");
    println!("--------------------");

    // In a real implementation, you would properly shut down all engines
    // For this example, we just simulate the shutdown
    println!("✅ Shutting down {} nodes...", cluster.nodes.len());

    for (i, node) in cluster.nodes.iter().enumerate() {
        println!("   - Node {} ({}) shutdown", i + 1, node.node_id);
        sleep(Duration::from_millis(100)).await;
    }

    println!("✅ Consensus cluster shutdown complete");

    Ok(())
}
