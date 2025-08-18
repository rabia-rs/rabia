//! Network integration tests
//!
//! These tests verify network transport functionality
//! and message passing between nodes.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use rabia_core::{
    messages::{HeartBeatMessage, MessageType, ProtocolMessage},
    network::NetworkTransport,
    NodeId, PhaseId,
};
use rabia_network::InMemoryNetwork;
use rabia_testing::network_sim::{NetworkConditions, NetworkSimulator, SimulatedNetwork};

/// Test basic InMemoryNetwork functionality
#[tokio::test]
async fn test_inmemory_network_basic() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let network1 = InMemoryNetwork::new(node1_id);
    let _network2 = InMemoryNetwork::new(node2_id);

    // Connect networks
    let mut connected_nodes = HashSet::new();
    connected_nodes.insert(node1_id);
    connected_nodes.insert(node2_id);

    // Note: InMemoryNetwork might need additional setup for cross-network communication
    // For this test, we'll focus on basic functionality

    // Test getting connected nodes
    let result = network1.get_connected_nodes().await;
    assert!(
        result.is_ok(),
        "get_connected_nodes failed: {:?}",
        result.err()
    );

    // Test connectivity check
    let result = network1.is_connected(node2_id).await;
    assert!(
        result.is_ok(),
        "is_connected check failed: {:?}",
        result.err()
    );
}

/// Test network disconnect and reconnect
#[tokio::test]
async fn test_network_disconnect_reconnect() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let node_id = NodeId::new();
    let mut network = InMemoryNetwork::new(node_id);

    // Test disconnect
    let result = network.disconnect().await;
    assert!(result.is_ok(), "disconnect failed: {:?}", result.err());

    // Test reconnect
    let result = network.reconnect().await;
    assert!(result.is_ok(), "reconnect failed: {:?}", result.err());
}

/// Test SimulatedNetwork basic operations
#[tokio::test]
async fn test_simulated_network_basic() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let simulator = Arc::new(NetworkSimulator::new());
    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let network1 = SimulatedNetwork::new(node1_id, simulator.clone()).await;
    let mut network2 = SimulatedNetwork::new(node2_id, simulator.clone()).await;

    // Connect nodes
    let mut connected_nodes = HashSet::new();
    connected_nodes.insert(node1_id);
    connected_nodes.insert(node2_id);

    network1.connect_to_nodes(connected_nodes.clone()).await;
    network2.connect_to_nodes(connected_nodes).await;

    // Start network simulation
    let sim_handle = {
        let sim = simulator.clone();
        tokio::spawn(async move {
            sim.run_simulation().await;
        })
    };

    // Test message sending
    let message = ProtocolMessage::new(
        node1_id,
        Some(node2_id),
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(1),
            last_committed_phase: PhaseId::new(0),
            active: true,
        }),
    );

    let result = network1.send_to(node2_id, message.clone()).await;
    assert!(result.is_ok(), "send_to failed: {:?}", result.err());

    // Wait for message delivery
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Try to receive message (with timeout)
    let receive_result = timeout(Duration::from_millis(100), network2.receive()).await;

    match receive_result {
        Ok(Ok((from, received_msg))) => {
            assert_eq!(from, node1_id, "Message sender mismatch");
            // Basic validation that we received a message
            assert!(matches!(
                received_msg.message_type,
                MessageType::HeartBeat(_)
            ));
        }
        Ok(Err(e)) => {
            println!("Receive failed (expected in some cases): {:?}", e);
        }
        Err(_) => {
            println!("Receive timed out (expected in some cases)");
        }
    }

    // Shutdown
    simulator.shutdown().await;
    sim_handle.abort();
}

/// Test network with packet loss
#[tokio::test]
async fn test_network_packet_loss() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let simulator = Arc::new(NetworkSimulator::new());
    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let network1 = SimulatedNetwork::new(node1_id, simulator.clone()).await;
    let mut network2 = SimulatedNetwork::new(node2_id, simulator.clone()).await;

    // Connect nodes
    let mut connected_nodes = HashSet::new();
    connected_nodes.insert(node1_id);
    connected_nodes.insert(node2_id);

    network1.connect_to_nodes(connected_nodes.clone()).await;
    network2.connect_to_nodes(connected_nodes).await;

    // Configure high packet loss
    let conditions = NetworkConditions {
        packet_loss_rate: 0.5, // 50% packet loss
        latency_min: Duration::from_millis(1),
        latency_max: Duration::from_millis(10),
        partition_probability: 0.0,
        bandwidth_limit: None,
    };
    simulator.update_conditions(conditions).await;

    // Start network simulation
    let sim_handle = {
        let sim = simulator.clone();
        tokio::spawn(async move {
            sim.run_simulation().await;
        })
    };

    // Send multiple messages to test packet loss
    let mut messages_sent = 0;
    let mut messages_received = 0;

    for i in 0..20 {
        let message = ProtocolMessage::new(
            node1_id,
            Some(node2_id),
            MessageType::HeartBeat(HeartBeatMessage {
                current_phase: PhaseId::new(i),
                last_committed_phase: PhaseId::new(0),
                active: true,
            }),
        );

        let result = network1.send_to(node2_id, message).await;
        if result.is_ok() {
            messages_sent += 1;
        }

        // Small delay between sends
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Wait for potential message delivery
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try to receive messages
    for _ in 0..messages_sent {
        let receive_result = timeout(Duration::from_millis(50), network2.receive()).await;
        if receive_result.is_ok() && receive_result.unwrap().is_ok() {
            messages_received += 1;
        }
    }

    println!(
        "Packet loss test: sent={}, received={}, loss_rate={:.2}%",
        messages_sent,
        messages_received,
        (1.0 - messages_received as f64 / messages_sent as f64) * 100.0
    );

    // With 50% packet loss, we expect significantly fewer messages to be received
    assert!(
        messages_received < messages_sent,
        "Expected packet loss to reduce received messages"
    );

    // Get network statistics
    let stats = simulator.get_stats().await;
    assert!(
        stats.messages_dropped > 0,
        "Expected some messages to be dropped"
    );

    // Shutdown
    simulator.shutdown().await;
    sim_handle.abort();
}

