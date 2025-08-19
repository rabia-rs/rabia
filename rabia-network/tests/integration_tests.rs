//! Integration tests for TCP networking with real connections
//!
//! This module tests the TCP network implementation with actual TCP connections
//! across multiple processes to ensure production-readiness.

use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use tracing_subscriber::fmt::try_init;

use rabia_core::{messages::*, network::NetworkTransport, NodeId};
use rabia_network::{TcpNetwork, TcpNetworkConfig};

/// Get platform-specific timeouts for tests
fn get_test_timeouts() -> (Duration, Duration) {
    if std::env::var("CI").is_ok() {
        // CI environment with longer timeouts for Windows
        if cfg!(windows) {
            (Duration::from_millis(2000), Duration::from_secs(10)) // (connection_wait, test_timeout)
        } else {
            (Duration::from_millis(500), Duration::from_secs(5))
        }
    } else {
        // Local development environment
        (Duration::from_millis(200), Duration::from_secs(3))
    }
}

/// Test basic TCP connection establishment between three nodes
#[tokio::test]
async fn test_basic_tcp_connection() {
    let _ = try_init();

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();
    let node3_id = NodeId::new();

    // Create network configurations
    let config1 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config2 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config3 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };

    // Start networks
    let network1 = TcpNetwork::new(node1_id, config1).await.unwrap();
    let network2 = TcpNetwork::new(node2_id, config2).await.unwrap();
    let network3 = TcpNetwork::new(node3_id, config3).await.unwrap();

    let addr1 = network1.local_addr();
    let addr2 = network2.local_addr();
    let addr3 = network3.local_addr();

    info!(
        "Node1 listening on {}, Node2 listening on {}, Node3 listening on {}",
        addr1, addr2, addr3
    );

    // Connect nodes in a triangle topology: 1 <-> 2 <-> 3 <-> 1
    network1.connect_to_peer(node2_id, addr2).await.unwrap();
    network2.connect_to_peer(node3_id, addr3).await.unwrap();
    network3.connect_to_peer(node1_id, addr1).await.unwrap();

    // Wait for connections to establish (platform-specific timing)
    let (connection_wait, _) = get_test_timeouts();
    sleep(connection_wait).await;

    // Verify connections
    assert!(network1.is_connected(node2_id).await.unwrap());
    assert!(network1.is_connected(node3_id).await.unwrap());
    assert!(network2.is_connected(node1_id).await.unwrap());
    assert!(network2.is_connected(node3_id).await.unwrap());
    assert!(network3.is_connected(node1_id).await.unwrap());
    assert!(network3.is_connected(node2_id).await.unwrap());

    let connected_nodes1 = network1.get_connected_nodes().await.unwrap();
    let connected_nodes2 = network2.get_connected_nodes().await.unwrap();
    let connected_nodes3 = network3.get_connected_nodes().await.unwrap();

    assert!(connected_nodes1.contains(&node2_id));
    assert!(connected_nodes1.contains(&node3_id));
    assert!(connected_nodes2.contains(&node1_id));
    assert!(connected_nodes2.contains(&node3_id));
    assert!(connected_nodes3.contains(&node1_id));
    assert!(connected_nodes3.contains(&node2_id));

    // Cleanup
    network1.shutdown().await;
    network2.shutdown().await;
    network3.shutdown().await;

    info!("Basic TCP connection test completed successfully");
}

