//! Leader management and coordination for Rabia consensus clusters.

use crate::{HealthMonitor, LeaderError, LeaderNotificationBus, LeaderResult, TopologyManager};
use rabia_core::{ConsensusState, NodeId};
// Note: removed serde derives since we're using timestamps instead of Instant
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the Leader Manager
#[derive(Debug, Clone)]
pub struct LeaderConfig {
    /// Duration between leadership heartbeats
    pub heartbeat_interval: Duration,

    /// Timeout for leadership election
    pub election_timeout: Duration,

    /// Maximum time without leader before triggering election
    pub leader_timeout: Duration,

    /// Minimum number of healthy nodes required for leadership
    pub min_healthy_nodes: usize,

    /// Enable automatic leadership failover
    pub auto_failover: bool,

    /// Leadership priority (higher = more likely to become leader)
    pub leadership_priority: u8,
}

impl Default for LeaderConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(1000),
            election_timeout: Duration::from_millis(5000),
            leader_timeout: Duration::from_millis(3000),
            min_healthy_nodes: 1,
            auto_failover: true,
            leadership_priority: 100,
        }
    }
}

/// Current state of leadership in the cluster
#[derive(Debug, Clone, PartialEq)]
pub enum LeaderState {
    /// No leader currently elected
    NoLeader,

    /// This node is the leader
    Leader {
        /// When leadership was established
        since: u64,
        /// Current term/epoch
        term: u64,
    },

    /// Another node is the leader
    Follower {
        /// ID of the current leader
        leader_id: NodeId,
        /// When we last heard from the leader
        last_heartbeat: u64,
        /// Current term/epoch
        term: u64,
    },

    /// Leadership election in progress
    ElectionInProgress {
        /// Election ID
        election_id: Uuid,
        /// When election started
        started_at: u64,
        /// Current term/epoch
        term: u64,
    },
}

/// Statistics about leadership operations
#[derive(Debug, Default, Clone)]
pub struct LeadershipStats {
    pub elections_started: u64,
    pub elections_won: u64,
    pub elections_lost: u64,
    pub leadership_changes: u64,
    pub heartbeats_sent: u64,
    pub heartbeats_received: u64,
    pub failovers_triggered: u64,
}

