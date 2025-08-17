//! Example demonstrating leader management in a Rabia cluster.
//!
//! This example shows how to:
//! - Create and configure a Leader Manager
//! - Monitor leadership changes and topology events
//! - Handle consensus state coordination
//! - Implement custom notification handlers

use rabia_core::{ConsensusState, NodeId};
use rabia_leader::{
    ConsensusChange, LeaderConfig, LeaderManager, LeaderNotification, LeaderNotificationBus,
    LeadershipChange, NotificationFilter, TopologyChange,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Leader Manager example");

    // Run the example
    run_leader_example().await?;

    Ok(())
}

async fn run_leader_example() -> Result<(), Box<dyn std::error::Error>> {
    // Configure the leader manager
    let config = LeaderConfig {
        heartbeat_interval: Duration::from_millis(1000),
        election_timeout: Duration::from_millis(3000),
        leader_timeout: Duration::from_millis(2000),
        min_healthy_nodes: 2,
        auto_failover: true,
        leadership_priority: 150, // Higher priority
    };

    // Create leader manager
    info!("Creating Leader Manager...");
    let leader_manager = std::sync::Arc::new(LeaderManager::new(config).await?);

    // Create notification handler
    let notification_handler = create_notification_handler().await?;

    // Simulate a cluster with multiple nodes
    info!("Setting up cluster simulation...");
    simulate_cluster_scenario(&leader_manager, &notification_handler).await?;

    info!("Leader Manager example completed");
    Ok(())
}

async fn create_notification_handler() -> Result<LeaderNotificationBus, Box<dyn std::error::Error>>
{
    let notification_bus = LeaderNotificationBus::new();

    // Subscribe to all leadership events
    let (_leadership_id, mut leadership_rx) = notification_bus
        .subscribe(NotificationFilter::Leadership)
        .await?;

    // Subscribe to topology events
    let (_topology_id, mut topology_rx) = notification_bus
        .subscribe(NotificationFilter::Topology)
        .await?;

    // Subscribe to consensus events
    let (_consensus_id, mut consensus_rx) = notification_bus
        .subscribe(NotificationFilter::Consensus)
        .await?;

    // Spawn handlers for different notification types
    tokio::spawn(async move {
        while let Some(notification) = leadership_rx.recv().await {
            handle_leadership_notification(notification).await;
        }
        info!("Leadership notification handler stopped");
    });

    tokio::spawn(async move {
        while let Some(notification) = topology_rx.recv().await {
            handle_topology_notification(notification).await;
        }
        info!("Topology notification handler stopped");
    });

    tokio::spawn(async move {
        while let Some(notification) = consensus_rx.recv().await {
            handle_consensus_notification(notification).await;
        }
        info!("Consensus notification handler stopped");
    });

    Ok(notification_bus)
}

async fn handle_leadership_notification(notification: LeaderNotification) {
    if let LeaderNotification::Leadership(change) = notification {
        match change {
            LeadershipChange::LeaderElected { node_id, term, .. } => {
                info!("🏆 New leader elected: {} (term {})", node_id, term);
            }
            LeadershipChange::LeaderSteppedDown {
                node_id,
                term,
                reason,
                ..
            } => {
                warn!(
                    "📉 Leader {} stepped down (term {}): {}",
                    node_id, term, reason
                );
            }
            LeadershipChange::ElectionStarted {
                election_id, term, ..
            } => {
                info!(
                    "🗳️  Leadership election started: {} (term {})",
                    election_id, term
                );
            }
            LeadershipChange::LeaderHeartbeat {
                leader_id, term, ..
            } => {
                info!("💓 Heartbeat from leader {} (term {})", leader_id, term);
            }
            _ => {}
        }
    }
}

async fn handle_topology_notification(notification: LeaderNotification) {
    if let LeaderNotification::Topology(change) = notification {
        match change {
            TopologyChange::NodeJoined {
                node_id, address, ..
            } => {
                info!("➕ Node {} joined cluster at {}", node_id, address);
            }
            TopologyChange::NodeLeft {
                node_id, reason, ..
            } => {
                warn!("➖ Node {} left cluster: {}", node_id, reason);
            }
            TopologyChange::NodeHealthChanged {
                node_id,
                is_healthy,
                reason,
                ..
            } => {
                if is_healthy {
                    info!("✅ Node {} is now healthy: {}", node_id, reason);
                } else {
                    warn!("❌ Node {} is unhealthy: {}", node_id, reason);
                }
            }
            TopologyChange::QuorumChanged {
                has_quorum,
                healthy_nodes,
                required_nodes,
                ..
            } => {
                if has_quorum {
                    info!(
                        "✅ Cluster has quorum: {}/{} nodes healthy",
                        healthy_nodes, required_nodes
                    );
                } else {
                    error!(
                        "❌ Cluster lost quorum: {}/{} nodes healthy",
                        healthy_nodes, required_nodes
                    );
                }
            }
            _ => {}
        }
    }
}