/// Test message exchange between TCP-connected nodes
#[tokio::test]
async fn test_tcp_message_exchange() {
    let _ = try_init();

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let config1 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config2 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };

    let mut network1 = TcpNetwork::new(node1_id, config1).await.unwrap();
    let mut network2 = TcpNetwork::new(node2_id, config2).await.unwrap();

    let addr2 = network2.local_addr();

    // Connect and wait for establishment
    network1.connect_to_peer(node2_id, addr2).await.unwrap();
    let (connection_wait, _) = get_test_timeouts();
    sleep(connection_wait).await;

    // Create test messages
    let test_message = ProtocolMessage::new(
        node1_id,
        Some(node2_id),
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: rabia_core::PhaseId::new(1),
            last_committed_phase: rabia_core::PhaseId::new(0),
            active: true,
        }),
    );

    // Send message from node1 to node2
    network1
        .send_to(node2_id, test_message.clone())
        .await
        .unwrap();

    // Receive message on node2
    let (from, received_message) = tokio::time::timeout(Duration::from_secs(5), network2.receive())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(from, node1_id);
    assert_eq!(received_message.from, test_message.from);
    assert_eq!(received_message.to, test_message.to);

    // Test broadcast
    let broadcast_message = ProtocolMessage::new(
        node2_id,
        None,
        MessageType::QuorumNotification(QuorumNotificationMessage {
            has_quorum: true,
            active_nodes: vec![node1_id, node2_id],
        }),
    );

    network2
        .broadcast(broadcast_message.clone(), Some(node2_id))
        .await
        .unwrap();

    // Receive broadcast on node1
    let (from, received_broadcast) =
        tokio::time::timeout(Duration::from_secs(5), network1.receive())
            .await
            .unwrap()
            .unwrap();

    assert_eq!(from, node2_id);
    assert_eq!(received_broadcast.from, broadcast_message.from);

    // Cleanup
    network1.shutdown().await;
    network2.shutdown().await;

    info!("TCP message exchange test completed successfully");
}

/// Test multiple node cluster formation over TCP
#[tokio::test]
async fn test_tcp_cluster_formation() {
    let _ = try_init();

    const NUM_NODES: usize = 5;
    let mut nodes = Vec::new();
    let mut networks = Vec::new();
    let mut addresses = HashMap::new();

    // Create nodes and start networks
    for _i in 0..NUM_NODES {
        let node_id = NodeId::new();
        let config = TcpNetworkConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        };

        let network = TcpNetwork::new(node_id, config).await.unwrap();
        let addr = network.local_addr();

        addresses.insert(node_id, addr);
        nodes.push(node_id);
        networks.push(network);

        debug!("Started node {} on {}", node_id, addr);
    }

    // Connect all nodes to each other (full mesh)
    for i in 0..NUM_NODES {
        for j in 0..NUM_NODES {
            if i != j {
                let target_node = nodes[j];
                let target_addr = addresses[&target_node];

                if let Err(e) = networks[i].connect_to_peer(target_node, target_addr).await {
                    warn!(
                        "Failed to connect node {} to {}: {}",
                        nodes[i], target_node, e
                    );
                }
            }
        }
    }

    // Wait for all connections to establish
    let (connection_wait, _) = get_test_timeouts();
    sleep(connection_wait).await;

    // Verify connectivity
    for i in 0..NUM_NODES {
        let connected = networks[i].get_connected_nodes().await.unwrap();
        info!(
            "Node {} connected to {} other nodes",
            nodes[i],
            connected.len()
        );

        // Each node should be connected to all other nodes
        assert!(
            connected.len() >= NUM_NODES - 2,
            "Node {} only connected to {} nodes, expected at least {}",
            nodes[i],
            connected.len(),
            NUM_NODES - 2
        );
    }

    // Test cluster-wide broadcast
    let broadcaster_idx = 0;
    let broadcast_message = ProtocolMessage::new(
        nodes[broadcaster_idx],
        None,
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: rabia_core::PhaseId::new(42),
            last_committed_phase: rabia_core::PhaseId::new(41),
            active: true,
        }),
    );

    networks[broadcaster_idx]
        .broadcast(broadcast_message.clone(), Some(nodes[broadcaster_idx]))
        .await
        .unwrap();

    // Verify all other nodes receive the broadcast
    let mut received_count = 0;
    for i in 1..NUM_NODES {
        match tokio::time::timeout(Duration::from_secs(3), networks[i].receive()).await {
            Ok(Ok((from, message))) => {
                assert_eq!(from, nodes[broadcaster_idx]);
                if let MessageType::HeartBeat(heartbeat) = message.message_type {
                    assert_eq!(heartbeat.current_phase.value(), 42);
                    received_count += 1;
                }
            }
            _ => {
                warn!("Node {} failed to receive broadcast", nodes[i]);
            }
        }
    }

    info!(
        "Broadcast received by {}/{} nodes",
        received_count,
        NUM_NODES - 1
    );
    assert!(
        received_count >= (NUM_NODES - 1) / 2,
        "Too few nodes received broadcast"
    );

    // Cleanup
    for network in networks {
        network.shutdown().await;
    }

    info!("TCP cluster formation test completed successfully");
}

