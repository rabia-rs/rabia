//! # TCP Networking Example
//!
//! This example demonstrates how to use the TCP network implementation
//! to create a distributed consensus cluster with real network connections.

use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use rabia_core::{
    messages::{HeartBeatMessage, MessageType, ProposeMessage, ProtocolMessage},
    network::NetworkTransport,
    BatchId, Command, CommandBatch, NodeId, PhaseId, StateValue,
};
use rabia_network::{TcpNetwork, TcpNetworkConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("🚀 Starting TCP Networking Example");

    // Example 1: Three-node cluster
    info!("📡 Example 1: Three-node TCP cluster");
    run_three_node_cluster().await?;

    // Example 2: Multi-node cluster with message broadcasting
    info!("📡 Example 2: Multi-node cluster with broadcasting");
    run_multi_node_cluster().await?;

    // Example 3: Dynamic node joining and leaving
    info!("📡 Example 3: Dynamic topology changes");
    run_dynamic_topology().await?;

    info!("✅ TCP Networking Example completed successfully!");
    Ok(())
}

/// Example 1: Basic three-node TCP cluster
async fn run_three_node_cluster() -> Result<(), Box<dyn std::error::Error>> {
    info!("Setting up three TCP nodes...");

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();
    let node3_id = NodeId::new();

    // Configure networks with specific ports
    let config1 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:9001".parse()?,
        peer_addresses: HashMap::new(),
        ..Default::default()
    };

    let config2 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:9002".parse()?,
        peer_addresses: HashMap::new(),
        ..Default::default()
    };

    let config3 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:9003".parse()?,
        peer_addresses: HashMap::new(),
        ..Default::default()
    };

    // Start networks
    let mut network1 = TcpNetwork::new(node1_id, config1).await?;
    let mut network2 = TcpNetwork::new(node2_id, config2).await?;
    let mut network3 = TcpNetwork::new(node3_id, config3).await?;

    info!(
        "Node 1: {} listening on {}",
        node1_id,
        network1.local_addr()
    );
    info!(
        "Node 2: {} listening on {}",
        node2_id,
        network2.local_addr()
    );
    info!(
        "Node 3: {} listening on {}",
        node3_id,
        network3.local_addr()
    );

    // Connect the nodes in a full mesh
    info!("Connecting nodes in full mesh...");
    network1
        .connect_to_peer(node2_id, network2.local_addr())
        .await?;
    network1
        .connect_to_peer(node3_id, network3.local_addr())
        .await?;
    network2
        .connect_to_peer(node3_id, network3.local_addr())
        .await?;

    // Wait for connections to establish
    sleep(Duration::from_millis(500)).await;

    // Verify connectivity
    let connected1 = network1.get_connected_nodes().await?;
    let connected2 = network2.get_connected_nodes().await?;
    let connected3 = network3.get_connected_nodes().await?;

    info!("Node 1 connected to: {:?}", connected1);
    info!("Node 2 connected to: {:?}", connected2);
    info!("Node 3 connected to: {:?}", connected3);

    // Exchange messages
    info!("Exchanging test messages...");

    // Send heartbeat from node1 to node2
    let heartbeat = ProtocolMessage::new(
        node1_id,
        Some(node2_id),
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(1),
            last_committed_phase: PhaseId::new(0),
            active: true,
        }),
    );

    network1.send_to(node2_id, heartbeat).await?;

    // Receive on node2
    match tokio::time::timeout(Duration::from_secs(3), network2.receive()).await {
        Ok(Ok((from, message))) => {
            info!("✅ Node 2 received message from {}", from);
            if let MessageType::HeartBeat(hb) = message.message_type {
                info!(
                    "   Heartbeat: phase={}, active={}",
                    hb.current_phase.value(),
                    hb.active
                );
            }
        }
        Ok(Err(e)) => warn!("❌ Error receiving message: {}", e),
        Err(_) => warn!("❌ Timeout receiving message"),
    }

    // Send consensus proposal from node2 to node3
    let commands = vec![
        Command::new("SET user:alice name=Alice"),
        Command::new("SET user:bob name=Bob"),
        Command::new("GET user:alice"),
    ];
    let batch = CommandBatch::new(commands);

    let proposal = ProtocolMessage::new(
        node2_id,
        Some(node3_id),
        MessageType::Propose(ProposeMessage {
            phase_id: PhaseId::new(1),
            batch_id: BatchId::new(),
            value: StateValue::V1,
            batch: Some(batch),
        }),
    );

    network2.send_to(node3_id, proposal).await?;

    // Receive on node3
    match tokio::time::timeout(Duration::from_secs(3), network3.receive()).await {
        Ok(Ok((from, message))) => {
            info!("✅ Node 3 received proposal from {}", from);
            if let MessageType::Propose(prop) = message.message_type {
                info!(
                    "   Proposal: phase={}, value={:?}",
                    prop.phase_id.value(),
                    prop.value
                );
                if let Some(batch) = prop.batch {
                    info!("   Commands: {} in batch", batch.commands.len());
                }
            }
        }
        Ok(Err(e)) => warn!("❌ Error receiving proposal: {}", e),
        Err(_) => warn!("❌ Timeout receiving proposal"),
    }

    // Test broadcast from node3 to all others
    info!("Testing broadcast from node 3...");
    let broadcast_msg = ProtocolMessage::new(
        node3_id,
        None, // Broadcast to all
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(2),
            last_committed_phase: PhaseId::new(1),
            active: true,
        }),
    );

    network3.broadcast(broadcast_msg, Some(node3_id)).await?;

    // Receive broadcast on nodes 1 and 2
    let mut received_count = 0;
    for (i, network) in [&mut network1, &mut network2].iter_mut().enumerate() {
        match tokio::time::timeout(Duration::from_secs(3), network.receive()).await {
            Ok(Ok((from, _))) => {
                received_count += 1;
                info!("✅ Node {} received broadcast from {}", i + 1, from);
            }
            _ => warn!("❌ Node {} failed to receive broadcast", i + 1),
        }
    }

    info!(
        "📊 Broadcast results: {}/2 nodes received message",
        received_count
    );

    // Cleanup
    network1.shutdown().await;
    network2.shutdown().await;
    network3.shutdown().await;

    info!("✅ Three-node cluster example completed\n");
    Ok(())
}

