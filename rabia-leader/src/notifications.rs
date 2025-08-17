//! Notification system for leadership and topology changes.

use crate::LeaderResult;
use rabia_core::{ConsensusState, NodeId};
// Note: removed serde derives since we're using timestamps instead of Instant
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, warn};
use uuid::Uuid;

/// Types of leadership-related notifications
#[derive(Debug, Clone)]
pub enum LeadershipChange {
    /// A new leader has been elected
    LeaderElected {
        node_id: NodeId,
        term: u64,
        timestamp: u64,
    },

    /// Current leader has stepped down
    LeaderSteppedDown {
        node_id: NodeId,
        term: u64,
        reason: String,
        timestamp: u64,
    },

    /// Leadership election has started
    ElectionStarted {
        election_id: Uuid,
        term: u64,
        timestamp: u64,
    },

    /// Leadership election has completed
    ElectionCompleted {
        election_id: Uuid,
        winner: Option<NodeId>,
        term: u64,
        timestamp: u64,
    },

    /// Heartbeat from current leader
    LeaderHeartbeat {
        leader_id: NodeId,
        term: u64,
        timestamp: u64,
    },
}

/// Types of topology-related notifications
#[derive(Debug, Clone)]
pub enum TopologyChange {
    /// A node has joined the cluster
    NodeJoined {
        node_id: NodeId,
        address: std::net::SocketAddr,
        timestamp: u64,
    },

    /// A node has left the cluster
    NodeLeft {
        node_id: NodeId,
        reason: String,
        timestamp: u64,
    },

    /// A node's health status has changed
    NodeHealthChanged {
        node_id: NodeId,
        is_healthy: bool,
        reason: String,
        timestamp: u64,
    },

    /// Cluster topology has been updated
    TopologyUpdated {
        version: u64,
        node_count: usize,
        healthy_count: usize,
        timestamp: u64,
    },

    /// Cluster quorum status has changed
    QuorumChanged {
        has_quorum: bool,
        healthy_nodes: usize,
        required_nodes: usize,
        timestamp: u64,
    },
}

/// Types of consensus-related notifications
#[derive(Debug, Clone)]
pub enum ConsensusChange {
    /// Consensus state has appeared/started
    ConsensusAppeared {
        node_id: NodeId,
        state: ConsensusState,
        timestamp: u64,
    },

    /// Consensus state has disappeared/stopped
    ConsensusDisappeared {
        node_id: NodeId,
        reason: String,
        timestamp: u64,
    },

    /// Consensus state has changed
    ConsensusStateChanged {
        node_id: NodeId,
        old_state: ConsensusState,
        new_state: ConsensusState,
        timestamp: u64,
    },
}

/// Combined notification type
#[derive(Debug, Clone)]
pub enum LeaderNotification {
    Leadership(LeadershipChange),
    Topology(TopologyChange),
    Consensus(ConsensusChange),
}

/// Subscription filter for notifications
#[derive(Clone)]
pub enum NotificationFilter {
    /// Subscribe to all notifications
    All,

    /// Subscribe to leadership changes only
    Leadership,

    /// Subscribe to topology changes only
    Topology,

    /// Subscribe to consensus changes only
    Consensus,

    /// Subscribe to notifications for specific node
    Node(NodeId),

    /// Custom filter function
    Custom(Arc<dyn Fn(&LeaderNotification) -> bool + Send + Sync>),
}

/// Unique identifier for a subscription
pub type SubscriptionId = Uuid;

/// Statistics about notification delivery
#[derive(Debug, Default, Clone)]
pub struct NotificationStats {
    pub notifications_sent: u64,
    pub notifications_delivered: u64,
    pub notifications_dropped: u64,
    pub active_subscriptions: usize,
    pub total_subscriptions: u64,
}

/// Type alias for complex subscriber map
type SubscriberMap = HashMap<
    SubscriptionId,
    (
        NotificationFilter,
        mpsc::UnboundedSender<LeaderNotification>,
    ),
>;

/// Notification bus for leader-related events
pub struct LeaderNotificationBus {
    broadcast_tx: broadcast::Sender<LeaderNotification>,
    subscribers: Arc<RwLock<SubscriberMap>>,
    stats: Arc<RwLock<NotificationStats>>,
}

impl Default for LeaderNotificationBus {
    fn default() -> Self {
        Self::new()
    }
}