/// Leader Manager coordinates cluster leadership and consensus state
pub struct LeaderManager {
    config: LeaderConfig,
    node_id: NodeId,
    state: Arc<RwLock<LeaderState>>,
    topology: Arc<TopologyManager>,
    health_monitor: Arc<HealthMonitor>,
    notification_bus: Arc<LeaderNotificationBus>,
    stats: Arc<RwLock<LeadershipStats>>,
    consensus_state: Arc<RwLock<HashMap<NodeId, ConsensusState>>>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl LeaderManager {
    /// Create a new Leader Manager
    pub async fn new(config: LeaderConfig) -> LeaderResult<Self> {
        let node_id = NodeId::new();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let topology = Arc::new(TopologyManager::new().await?);
        let health_monitor = Arc::new(HealthMonitor::new().await?);
        let notification_bus = Arc::new(LeaderNotificationBus::new());

        Ok(Self {
            config,
            node_id,
            state: Arc::new(RwLock::new(LeaderState::NoLeader)),
            topology,
            health_monitor,
            notification_bus,
            stats: Arc::new(RwLock::new(LeadershipStats::default())),
            consensus_state: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Start the leader manager
    pub async fn start(self: Arc<Self>) -> LeaderResult<()> {
        info!("Starting Leader Manager for node {}", self.node_id);

        // Start background tasks
        self.start_heartbeat_task().await?;
        self.start_election_monitor().await?;
        self.start_health_monitor().await?;

        // Initial leader election if no leader present
        if matches!(*self.state.read().await, LeaderState::NoLeader) {
            self.trigger_election().await?;
        }

        Ok(())
    }

    /// Stop the leader manager
    pub async fn stop(&self) -> LeaderResult<()> {
        info!("Stopping Leader Manager for node {}", self.node_id);

        // Signal shutdown
        if let Err(e) = self.shutdown_tx.send(true) {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // If we're the leader, gracefully step down
        if self.is_leader().await {
            self.step_down().await?;
        }

        Ok(())
    }

    /// Check if this node is currently the leader
    pub async fn is_leader(&self) -> bool {
        matches!(*self.state.read().await, LeaderState::Leader { .. })
    }

    /// Get the current leader state
    pub async fn get_state(&self) -> LeaderState {
        self.state.read().await.clone()
    }

    /// Get the current leader node ID (if any)
    pub async fn get_leader(&self) -> Option<NodeId> {
        match &*self.state.read().await {
            LeaderState::Leader { .. } => Some(self.node_id),
            LeaderState::Follower { leader_id, .. } => Some(*leader_id),
            _ => None,
        }
    }

    /// Trigger a new leadership election
    pub async fn trigger_election(&self) -> LeaderResult<()> {
        let mut state = self.state.write().await;
        let current_term = self.get_current_term(&state);
        let new_term = current_term + 1;
        let election_id = Uuid::new_v4();

        info!(
            "Triggering leadership election for term {} (election {})",
            new_term, election_id
        );

        *state = LeaderState::ElectionInProgress {
            election_id,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            term: new_term,
        };

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.elections_started += 1;
        }

        // Notify about election start
        self.notification_bus
            .notify_election_started(election_id, new_term)
            .await;

        // Start election process
        drop(state);
        self.conduct_election(election_id, new_term).await
    }

    /// Gracefully step down from leadership
    pub async fn step_down(&self) -> LeaderResult<()> {
        let mut state = self.state.write().await;

        if let LeaderState::Leader { term, .. } = &*state {
            info!("Stepping down from leadership (term {})", term);

            // Notify cluster about stepping down
            self.notification_bus
                .notify_leadership_lost(self.node_id, *term)
                .await;

            *state = LeaderState::NoLeader;

            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.leadership_changes += 1;
            }
        }

        Ok(())
    }

    /// Update consensus state for a node
    pub async fn update_consensus_state(
        &self,
        node_id: NodeId,
        state: ConsensusState,
    ) -> LeaderResult<()> {
        let mut consensus_state = self.consensus_state.write().await;
        consensus_state.insert(node_id, state);

        // Notify about consensus state change
        self.notification_bus
            .notify_consensus_state_changed(node_id, state)
            .await;

        Ok(())
    }

    /// Get consensus state for all nodes
    pub async fn get_consensus_states(&self) -> HashMap<NodeId, ConsensusState> {
        self.consensus_state.read().await.clone()
    }

    /// Get leadership statistics
    pub async fn get_stats(&self) -> LeadershipStats {
        self.stats.read().await.clone()
    }

    // Private methods

    async fn start_heartbeat_task(&self) -> LeaderResult<()> {
        let state = Arc::clone(&self.state);
        let notification_bus = Arc::clone(&self.notification_bus);
        let stats = Arc::clone(&self.stats);
        let mut shutdown_rx = self.shutdown_rx.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let node_id = self.node_id;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let current_state = state.read().await.clone();

                        if let LeaderState::Leader { term, .. } = current_state {
                            // Send heartbeat as leader
                            notification_bus.notify_heartbeat(node_id, term).await;

                            let mut stats_guard = stats.write().await;
                            stats_guard.heartbeats_sent += 1;
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn start_election_monitor(self: &Arc<Self>) -> LeaderResult<()> {
        let state = Arc::clone(&self.state);
        let leader_timeout = self.config.leader_timeout;
        let mut shutdown_rx = self.shutdown_rx.clone();
        let self_clone = Arc::clone(self);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let current_state = state.read().await.clone();

                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;

                        match current_state {
                            LeaderState::Follower { last_heartbeat, .. } => {
                                if now.saturating_sub(last_heartbeat) > leader_timeout.as_millis() as u64 {
                                    warn!("Leader timeout exceeded, triggering election");
                                    if let Err(e) = self_clone.trigger_election().await {
                                        error!("Failed to trigger election: {}", e);
                                    }
                                }
                            }
                            LeaderState::ElectionInProgress { started_at, .. } => {
                                if now.saturating_sub(started_at) > self_clone.config.election_timeout.as_millis() as u64 {
                                    warn!("Election timeout, retrying");
                                    if let Err(e) = self_clone.trigger_election().await {
                                        error!("Failed to retry election: {}", e);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn start_health_monitor(&self) -> LeaderResult<()> {
        // Start health monitoring for the cluster
        self.health_monitor.clone().start_monitoring().await?;
        Ok(())
    }

    async fn conduct_election(&self, election_id: Uuid, term: u64) -> LeaderResult<()> {
        debug!("Conducting election {} for term {}", election_id, term);

        // Get healthy nodes from topology
        let healthy_nodes = self.topology.get_healthy_nodes().await?;

        if healthy_nodes.len() < self.config.min_healthy_nodes {
            return Err(LeaderError::ElectionFailed {
                reason: format!(
                    "Insufficient healthy nodes: {} < {}",
                    healthy_nodes.len(),
                    self.config.min_healthy_nodes
                ),
            });
        }

        // Simple leader election based on node priority and ID
        let winner = self.determine_election_winner(&healthy_nodes).await?;

        if winner == self.node_id {
            self.become_leader(term).await?;
        } else {
            self.become_follower(winner, term).await?;
        }

        Ok(())
    }

    async fn determine_election_winner(&self, nodes: &[NodeId]) -> LeaderResult<NodeId> {
        // Simple deterministic election based on node characteristics
        // In a real implementation, this would involve voting and consensus

        let mut best_candidate = nodes[0];
        let mut best_priority = 0;

        for &node_id in nodes {
            let priority = if node_id == self.node_id {
                self.config.leadership_priority
            } else {
                // In practice, would query other nodes for their priority
                100 // Default priority
            };

            if priority > best_priority || (priority == best_priority && node_id < best_candidate) {
                best_candidate = node_id;
                best_priority = priority;
            }
        }

        Ok(best_candidate)
    }

    async fn become_leader(&self, term: u64) -> LeaderResult<()> {
        info!("Becoming leader for term {}", term);

        let mut state = self.state.write().await;
        *state = LeaderState::Leader {
            since: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            term,
        };

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.elections_won += 1;
            stats.leadership_changes += 1;
        }

        // Notify about leadership gained
        self.notification_bus
            .notify_leadership_gained(self.node_id, term)
            .await;

        Ok(())
    }

    async fn become_follower(&self, leader_id: NodeId, term: u64) -> LeaderResult<()> {
        debug!("Becoming follower of {} for term {}", leader_id, term);

        let mut state = self.state.write().await;
        *state = LeaderState::Follower {
            leader_id,
            last_heartbeat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            term,
        };

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.elections_lost += 1;
        }

        Ok(())
    }

    fn get_current_term(&self, state: &LeaderState) -> u64 {
        match state {
            LeaderState::Leader { term, .. } => *term,
            LeaderState::Follower { term, .. } => *term,
            LeaderState::ElectionInProgress { term, .. } => *term,
            LeaderState::NoLeader => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_leader_manager_creation() {
        let config = LeaderConfig::default();
        let manager = LeaderManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_initial_state() {
        let config = LeaderConfig::default();
        let manager = LeaderManager::new(config).await.unwrap();

        let state = manager.get_state().await;
        assert_eq!(state, LeaderState::NoLeader);
        assert!(!manager.is_leader().await);
    }

    #[tokio::test]
    async fn test_election_trigger() {
        let config = LeaderConfig::default();
        let manager = LeaderManager::new(config).await.unwrap();

        // This will fail due to insufficient nodes, but should update state
        let _ = manager.trigger_election().await;

        let state = manager.get_state().await;
        assert!(matches!(state, LeaderState::ElectionInProgress { .. }));
    }
}
