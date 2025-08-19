use rabia_core::{
    network::ClusterConfig, state_machine::InMemoryStateMachine, Command, CommandBatch, NodeId,
};
use rabia_engine::{RabiaConfig, RabiaEngine};
use rabia_network::InMemoryNetwork;
use rabia_persistence::InMemoryPersistence;
use std::collections::HashSet;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Create cluster configuration with 3 nodes (minimum for consensus)
    let node1_id = NodeId::new();
    let node2_id = NodeId::new();
    let node3_id = NodeId::new();
    
    let mut all_nodes = HashSet::new();
    all_nodes.insert(node1_id);
    all_nodes.insert(node2_id);
    all_nodes.insert(node3_id);
    
    let cluster_config = ClusterConfig::new(node1_id, all_nodes);

    // Create components for the primary node
    let state_machine = InMemoryStateMachine::new();
    let network = InMemoryNetwork::new(node1_id);
    let persistence = InMemoryPersistence::new();
    let config = RabiaConfig::default();

    // Create command channel
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    // Create and start the consensus engine
    let engine = RabiaEngine::new(
        node1_id,
        config,
        cluster_config,
        state_machine,
        network,
        persistence,
        cmd_rx,
    );

    println!("🚀 Starting Rabia consensus engine for primary node {} in 3-node cluster", node1_id);
    println!("   - Node 1 (primary): {}", node1_id);
    println!("   - Node 2: {}", node2_id);
    println!("   - Node 3: {}", node3_id);

    // In a real application, you would run the engine in a separate task
    // and use the command channel to interact with it
    println!("✅ Rabia consensus engine created successfully!");
    println!("📊 The engine includes:");
    println!("   • Async/await based architecture with Tokio");
    println!("   • Thread-safe in-memory state management");
    println!("   • Pluggable persistence and network layers");
    println!("   • Comprehensive error handling and recovery");
    println!("   • State synchronization and conflict resolution");
    println!("   • Automatic cleanup and garbage collection");

    // Create a sample command batch
    let commands = vec![
        Command::new("SET key1 value1"),
        Command::new("SET key2 value2"),
        Command::new("GET key1"),
    ];
    let batch = CommandBatch::new(commands);

    println!(
        "📝 Sample command batch created with {} commands",
        batch.commands.len()
    );
    println!("🔐 Batch checksum: {}", batch.checksum());

    Ok(())
}