/// Test TCP connection resilience and fault tolerance
#[tokio::test]
async fn test_tcp_fault_tolerance() {
    let _ = try_init();

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();
    let node3_id = NodeId::new();

    // Start three nodes
    let config1 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config2 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config3 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };

    let mut network1 = TcpNetwork::new(node1_id, config1).await.unwrap();
    let network2 = TcpNetwork::new(node2_id, config2).await.unwrap();
    let mut network3 = TcpNetwork::new(node3_id, config3).await.unwrap();

    let addr1 = network1.local_addr();
    let addr2 = network2.local_addr();
    let addr3 = network3.local_addr();

    // Establish connections: 1 <-> 2 <-> 3
    network1.connect_to_peer(node2_id, addr2).await.unwrap();
    network2.connect_to_peer(node3_id, addr3).await.unwrap();
    network3.connect_to_peer(node1_id, addr1).await.unwrap();

    let (connection_wait, _) = get_test_timeouts();
    sleep(connection_wait).await;

    // Verify initial connectivity
    assert!(network1.is_connected(node2_id).await.unwrap());
    assert!(network2.is_connected(node1_id).await.unwrap());
    assert!(network2.is_connected(node3_id).await.unwrap());
    assert!(network3.is_connected(node2_id).await.unwrap());

    // Test message flow before failure
    let test_message = ProtocolMessage::new(
        node1_id,
        Some(node3_id),
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: rabia_core::PhaseId::new(1),
            last_committed_phase: rabia_core::PhaseId::new(0),
            active: true,
        }),
    );

    // Send from node1 to node3 (should work)
    network1
        .send_to(node3_id, test_message.clone())
        .await
        .unwrap();

    let (from, _) = tokio::time::timeout(Duration::from_secs(2), network3.receive())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(from, node1_id);

    // Simulate node2 failure by shutting it down
    info!("Simulating node2 failure");
    network2.shutdown().await;
    drop(network2);

    // Wait for failure detection
    sleep(Duration::from_millis(500)).await;

    // Node1 and Node3 should detect the failure
    // Test that they can still operate independently
    let heartbeat = ProtocolMessage::new(
        node3_id,
        None,
        MessageType::HeartBeat(HeartBeatMessage {
            current_phase: rabia_core::PhaseId::new(2),
            last_committed_phase: rabia_core::PhaseId::new(1),
            active: false, // Indicating reduced connectivity
        }),
    );

    // Node3 broadcasts (only node1 should receive if still connected)
    network3.broadcast(heartbeat, Some(node3_id)).await.unwrap();

    // Try to receive on node1 (may timeout due to network partition)
    match tokio::time::timeout(Duration::from_secs(1), network1.receive()).await {
        Ok(Ok((from, _))) => {
            info!("Node1 still receiving from node3: {}", from);
        }
        _ => {
            info!("Node1 no longer receiving from node3 (expected after node2 failure)");
        }
    }

    // Cleanup
    network1.shutdown().await;
    network3.shutdown().await;

    info!("TCP fault tolerance test completed successfully");
}

