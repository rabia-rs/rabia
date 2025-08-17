use std::collections::HashSet;
use tokio::sync::mpsc;
use rabia_core::{
    NodeId, CommandBatch, Command,
    network::ClusterConfig,
    state_machine::InMemoryStateMachine,
};
use rabia_engine::{RabiaEngine, RabiaConfig};
use rabia_network::InMemoryNetwork;
use rabia_persistence::InMemoryPersistence;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Create node configuration
    let node_id = NodeId::new();
    let mut all_nodes = HashSet::new();
    all_nodes.insert(node_id);
    let cluster_config = ClusterConfig::new(node_id, all_nodes);

    // Create components
    let state_machine = InMemoryStateMachine::new();
    let network = InMemoryNetwork::new(node_id);
    let persistence = InMemoryPersistence::new();
    let config = RabiaConfig::default();

    // Create command channel
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    // Create and start the consensus engine
    let engine = RabiaEngine::new(
        node_id,
        config,
        cluster_config,
        state_machine,
        network,
        persistence,
        cmd_rx,
    );

    println!("🚀 Starting Rabia consensus engine for node {}", node_id);
    
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
    
    println!("📝 Sample command batch created with {} commands", batch.commands.len());
    println!("🔐 Batch checksum: {}", batch.checksum());

    Ok(())
}