/// Example 2: Multi-node cluster with broadcasting
async fn run_multi_node_cluster() -> Result<(), Box<dyn std::error::Error>> {
    const NODE_COUNT: usize = 5;
    let mut networks = Vec::new();
    let mut node_ids = Vec::new();

    info!("Setting up {}-node TCP cluster...", NODE_COUNT);

    // Create all nodes
    for i in 0..NODE_COUNT {
        let node_id = NodeId::new();
        let config = TcpNetworkConfig {
            bind_addr: format!("127.0.0.1:{}", 9010 + i).parse()?,
            ..Default::default()
        };

        let network = TcpNetwork::new(node_id, config).await?;
        info!(
            "Node {}: {} listening on {}",
            i + 1,
            node_id,
            network.local_addr()
        );

        node_ids.push(node_id);
        networks.push(network);
    }

    // Connect all nodes in a full mesh
    info!("Establishing full mesh connectivity...");
    for i in 0..networks.len() {
        for j in 0..networks.len() {
            if i != j {
                let target_node = node_ids[j];
                let target_addr = networks[j].local_addr();

                if let Err(e) = networks[i].connect_to_peer(target_node, target_addr).await {
                    warn!("Failed to connect node {} to {}: {}", i, j, e);
                }
            }
        }
    }

    // Wait for all connections
    sleep(Duration::from_millis(1000)).await;

    // Verify connectivity
    for (i, network) in networks.iter().enumerate() {
        let connected = network.get_connected_nodes().await?;
        info!("Node {} connected to {} peers", i + 1, connected.len());
    }

    // Test broadcast from node 0
    info!("Broadcasting from Node 1 to all other nodes...");
    let broadcast_message = ProtocolMessage::new(
        node_ids[0],
        None, // Broadcast to all
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(42),
            last_committed_phase: PhaseId::new(41),
            active: true,
        }),
    );

    networks[0]
        .broadcast(broadcast_message, Some(node_ids[0]))
        .await?;

    // Receive broadcast on all other nodes
    let mut received_count = 0;
    for (i, network) in networks.iter_mut().enumerate().skip(1) {
        match tokio::time::timeout(Duration::from_secs(5), network.receive()).await {
            Ok(Ok((from, message))) => {
                received_count += 1;
                info!("✅ Node {} received broadcast from {}", i + 1, from);
                if let MessageType::HeartBeat(hb) = message.message_type {
                    info!("   Phase: {}", hb.current_phase.value());
                }
            }
            Ok(Err(e)) => warn!("❌ Node {} error receiving: {}", i + 1, e),
            Err(_) => warn!("❌ Node {} timeout receiving broadcast", i + 1),
        }
    }

    info!(
        "📊 Broadcast results: {}/{} nodes received message",
        received_count,
        NODE_COUNT - 1
    );

    // Cleanup
    for network in networks {
        network.shutdown().await;
    }

    info!("✅ Multi-node cluster example completed\n");
    Ok(())
}