async fn handle_consensus_notification(notification: LeaderNotification) {
    if let LeaderNotification::Consensus(change) = notification {
        match change {
            ConsensusChange::ConsensusAppeared { node_id, state, .. } => {
                info!(
                    "🔄 Consensus appeared on node {} with state {:?}",
                    node_id, state
                );
            }
            ConsensusChange::ConsensusDisappeared {
                node_id, reason, ..
            } => {
                warn!("❌ Consensus disappeared on node {}: {}", node_id, reason);
            }
            ConsensusChange::ConsensusStateChanged {
                node_id, new_state, ..
            } => {
                info!(
                    "🔄 Consensus state changed on node {} to {:?}",
                    node_id, new_state
                );
            }
        }
    }
}

async fn simulate_cluster_scenario(
    leader_manager: &std::sync::Arc<LeaderManager>,
    notification_bus: &LeaderNotificationBus,
) -> Result<(), Box<dyn std::error::Error>> {
    // Start the leader manager
    info!("Starting leader manager...");
    leader_manager.clone().start().await?;

    // Wait a bit for initial state
    sleep(Duration::from_millis(500)).await;

    // Check initial state
    let initial_state = leader_manager.get_state().await;
    info!("Initial leader state: {:?}", initial_state);

    // Simulate some cluster nodes joining
    info!("Simulating cluster nodes joining...");

    let nodes = vec![
        (
            NodeId::from(1),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8001),
        ),
        (
            NodeId::from(2),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8002),
        ),
        (
            NodeId::from(3),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8003),
        ),
    ];

    for (node_id, address) in nodes {
        notification_bus.notify_node_joined(node_id, address).await;

        // Simulate consensus appearing on each node
        notification_bus
            .notify_consensus_appeared(node_id, ConsensusState::Active)
            .await;

        sleep(Duration::from_millis(200)).await;
    }

    // Simulate health changes
    sleep(Duration::from_millis(1000)).await;
    info!("Simulating health status changes...");

    notification_bus
        .notify_node_health_changed(
            NodeId::from(2),
            false,
            "Network connectivity issues".to_string(),
        )
        .await;

    sleep(Duration::from_millis(500)).await;

    // Simulate quorum change
    notification_bus.notify_quorum_changed(false, 2, 3).await;

    sleep(Duration::from_millis(1000)).await;

    // Node recovery
    info!("Simulating node recovery...");
    notification_bus
        .notify_node_health_changed(
            NodeId::from(2),
            true,
            "Network connectivity restored".to_string(),
        )
        .await;

    notification_bus.notify_quorum_changed(true, 3, 3).await;

    // Update some consensus states
    sleep(Duration::from_millis(500)).await;
    info!("Updating consensus states...");

    leader_manager
        .update_consensus_state(NodeId::from(1), ConsensusState::Active)
        .await?;
    leader_manager
        .update_consensus_state(NodeId::from(2), ConsensusState::Idle)
        .await?;
    leader_manager
        .update_consensus_state(NodeId::from(3), ConsensusState::Active)
        .await?;

    // Show final statistics
    sleep(Duration::from_millis(1000)).await;
    let stats = leader_manager.get_stats().await;
    info!("Final leadership stats: {:?}", stats);

    let notification_stats = notification_bus.get_stats().await;
    info!("Notification stats: {:?}", notification_stats);

    // Check final leader state
    let final_state = leader_manager.get_state().await;
    info!("Final leader state: {:?}", final_state);

    if leader_manager.is_leader().await {
        info!("This node is the cluster leader!");
    } else if let Some(leader_id) = leader_manager.get_leader().await {
        info!("Cluster leader is: {}", leader_id);
    } else {
        info!("No current cluster leader");
    }

    // Demonstrate stepping down if we're the leader
    if leader_manager.is_leader().await {
        info!("Stepping down from leadership...");
        leader_manager.step_down().await?;

        sleep(Duration::from_millis(500)).await;
        let after_stepdown = leader_manager.get_state().await;
        info!("State after stepping down: {:?}", after_stepdown);
    }

    // Stop the leader manager
    info!("Stopping leader manager...");
    leader_manager.stop().await?;

    Ok(())
}

/// Example of custom notification filter
#[allow(dead_code)]
async fn custom_filter_example(
    notification_bus: &LeaderNotificationBus,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;

    // Create a custom filter that only accepts notifications for specific nodes
    let target_nodes = [NodeId::from(1), NodeId::from(2)];
    let filter = NotificationFilter::Custom(Arc::new(move |notification| match notification {
        LeaderNotification::Leadership(change) => match change {
            LeadershipChange::LeaderElected { node_id, .. } => target_nodes.contains(node_id),
            LeadershipChange::LeaderSteppedDown { node_id, .. } => target_nodes.contains(node_id),
            _ => false,
        },
        _ => false,
    }));

    let (_id, mut rx) = notification_bus.subscribe(filter).await?;

    // Handle filtered notifications
    tokio::spawn(async move {
        while let Some(notification) = rx.recv().await {
            info!("Custom filtered notification: {:?}", notification);
        }
    });

    Ok(())
}