impl LeaderNotificationBus {
    /// Create a new notification bus
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            broadcast_tx,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NotificationStats::default())),
        }
    }

    /// Create with custom buffer size
    pub fn with_capacity(capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(capacity);

        Self {
            broadcast_tx,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NotificationStats::default())),
        }
    }

    /// Subscribe to notifications with a filter
    pub async fn subscribe(
        &self,
        filter: NotificationFilter,
    ) -> LeaderResult<(SubscriptionId, mpsc::UnboundedReceiver<LeaderNotification>)> {
        let subscription_id = Uuid::new_v4();
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut subscribers = self.subscribers.write().await;
            subscribers.insert(subscription_id, (filter, tx));

            let mut stats = self.stats.write().await;
            stats.active_subscriptions = subscribers.len();
            stats.total_subscriptions += 1;
        }

        debug!("Created subscription {} with filter", subscription_id);

        Ok((subscription_id, rx))
    }

    /// Unsubscribe from notifications
    pub async fn unsubscribe(&self, subscription_id: SubscriptionId) -> LeaderResult<()> {
        let mut subscribers = self.subscribers.write().await;

        if subscribers.remove(&subscription_id).is_some() {
            debug!("Removed subscription {}", subscription_id);

            let mut stats = self.stats.write().await;
            stats.active_subscriptions = subscribers.len();
        }

        Ok(())
    }

    /// Broadcast a notification to all subscribers
    async fn broadcast(&self, notification: LeaderNotification) {
        {
            let mut stats = self.stats.write().await;
            stats.notifications_sent += 1;
        }

        // Send to broadcast channel (for new subscribers)
        if let Err(e) = self.broadcast_tx.send(notification.clone()) {
            warn!("Failed to send to broadcast channel: {}", e);
        }

        // Send to filtered subscribers
        let subscribers = self.subscribers.read().await;
        let mut delivered = 0;
        let mut dropped = 0;

        for (_, (filter, tx)) in subscribers.iter() {
            if self.matches_filter(filter, &notification) {
                match tx.send(notification.clone()) {
                    Ok(_) => delivered += 1,
                    Err(_) => {
                        dropped += 1;
                        warn!("Failed to deliver notification to subscriber");
                    }
                }
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.notifications_delivered += delivered;
            stats.notifications_dropped += dropped;
        }

        debug!(
            "Broadcast notification: delivered={}, dropped={}",
            delivered, dropped
        );
    }

    /// Notify about leadership gained
    pub async fn notify_leadership_gained(&self, node_id: NodeId, term: u64) {
        let notification = LeaderNotification::Leadership(LeadershipChange::LeaderElected {
            node_id,
            term,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about leadership lost
    pub async fn notify_leadership_lost(&self, node_id: NodeId, term: u64) {
        let notification = LeaderNotification::Leadership(LeadershipChange::LeaderSteppedDown {
            node_id,
            term,
            reason: "Stepped down gracefully".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about election started
    pub async fn notify_election_started(&self, election_id: Uuid, term: u64) {
        let notification = LeaderNotification::Leadership(LeadershipChange::ElectionStarted {
            election_id,
            term,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about heartbeat
    pub async fn notify_heartbeat(&self, leader_id: NodeId, term: u64) {
        let notification = LeaderNotification::Leadership(LeadershipChange::LeaderHeartbeat {
            leader_id,
            term,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about node joining
    pub async fn notify_node_joined(&self, node_id: NodeId, address: std::net::SocketAddr) {
        let notification = LeaderNotification::Topology(TopologyChange::NodeJoined {
            node_id,
            address,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about node leaving
    pub async fn notify_node_left(&self, node_id: NodeId, reason: String) {
        let notification = LeaderNotification::Topology(TopologyChange::NodeLeft {
            node_id,
            reason,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about node health change
    pub async fn notify_node_health_changed(
        &self,
        node_id: NodeId,
        is_healthy: bool,
        reason: String,
    ) {
        let notification = LeaderNotification::Topology(TopologyChange::NodeHealthChanged {
            node_id,
            is_healthy,
            reason,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about quorum change
    pub async fn notify_quorum_changed(
        &self,
        has_quorum: bool,
        healthy_nodes: usize,
        required_nodes: usize,
    ) {
        let notification = LeaderNotification::Topology(TopologyChange::QuorumChanged {
            has_quorum,
            healthy_nodes,
            required_nodes,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about consensus appearance
    pub async fn notify_consensus_appeared(&self, node_id: NodeId, state: ConsensusState) {
        let notification = LeaderNotification::Consensus(ConsensusChange::ConsensusAppeared {
            node_id,
            state,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about consensus disappearance
    pub async fn notify_consensus_disappeared(&self, node_id: NodeId, reason: String) {
        let notification = LeaderNotification::Consensus(ConsensusChange::ConsensusDisappeared {
            node_id,
            reason,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Notify about consensus state change
    pub async fn notify_consensus_state_changed(&self, node_id: NodeId, state: ConsensusState) {
        // For this notification, we need the old state, but for simplicity
        // we'll use a default old state. In practice, this would be tracked.
        let notification = LeaderNotification::Consensus(ConsensusChange::ConsensusStateChanged {
            node_id,
            old_state: ConsensusState::Idle, // Would be tracked in practice
            new_state: state,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });

        self.broadcast(notification).await;
    }

    /// Get notification statistics
    pub async fn get_stats(&self) -> NotificationStats {
        self.stats.read().await.clone()
    }

    /// Get number of active subscriptions
    pub async fn subscription_count(&self) -> usize {
        self.subscribers.read().await.len()
    }

    // Private methods

    fn matches_filter(
        &self,
        filter: &NotificationFilter,
        notification: &LeaderNotification,
    ) -> bool {
        match filter {
            NotificationFilter::All => true,
            NotificationFilter::Leadership => {
                matches!(notification, LeaderNotification::Leadership(_))
            }
            NotificationFilter::Topology => matches!(notification, LeaderNotification::Topology(_)),
            NotificationFilter::Consensus => {
                matches!(notification, LeaderNotification::Consensus(_))
            }
            NotificationFilter::Node(node_id) => {
                self.notification_involves_node(notification, *node_id)
            }
            NotificationFilter::Custom(func) => func(notification),
        }
    }

    fn notification_involves_node(
        &self,
        notification: &LeaderNotification,
        node_id: NodeId,
    ) -> bool {
        match notification {
            LeaderNotification::Leadership(change) => match change {
                LeadershipChange::LeaderElected { node_id: id, .. } => *id == node_id,
                LeadershipChange::LeaderSteppedDown { node_id: id, .. } => *id == node_id,
                LeadershipChange::LeaderHeartbeat { leader_id, .. } => *leader_id == node_id,
                _ => false,
            },
            LeaderNotification::Topology(change) => match change {
                TopologyChange::NodeJoined { node_id: id, .. } => *id == node_id,
                TopologyChange::NodeLeft { node_id: id, .. } => *id == node_id,
                TopologyChange::NodeHealthChanged { node_id: id, .. } => *id == node_id,
                _ => false,
            },
            LeaderNotification::Consensus(change) => match change {
                ConsensusChange::ConsensusAppeared { node_id: id, .. } => *id == node_id,
                ConsensusChange::ConsensusDisappeared { node_id: id, .. } => *id == node_id,
                ConsensusChange::ConsensusStateChanged { node_id: id, .. } => *id == node_id,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_notification_bus_creation() {
        let bus = LeaderNotificationBus::new();
        assert_eq!(bus.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_subscribe_and_unsubscribe() {
        let bus = LeaderNotificationBus::new();

        let (id, _rx) = bus.subscribe(NotificationFilter::All).await.unwrap();
        assert_eq!(bus.subscription_count().await, 1);

        bus.unsubscribe(id).await.unwrap();
        assert_eq!(bus.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_leadership_notification() {
        let bus = LeaderNotificationBus::new();
        let node_id = NodeId::from(1);

        let (_id, mut rx) = bus.subscribe(NotificationFilter::Leadership).await.unwrap();

        bus.notify_leadership_gained(node_id, 1).await;

        let notification = rx.recv().await.unwrap();
        assert!(matches!(notification, LeaderNotification::Leadership(_)));
    }

    #[tokio::test]
    async fn test_topology_notification() {
        let bus = LeaderNotificationBus::new();
        let node_id = NodeId::from(1);
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8001);

        let (_id, mut rx) = bus.subscribe(NotificationFilter::Topology).await.unwrap();

        bus.notify_node_joined(node_id, address).await;

        let notification = rx.recv().await.unwrap();
        assert!(matches!(notification, LeaderNotification::Topology(_)));
    }

    #[tokio::test]
    async fn test_consensus_notification() {
        let bus = LeaderNotificationBus::new();
        let node_id = NodeId::from(1);

        let (_id, mut rx) = bus.subscribe(NotificationFilter::Consensus).await.unwrap();

        bus.notify_consensus_appeared(node_id, ConsensusState::Active)
            .await;

        let notification = rx.recv().await.unwrap();
        assert!(matches!(notification, LeaderNotification::Consensus(_)));
    }

    #[tokio::test]
    async fn test_filtered_subscriptions() {
        let bus = LeaderNotificationBus::new();
        let node_id = NodeId::from(1);

        let (_id1, mut rx1) = bus.subscribe(NotificationFilter::Leadership).await.unwrap();
        let (_id2, mut rx2) = bus.subscribe(NotificationFilter::Topology).await.unwrap();

        // Send leadership notification
        bus.notify_leadership_gained(node_id, 1).await;

        // Only leadership subscriber should receive it
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_err());
    }
}