/// Example 3: Dynamic topology changes
async fn run_dynamic_topology() -> Result<(), Box<dyn std::error::Error>> {
    info!("Demonstrating dynamic topology changes...");

    // Start with 3 nodes
    let mut networks = Vec::new();
    let mut node_ids = Vec::new();

    for i in 0..3 {
        let node_id = NodeId::new();
        let config = TcpNetworkConfig {
            bind_addr: format!("127.0.0.1:{}", 9020 + i).parse()?,
            ..Default::default()
        };

        let network = TcpNetwork::new(node_id, config).await?;
        info!(
            "Initial node {}: {} on {}",
            i + 1,
            node_id,
            network.local_addr()
        );

        node_ids.push(node_id);
        networks.push(network);
    }

    // Connect initial nodes
    info!("Connecting initial 3-node cluster...");
    for i in 0..networks.len() {
        for j in 0..networks.len() {
            if i != j {
                let target_node = node_ids[j];
                let target_addr = networks[j].local_addr();
                networks[i]
                    .connect_to_peer(target_node, target_addr)
                    .await
                    .ok();
            }
        }
    }

    sleep(Duration::from_millis(300)).await;

    // Verify initial connectivity
    for (i, network) in networks.iter().enumerate() {
        let connected = network.get_connected_nodes().await?;
        info!(
            "Initial Node {} connected to {} peers",
            i + 1,
            connected.len()
        );
    }

    // Add a new node dynamically
    info!("Adding new node to the cluster...");
    let new_node_id = NodeId::new();
    let new_config = TcpNetworkConfig {
        bind_addr: "127.0.0.1:9024".parse()?,
        ..Default::default()
    };

    let mut new_network = TcpNetwork::new(new_node_id, new_config).await?;
    info!("New node: {} on {}", new_node_id, new_network.local_addr());

    // Connect new node to existing nodes
    for (i, existing_network) in networks.iter().enumerate() {
        let existing_node = node_ids[i];
        let existing_addr = existing_network.local_addr();

        // New node connects to existing
        new_network
            .connect_to_peer(existing_node, existing_addr)
            .await
            .ok();

        // Existing node connects to new (bidirectional)
        existing_network
            .connect_to_peer(new_node_id, new_network.local_addr())
            .await
            .ok();
    }

    sleep(Duration::from_millis(300)).await;

    // Test communication with new topology
    info!("Testing communication in expanded cluster...");
    let test_message = ProtocolMessage::new(
        new_node_id,
        None,
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(100),
            last_committed_phase: PhaseId::new(99),
            active: true,
        }),
    );

    new_network
        .broadcast(test_message, Some(new_node_id))
        .await?;

    let mut received_by_original = 0;
    for (i, network) in networks.iter_mut().enumerate() {
        if let Ok(Ok((from, _))) = tokio::time::timeout(Duration::from_secs(3), network.receive()).await {
            received_by_original += 1;
            info!("✅ Original node {} received from new node {}", i + 1, from);
        }
    }

    info!(
        "📊 New node successfully communicated with {}/3 original nodes",
        received_by_original
    );

    // Simulate node failure (shutdown node 1)
    info!("Simulating failure of Node 2...");
    networks[1].shutdown().await;

    sleep(Duration::from_millis(200)).await;

    // Test remaining connectivity
    info!("Testing connectivity after node failure...");
    let recovery_message = ProtocolMessage::new(
        node_ids[0],
        None,
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(200),
            last_committed_phase: PhaseId::new(199),
            active: false, // Indicating degraded state
        }),
    );

    networks[0]
        .broadcast(recovery_message, Some(node_ids[0]))
        .await?;

    let mut received_after_failure = 0;
    // Try to receive on remaining nodes (skip the failed node)
    for i in [0, 2].iter() {
        if *i == 0 {
            continue;
        } // Skip sender
        match tokio::time::timeout(Duration::from_secs(2), networks[*i].receive()).await {
            Ok(Ok(_)) => {
                received_after_failure += 1;
                info!("✅ Node {} still receiving after failure", i + 1);
            }
            _ => {
                info!("⚠️ Node {} may be partitioned after failure", i + 1);
            }
        }
    }

    // Check new node connectivity
    match tokio::time::timeout(Duration::from_secs(2), new_network.receive()).await {
        Ok(Ok(_)) => {
            received_after_failure += 1;
            info!("✅ New node still receiving after failure");
        }
        _ => {
            info!("⚠️ New node may be partitioned after failure");
        }
    }

    info!(
        "📊 After failure: {}/2 remaining nodes still connected",
        received_after_failure
    );

    // Cleanup
    for network in networks {
        network.shutdown().await;
    }
    new_network.shutdown().await;

    info!("✅ Dynamic topology example completed\n");
    Ok(())
}

/// Helper function to demonstrate network statistics
#[allow(dead_code)]
async fn show_network_stats(networks: &[TcpNetwork]) -> Result<(), Box<dyn std::error::Error>> {
    info!("📊 Network Statistics:");
    for (i, network) in networks.iter().enumerate() {
        let connected = network.get_connected_nodes().await?;
        info!("  Node {}: {} connections", i + 1, connected.len());
    }
    Ok(())
}