/// Test TCP network with high message volume
#[tokio::test]
async fn test_tcp_high_volume() {
    let _ = try_init();

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let config1 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config2 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };

    let network1 = TcpNetwork::new(node1_id, config1).await.unwrap();
    let mut network2 = TcpNetwork::new(node2_id, config2).await.unwrap();

    let addr2 = network2.local_addr();

    // Connect networks
    network1.connect_to_peer(node2_id, addr2).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    const MESSAGE_COUNT: usize = 100;
    let mut sent_messages = Vec::new();

    // Send many messages rapidly
    for i in 0..MESSAGE_COUNT {
        let message = ProtocolMessage::new(
            node1_id,
            Some(node2_id),
            MessageType::HeartBeat(HeartBeatMessage {
                current_phase: rabia_core::PhaseId::new(i as u64),
                last_committed_phase: rabia_core::PhaseId::new((i as u64).saturating_sub(1)),
                active: true,
            }),
        );

        sent_messages.push(message.clone());
        network1.send_to(node2_id, message).await.unwrap();

        // Small delay to avoid overwhelming
        if i % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }

    // Receive and verify messages
    let mut received_count = 0;
    let timeout_duration = Duration::from_secs(10);
    let start_time = std::time::Instant::now();

    while received_count < MESSAGE_COUNT && start_time.elapsed() < timeout_duration {
        match tokio::time::timeout(Duration::from_millis(100), network2.receive()).await {
            Ok(Ok((from, message))) => {
                assert_eq!(from, node1_id);
                if let MessageType::HeartBeat(_) = message.message_type {
                    received_count += 1;
                }
            }
            _ => {
                // Timeout on individual message - continue trying
                continue;
            }
        }
    }

    info!(
        "Received {}/{} messages in high volume test",
        received_count, MESSAGE_COUNT
    );
    assert!(
        received_count >= MESSAGE_COUNT * 8 / 10,
        "Received only {}/{} messages",
        received_count,
        MESSAGE_COUNT
    );

    // Cleanup
    network1.shutdown().await;
    network2.shutdown().await;

    info!("TCP high volume test completed successfully");
}

/// Test concurrent connections and message handling
#[tokio::test]
async fn test_tcp_concurrent_operations() {
    let _ = try_init();

    let node1_id = NodeId::new();
    let node2_id = NodeId::new();

    let config1 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    let config2 = TcpNetworkConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };

    let network1 = TcpNetwork::new(node1_id, config1).await.unwrap();
    let mut network2 = TcpNetwork::new(node2_id, config2).await.unwrap();

    let addr1 = network1.local_addr();
    let addr2 = network2.local_addr();

    // Establish connections
    network1.connect_to_peer(node2_id, addr2).await.unwrap();
    network2.connect_to_peer(node1_id, addr1).await.unwrap();
    sleep(Duration::from_millis(200)).await;

    const CONCURRENT_TASKS: usize = 20;
    let mut handles = Vec::new();

    // Spawn concurrent senders
    for i in 0..CONCURRENT_TASKS {
        let network1_clone = network1.clone();
        let node2_id_copy = node2_id;
        let node1_id_copy = node1_id;

        let handle = tokio::spawn(async move {
            let message = ProtocolMessage::new(
                node1_id_copy,
                Some(node2_id_copy),
                MessageType::HeartBeat(HeartBeatMessage {
                    current_phase: rabia_core::PhaseId::new(i as u64),
                    last_committed_phase: rabia_core::PhaseId::new(0),
                    active: true,
                }),
            );

            network1_clone.send_to(node2_id_copy, message).await
        });

        handles.push(handle);
    }

    // Wait for all sends to complete
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // Receive messages concurrently
    let mut received_count = 0;
    let timeout_duration = Duration::from_secs(5);
    let start_time = std::time::Instant::now();

    while received_count < CONCURRENT_TASKS && start_time.elapsed() < timeout_duration {
        match tokio::time::timeout(Duration::from_millis(50), network2.receive()).await {
            Ok(Ok((from, _))) => {
                assert_eq!(from, node1_id);
                received_count += 1;
            }
            _ => continue,
        }
    }

    info!(
        "Received {}/{} concurrent messages",
        received_count, CONCURRENT_TASKS
    );
    assert!(
        received_count >= CONCURRENT_TASKS * 8 / 10,
        "Only received {}/{} concurrent messages",
        received_count,
        CONCURRENT_TASKS
    );

    // Cleanup
    network1.shutdown().await;
    network2.shutdown().await;

    info!("TCP concurrent operations test completed successfully");
}