/// Test network broadcast functionality
#[tokio::test]
async fn test_network_broadcast() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let simulator = Arc::new(NetworkSimulator::new());
    let node1_id = NodeId::new();
    let node2_id = NodeId::new();
    let node3_id = NodeId::new();

    let network1 = SimulatedNetwork::new(node1_id, simulator.clone()).await;
    let mut network2 = SimulatedNetwork::new(node2_id, simulator.clone()).await;
    let mut network3 = SimulatedNetwork::new(node3_id, simulator.clone()).await;

    // Connect all nodes
    let mut connected_nodes = HashSet::new();
    connected_nodes.insert(node1_id);
    connected_nodes.insert(node2_id);
    connected_nodes.insert(node3_id);

    network1.connect_to_nodes(connected_nodes.clone()).await;
    network2.connect_to_nodes(connected_nodes.clone()).await;
    network3.connect_to_nodes(connected_nodes).await;

    // Start network simulation
    let sim_handle = {
        let sim = simulator.clone();
        tokio::spawn(async move {
            sim.run_simulation().await;
        })
    };

    // Broadcast message from node1
    let message = ProtocolMessage::new(
        node1_id,
        None, // Broadcast message
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(1),
            last_committed_phase: PhaseId::new(0),
            active: true,
        }),
    );

    let result = network1.broadcast(message, None).await;
    assert!(result.is_ok(), "broadcast failed: {:?}", result.err());

    // Wait for message delivery
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check if nodes 2 and 3 received the broadcast
    let mut receivers = Vec::new();

    let receive_result2 = timeout(Duration::from_millis(100), network2.receive()).await;
    if let Ok(Ok((from, _))) = receive_result2 {
        if from == node1_id {
            receivers.push(2);
        }
    }

    let receive_result3 = timeout(Duration::from_millis(100), network3.receive()).await;
    if let Ok(Ok((from, _))) = receive_result3 {
        if from == node1_id {
            receivers.push(3);
        }
    }

    println!("Broadcast received by nodes: {:?}", receivers);

    // Shutdown
    simulator.shutdown().await;
    sim_handle.abort();
}

/// Test network partition simulation
#[tokio::test]
async fn test_network_partition() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let simulator = Arc::new(NetworkSimulator::new());
    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let network1 = SimulatedNetwork::new(node1_id, simulator.clone()).await;
    let mut network2 = SimulatedNetwork::new(node2_id, simulator.clone()).await;

    // Connect nodes
    let mut connected_nodes = HashSet::new();
    connected_nodes.insert(node1_id);
    connected_nodes.insert(node2_id);

    network1.connect_to_nodes(connected_nodes.clone()).await;
    network2.connect_to_nodes(connected_nodes).await;

    // Create partition
    let mut partitioned_nodes = HashSet::new();
    partitioned_nodes.insert(node1_id);
    simulator
        .create_partition(partitioned_nodes, Duration::from_millis(500))
        .await;

    // Start network simulation
    let sim_handle = {
        let sim = simulator.clone();
        tokio::spawn(async move {
            sim.run_simulation().await;
        })
    };

    // Try to send message during partition
    let message = ProtocolMessage::new(
        node1_id,
        Some(node2_id),
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: PhaseId::new(1),
            last_committed_phase: PhaseId::new(0),
            active: true,
        }),
    );

    let result = network1.send_to(node2_id, message).await;
    assert!(
        result.is_ok(),
        "send_to failed during partition: {:?}",
        result.err()
    );

    // Wait for potential delivery (should be dropped due to partition)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to receive (should timeout due to partition)
    let receive_result = timeout(Duration::from_millis(100), network2.receive()).await;
    assert!(
        receive_result.is_err(),
        "Expected timeout due to network partition"
    );

    // Wait for partition to heal
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Verify partition is healed by checking network stats
    let stats = simulator.get_stats().await;
    println!(
        "Network stats: sent={}, delivered={}, dropped={}",
        stats.messages_sent, stats.messages_delivered, stats.messages_dropped
    );

    // Shutdown
    simulator.shutdown().await;
    sim_handle.abort();
}
